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
    .map_err(|e| {
        let detalle = e.to_string();
        // el fallo de MAC es siempre la contraseña; el detalle técnico sobra
        if detalle.contains("MAC") {
            "Contraseña del .p12 incorrecta".to_string()
        } else {
            format!("No se ha podido abrir el .p12 (¿contraseña incorrecta?): {detalle}")
        }
    })?;
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

/// ¿Lleva el fichero una firma digital? Se mira el `/ByteRange`, que es lo
/// que distingue a un PDF firmado: cualquier reescritura del cuerpo mueve
/// los desplazamientos que ese array fija y deja la firma por inválida.
/// Es el mismo criterio que usa `save_pdf` para copiar byte a byte.
pub fn esta_firmado(path: &str) -> bool {
    std::fs::read(path)
        .map(|b| find_subslice(&b, b"/ByteRange").is_some())
        .unwrap_or(false)
}

/// Frase para el usuario cuando una operación destruiría la firma. Ni
/// «ByteRange» ni «PKCS#7»: qué pasa y qué hacer.
pub const AVISO_FIRMADO: &str =
    "El documento está firmado y protegerlo con contraseña invalidaría la firma. \
     Guarda antes una copia sin firmar y protege esa.";

/// Cómo se ve la firma en la página. Sin `rect` el campo es invisible
/// (`/Rect [0 0 0 0]`), que es lo que hacía Vitela hasta el ciclo 3 y lo
/// que sigue haciendo si la UI no dibuja el rectángulo.
#[derive(Default)]
pub struct Apariencia {
    /// Rectángulo del widget en el espacio PROPIO de la página (la UI lo
    /// convierte con la `rotation` de `get_page_sizes`, como en todos los
    /// comandos que escriben).
    pub rect: Option<crate::Rect>,
    /// Página donde va la firma (0 por defecto).
    pub page_index: Option<u16>,
    /// Nombre que se pinta («Firmado por …»); sin él, el sujeto del
    /// certificado.
    pub signer_name: Option<String>,
    /// Firma manuscrita en PNG base64, dibujada encima del texto.
    pub signature_png: Option<String>,
}

/// Incrusta un PNG como XObject de imagen con su canal alfa en `/SMask`
/// (una firma manuscrita casi siempre viene con el fondo transparente).
/// Devuelve el objeto y su tamaño en píxeles.
fn imagen_png(doc: &mut LoDoc, png_base64: &str) -> Result<(lopdf::ObjectId, u32, u32), String> {
    use base64::Engine;
    use lopdf::Stream;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(png_base64.trim())
        .map_err(|e| format!("La imagen de la firma no es base64 válido: {e}"))?;
    let img = image::load_from_memory(&bytes)
        .map_err(|e| format!("No se ha podido leer la imagen de la firma: {e}"))?
        .to_rgba8();
    let (w, h) = img.dimensions();
    let mut rgb = Vec::with_capacity((w * h * 3) as usize);
    let mut alfa = Vec::with_capacity((w * h) as usize);
    for p in img.pixels() {
        rgb.extend_from_slice(&p.0[..3]);
        alfa.push(p.0[3]);
    }
    let mut cabecera = |espacio: &str, datos: Vec<u8>| {
        let mut d = Dictionary::new();
        d.set("Type", Object::Name(b"XObject".to_vec()));
        d.set("Subtype", Object::Name(b"Image".to_vec()));
        d.set("Width", w as i64);
        d.set("Height", h as i64);
        d.set("ColorSpace", Object::Name(espacio.as_bytes().to_vec()));
        d.set("BitsPerComponent", 8i64);
        let mut stream = Stream::new(d, datos);
        let _ = stream.compress();
        doc.add_object(stream)
    };
    let smask = cabecera("DeviceGray", alfa);
    let img_id = cabecera("DeviceRGB", rgb);
    doc.get_object_mut(img_id)
        .and_then(|o| o.as_stream_mut())
        .map_err(|e| e.to_string())?
        .dict
        .set("SMask", Object::Reference(smask));
    Ok((img_id, w, h))
}

/// Apariencia del widget de firma: un Form XObject con la firma manuscrita
/// arriba (si llega) y, debajo, «Firmado por …» y la fecha en Helvetica.
fn apariencia_firma(
    doc: &mut LoDoc,
    w: f32,
    h: f32,
    nombre: &str,
    fecha: &str,
    png: Option<&str>,
) -> Result<lopdf::ObjectId, String> {
    use lopdf::Stream;
    let mut recursos = Dictionary::new();
    let mut ops: Vec<u8> = Vec::new();
    // marco discreto, como el sello de firma de Acrobat
    ops.extend_from_slice(
        format!("q 0.35 0.35 0.35 RG 0.7 w 0.35 0.35 {:.2} {:.2} re S Q\n", w - 0.7, h - 0.7)
            .as_bytes(),
    );
    let mut alto_texto = (h * 0.42).min(26.0);
    if png.is_none() {
        alto_texto = h;
    }
    if let Some(png) = png {
        let (img_id, iw, ih) = imagen_png(doc, png)?;
        let caja_h = h - alto_texto;
        // la firma manuscrita cabe entera y centrada, sin deformarse
        let escala = ((w - 8.0) / iw as f32).min((caja_h - 6.0) / ih as f32).max(0.0);
        let (dw, dh) = (iw as f32 * escala, ih as f32 * escala);
        ops.extend_from_slice(
            format!(
                "q {dw:.2} 0 0 {dh:.2} {:.2} {:.2} cm /Firma Do Q\n",
                (w - dw) / 2.0,
                alto_texto + (caja_h - dh) / 2.0
            )
            .as_bytes(),
        );
        let mut xobjects = Dictionary::new();
        xobjects.set("Firma", Object::Reference(img_id));
        recursos.set("XObject", Object::Dictionary(xobjects));
    }
    let size = (alto_texto / 2.6).clamp(5.0, 10.0);
    let helv = crate::seguridad::fuente_helvetica(doc);
    let mut fuentes = Dictionary::new();
    fuentes.set("Helv", Object::Reference(helv));
    recursos.set("Font", Object::Dictionary(fuentes));
    let base = if png.is_some() { alto_texto - size * 1.4 } else { h / 2.0 - size * 0.2 };
    ops.extend_from_slice(
        format!(
            "BT /Helv {size:.2} Tf {:.2} TL 0 0 0 rg 4 {:.2} Td\n",
            size * 1.25,
            base.max(2.0)
        )
        .as_bytes(),
    );
    for (n, linea) in [format!("Firmado por {nombre}"), fecha.to_string()]
        .iter()
        .enumerate()
    {
        if n > 0 {
            ops.extend_from_slice(b"T* ");
        }
        ops.push(b'(');
        ops.extend_from_slice(&crate::anotaciones2::winansi(linea));
        ops.extend_from_slice(b") Tj\n");
    }
    ops.extend_from_slice(b"ET\n");

    let mut forma = Dictionary::new();
    forma.set("Type", Object::Name(b"XObject".to_vec()));
    forma.set("Subtype", Object::Name(b"Form".to_vec()));
    forma.set("FormType", 1i64);
    forma.set(
        "BBox",
        Object::Array(vec![0.into(), 0.into(), w.into(), h.into()]),
    );
    forma.set("Resources", Object::Dictionary(recursos));
    Ok(doc.add_object(Stream::new(forma, ops)))
}

/// Firma el PDF de `src_path` y escribe el resultado en `dest_path`.
pub fn sign(
    src_path: &str,
    dest_path: &str,
    cred: &Credenciales,
    reason: Option<String>,
    apariencia: &Apariencia,
) -> Result<(), String> {
    let mut doc = LoDoc::load(src_path).map_err(|e| format!("No se ha podido leer el PDF: {e}"))?;
    let pagina = apariencia.page_index.unwrap_or(0) as u32 + 1;
    let page_id = *doc
        .get_pages()
        .get(&pagina)
        .ok_or("La página donde va la firma no existe")?;

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
    // quién firma, como lo escribe Acrobat: el nombre que dio la UI o, si no
    // dio ninguno, el del certificado
    let nombre = apariencia
        .signer_name
        .clone()
        .unwrap_or_else(|| nombre_comun(&cred.cert.tbs_certificate.subject.to_string()));
    sig.set("Name", crate::documento::cadena_pdf(&nombre));
    if let Some(r) = reason {
        sig.set("Reason", Object::string_literal(r));
    }
    let sig_id = doc.add_object(sig);

    // widget de firma: invisible sin `rect`, y con su propia apariencia si
    // la UI dibujó el rectángulo (que es como firma Acrobat)
    let mut widget = Dictionary::new();
    widget.set("Type", Object::Name(b"Annot".to_vec()));
    widget.set("Subtype", Object::Name(b"Widget".to_vec()));
    widget.set("FT", Object::Name(b"Sig".to_vec()));
    widget.set("T", Object::string_literal("Firma1"));
    let caja = match &apariencia.rect {
        Some(r) => {
            let geo = crate::formularios2::geo_pagina(&doc, page_id)?;
            let c = geo.ui_rect_a_pdf(r);
            [c.left().value, c.bottom().value, c.right().value, c.top().value]
        }
        None => [0.0; 4],
    };
    widget.set(
        "Rect",
        Object::Array(vec![
            caja[0].into(),
            caja[1].into(),
            caja[2].into(),
            caja[3].into(),
        ]),
    );
    widget.set("F", 132i64); // Print + Locked
    widget.set("V", Object::Reference(sig_id));
    widget.set("P", Object::Reference(page_id));
    if apariencia.rect.is_some() {
        let ap_id = apariencia_firma(
            &mut doc,
            caja[2] - caja[0],
            caja[3] - caja[1],
            &nombre,
            &chrono::Local::now().format("%d/%m/%Y %H:%M").to_string(),
            apariencia.signature_png.as_deref(),
        )?;
        let mut ap = Dictionary::new();
        ap.set("N", Object::Reference(ap_id));
        widget.set("AP", Object::Dictionary(ap));
        widget.set("DA", Object::string_literal("/Helv 0 Tf 0 g"));
    }
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
        .map_err(|e| format!("No se ha podido serializar: {e}"))?;
    let marker: Vec<u8> = {
        let mut v = vec![b'<'];
        v.extend(std::iter::repeat_n(b'0', SIG_LEN * 2));
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

    std::fs::write(dest_path, &out).map_err(|e| format!("No se ha podido escribir: {e}"))
}

/// Lo que se sabe de una firma tras comprobarla, en el lenguaje de la UI:
/// nada de «CMS» ni «ByteRange» sale de aquí.
#[derive(serde::Serialize, Debug)]
pub struct FirmaInfo {
    /// Quien firma, tal como se enseña: el `/Name` del diccionario de firma
    /// o, si no lo lleva, el CN del certificado.
    pub name: String,
    pub reason: String,
    /// Fecha de la firma en ISO 8601 (vacía si el documento no la trae).
    pub signed_at: String,
    /// ¿La firma cubre el fichero entero salvo su propio hueco? Si no, hay
    /// contenido añadido después que la firma no avala.
    pub covers_whole_file: bool,
    /// ¿El documento sigue igual que cuando se firmó? Es el resumen honesto
    /// de dos comprobaciones: el hash de lo firmado coincide con el que
    /// lleva dentro la firma, y la firma RSA de esos datos la hizo la clave
    /// del certificado embebido.
    pub digest_ok: bool,
    pub cert_subject: String,
    pub cert_issuer: String,
    pub not_before: String,
    pub not_after: String,
    pub expired: bool,
    /// Autofirmado: nadie más responde por ese certificado. Vitela no
    /// consulta el llavero del sistema, así que esto y `expired` es todo lo
    /// que se puede decir del certificado sin mentir.
    pub self_signed: bool,
    pub page_index: Option<u16>,
    /// Rectángulo del widget en el espacio de la página VISTA, como el
    /// resto de comandos que leen anotaciones. `None` si la firma es
    /// invisible.
    pub rect: Option<crate::Rect>,
}

/// Comprueba las firmas del documento: por cada campo `/Sig`, si su
/// `/ByteRange` cubre el fichero entero salvo el hueco de `/Contents`, si el
/// SHA-256 de esos rangos es el que va firmado dentro, si la firma RSA la
/// hizo la clave del certificado que viaja en ella, y qué dice ese
/// certificado (sujeto, emisor y validez).
///
/// **Sin cadena de confianza**: no se consulta el llavero del sistema. Por
/// eso no hay ningún «válida» aquí: hay `digest_ok` (el documento no ha
/// cambiado), `expired` y `self_signed`, y la UI dice exactamente eso.
#[tauri::command(async)]
pub fn verify_signatures(path: String) -> Result<Vec<FirmaInfo>, String> {
    crate::on_pdfium_thread(move || {
        let bytes = std::fs::read(&path).map_err(|e| {
            crate::mensaje_llano(format!("No se ha podido leer el documento: {e}"))
        })?;
        if find_subslice(&bytes, b"/ByteRange").is_none() {
            return Ok(Vec::new());
        }
        let doc = LoDoc::load_mem(&bytes)
            .map_err(|e| crate::mensaje_llano(format!("No se ha podido leer el PDF: {e}")))?;
        Ok(firmas_de(&doc, &bytes))
    })
}

/// Los campos de firma del documento, con la página y el rect de su widget.
fn firmas_de(doc: &LoDoc, bytes: &[u8]) -> Vec<FirmaInfo> {
    let mut vistos: Vec<lopdf::ObjectId> = Vec::new();
    let mut out = Vec::new();
    for (n, page_id) in doc.get_pages().values().enumerate() {
        let Some(annots) = crate::anotaciones::lista_annots(doc, n as u16) else {
            continue;
        };
        for a in annots {
            let Some(annot) = dict_de(doc, &a) else { continue };
            if annot.get(b"FT").and_then(|o| o.as_name()).unwrap_or_default() != b"Sig" {
                continue;
            }
            let Ok(Object::Reference(sig_id)) = annot.get(b"V") else {
                continue;
            };
            if vistos.contains(sig_id) {
                continue;
            }
            vistos.push(*sig_id);
            let Some(sig) = dict_de(doc, &Object::Reference(*sig_id)) else {
                continue;
            };
            let rect = rect_del_widget(doc, &annot, *page_id);
            out.push(lee_firma(&sig, bytes, Some(n as u16), rect));
        }
    }
    // firmas cuyo campo no cuelga de ninguna página (raro, pero legal)
    for id in campos_de_firma(doc) {
        if vistos.contains(&id) {
            continue;
        }
        if let Some(sig) = dict_de(doc, &Object::Reference(id)) {
            out.push(lee_firma(&sig, bytes, None, None));
        }
    }
    out
}

fn dict_de(doc: &LoDoc, o: &Object) -> Option<Dictionary> {
    match o {
        Object::Dictionary(d) => Some(d.clone()),
        Object::Reference(id) => doc.get_object(*id).ok()?.as_dict().ok().cloned(),
        _ => None,
    }
}

/// El `/Rect` del widget en el espacio de la página vista, que es lo que la
/// UI pinta (igual que `get_annotations`).
fn rect_del_widget(doc: &LoDoc, annot: &Dictionary, page_id: lopdf::ObjectId) -> Option<crate::Rect> {
    let caja: Vec<f32> = annot
        .get(b"Rect")
        .and_then(|o| o.as_array())
        .ok()?
        .iter()
        .filter_map(|o| o.as_float().ok().or_else(|| o.as_i64().ok().map(|n| n as f32)))
        .collect();
    if caja.len() != 4 || (caja[2] - caja[0]).abs() < 1.0 || (caja[3] - caja[1]).abs() < 1.0 {
        return None;
    }
    let geo = crate::formularios2::geo_pagina(doc, page_id).ok()?;
    Some(geo.pdf_rect_a_ui(&pdfium_render::prelude::PdfRect::new(
        pdfium_render::prelude::PdfPoints::new(caja[1].min(caja[3])),
        pdfium_render::prelude::PdfPoints::new(caja[0].min(caja[2])),
        pdfium_render::prelude::PdfPoints::new(caja[1].max(caja[3])),
        pdfium_render::prelude::PdfPoints::new(caja[0].max(caja[2])),
    )))
}

/// Los `/V` de los campos `/FT /Sig` del AcroForm.
fn campos_de_firma(doc: &LoDoc) -> Vec<lopdf::ObjectId> {
    let mut out = Vec::new();
    let Some(campos) = doc
        .trailer
        .get(b"Root")
        .and_then(|o| o.as_reference())
        .ok()
        .and_then(|root| dict_de(doc, &Object::Reference(root)))
        .and_then(|cat| dict_de(doc, cat.get(b"AcroForm").ok()?))
        .and_then(|form| form.get(b"Fields").ok().cloned())
    else {
        return out;
    };
    let lista = match &campos {
        Object::Array(a) => a.clone(),
        Object::Reference(id) => doc
            .get_object(*id)
            .and_then(|o| o.as_array())
            .cloned()
            .unwrap_or_default(),
        _ => return out,
    };
    for c in lista {
        let Some(campo) = dict_de(doc, &c) else { continue };
        if campo.get(b"FT").and_then(|o| o.as_name()).unwrap_or_default() != b"Sig" {
            continue;
        }
        if let Ok(Object::Reference(id)) = campo.get(b"V") {
            out.push(*id);
        }
    }
    out
}

/// Comprueba una firma concreta.
fn lee_firma(
    sig: &Dictionary,
    bytes: &[u8],
    page_index: Option<u16>,
    rect: Option<crate::Rect>,
) -> FirmaInfo {
    let texto = |clave: &[u8]| {
        sig.get(clave)
            .map(crate::anotaciones::texto_de_cadena_pdf)
            .unwrap_or_default()
    };
    let mut info = FirmaInfo {
        name: texto(b"Name"),
        reason: texto(b"Reason"),
        signed_at: crate::anotaciones::fecha_pdf_a_iso(&texto(b"M")),
        covers_whole_file: false,
        digest_ok: false,
        cert_subject: String::new(),
        cert_issuer: String::new(),
        not_before: String::new(),
        not_after: String::new(),
        expired: false,
        self_signed: false,
        page_index,
        rect,
    };
    let rangos: Vec<usize> = sig
        .get(b"ByteRange")
        .and_then(|o| o.as_array())
        .map(|a| a.iter().filter_map(|o| o.as_i64().ok()).map(|n| n as usize).collect())
        .unwrap_or_default();
    let Object::String(contents, _) = sig.get(b"Contents").unwrap_or(&Object::Null) else {
        return info;
    };
    if rangos.len() != 4 {
        return info;
    }
    let (a, b, c, d) = (rangos[0], rangos[1], rangos[2], rangos[3]);
    if a + b > bytes.len() || c + d > bytes.len() || c < a + b {
        return info;
    }
    // el hueco entre los dos rangos tiene que ser justo el /Contents
    let hueco = &bytes[a + b..c];
    info.covers_whole_file = a == 0
        && c + d == bytes.len()
        && hueco.first() == Some(&b'<')
        && hueco.last() == Some(&b'>');

    let mut hasher = Sha256::new();
    hasher.update(&bytes[a..a + b]);
    hasher.update(&bytes[c..c + d]);
    let digest = hasher.finalize();
    if let Some(cms) = lee_cms(contents) {
        info.digest_ok = cms.digest == digest.as_slice() && cms.firma_ok;
        info.cert_subject = cms.subject;
        info.cert_issuer = cms.issuer;
        info.not_before = cms.not_before;
        info.not_after = cms.not_after;
        info.expired = cms.expired;
        info.self_signed = cms.self_signed;
        if info.name.is_empty() {
            info.name = nombre_comun(&info.cert_subject);
        }
    }
    info
}

struct DatosCms {
    digest: Vec<u8>,
    firma_ok: bool,
    subject: String,
    issuer: String,
    not_before: String,
    not_after: String,
    expired: bool,
    self_signed: bool,
}

/// Saca del PKCS#7 el hash firmado, el certificado y si la firma RSA de los
/// atributos firmados la hizo la clave de ese certificado.
fn lee_cms(der_con_relleno: &[u8]) -> Option<DatosCms> {
    use der::{Encode, Reader};
    let ci: cms::content_info::ContentInfo = {
        // el hueco de /Contents lleva ceros de relleno tras el DER, así que
        // no se puede exigir que se consuma entero
        let mut reader = der::SliceReader::new(der_con_relleno).ok()?;
        reader.decode().ok()?
    };
    if ci.content_type != const_oid::db::rfc5911::ID_SIGNED_DATA {
        return None;
    }
    let sd: cms::signed_data::SignedData = ci.content.decode_as().ok()?;
    let signer = sd.signer_infos.0.iter().next()?;
    let attrs = signer.signed_attrs.as_ref()?;
    let md = attrs
        .iter()
        .find(|a| a.oid == const_oid::db::rfc5911::ID_MESSAGE_DIGEST)?;
    let md_der = md.values.iter().next()?.to_der().ok()?;
    // OCTET STRING: 0x04, longitud, bytes
    let digest = md_der.get(2..)?.to_vec();

    let cert = sd.certificates.as_ref().and_then(|c| {
        c.0.iter().find_map(|c| match c {
            CertificateChoices::Certificate(cert) => Some(cert.clone()),
            _ => None,
        })
    })?;
    let spki = cert.tbs_certificate.subject_public_key_info.to_der().ok()?;
    let firma_ok = (|| {
        use rsa::pkcs8::DecodePublicKey;
        use rsa::signature::Verifier;
        let clave = rsa::RsaPublicKey::from_public_key_der(&spki).ok()?;
        let vk = rsa::pkcs1v15::VerifyingKey::<Sha256>::new(clave);
        let firma = rsa::pkcs1v15::Signature::try_from(signer.signature.as_bytes()).ok()?;
        Some(vk.verify(&attrs.to_der().ok()?, &firma).is_ok())
    })()
    .unwrap_or(false);

    let subject = cert.tbs_certificate.subject.to_string();
    let issuer = cert.tbs_certificate.issuer.to_string();
    let iso = |t: &x509_cert::time::Time| {
        chrono::DateTime::<chrono::Utc>::from(t.to_system_time()).to_rfc3339()
    };
    let not_after_st = cert.tbs_certificate.validity.not_after.to_system_time();
    Some(DatosCms {
        digest,
        firma_ok,
        self_signed: subject == issuer,
        subject,
        issuer,
        not_before: iso(&cert.tbs_certificate.validity.not_before),
        not_after: iso(&cert.tbs_certificate.validity.not_after),
        expired: std::time::SystemTime::now() > not_after_st,
    })
}

/// El CN de un sujeto RFC 4514 («CN=Jorge,O=Vitela»); si no lo lleva, el
/// sujeto entero, que siempre es mejor que una casilla vacía.
fn nombre_comun(sujeto: &str) -> String {
    sujeto
        .split(',')
        .map(str::trim)
        .find_map(|p| p.strip_prefix("CN="))
        .unwrap_or(sujeto)
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::crea_pdf;
    use base64::Engine;

    fn credenciales() -> Credenciales {
        credenciales_pem(
            include_str!("../fixtures/test_cert.pem"),
            include_str!("../fixtures/test_key.pem"),
        )
        .expect("credenciales de prueba")
    }

    /// PNG opaco de un color, en base64 (la «firma manuscrita»).
    fn png_b64(w: u32, h: u32) -> String {
        let img = image::RgbaImage::from_pixel(w, h, image::Rgba([20, 40, 160, 255]));
        let mut buf = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(img)
            .write_to(&mut buf, image::ImageFormat::Png)
            .expect("codificar png");
        base64::engine::general_purpose::STANDARD.encode(buf.into_inner())
    }

    /// ¿Hay tinta (algo que no sea papel blanco) dentro de ese rect de la
    /// página vista? Es el único juez de que la firma «se ve».
    fn hay_tinta(path: &str, pagina: u16, r: &crate::Rect) -> bool {
        let sizes = crate::get_page_sizes(path.to_string()).expect("tamaños");
        let escala = 600.0 / sizes[pagina as usize].width;
        let png = crate::render_page_png(path.to_string(), pagina, 600, true).expect("render");
        let img = image::load_from_memory(&png).expect("PNG").to_rgba8();
        let (x0, y0) = ((r.x * escala) as u32 + 2, (r.y * escala) as u32 + 2);
        let (x1, y1) = (
            ((r.x + r.w) * escala) as u32 - 2,
            ((r.y + r.h) * escala) as u32 - 2,
        );
        for x in x0..x1.min(img.width()) {
            for y in y0..y1.min(img.height()) {
                let p = img.get_pixel(x, y).0;
                if p[0] < 240 || p[1] < 240 || p[2] < 240 {
                    return true;
                }
            }
        }
        false
    }

    /// Acrobat firma dibujando primero el rectángulo en la página: la firma
    /// se ve, y al reabrir el documento dice si vale. Sin `rect` la firma
    /// sigue siendo invisible, que es lo que hacía Vitela hasta ahora.
    #[test]
    fn firma_visible_y_verificada() {
        let dir = std::env::temp_dir();
        let src = dir.join("firma-visible-src.pdf");
        let dest = dir.join("firma-visible-out.pdf");
        crea_pdf(&["Uno", "Dos"], &src);
        let rect = crate::Rect { x: 60.0, y: 500.0, w: 220.0, h: 90.0 };
        let ap = Apariencia {
            rect: Some(rect.clone()),
            page_index: Some(1),
            signer_name: Some("Jorge Gómez".into()),
            signature_png: Some(png_b64(120, 40)),
        };
        sign(
            &src.to_string_lossy(),
            &dest.to_string_lossy(),
            &credenciales(),
            Some("Conforme".into()),
            &ap,
        )
        .expect("firmar con apariencia");

        let ruta = dest.to_string_lossy().into_owned();
        assert!(
            hay_tinta(&ruta, 1, &rect),
            "la firma tiene que verse dentro de su rectángulo, en la página 2"
        );

        let firmas = verify_signatures(ruta.clone()).expect("verificar");
        assert_eq!(firmas.len(), 1, "una firma");
        let f = &firmas[0];
        assert!(f.digest_ok, "el documento no ha cambiado desde la firma");
        assert!(f.covers_whole_file, "el ByteRange cubre el fichero entero");
        assert_eq!(f.page_index, Some(1), "la firma está en la página 2");
        let r = f.rect.as_ref().expect("la firma visible trae su rect");
        assert!(
            (r.x - rect.x).abs() < 1.0
                && (r.y - rect.y).abs() < 1.0
                && (r.w - rect.w).abs() < 1.0
                && (r.h - rect.h).abs() < 1.0,
            "el rect leído es ({:.1},{:.1}) {:.1}x{:.1}",
            r.x,
            r.y,
            r.w,
            r.h
        );
        assert_eq!(f.name, "Jorge Gómez");
        assert_eq!(f.reason, "Conforme");
        assert!(!f.signed_at.is_empty(), "la fecha de firma");
        assert!(!f.cert_subject.is_empty() && !f.cert_issuer.is_empty());
        assert!(f.self_signed, "el certificado de prueba es autofirmado");
        assert!(!f.expired, "el certificado de prueba está en vigor");

        // tocar un byte del cuerpo: la firma deja de valer
        let tocado = dir.join("firma-visible-tocado.pdf");
        let mut bytes = std::fs::read(&dest).expect("leer firmado");
        // dentro del primer stream de contenido: cambia el documento sin
        // romper el árbol de objetos
        let i = find_subslice(&bytes, b"stream").expect("un stream") + 20;
        bytes[i] ^= 0xFF;
        std::fs::write(&tocado, &bytes).expect("escribir tocado");
        let f = &verify_signatures(tocado.to_string_lossy().into_owned()).expect("verificar")[0];
        assert!(
            !f.digest_ok,
            "cambiar un byte del cuerpo tiene que romper la comprobación"
        );

        // sin rect, la firma sigue siendo invisible (lo de siempre)
        let invisible = dir.join("firma-invisible-out.pdf");
        sign(
            &src.to_string_lossy(),
            &invisible.to_string_lossy(),
            &credenciales(),
            None,
            &Apariencia::default(),
        )
        .expect("firmar sin apariencia");
        let f = &verify_signatures(invisible.to_string_lossy().into_owned()).expect("verificar")[0];
        assert!(f.digest_ok && f.covers_whole_file);
        assert!(f.rect.is_none(), "sin rect no hay firma que ver");

        for p in [&src, &dest, &tocado, &invisible] {
            std::fs::remove_file(p).ok();
        }
    }
}
