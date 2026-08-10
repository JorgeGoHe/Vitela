//! Firma digital (PAdES básico): inserta un campo de firma invisible con
//! ByteRange y firma PKCS#7 detached (RSA + SHA-256). PDFium no firma, así
//! que la cirugía del documento se hace con lopdf y la criptografía con
//! RustCrypto. No usa PDFium: no necesita el hilo dedicado.

use cms::builder::{SignedDataBuilder, SignerInfoBuilder};
use cms::cert::{CertificateChoices, IssuerAndSerialNumber};
use cms::signed_data::{EncapsulatedContentInfo, SignerIdentifier};
use der::{DecodePem, Encode};
use lopdf::{Dictionary, Document as LoDoc, Object, StringFormat};
use rsa::pkcs8::DecodePrivateKey;
use sha2::{Digest, Sha256};
use x509_cert::spki::AlgorithmIdentifierOwned;

/// Hueco reservado para la firma DER dentro de /Contents (en bytes).
const SIG_LEN: usize = 8192;

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

/// Certificado y clave privada listos para firmar.
pub struct Credenciales {
    cert: x509_cert::Certificate,
    key: rsa::RsaPrivateKey,
}

/// Credenciales desde certificado + clave en PEM (RSA sin cifrar).
pub fn credenciales_pem(cert_pem: &str, key_pem: &str) -> Result<Credenciales, String> {
    let cert = x509_cert::Certificate::from_pem(cert_pem)
        .map_err(|e| format!("Certificado PEM inválido: {e}"))?;
    let key = rsa::RsaPrivateKey::from_pkcs8_pem(key_pem)
        .or_else(|_| {
            use rsa::pkcs1::DecodeRsaPrivateKey;
            rsa::RsaPrivateKey::from_pkcs1_pem(key_pem)
        })
        .map_err(|e| format!("Clave privada PEM inválida (RSA sin cifrar): {e}"))?;
    Ok(Credenciales { cert, key })
}

/// Credenciales desde un contenedor PKCS#12 (.p12/.pfx) con contraseña.
pub fn credenciales_p12(p12_bytes: &[u8], password: &str) -> Result<Credenciales, String> {
    use der::Decode;
    let store = p12_keystore::KeyStore::from_pkcs12(
        p12_bytes,
        password,
        p12_keystore::Pkcs12ImportPolicy::default(),
    )
    .map_err(|e| format!("No se pudo abrir el .p12 (¿contraseña incorrecta?): {e}"))?;
    let (_alias, chain) = store
        .private_key_chain()
        .ok_or("El .p12 no contiene ninguna clave privada")?;
    let key = rsa::RsaPrivateKey::from_pkcs8_der(chain.key().as_der())
        .map_err(|e| format!("La clave del .p12 no es RSA sin cifrar: {e}"))?;
    let cert_der = chain
        .certs()
        .first()
        .ok_or("El .p12 no contiene certificado")?
        .as_der();
    let cert = x509_cert::Certificate::from_der(cert_der)
        .map_err(|e| format!("Certificado del .p12 inválido: {e}"))?;
    Ok(Credenciales { cert, key })
}

/// Construye el CMS SignedData detached sobre el digest dado.
fn build_cms(cred: &Credenciales, digest: &[u8]) -> Result<Vec<u8>, String> {
    let cert = cred.cert.clone();
    let key = cred.key.clone();
    let signer_id = SignerIdentifier::IssuerAndSerialNumber(IssuerAndSerialNumber {
        issuer: cert.tbs_certificate.issuer.clone(),
        serial_number: cert.tbs_certificate.serial_number.clone(),
    });
    let digest_alg = AlgorithmIdentifierOwned {
        oid: const_oid::db::rfc5912::ID_SHA_256,
        parameters: None,
    };
    let content = EncapsulatedContentInfo {
        econtent_type: const_oid::db::rfc5911::ID_DATA,
        econtent: None,
    };
    let signing_key = rsa::pkcs1v15::SigningKey::<Sha256>::new(key);
    let si_builder = SignerInfoBuilder::new(
        &signing_key,
        signer_id,
        digest_alg.clone(),
        &content,
        Some(digest),
    )
    .map_err(|e| format!("SignerInfo: {e}"))?;
    let mut builder = SignedDataBuilder::new(&content);
    let signed = builder
        .add_digest_algorithm(digest_alg)
        .map_err(|e| e.to_string())?
        .add_certificate(CertificateChoices::Certificate(cert))
        .map_err(|e| e.to_string())?
        .add_signer_info::<rsa::pkcs1v15::SigningKey<Sha256>, rsa::pkcs1v15::Signature>(
            si_builder,
        )
        .map_err(|e| e.to_string())?
        .build()
        .map_err(|e| e.to_string())?;
    signed.to_der().map_err(|e| e.to_string())
}

/// Firma el PDF de `src_path` y escribe el resultado en `dest_path`.
pub fn sign(
    src_path: &str,
    dest_path: &str,
    cred: &Credenciales,
    reason: Option<String>,
) -> Result<(), String> {
    let mut doc = LoDoc::load(src_path).map_err(|e| format!("No se pudo leer el PDF: {e}"))?;
    let page_id = *doc.get_pages().get(&1).ok_or("El PDF no tiene páginas")?;

    // diccionario de firma con huecos para ByteRange y Contents
    let mut sig = Dictionary::new();
    sig.set("Type", Object::Name(b"Sig".to_vec()));
    sig.set("Filter", Object::Name(b"Adobe.PPKLite".to_vec()));
    sig.set("SubFilter", Object::Name(b"adbe.pkcs7.detached".to_vec()));
    sig.set(
        "Contents",
        Object::String(vec![0u8; SIG_LEN], StringFormat::Hexadecimal),
    );
    sig.set(
        "ByteRange",
        Object::Array(vec![
            0i64.into(),
            1_000_000_000_000i64.into(),
            1_000_000_000_000i64.into(),
            1_000_000_000_000i64.into(),
        ]),
    );
    let fecha = chrono::Utc::now().format("D:%Y%m%d%H%M%SZ").to_string();
    sig.set("M", Object::string_literal(fecha));
    if let Some(r) = reason {
        sig.set("Reason", Object::string_literal(r));
    }
    let sig_id = doc.add_object(sig);

    // widget de firma invisible en la primera página
    let mut widget = Dictionary::new();
    widget.set("Type", Object::Name(b"Annot".to_vec()));
    widget.set("Subtype", Object::Name(b"Widget".to_vec()));
    widget.set("FT", Object::Name(b"Sig".to_vec()));
    widget.set("T", Object::string_literal("Firma1"));
    widget.set(
        "Rect",
        Object::Array(vec![0.into(), 0.into(), 0.into(), 0.into()]),
    );
    widget.set("F", 132i64);
    widget.set("V", Object::Reference(sig_id));
    widget.set("P", Object::Reference(page_id));
    let widget_id = doc.add_object(widget);

    // añadir el widget a los Annots de la página (array directo o referencia)
    let annots_target = {
        let page = doc
            .get_object(page_id)
            .and_then(|o| o.as_dict())
            .map_err(|e| e.to_string())?;
        match page.get(b"Annots") {
            Ok(Object::Reference(rid)) => Some(*rid),
            _ => None,
        }
    };
    if let Some(rid) = annots_target {
        let arr = doc
            .get_object_mut(rid)
            .and_then(|o| o.as_array_mut())
            .map_err(|e| e.to_string())?;
        arr.push(Object::Reference(widget_id));
    } else {
        let page = doc
            .get_object_mut(page_id)
            .and_then(|o| o.as_dict_mut())
            .map_err(|e| e.to_string())?;
        match page.get_mut(b"Annots") {
            Ok(Object::Array(arr)) => arr.push(Object::Reference(widget_id)),
            _ => page.set("Annots", Object::Array(vec![Object::Reference(widget_id)])),
        }
    }

    // AcroForm del catálogo: crear o fusionar, con SigFlags 3
    let catalog_id = doc
        .trailer
        .get(b"Root")
        .and_then(|o| o.as_reference())
        .map_err(|e| e.to_string())?;
    let existing_form: Option<Dictionary> = {
        let catalog = doc
            .get_object(catalog_id)
            .and_then(|o| o.as_dict())
            .map_err(|e| e.to_string())?;
        match catalog.get(b"AcroForm") {
            Ok(Object::Dictionary(d)) => Some(d.clone()),
            Ok(Object::Reference(rid)) => doc
                .get_object(*rid)
                .ok()
                .and_then(|o| o.as_dict().ok())
                .cloned(),
            _ => None,
        }
    };
    let mut form = existing_form.unwrap_or_default();
    match form.get_mut(b"Fields") {
        Ok(Object::Array(arr)) => arr.push(Object::Reference(widget_id)),
        _ => form.set("Fields", Object::Array(vec![Object::Reference(widget_id)])),
    }
    form.set("SigFlags", 3i64);
    let catalog = doc
        .get_object_mut(catalog_id)
        .and_then(|o| o.as_dict_mut())
        .map_err(|e| e.to_string())?;
    catalog.set("AcroForm", Object::Dictionary(form));

    // serializar y localizar el hueco de /Contents
    let mut out = Vec::new();
    doc.save_to(&mut out)
        .map_err(|e| format!("No se pudo serializar: {e}"))?;
    let marker: Vec<u8> = {
        let mut v = vec![b'<'];
        v.extend(std::iter::repeat(b'0').take(SIG_LEN * 2));
        v.push(b'>');
        v
    };
    let contents_start =
        find_subslice(&out, &marker).ok_or("No se encontró el hueco de la firma")?;
    let contents_end = contents_start + marker.len();

    // parchear ByteRange manteniendo la longitud del hueco
    let br_pos = find_subslice(&out, b"/ByteRange").ok_or("No se encontró /ByteRange")?;
    let open = br_pos
        + out[br_pos..]
            .iter()
            .position(|&c| c == b'[')
            .ok_or("ByteRange sin '['")?;
    let close = open
        + out[open..]
            .iter()
            .position(|&c| c == b']')
            .ok_or("ByteRange sin ']'")?;
    let hueco = close - open - 1;
    let a = contents_start as i64;
    let b = contents_end as i64;
    let total = out.len() as i64;
    let nuevo = format!("0 {a} {} {}", b, total - b);
    if nuevo.len() > hueco {
        return Err("El ByteRange no cabe en el hueco reservado".into());
    }
    let relleno = format!("{nuevo:<hueco$}");
    out[open + 1..close].copy_from_slice(relleno.as_bytes());

    // digest sobre todo menos el hueco de Contents, y firma CMS
    let mut hasher = Sha256::new();
    hasher.update(&out[..contents_start]);
    hasher.update(&out[contents_end..]);
    let digest = hasher.finalize();
    let der = build_cms(cred, &digest)?;
    if der.len() > SIG_LEN {
        return Err("La firma no cabe en el hueco reservado".into());
    }
    let hex: String = der.iter().map(|byte| format!("{byte:02X}")).collect();
    out[contents_start + 1..contents_start + 1 + hex.len()].copy_from_slice(hex.as_bytes());

    std::fs::write(dest_path, &out).map_err(|e| format!("No se pudo escribir: {e}"))
}
