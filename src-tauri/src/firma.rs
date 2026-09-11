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
// el hueco reservado para el PKCS#7. Con sello de tiempo el CMS crece lo
// que ocupe el token de la autoridad (unos 4 KB con su cadena), así que
// desde el ciclo 9 hay sitio de sobra: el hueco es relleno, no pesa nada
// que importe.
const SIG_LEN: usize = 16384;

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

/// Certificado y clave privada listos para firmar.
pub struct Credenciales {
    cert: x509_cert::Certificate,
    key: rsa::RsaPrivateKey,
    /// La cadena que venga con el certificado (los intermedios, sin el
    /// del firmante). Un `.p12` la trae y hasta el ciclo 9 se tiraba: sin
    /// ella, quien verifica la firma fuera de aquí no puede subir hasta la
    /// raíz y la marca como «emisor desconocido».
    cadena: Vec<x509_cert::Certificate>,
}

/// Credenciales desde certificado + clave en PEM (RSA sin cifrar).
pub fn credenciales_pem(cert_pem: &str, key_pem: &str) -> Result<Credenciales, String> {
    let cert = x509_cert::Certificate::from_pem(cert_pem).map_err(|_| {
        "Ese fichero no es un certificado que Vitela sepa leer: tiene que ser un .pem \
         o un .crt con el certificado dentro"
            .to_string()
    })?;
    let key = rsa::RsaPrivateKey::from_pkcs8_pem(key_pem)
        .or_else(|_| {
            use rsa::pkcs1::DecodeRsaPrivateKey;
            rsa::RsaPrivateKey::from_pkcs1_pem(key_pem)
        })
        .map_err(|_| {
            "Ese fichero no es una clave privada que Vitela sepa leer: tiene que ser una \
             clave RSA en PEM y sin contraseña"
                .to_string()
        })?;
    Ok(Credenciales {
        cert,
        key,
        cadena: Vec::new(),
    })
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
    // los intermedios del contenedor viajan con la firma: es lo que deja
    // que otro visor llegue hasta la raíz
    let cadena = chain
        .certs()
        .iter()
        .skip(1)
        .filter_map(|c| x509_cert::Certificate::from_der(c.as_der()).ok())
        .collect();
    Ok(Credenciales { cert, key, cadena })
}

/// **La clave privada del usuario**, en los dos formatos que ya acepta
/// firmar y con el mismo selector: un `.p12`/`.pfx` con su contraseña o un
/// PEM sin cifrar. Es lo que hace falta para abrir un PDF cifrado para tu
/// certificado, donde el certificado en sí no pinta nada: quien abre el
/// sobre es la clave.
pub(crate) fn clave_privada(
    key_path: &str,
    password: Option<&str>,
) -> Result<rsa::RsaPrivateKey, String> {
    let bytes = std::fs::read(key_path)
        .map_err(|e| crate::mensaje_llano(format!("No se ha podido leer {key_path}: {e}")))?;
    let minus = key_path.to_lowercase();
    if minus.ends_with(".p12") || minus.ends_with(".pfx") {
        return credenciales_p12(&bytes, password.unwrap_or("")).map(|c| c.key);
    }
    let pem = String::from_utf8_lossy(&bytes);
    rsa::RsaPrivateKey::from_pkcs8_pem(&pem)
        .or_else(|_| {
            use rsa::pkcs1::DecodeRsaPrivateKey;
            rsa::RsaPrivateKey::from_pkcs1_pem(&pem)
        })
        .map_err(|_| {
            "Ese fichero no lleva una clave privada que Vitela sepa leer: hace falta un \
             .p12 con su contraseña o un PEM sin cifrar"
                .to_string()
        })
}

/// Lo que sale de firmar, para que la interfaz pueda contarlo: si la/// Lo que sale de firmar, para que la interfaz pueda contarlo: si la
/// firma lleva sello de tiempo, cuál, y —si se pidió y no se pudo— por
/// qué se ha firmado sin él.
///
/// Firmar sin sello **no es un fallo**: la firma vale igual y su fecha es
/// la del reloj del que firmó, que es lo que hay que decir. Lo que no
/// puede pasar es que la firma se caiga entera después de haber elegido
/// dónde guardarla porque el servidor de sellado no contestaba.
#[derive(serde::Serialize, Debug, Default)]
pub struct InformeFirma {
    /// ¿Lleva sello de tiempo de una autoridad?
    pub sellada: bool,
    /// Quién y cuándo, para enseñarlo.
    pub sello: Option<crate::tsa::SelloDeTiempo>,
    /// Vacío si no hay nada que contar; si no, en llano, por qué la firma
    /// ha salido sin sello.
    pub aviso: String,
    /// ¿Se han archivado los certificados de la cadena en el `/DSS`?
    pub ltv: bool,
}

/// Construye el CMS SignedData detached sobre el digest dado.
///
/// Con `tsa_url` se le pide a la autoridad un sello de tiempo sobre la
/// firma ya hecha y se mete como **atributo no firmado** del SignerInfo,
/// que es donde lo pone el RFC 3161: no toca lo firmado, así que la firma
/// sigue valiendo si el sello no llega.
fn build_cms(
    cred: &Credenciales,
    digest: &[u8],
    tsa_url: Option<&str>,
) -> Result<(Vec<u8>, InformeFirma), String> {
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
    builder
        .add_digest_algorithm(digest_alg)
        .map_err(|e| e.to_string())?
        .add_certificate(CertificateChoices::Certificate(cert))
        .map_err(|e| e.to_string())?;
    // la cadena que venga con el certificado viaja dentro de la firma
    for intermedio in &cred.cadena {
        builder
            .add_certificate(CertificateChoices::Certificate(intermedio.clone()))
            .map_err(|e| e.to_string())?;
    }
    let signed = builder
        .add_signer_info::<rsa::pkcs1v15::SigningKey<Sha256>, rsa::pkcs1v15::Signature>(si_builder)
        .map_err(|e| e.to_string())?
        .build()
        .map_err(|e| e.to_string())?;
    let der = signed.to_der().map_err(|e| e.to_string())?;
    let Some(url) = tsa_url.map(str::trim).filter(|u| !u.is_empty()) else {
        return Ok((der, InformeFirma::default()));
    };
    match sella(&der, url) {
        Ok((con_sello, sello)) => Ok((
            con_sello,
            InformeFirma {
                sellada: true,
                sello: Some(sello),
                ..Default::default()
            },
        )),
        // el sello no ha podido ser: la firma sale sin él y se dice
        Err(aviso) => Ok((
            der,
            InformeFirma {
                sellada: false,
                sello: None,
                aviso,
                ltv: false,
            },
        )),
    }
}

/// Le pide el sello a la autoridad sobre la firma ya hecha y lo mete en el
/// CMS como atributo no firmado.
fn sella(der: &[u8], url: &str) -> Result<(Vec<u8>, crate::tsa::SelloDeTiempo), String> {
    use der::{Decode, Encode, Reader};
    let ci: cms::content_info::ContentInfo = {
        let mut reader = der::SliceReader::new(der).map_err(|e| e.to_string())?;
        reader.decode().map_err(|e| e.to_string())?
    };
    let mut sd: cms::signed_data::SignedData = ci.content.decode_as().map_err(|e| e.to_string())?;
    let firma = sd
        .signer_infos
        .0
        .as_slice()
        .first()
        .ok_or("La firma no tiene firmante")?
        .signature
        .as_bytes()
        .to_vec();
    let token = crate::tsa::pide_token(url, &firma)?;
    let sello = crate::tsa::lee_sello(&token)
        .ok_or("El sello de tiempo que ha devuelto el servidor no se ha podido leer")?;
    let valor = der::Any::from_der(&token).map_err(|e| e.to_string())?;
    let atributo = x509_cert::attr::Attribute {
        oid: crate::tsa::OID_TOKEN,
        values: der::asn1::SetOfVec::try_from(vec![valor]).map_err(|e| e.to_string())?,
    };
    let mut infos: Vec<cms::signed_data::SignerInfo> = sd.signer_infos.0.iter().cloned().collect();
    let unsigned = der::asn1::SetOfVec::try_from(vec![atributo]).map_err(|e| e.to_string())?;
    infos[0].unsigned_attrs = Some(unsigned);
    // el SignerInfo con atributos no firmados es de versión 1 igual: lo que
    // sube la versión son el sid y el tipo de contenido, que no cambian
    sd.signer_infos = cms::signed_data::SignerInfos(
        der::asn1::SetOfVec::try_from(infos).map_err(|e| e.to_string())?,
    );
    let contenido = der::Any::encode_from(&sd).map_err(|e| e.to_string())?;
    let nuevo = cms::content_info::ContentInfo {
        content_type: const_oid::db::rfc5911::ID_SIGNED_DATA,
        content: contenido,
    };
    Ok((nuevo.to_der().map_err(|e| e.to_string())?, sello))
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
///
/// Con `certifica` el sello dice **«Certificado por …»** (AC-082). El
/// diálogo enseñaba esa previa y en el PDF se escribía «Firmado por …»,
/// así que quien lo abría fuera de Vitela veía una firma normal donde
/// había una certificación, que es justo lo que distingue «firmado» de
/// «esta es la versión buena».
#[allow(clippy::too_many_arguments)]
fn apariencia_firma(
    doc: &mut LoDoc,
    w: f32,
    h: f32,
    nombre: &str,
    fecha: &str,
    motivo: Option<&str>,
    png: Option<&str>,
    certifica: bool,
) -> Result<lopdf::ObjectId, String> {
    use lopdf::Stream;
    let mut recursos = Dictionary::new();
    let mut ops: Vec<u8> = Vec::new();
    // marco discreto, como el sello de firma de Acrobat
    ops.extend_from_slice(
        format!(
            "q 0.35 0.35 0.35 RG 0.7 w 0.35 0.35 {:.2} {:.2} re S Q\n",
            w - 0.7,
            h - 0.7
        )
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
        let escala = ((w - 8.0) / iw as f32)
            .min((caja_h - 6.0) / ih as f32)
            .max(0.0);
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
    let n_lineas = 2.0
        + if motivo.map(str::trim).is_some_and(|m| !m.is_empty()) {
            1.0
        } else {
            0.0
        };
    // el texto arranca arriba de su banda: con el motivo son tres líneas y
    // la última tiene que seguir cayendo dentro de la caja
    let base = if png.is_some() {
        alto_texto - size * 1.4
    } else {
        h / 2.0 + size * (n_lineas - 2.0) * 0.6 - size * 0.2
    };
    ops.extend_from_slice(
        format!(
            "BT /Helv {size:.2} Tf {:.2} TL 0 0 0 rg 4 {:.2} Td\n",
            size * 1.25,
            base.max(2.0)
        )
        .as_bytes(),
    );
    // el sello de Acrobat pone el motivo debajo del nombre y la fecha
    let que = if certifica { "Certificado" } else { "Firmado" };
    let mut lineas = vec![format!("{que} por {nombre}"), fecha.to_string()];
    if let Some(motivo) = motivo.map(str::trim).filter(|m| !m.is_empty()) {
        lineas.push(format!("Motivo: {motivo}"));
    }
    for (n, linea) in lineas.iter().enumerate() {
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

/// Dónde se escribe el campo de firma: sobre el documento entero, que se
/// reescribe (la primera firma), o sobre una **revisión nueva** pegada al
/// final, dejando los bytes de antes exactamente donde estaban (la segunda
/// y siguientes). Es la diferencia entre poder firmar un contrato entre dos
/// personas y no poder.
enum Destino<'a> {
    Entero(&'a mut LoDoc),
    Incremental(&'a mut lopdf::IncrementalDocument),
}

impl Destino<'_> {
    /// El documento donde se escriben los objetos nuevos.
    fn escritura(&mut self) -> &mut LoDoc {
        match self {
            Destino::Entero(doc) => doc,
            Destino::Incremental(inc) => &mut inc.new_document,
        }
    }

    /// El documento donde se lee lo que ya había (páginas, catálogo,
    /// `/Annots`, `/AcroForm`).
    fn lectura(&self) -> &LoDoc {
        match self {
            Destino::Entero(doc) => doc,
            Destino::Incremental(inc) => inc.get_prev_documents(),
        }
    }

    fn add_object(&mut self, objeto: impl Into<Object>) -> lopdf::ObjectId {
        self.escritura().add_object(objeto)
    }

    /// Cambia un objeto que ya existía. En una actualización incremental
    /// hay que **traérselo antes** a la revisión nueva: es esa copia la que
    /// se escribe al final del fichero, sin tocar la de atrás.
    fn cambia(
        &mut self,
        id: lopdf::ObjectId,
        f: impl FnOnce(&mut Object) -> Result<(), String>,
    ) -> Result<(), String> {
        if let Destino::Incremental(inc) = self {
            inc.opt_clone_object_to_new_document(id)
                .map_err(|e| e.to_string())?;
        }
        let objeto = self
            .escritura()
            .get_object_mut(id)
            .map_err(|e| e.to_string())?;
        f(objeto)
    }
}

/// Escribe el campo de firma —diccionario `/Sig` con sus huecos, widget con
/// su apariencia, `/Annots` de la página y `/AcroForm` del catálogo— en el
/// destino que se le dé. Deja el `/Contents` en ceros y el `/ByteRange` con
/// números largos: los dos huecos los rellena [`cose_la_firma`] cuando el
/// fichero ya está serializado y se sabe dónde ha caído cada cosa.
#[allow(clippy::too_many_arguments)]
fn escribe_campo_de_firma(
    destino: &mut Destino,
    page_id: lopdf::ObjectId,
    cred: &Credenciales,
    reason: Option<String>,
    apariencia: &Apariencia,
    ordinal: usize,
    certifica: Option<u8>,
    ltv: bool,
    ocsp: Option<Vec<u8>>,
) -> Result<(), String> {
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
        .unwrap_or_else(|| nombre_llano(&cred.cert.tbs_certificate.subject.to_string()));
    sig.set("Name", crate::documento::cadena_pdf(&nombre));
    if let Some(r) = reason.as_deref() {
        // como el /Name: en UTF-16 si lleva acentos, que si no un motivo
        // con «también» sale «tambiÃ©n» en la tarjeta de cualquier visor
        sig.set("Reason", crate::documento::cadena_pdf(r));
    }
    // **Certificar** (`/DocMDP`): la firma no solo dice quién firmó, dice
    // **qué se puede cambiar después sin romperla**. Es lo que distingue
    // «firmado» de «esta es la versión buena». Los tres niveles son los del
    // spec y los del diálogo de Acrobat: 1 = ningún cambio, 2 = rellenar
    // formularios y firmar, 3 = además comentar.
    if let Some(nivel) = certifica {
        let mut params = Dictionary::new();
        params.set("Type", Object::Name(b"TransformParams".to_vec()));
        params.set("P", Object::Integer(nivel as i64));
        params.set("V", Object::Name(b"1.2".to_vec()));
        let mut referencia = Dictionary::new();
        referencia.set("Type", Object::Name(b"SigRef".to_vec()));
        referencia.set("TransformMethod", Object::Name(b"DocMDP".to_vec()));
        referencia.set("TransformParams", Object::Dictionary(params));
        sig.set(
            "Reference",
            Object::Array(vec![Object::Dictionary(referencia)]),
        );
    }
    let sig_id = destino.add_object(sig);
    // los certificados del `/DSS`, si se ha pedido archivarlos. Se crean
    // aquí porque dentro del `cambia` del catálogo ya no se puede añadir
    // objetos al documento
    let dss_id: Option<Vec<Object>> = ltv.then(|| {
        std::iter::once(&cred.cert)
            .chain(cred.cadena.iter())
            .filter_map(|c| c.to_der().ok())
            .map(|der| {
                let mut s = lopdf::Stream::new(Dictionary::new(), der);
                let _ = s.compress();
                Object::Reference(destino.add_object(Object::Stream(s)))
            })
            .collect()
    });
    // y la prueba de revocación, si el respondedor ha contestado
    let ocsp_id: Option<Object> = ocsp.map(|der| {
        let mut s = lopdf::Stream::new(Dictionary::new(), der);
        let _ = s.compress();
        Object::Reference(destino.add_object(Object::Stream(s)))
    });

    // widget de firma: invisible sin `rect`, y con su propia apariencia si
    // la UI dibujó el rectángulo (que es como firma Acrobat)
    let mut widget = Dictionary::new();
    widget.set("Type", Object::Name(b"Annot".to_vec()));
    widget.set("Subtype", Object::Name(b"Widget".to_vec()));
    widget.set("FT", Object::Name(b"Sig".to_vec()));
    // el nombre del campo tiene que ser único: dos «Firma1» en el mismo
    // /AcroForm son el mismo campo para cualquier visor
    widget.set("T", Object::string_literal(format!("Firma{ordinal}")));
    let caja = match &apariencia.rect {
        Some(r) => {
            let geo = crate::formularios2::geo_pagina(destino.lectura(), page_id)?;
            let c = geo.ui_rect_a_pdf(r);
            [
                c.left().value,
                c.bottom().value,
                c.right().value,
                c.top().value,
            ]
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
            destino.escritura(),
            caja[2] - caja[0],
            caja[3] - caja[1],
            &nombre,
            &chrono::Local::now().format("%d/%m/%Y %H:%M").to_string(),
            reason.as_deref(),
            apariencia.signature_png.as_deref(),
            certifica.is_some(),
        )?;
        let mut ap = Dictionary::new();
        ap.set("N", Object::Reference(ap_id));
        widget.set("AP", Object::Dictionary(ap));
        widget.set("DA", Object::string_literal("/Helv 0 Tf 0 g"));
    }
    let widget_id = destino.add_object(widget);

    // añadir el widget a los Annots de la página (array directo o referencia)
    let annots_target = {
        let page = destino
            .lectura()
            .get_object(page_id)
            .and_then(|o| o.as_dict())
            .map_err(|e| e.to_string())?;
        match page.get(b"Annots") {
            Ok(Object::Reference(rid)) => Some(*rid),
            _ => None,
        }
    };
    if let Some(rid) = annots_target {
        destino.cambia(rid, |o| {
            o.as_array_mut()
                .map_err(|e| e.to_string())?
                .push(Object::Reference(widget_id));
            Ok(())
        })?;
    } else {
        destino.cambia(page_id, |o| {
            let page = o.as_dict_mut().map_err(|e| e.to_string())?;
            match page.get_mut(b"Annots") {
                Ok(Object::Array(arr)) => arr.push(Object::Reference(widget_id)),
                _ => page.set("Annots", Object::Array(vec![Object::Reference(widget_id)])),
            }
            Ok(())
        })?;
    }

    // AcroForm del catálogo: crear o fusionar, con SigFlags 3
    let catalog_id = destino
        .lectura()
        .trailer
        .get(b"Root")
        .and_then(|o| o.as_reference())
        .map_err(|e| e.to_string())?;
    let existing_form: Option<Dictionary> = {
        let doc = destino.lectura();
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
    // el /Fields puede ser una referencia a un array: hay que resolverlo o
    // la firma nueva se quedaría fuera del formulario
    let fields_ref = match form.get(b"Fields") {
        Ok(Object::Reference(rid)) => Some(*rid),
        _ => None,
    };
    match fields_ref {
        Some(rid) => {
            let mut arr = destino
                .lectura()
                .get_object(rid)
                .and_then(|o| o.as_array())
                .cloned()
                .unwrap_or_default();
            arr.push(Object::Reference(widget_id));
            form.set("Fields", Object::Array(arr));
        }
        None => match form.get_mut(b"Fields") {
            Ok(Object::Array(arr)) => arr.push(Object::Reference(widget_id)),
            _ => form.set("Fields", Object::Array(vec![Object::Reference(widget_id)])),
        },
    }
    form.set("SigFlags", 3i64);
    destino.cambia(catalog_id, |o| {
        let catalog = o.as_dict_mut().map_err(|e| e.to_string())?;
        catalog.set("AcroForm", Object::Dictionary(form));
        // el catálogo señala **cuál** es la firma de certificación: sin
        // esto el `/DocMDP` del diccionario de firma no lo mira nadie
        if certifica.is_some() {
            let mut perms = Dictionary::new();
            perms.set("DocMDP", Object::Reference(sig_id));
            catalog.set("Perms", Object::Dictionary(perms));
        }
        if let Some(dss) = dss_id {
            // **LTV**: los certificados de la cadena archivados en el
            // documento, para poder seguir comprobando la firma dentro de
            // años sin ir a buscarlos a ninguna parte, y con ellos la
            // respuesta OCSP —la prueba de que el certificado seguía
            // vigente el día que se firmó—, que es lo que convierte
            // «tenemos los papeles» en «tenemos la prueba»
            let mut d = Dictionary::new();
            d.set("Type", Object::Name(b"DSS".to_vec()));
            d.set("Certs", Object::Array(dss));
            if let Some(o) = ocsp_id {
                d.set("OCSPs", Object::Array(vec![o]));
            }
            catalog.set("DSS", Object::Dictionary(d));
        }
        Ok(())
    })
}

/// Rellena los dos huecos que dejó [`escribe_campo_de_firma`] sobre el
/// fichero ya serializado: el `/ByteRange` (que solo se puede escribir
/// cuando se sabe dónde ha caído el `/Contents`) y el PKCS#7.
///
/// `desde` es el byte a partir del cual buscar: en una actualización
/// incremental, el principio de la revisión nueva, porque delante hay otra
/// firma con su propio `/ByteRange` y su propio `/Contents`.
fn cose_la_firma(
    out: &mut [u8],
    desde: usize,
    cred: &Credenciales,
    tsa_url: Option<&str>,
) -> Result<InformeFirma, String> {
    let marker: Vec<u8> = {
        let mut v = vec![b'<'];
        v.extend(std::iter::repeat_n(b'0', SIG_LEN * 2));
        v.push(b'>');
        v
    };
    let contents_start = desde
        + find_subslice(&out[desde..], &marker).ok_or("No se encontró el hueco de la firma")?;
    let contents_end = contents_start + marker.len();

    // parchear ByteRange manteniendo la longitud del hueco
    let br_pos =
        desde + find_subslice(&out[desde..], b"/ByteRange").ok_or("No se encontró /ByteRange")?;
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
    let (der, informe) = build_cms(cred, &digest, tsa_url)?;
    if der.len() > SIG_LEN {
        return Err("La firma no cabe en el hueco reservado".into());
    }
    let hex: String = der.iter().map(|byte| format!("{byte:02X}")).collect();
    out[contents_start + 1..contents_start + 1 + hex.len()].copy_from_slice(hex.as_bytes());
    Ok(informe)
}

/// Cuántas veces aparece `aguja` en `pajar`.
fn cuenta_subslices(pajar: &[u8], aguja: &[u8]) -> usize {
    let mut n = 0;
    let mut i = 0;
    while let Some(j) = find_subslice(&pajar[i..], aguja) {
        n += 1;
        i += j + aguja.len();
    }
    n
}

/// Firma el PDF de `src_path` y escribe el resultado en `dest_path`.
///
/// **Si el documento ya lleva firma, la nueva va detrás**, en una
/// actualización incremental: los bytes de antes se quedan **exactamente**
/// donde estaban y el fichero crece por el final con el campo nuevo, el
/// `/Annots` de su página, el `/AcroForm` actualizado, una tabla de
/// referencias cruzadas con `/Prev` y su `%%EOF`. Es el caso normal de un
/// contrato: dos personas firmando el mismo documento. La primera firma
/// deja de cubrir el fichero entero (`covers_whole_file`) y **eso no es
/// una manipulación**: hay bytes detrás que ella no avala, que es lo que
/// significa una revisión nueva.
pub fn sign(
    src_path: &str,
    dest_path: &str,
    cred: &Credenciales,
    reason: Option<String>,
    apariencia: &Apariencia,
    avanzado: &Avanzado,
) -> Result<InformeFirma, String> {
    firma_o_certifica(
        src_path, dest_path, cred, reason, apariencia, None, avanzado,
    )
}

/// Lo que la interfaz pide en el bloque «Avanzado» del diálogo de firmar,
/// apagado por defecto porque las dos cosas necesitan red.
#[derive(Default, Debug, Clone)]
pub struct Avanzado {
    /// La dirección del servidor de tiempo (RFC 3161). Sin ella, la firma
    /// lleva la fecha del reloj del que firma, que es lo que hay que
    /// contar.
    pub tsa_url: Option<String>,
    /// Archivar los certificados de la cadena en el `/DSS` del documento,
    /// para que la firma se pueda seguir comprobando dentro de años sin
    /// tener que ir a buscarlos.
    pub ltv: bool,
}

/// **Certificar el documento** (Acrobat: «Certificar con firma visible»).
/// Es la misma firma más un `/DocMDP` que dice qué se puede cambiar después
/// sin romperla: `nivel` 1 = ningún cambio, 2 = rellenar formularios y
/// firmar, 3 = además comentar. Los tres son los del spec y los del diálogo
/// de Acrobat.
///
/// **Solo puede certificar la primera firma**: el `/DocMDP` avala el
/// documento entero, y detrás de otra firma ya hay bytes que esta no ha
/// visto. El spec lo dice y los visores lo comprueban; decirlo antes es
/// mejor que escribir un documento que Acrobat marcará en rojo.
#[allow(clippy::too_many_arguments)]
pub fn certify(
    src_path: &str,
    dest_path: &str,
    cred: &Credenciales,
    reason: Option<String>,
    apariencia: &Apariencia,
    nivel: u8,
    avanzado: &Avanzado,
) -> Result<InformeFirma, String> {
    if !(1..=3).contains(&nivel) {
        return Err("El nivel de certificación es 1, 2 o 3".into());
    }
    if esta_firmado(src_path) {
        return Err(
            "Este documento ya lleva una firma. Certificar avala el documento entero, \
             así que solo puede hacerlo la primera"
                .into(),
        );
    }
    firma_o_certifica(
        src_path,
        dest_path,
        cred,
        reason,
        apariencia,
        Some(nivel),
        avanzado,
    )
}

/// Pregunta al respondedor OCSP del certificado si sigue vigente, para
/// archivar la respuesta con la firma. Devuelve la respuesta en DER y,
/// cuando no ha podido ser, la frase en llano que va al aviso: quien la
/// lee no sabe qué es un respondedor ni le hace falta.
fn pide_ocsp(cred: &Credenciales) -> (Option<Vec<u8>>, String) {
    let Some(url) = crate::ocsp::url_de(&cred.cert) else {
        return (
            None,
            "El certificado no dice dónde comprobar si sigue vigente, así que dentro del \
             documento van los certificados pero no esa prueba"
                .to_string(),
        );
    };
    // quien responde por el certificado es su emisor; en uno autofirmado,
    // él mismo
    let emisor = cred.cadena.first().unwrap_or(&cred.cert);
    match crate::ocsp::pide(&url, &cred.cert, emisor) {
        Ok(der) => (Some(der), String::new()),
        Err(e) => (None, e),
    }
}

#[allow(clippy::too_many_arguments)]
fn firma_o_certifica(
    src_path: &str,
    dest_path: &str,
    cred: &Credenciales,
    reason: Option<String>,
    apariencia: &Apariencia,
    certifica: Option<u8>,
    avanzado: &Avanzado,
) -> Result<InformeFirma, String> {
    let bytes = std::fs::read(src_path).map_err(|e| format!("No se ha podido leer el PDF: {e}"))?;
    let doc = LoDoc::load_mem(&bytes).map_err(|e| format!("No se ha podido leer el PDF: {e}"))?;
    let pagina = apariencia.page_index.unwrap_or(0) as u32 + 1;
    let page_id = *doc
        .get_pages()
        .get(&pagina)
        .ok_or("La página donde va la firma no existe")?;
    // el `/ByteRange` es lo que distingue a un PDF firmado
    let ordinal = cuenta_subslices(&bytes, b"/ByteRange") + 1;

    // **La prueba de que el certificado seguía vigente**, pedida aquí y
    // archivada dentro del documento: preguntar al abrir sería llamar por
    // teléfono a un tercero cada vez que alguien mira un PDF. Si el
    // respondedor no está, la firma sale igual y el aviso lo dice: la
    // revocación acompaña a la firma, no es una condición para hacerla
    let (ocsp, aviso_ocsp) = if avanzado.ltv {
        pide_ocsp(cred)
    } else {
        (None, String::new())
    };

    let (mut out, desde) = if ordinal > 1 {
        let anteriores = bytes.len();
        let mut inc = lopdf::IncrementalDocument::create_from(bytes, doc);
        escribe_campo_de_firma(
            &mut Destino::Incremental(&mut inc),
            page_id,
            cred,
            reason,
            apariencia,
            ordinal,
            certifica,
            avanzado.ltv,
            ocsp.clone(),
        )?;
        let mut out = Vec::new();
        inc.save_to(&mut out)
            .map_err(|e| format!("No se ha podido serializar: {e}"))?;
        (out, anteriores)
    } else {
        let mut doc = doc;
        escribe_campo_de_firma(
            &mut Destino::Entero(&mut doc),
            page_id,
            cred,
            reason,
            apariencia,
            ordinal,
            certifica,
            avanzado.ltv,
            ocsp.clone(),
        )?;
        let mut out = Vec::new();
        doc.save_to(&mut out)
            .map_err(|e| format!("No se ha podido serializar: {e}"))?;
        (out, 0)
    };
    let mut informe = cose_la_firma(&mut out, desde, cred, avanzado.tsa_url.as_deref())?;
    informe.ltv = avanzado.ltv;
    if !aviso_ocsp.is_empty() {
        if informe.aviso.is_empty() {
            informe.aviso = aviso_ocsp;
        } else {
            informe.aviso = format!("{} {aviso_ocsp}", informe.aviso);
        }
    }
    std::fs::write(dest_path, &out).map_err(|e| format!("No se ha podido escribir: {e}"))?;
    Ok(informe)
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
    /// Quién es el titular del certificado, **en llano**: el `CN` y, si no
    /// lo lleva, el `O`. Un DN RFC 4514 entero
    /// (`CN=AC FNMT Usuarios,OU=Ceres,O=FNMT-RCM,C=ES`) en una tarjeta que
    /// existe para decir algo en español es jerga; Acrobat enseña «AC FNMT
    /// Usuarios».
    pub cert_subject: String,
    /// Quién lo emitió, en llano, con el mismo criterio.
    pub cert_issuer: String,
    /// El DN completo del titular, tal como viene en el certificado, para
    /// el día que haya un «ver detalles del certificado» (y, mientras
    /// tanto, para el `title` de la tarjeta).
    pub cert_subject_dn: String,
    /// El DN completo del emisor.
    pub cert_issuer_dn: String,
    pub not_before: String,
    pub not_after: String,
    /// El certificado está **fuera de su periodo de validez**, por
    /// cualquiera de los dos lados: caducado o todavía sin entrar en vigor
    /// (`not_yet_valid` distingue cuál, para que la frase de la UI sea la
    /// que toca). Un certificado que aún no valía no se marcaba, y la
    /// tarjeta lo daba por bueno.
    pub expired: bool,
    /// Caso raro pero real (relojes mal puestos, certificados emitidos con
    /// fecha futura): el certificado todavía no había entrado en vigor.
    pub not_yet_valid: bool,
    /// Autofirmado: nadie más responde por ese certificado.
    pub self_signed: bool,
    /// **Quién responde por el certificado**, que es una pregunta distinta
    /// de si el documento ha cambiado: `"raiz_conocida"` (encadena con una
    /// raíz del almacén del sistema), `"autofirmado"` o `"desconocida"`.
    /// Ver `confianza.rs`: **no se comprueba la revocación**, así que la
    /// etiqueta honesta es «emitido por una autoridad reconocida», nunca
    /// «válida».
    pub confianza: String,
    pub page_index: Option<u16>,
    /// Rectángulo del widget en el espacio de la página VISTA, como el
    /// resto de comandos que leen anotaciones. `None` si la firma es
    /// invisible.
    pub rect: Option<crate::Rect>,
    /// El veredicto en una palabra, para que la UI no tenga que deducirlo
    /// de dos booleanos: `"ok"` (firma comprobada y documento intacto),
    /// `"modificado"` (comprobada y NO cuadra) y `"desconocido"` (no se ha
    /// podido comprobar: un algoritmo que Vitela todavía no sabe leer, un
    /// CMS ilegible o el certificado del firmante ausente). Acusar de
    /// manipulación un documento intacto porque la firma usa ECDSA es peor
    /// que no verificar nada, así que ese caso nunca sale en rojo.
    pub estado: String,
    /// Qué se ha comprobado, para la tarjeta: «RSA-2048 / SHA-256»,
    /// «ECDSA P-256 / SHA-384». Con lo no soportado, lo que se ha
    /// encontrado, para que se pueda contar.
    /// **Y es lo que distingue los dos «no se ha podido comprobar»**: con
    /// `"certificado del firmante ausente"` la culpa no es del algoritmo y
    /// la banda no puede decir que lo sea.
    pub algoritmo: String,
    /// **Del documento, no de esta firma**: vale lo mismo en todas las de
    /// la lista. Es cierto cuando todas las firmas están en `"ok"` y una de
    /// ellas cubre el fichero entero —que solo puede ser la última—, o sea
    /// cuando no hay nada escrito después de la última firma.
    ///
    /// Existe porque con dos firmas la banda decía «la firma es válida,
    /// pero hay cambios posteriores que no avala» (AC-064): el «cambio» era
    /// la segunda firma. Una revisión que solo añade una firma no es una
    /// manipulación, y Acrobat dice «Firmado y todas las firmas son
    /// válidas». Deducirlo mirando solo la primera `FirmaInfo` es lo que
    /// no puede hacer la UI.
    pub documento_intacto: bool,
    /// **Esta firma certifica el documento** y con qué nivel de `/DocMDP`:
    /// `Some(1)` ningún cambio, `Some(2)` rellenar formularios y firmar,
    /// `Some(3)` además comentar. `None` es una firma normal.
    ///
    /// Es la mitad que distingue «Firmado por Jorge» de «**Certificado**
    /// por Jorge · se pueden rellenar los formularios», que es lo que dice
    /// Acrobat. Se lee de la referencia de transformación del propio
    /// diccionario de firma y, si el catálogo señala esta firma en su
    /// `/Perms /DocMDP` sin nivel escrito, del defecto del spec (2).
    pub certifica: Option<u8>,
    /// **La hora sellada por una autoridad** (RFC 3161), si la firma la
    /// lleva. Es la diferencia entre «firmado el 10 de septiembre» y «el
    /// 10 de septiembre según el reloj del que firmó»: sin sello, la fecha
    /// la pone quien firma y Vitela la enseñaba como si fuera un hecho.
    pub sello_de_tiempo: Option<crate::tsa::SelloDeTiempo>,
    /// **Del documento, como `documento_intacto`**: dentro del PDF está
    /// archivada la prueba de que el certificado seguía vigente cuando se
    /// firmó (la respuesta OCSP del `/DSS /OCSPs`). Leerla **no toca la
    /// red**: Vitela no llama a nadie al abrir un documento.
    ///
    /// Sin esto, «LTV» prometía comprobar la revocación y solo guardaba
    /// los certificados, que es media promesa.
    pub ltv_archivado: bool,
    /// Cuándo se hizo esa comprobación (`producedAt`), en ISO 8601. Vacía
    /// si no hay prueba archivada o si no se sabe leer su fecha.
    pub ltv_fecha: String,
}

/// Firma comprobada y documento intacto.
pub const ESTADO_OK: &str = "ok";
/// Firma comprobada y el documento no cuadra con ella.
pub const ESTADO_MODIFICADO: &str = "modificado";
/// No se ha podido comprobar (nunca es una acusación).
pub const ESTADO_DESCONOCIDO: &str = "desconocido";

/// La ficha de un certificado suelto, para poder enseñarlo por su nombre.
#[derive(serde::Serialize, Debug)]
pub struct FichaCertificado {
    /// El titular en llano: el `CN` y, si no lo lleva, el `O`.
    pub nombre: String,
    /// Quién lo emitió, también en llano.
    pub emisor: String,
    /// Hasta cuándo vale, en ISO 8601.
    pub not_after: String,
}

/// **Lee un certificado del disco** (`.cer`, `.crt` o `.pem`) y dice de
/// quién es.
///
/// Es lo que hace falta para que «Cifrar con certificado» enseñe a sus
/// destinatarios por su nombre y no por el nombre del fichero: quien cifra
/// para tres personas tiene que poder comprobar que son las tres personas.
/// No abre ningún PDF ni toca el documento.
#[tauri::command(async)]
pub fn read_certificate(path: String) -> Result<FichaCertificado, String> {
    let cert = crate::seguridad::lee_certificado(&path)?;
    Ok(FichaCertificado {
        nombre: nombre_llano(&cert.tbs_certificate.subject.to_string()),
        emisor: nombre_llano(&cert.tbs_certificate.issuer.to_string()),
        not_after: chrono::DateTime::<chrono::Utc>::from(
            cert.tbs_certificate.validity.not_after.to_system_time(),
        )
        .to_rfc3339(),
    })
}

/// Comprueba las firmas del documento: por cada campo `/Sig`, si su/// Comprueba las firmas del documento: por cada campo `/Sig`, si su
/// `/ByteRange` cubre el fichero entero salvo el hueco de `/Contents`, si el
/// SHA-256 de esos rangos es el que va firmado dentro, si la firma RSA la
/// hizo la clave del certificado que viaja en ella, y qué dice ese
/// certificado (sujeto, emisor y validez).
///
/// **Con cadena de confianza desde el ciclo 5, y sin revocación**: se
/// consulta el almacén del sistema (el llavero en macOS, las raíces
/// nativas fuera) y el resultado va en `confianza` (`"raiz_conocida"`,
/// `"autofirmado"`, `"desconocida"`), evaluado en el momento de la firma
/// —el atributo firmado `signingTime`, que es la palabra del firmante:
/// sin sello de tiempo (TSA) no hay más, y es lo que hace Acrobat—. Ni
/// CRL ni OCSP, así que **sigue sin haber ningún «válida»** aquí: hay
/// `digest_ok` (el documento no ha cambiado), `expired`, `self_signed` y
/// quién responde por el certificado, y la UI dice exactamente eso.
#[tauri::command(async)]
pub fn verify_signatures(path: String) -> Result<Vec<FirmaInfo>, String> {
    crate::on_pdfium_thread(move || {
        let bytes = std::fs::read(&path)
            .map_err(|e| crate::mensaje_llano(format!("No se ha podido leer el documento: {e}")))?;
        if find_subslice(&bytes, b"/ByteRange").is_none() {
            return Ok(Vec::new());
        }
        let doc = LoDoc::load_mem(&bytes).map_err(|_| {
            "No se han podido comprobar las firmas: el PDF está incompleto o dañado, \
             así que pide otra copia a quien te lo mandó"
                .to_string()
        })?;
        let mut firmas = firmas_de(&doc, &bytes);
        // el veredicto del DOCUMENTO: todas las firmas comprobadas y nada
        // escrito después de la última (solo la última puede cubrir el
        // fichero entero). Va repetido en cada una porque la lista es lo
        // que viaja a la UI, y así no tiene que deducirlo de la primera
        let intacto = !firmas.is_empty()
            && firmas.iter().all(|f| f.estado == ESTADO_OK)
            && firmas.iter().any(|f| f.covers_whole_file);
        // y la prueba de revocación archivada, que también es del
        // documento: vive en el `/DSS` del catálogo, no en cada firma
        let (archivado, fecha) = prueba_archivada(&doc);
        for f in &mut firmas {
            f.documento_intacto = intacto;
            f.ltv_archivado = archivado;
            f.ltv_fecha = fecha.clone();
        }
        Ok(firmas)
    })
}

/// La respuesta OCSP archivada en el `/DSS /OCSPs` del catálogo y la fecha
/// en que se hizo. Es lo único que hace falta mirar para poder decir «el
/// certificado seguía vigente el 10 de septiembre», y se mira **dentro del
/// fichero**: al abrir un documento no se llama a nadie.
fn prueba_archivada(doc: &LoDoc) -> (bool, String) {
    let Some(dss) = doc
        .catalog()
        .ok()
        .and_then(|c| c.get(b"DSS").ok())
        .and_then(|o| match o {
            Object::Reference(id) => doc.get_object(*id).ok(),
            otro => Some(otro),
        })
        .and_then(|o| o.as_dict().ok())
    else {
        return (false, String::new());
    };
    let Ok(Object::Array(lista)) = dss.get(b"OCSPs") else {
        return (false, String::new());
    };
    for entrada in lista {
        let obj = match entrada {
            Object::Reference(id) => doc.get_object(*id).ok(),
            otro => Some(otro),
        };
        let Some(Ok(stream)) = obj.map(|o| o.as_stream()) else {
            continue;
        };
        let der = stream
            .decompressed_content()
            .unwrap_or_else(|_| stream.content.clone());
        return (true, crate::ocsp::fecha_de(&der).unwrap_or_default());
    }
    (false, String::new())
}

/// Los campos de firma del documento, con la página y el rect de su widget.
fn firmas_de(doc: &LoDoc, bytes: &[u8]) -> Vec<FirmaInfo> {
    let mut vistos: Vec<lopdf::ObjectId> = Vec::new();
    let mut out = Vec::new();
    let certificadora = perms_docmdp(doc);
    for (n, page_id) in doc.get_pages().values().enumerate() {
        let Some(annots) = crate::anotaciones::lista_annots(doc, n as u16) else {
            continue;
        };
        for a in annots {
            let Some(annot) = dict_de(doc, &a) else {
                continue;
            };
            if annot
                .get(b"FT")
                .and_then(|o| o.as_name())
                .unwrap_or_default()
                != b"Sig"
            {
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
            let nivel = nivel_docmdp(&sig, certificadora == Some(*sig_id));
            out.push(lee_firma(&sig, bytes, Some(n as u16), rect, nivel));
        }
    }
    // firmas cuyo campo no cuelga de ninguna página (raro, pero legal)
    for id in campos_de_firma(doc) {
        if vistos.contains(&id) {
            continue;
        }
        if let Some(sig) = dict_de(doc, &Object::Reference(id)) {
            let nivel = nivel_docmdp(&sig, certificadora == Some(id));
            out.push(lee_firma(&sig, bytes, None, None, nivel));
        }
    }
    out
}

/// Cuál es la firma que **certifica** el documento, según el catálogo:
/// `/Root /Perms /DocMDP`. Es donde mira Acrobat, y es la única forma de
/// saber que un `/DocMDP` escrito en un diccionario de firma está de
/// verdad en vigor para el documento.
fn perms_docmdp(doc: &LoDoc) -> Option<lopdf::ObjectId> {
    let root = doc
        .trailer
        .get(b"Root")
        .and_then(|o| o.as_reference())
        .ok()?;
    let catalogo = dict_de(doc, &Object::Reference(root))?;
    let perms = dict_de(doc, catalogo.get(b"Perms").ok()?)?;
    perms.get(b"DocMDP").and_then(|o| o.as_reference()).ok()
}

/// El nivel de certificación de una firma: la `/P` de la referencia de
/// transformación con `/TransformMethod /DocMDP`. `señalada` dice si el
/// catálogo apunta a esta firma; con eso y sin `/P` escrita se toma el
/// defecto del spec, que es 2.
fn nivel_docmdp(sig: &Dictionary, senalada: bool) -> Option<u8> {
    let referencias = sig.get(b"Reference").and_then(|o| o.as_array()).ok();
    let mut nivel = None;
    for r in referencias.into_iter().flatten() {
        let Object::Dictionary(d) = r else { continue };
        if d.get(b"TransformMethod")
            .and_then(|o| o.as_name())
            .unwrap_or_default()
            != b"DocMDP"
        {
            continue;
        }
        nivel = d
            .get(b"TransformParams")
            .and_then(|o| o.as_dict())
            .ok()
            .and_then(|p| p.get(b"P").and_then(|o| o.as_i64()).ok())
            .filter(|n| (1..=3).contains(n))
            .map(|n| n as u8)
            .or(Some(2));
        break;
    }
    match (nivel, senalada) {
        (Some(n), _) => Some(n),
        (None, true) => Some(2),
        (None, false) => None,
    }
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
fn rect_del_widget(
    doc: &LoDoc,
    annot: &Dictionary,
    page_id: lopdf::ObjectId,
) -> Option<crate::Rect> {
    let caja: Vec<f32> = annot
        .get(b"Rect")
        .and_then(|o| o.as_array())
        .ok()?
        .iter()
        .filter_map(|o| {
            o.as_float()
                .ok()
                .or_else(|| o.as_i64().ok().map(|n| n as f32))
        })
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
        let Some(campo) = dict_de(doc, &c) else {
            continue;
        };
        if campo
            .get(b"FT")
            .and_then(|o| o.as_name())
            .unwrap_or_default()
            != b"Sig"
        {
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
    certifica: Option<u8>,
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
        cert_subject_dn: String::new(),
        cert_issuer_dn: String::new(),
        not_before: String::new(),
        not_after: String::new(),
        expired: false,
        not_yet_valid: false,
        self_signed: false,
        confianza: crate::confianza::DESCONOCIDA.to_string(),
        page_index,
        rect,
        // mientras no se compruebe nada, lo honesto es «no se sabe»
        estado: ESTADO_DESCONOCIDO.to_string(),
        algoritmo: String::new(),
        // lo pone `verify_signatures` cuando ya están todas leídas: es del
        // documento, no de esta firma
        documento_intacto: false,
        ltv_archivado: false,
        ltv_fecha: String::new(),
        certifica,
        sello_de_tiempo: None,
    };
    let rangos: Vec<usize> = sig
        .get(b"ByteRange")
        .and_then(|o| o.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|o| o.as_i64().ok())
                .map(|n| n as usize)
                .collect()
        })
        .unwrap_or_default();
    let Object::String(contents, _) = sig.get(b"Contents").unwrap_or(&Object::Null) else {
        return info;
    };
    if rangos.len() != 4 {
        return info;
    }
    let (a, b, c, d) = (rangos[0], rangos[1], rangos[2], rangos[3]);
    // **AC-083.** Un `/ByteRange` que se sale del fichero —o que se solapa
    // consigo mismo— es **prueba** de que el fichero ha cambiado, no una
    // duda: los números los escribió quien firmó y ya no cuadran con lo que
    // hay. Antes se salía por aquí con el «no se ha podido comprobar» que
    // trae puesto `info`, y un documento firmado y luego recortado se
    // presentaba en gris diciendo que usaba un tipo de firma que Vitela no
    // sabe leer, siendo la misma que Vitela acababa de escribir.
    if a + b > bytes.len() || c + d > bytes.len() || c < a + b {
        info.estado = ESTADO_MODIFICADO.to_string();
        return info;
    }
    // el hueco entre los dos rangos tiene que ser justo el /Contents; si
    // ahí hay otra cosa, los bytes se han movido
    let hueco = &bytes[a + b..c];
    if hueco.first() != Some(&b'<') || hueco.last() != Some(&b'>') {
        info.estado = ESTADO_MODIFICADO.to_string();
        return info;
    }
    info.covers_whole_file = a == 0 && c + d == bytes.len();

    if let Some(cms) = lee_cms(contents) {
        info.cert_subject = nombre_llano(&cms.subject);
        info.cert_issuer = nombre_llano(&cms.issuer);
        info.cert_subject_dn = cms.subject;
        info.cert_issuer_dn = cms.issuer;
        info.not_before = cms.not_before;
        info.not_after = cms.not_after;
        info.expired = cms.expired;
        info.not_yet_valid = cms.not_yet_valid;
        info.self_signed = cms.self_signed;
        info.confianza = cms.confianza;
        info.sello_de_tiempo = cms.sello;
        info.algoritmo = cms.algoritmo;
        // el hash del /ByteRange se calcula con el algoritmo que declara la
        // firma, no siempre SHA-256: con SHA-384 o SHA-512 el documento
        // salía «modificado» estando intacto
        let mismo_hash = match cms.hash {
            Some(h) => h.digest(&[&bytes[a..a + b], &bytes[c..c + d]]) == cms.digest,
            None => false,
        };
        info.digest_ok = mismo_hash && cms.firma_ok == Some(true);
        info.estado = match (cms.hash, cms.firma_ok) {
            // no sabemos leer el algoritmo, o no está el certificado del
            // firmante: no se ha podido comprobar, y no se acusa
            (None, _) | (_, None) => ESTADO_DESCONOCIDO,
            _ if info.digest_ok => ESTADO_OK,
            _ => ESTADO_MODIFICADO,
        }
        .to_string();
        if info.name.is_empty() {
            info.name = info.cert_subject.clone();
        }
    }
    info
}

/// Los tres hashes de la familia SHA-2 que se usan en las firmas de PDF.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Hash {
    Sha256,
    Sha384,
    Sha512,
}

impl Hash {
    fn de_oid(oid: const_oid::ObjectIdentifier) -> Option<Hash> {
        use const_oid::db::rfc5912::{ID_SHA_256, ID_SHA_384, ID_SHA_512};
        match oid {
            ID_SHA_256 => Some(Hash::Sha256),
            ID_SHA_384 => Some(Hash::Sha384),
            ID_SHA_512 => Some(Hash::Sha512),
            _ => None,
        }
    }

    fn digest(self, partes: &[&[u8]]) -> Vec<u8> {
        use sha2::{Sha384, Sha512};
        match self {
            Hash::Sha256 => {
                let mut h = Sha256::new();
                for p in partes {
                    h.update(p);
                }
                h.finalize().to_vec()
            }
            Hash::Sha384 => {
                let mut h = Sha384::new();
                for p in partes {
                    h.update(p);
                }
                h.finalize().to_vec()
            }
            Hash::Sha512 => {
                let mut h = Sha512::new();
                for p in partes {
                    h.update(p);
                }
                h.finalize().to_vec()
            }
        }
    }

    fn nombre(self) -> &'static str {
        match self {
            Hash::Sha256 => "SHA-256",
            Hash::Sha384 => "SHA-384",
            Hash::Sha512 => "SHA-512",
        }
    }
}

struct DatosCms {
    /// El `messageDigest` que va firmado dentro del CMS.
    digest: Vec<u8>,
    /// Con qué hash hay que calcular el del `/ByteRange` para compararlo.
    /// `None` si el algoritmo no es uno de los que Vitela sabe leer.
    hash: Option<Hash>,
    /// `Some(true)` la firma la hizo la clave del certificado del firmante;
    /// `Some(false)` no; `None` no se ha podido comprobar (algoritmo no
    /// soportado o certificado del firmante ausente), que **no** es lo
    /// mismo que `Some(false)`.
    firma_ok: Option<bool>,
    /// Qué se ha comprobado, en la lengua de la tarjeta del panel.
    algoritmo: String,
    subject: String,
    issuer: String,
    not_before: String,
    not_after: String,
    expired: bool,
    not_yet_valid: bool,
    self_signed: bool,
    /// Ver `confianza.rs`.
    confianza: String,
    /// El sello de tiempo de una autoridad, si la firma lo lleva.
    sello: Option<crate::tsa::SelloDeTiempo>,
}

/// ¿El certificado está fuera de su periodo de validez, y por qué lado?
/// Devuelve `(fuera, todavia_no)`: lo segundo distingue «caducado el …» de
/// «todavía no era válido». Se mira **`not_before` además de `not_after`**:
/// un certificado emitido con fecha futura (reloj mal puesto al emitirlo)
/// no vale, y hasta el ciclo 5 Vitela lo daba por bueno.
fn fuera_de_vigor(
    not_before: std::time::SystemTime,
    not_after: std::time::SystemTime,
    ahora: std::time::SystemTime,
) -> (bool, bool) {
    let todavia_no = ahora < not_before;
    (todavia_no || ahora > not_after, todavia_no)
}

/// Saca del PKCS#7 el hash firmado, el certificado **del firmante** (el que
/// señala el `SignerIdentifier`, no el primero del bolso: en una firma
/// cualificada viaja la cadena entera y la CA suele ir delante) y si la
/// firma de los atributos firmados la hizo la clave de ese certificado.
///
/// Se aceptan RSA PKCS#1 v1.5, RSA-PSS y ECDSA P-256/P-384, con SHA-256,
/// SHA-384 y SHA-512. Lo que no se reconoce se devuelve como «no
/// comprobado», nunca como firma inválida.
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
    let hash = Hash::de_oid(signer.digest_alg.oid);

    let bolso = certificados_del_bolso(&sd);
    let (cert, era_el_del_firmante) = certificado_del_firmante(&bolso, &signer.sid)?;
    let spki = cert.tbs_certificate.subject_public_key_info.to_der().ok()?;
    let datos = attrs.to_der().ok()?;
    let (firma_ok, algoritmo) = if era_el_del_firmante {
        comprueba_firma(
            signer.signature_algorithm.oid,
            &spki,
            &datos,
            signer.signature.as_bytes(),
            hash,
        )
    } else {
        // sin el certificado del firmante no hay nada que comprobar: el
        // primero del bolso puede ser la CA, y verificar contra ella daría
        // «no válida» sobre una firma buena
        (None, "certificado del firmante ausente".to_string())
    };

    let subject = cert.tbs_certificate.subject.to_string();
    let issuer = cert.tbs_certificate.issuer.to_string();
    let iso = |t: &x509_cert::time::Time| {
        chrono::DateTime::<chrono::Utc>::from(t.to_system_time()).to_rfc3339()
    };
    // la confianza se evalúa en el momento de la firma: un certificado
    // caducado hoy era bueno cuando se firmó, y eso es lo que mira Acrobat
    let momento = hora_de_la_firma(attrs).unwrap_or_else(std::time::SystemTime::now);
    let confianza = crate::confianza::confianza_del_firmante(&cert, &bolso, momento);
    let (fuera, todavia_no) = fuera_de_vigor(
        cert.tbs_certificate.validity.not_before.to_system_time(),
        cert.tbs_certificate.validity.not_after.to_system_time(),
        std::time::SystemTime::now(),
    );
    Some(DatosCms {
        digest,
        hash,
        firma_ok,
        algoritmo,
        self_signed: crate::confianza::es_autofirmado(&cert),
        subject,
        issuer,
        not_before: iso(&cert.tbs_certificate.validity.not_before),
        not_after: iso(&cert.tbs_certificate.validity.not_after),
        expired: fuera,
        not_yet_valid: todavia_no,
        confianza,
        sello: sello_de(signer),
    })
}

/// El sello de tiempo que viaje en los atributos **no firmados** del
/// SignerInfo, que es donde lo pone el RFC 3161.
fn sello_de(signer: &cms::signed_data::SignerInfo) -> Option<crate::tsa::SelloDeTiempo> {
    use der::Encode;
    let attrs = signer.unsigned_attrs.as_ref()?;
    let token = attrs.iter().find(|a| a.oid == crate::tsa::OID_TOKEN)?;
    let der = token.values.iter().next()?.to_der().ok()?;
    crate::tsa::lee_sello(&der)
}

/// El certificado que señala el `SignerIdentifier` (emisor + número de
/// serie, o el identificador de clave del sujeto). Si no está, se devuelve
/// el primero del bolso solo para poder enseñar algo, con `false` en el
/// segundo miembro: con ese no se verifica nada.
pub(crate) fn certificado_del_firmante(
    certificados: &[x509_cert::Certificate],
    sid: &SignerIdentifier,
) -> Option<(x509_cert::Certificate, bool)> {
    let suyo = certificados.iter().find(|cert| match sid {
        SignerIdentifier::IssuerAndSerialNumber(ias) => {
            cert.tbs_certificate.issuer == ias.issuer
                && cert.tbs_certificate.serial_number == ias.serial_number
        }
        SignerIdentifier::SubjectKeyIdentifier(ski) => {
            identificador_de_clave(cert).as_deref() == Some(ski.0.as_bytes())
        }
    });
    match suyo {
        Some(cert) => Some((cert.clone(), true)),
        None => certificados.first().map(|c| (c.clone(), false)),
    }
}

/// Todos los certificados que viajan en la firma: el del firmante y, en
/// una firma cualificada, la cadena hasta la raíz. La cadena es justo lo
/// que necesita `confianza.rs` para llegar al almacén del sistema.
pub(crate) fn certificados_del_bolso(
    sd: &cms::signed_data::SignedData,
) -> Vec<x509_cert::Certificate> {
    sd.certificates
        .as_ref()
        .map(|c| {
            c.0.iter()
                .filter_map(|c| match c {
                    CertificateChoices::Certificate(cert) => Some(cert.clone()),
                    _ => None,
                })
                .collect()
        })
        .unwrap_or_default()
}

/// La hora que declara el atributo firmado `signingTime`, si lo lleva.
fn hora_de_la_firma(attrs: &cms::signed_data::SignedAttributes) -> Option<std::time::SystemTime> {
    use der::{Decode, Encode};
    let attr = attrs
        .iter()
        .find(|a| a.oid == const_oid::db::rfc5911::ID_SIGNING_TIME)?;
    let der = attr.values.iter().next()?.to_der().ok()?;
    x509_cert::time::Time::from_der(&der)
        .ok()
        .map(|t| t.to_system_time())
}

/// El `SubjectKeyIdentifier` (extensión 2.5.29.14) del certificado.
fn identificador_de_clave(cert: &x509_cert::Certificate) -> Option<Vec<u8>> {
    use der::Decode;
    const SKI: const_oid::ObjectIdentifier = const_oid::ObjectIdentifier::new_unwrap("2.5.29.14");
    let ext = cert
        .tbs_certificate
        .extensions
        .as_ref()?
        .iter()
        .find(|e| e.extn_id == SKI)?;
    let octetos = der::asn1::OctetString::from_der(ext.extn_value.as_bytes()).ok()?;
    Some(octetos.as_bytes().to_vec())
}

/// Comprueba la firma de los atributos firmados con la clave pública del
/// certificado. Devuelve `None` cuando el algoritmo no está soportado —que
/// no es lo mismo que una firma que no cuadra— y la etiqueta que la UI
/// enseña en la tarjeta.
pub(crate) fn comprueba_firma(
    alg: const_oid::ObjectIdentifier,
    spki: &[u8],
    datos: &[u8],
    firma: &[u8],
    hash_del_digest: Option<Hash>,
) -> (Option<bool>, String) {
    use const_oid::db::rfc5912::{
        ECDSA_WITH_SHA_256, ECDSA_WITH_SHA_384, ECDSA_WITH_SHA_512, ID_RSASSA_PSS, RSA_ENCRYPTION,
        SHA_256_WITH_RSA_ENCRYPTION, SHA_384_WITH_RSA_ENCRYPTION, SHA_512_WITH_RSA_ENCRYPTION,
    };
    // el hash lo fija el algoritmo de firma cuando lo lleva dentro; si no
    // (rsaEncryption a secas, RSA-PSS), el del digest de los atributos
    let hash = match alg {
        SHA_256_WITH_RSA_ENCRYPTION | ECDSA_WITH_SHA_256 => Some(Hash::Sha256),
        SHA_384_WITH_RSA_ENCRYPTION | ECDSA_WITH_SHA_384 => Some(Hash::Sha384),
        SHA_512_WITH_RSA_ENCRYPTION | ECDSA_WITH_SHA_512 => Some(Hash::Sha512),
        _ => hash_del_digest,
    };
    let Some(hash) = hash else {
        return (None, format!("no reconocido ({alg})"));
    };
    match alg {
        RSA_ENCRYPTION
        | SHA_256_WITH_RSA_ENCRYPTION
        | SHA_384_WITH_RSA_ENCRYPTION
        | SHA_512_WITH_RSA_ENCRYPTION => {
            let (ok, bits) = verifica_rsa(spki, datos, firma, hash, false);
            (ok, format!("RSA-{bits} / {}", hash.nombre()))
        }
        ID_RSASSA_PSS => {
            let (ok, bits) = verifica_rsa(spki, datos, firma, hash, true);
            (ok, format!("RSA-PSS-{bits} / {}", hash.nombre()))
        }
        ECDSA_WITH_SHA_256 | ECDSA_WITH_SHA_384 | ECDSA_WITH_SHA_512 => {
            let (ok, curva) = verifica_ecdsa(spki, &hash.digest(&[datos]), firma);
            (ok, format!("ECDSA {curva} / {}", hash.nombre()))
        }
        _ => (None, format!("no reconocido ({alg})")),
    }
}

/// RSA PKCS#1 v1.5 o PSS, con el hash que toque. El segundo miembro es el
/// tamaño de la clave en bits, para la etiqueta.
fn verifica_rsa(
    spki: &[u8],
    datos: &[u8],
    firma: &[u8],
    hash: Hash,
    pss: bool,
) -> (Option<bool>, usize) {
    use rsa::pkcs8::DecodePublicKey;
    use rsa::signature::Verifier;
    use rsa::traits::PublicKeyParts;
    use sha2::{Sha384, Sha512};
    let Ok(clave) = rsa::RsaPublicKey::from_public_key_der(spki) else {
        return (None, 0);
    };
    let bits = clave.n().bits();
    macro_rules! v {
        ($h:ty) => {
            if pss {
                let vk = rsa::pss::VerifyingKey::<$h>::new(clave);
                match rsa::pss::Signature::try_from(firma) {
                    Ok(f) => Some(vk.verify(datos, &f).is_ok()),
                    Err(_) => Some(false),
                }
            } else {
                let vk = rsa::pkcs1v15::VerifyingKey::<$h>::new(clave);
                match rsa::pkcs1v15::Signature::try_from(firma) {
                    Ok(f) => Some(vk.verify(datos, &f).is_ok()),
                    Err(_) => Some(false),
                }
            }
        };
    }
    let ok = match hash {
        Hash::Sha256 => v!(Sha256),
        Hash::Sha384 => v!(Sha384),
        Hash::Sha512 => v!(Sha512),
    };
    (ok, bits)
}

/// ECDSA sobre P-256 o P-384 (las dos curvas de las firmas cualificadas).
/// Se verifica contra el hash ya calculado, así que vale cualquiera de los
/// tres SHA-2.
fn verifica_ecdsa(spki: &[u8], prehash: &[u8], firma: &[u8]) -> (Option<bool>, &'static str) {
    use p256::ecdsa::signature::hazmat::PrehashVerifier;
    use p256::pkcs8::DecodePublicKey;
    if let Ok(vk) = p256::ecdsa::VerifyingKey::from_public_key_der(spki) {
        let ok = p256::ecdsa::Signature::from_der(firma)
            .map(|f| vk.verify_prehash(prehash, &f).is_ok())
            .unwrap_or(false);
        return (Some(ok), "P-256");
    }
    if let Ok(vk) = p384::ecdsa::VerifyingKey::from_public_key_der(spki) {
        let ok = p384::ecdsa::Signature::from_der(firma)
            .map(|f| vk.verify_prehash(prehash, &f).is_ok())
            .unwrap_or(false);
        return (Some(ok), "P-384");
    }
    (None, "de curva desconocida")
}

/// El nombre de un DN RFC 4514 en llano: el `CN` y, si no lo lleva, el `O`;
/// si tampoco, el DN entero, que siempre es mejor que una casilla vacía.
///
/// Acrobat enseña «Emitido por: AC FNMT Usuarios», no
/// `CN=AC FNMT Usuarios,OU=Ceres,O=FNMT-RCM,C=ES`. El DN completo se
/// conserva aparte (`cert_subject_dn` / `cert_issuer_dn`).
///
/// **Las comas escapadas no parten el nombre**: en RFC 4514 una coma
/// dentro de un valor va como `\,` («CN=Pérez\, Ada,O=Vitela» es un solo
/// componente), y partir por comas a secas dejaba «Pérez\» en la tarjeta.
pub(crate) fn nombre_llano(dn: &str) -> String {
    let partes = componentes(dn);
    for clave in ["CN=", "O="] {
        if let Some(v) = partes.iter().find_map(|p| p.strip_prefix(clave)) {
            let v = desescapa(v);
            if !v.is_empty() {
                return v;
            }
        }
    }
    dn.to_string()
}

/// Parte un DN por sus comas **de verdad**: las precedidas de un número
/// impar de barras invertidas van dentro del valor.
fn componentes(dn: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut actual = String::new();
    let mut escapa = false;
    for c in dn.chars() {
        if escapa {
            actual.push(c);
            escapa = false;
            continue;
        }
        match c {
            '\\' => {
                actual.push(c);
                escapa = true;
            }
            ',' => {
                out.push(actual.trim().to_string());
                actual = String::new();
            }
            _ => actual.push(c),
        }
    }
    out.push(actual.trim().to_string());
    out
}

/// Quita las barras de escape de un valor RFC 4514 («Pérez\, Ada» →
/// «Pérez, Ada»).
fn desescapa(valor: &str) -> String {
    let mut out = String::new();
    let mut escapa = false;
    for c in valor.chars() {
        if escapa {
            out.push(c);
            escapa = false;
        } else if c == '\\' {
            escapa = true;
        } else {
            out.push(c);
        }
    }
    out.trim().to_string()
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

    /// Otro firmante, para el contrato que firman dos personas: el
    /// certificado que emitió la AC de prueba.
    fn otras_credenciales() -> Credenciales {
        credenciales_pem(
            include_str!("../fixtures/test_hija_cert.pem"),
            include_str!("../fixtures/test_hija_key.pem"),
        )
        .expect("las credenciales de la otra firmante")
    }

    /// **R20.** Una firma buena seguida de un cambio legítimo —rellenar un
    /// campo, una segunda firma, el DSS de una firma con LTV— no es una
    /// manipulación: el `/ByteRange` deja de cubrir el fichero (hay bytes
    /// detrás que la firma no avala) pero el hash de lo firmado sigue
    /// cuadrando. El backend tiene que decir las dos cosas por separado
    /// para que la UI no lo pinte en rojo.
    #[test]
    fn una_revision_detras_deja_la_firma_valida_y_el_fichero_sin_cubrir() {
        let dir = std::env::temp_dir();
        let src = dir.join("firma-revision-src.pdf");
        let dest = dir.join("firma-revision-out.pdf");
        crea_pdf(&["Contrato"], &src);
        sign(
            &src.to_string_lossy(),
            &dest.to_string_lossy(),
            &credenciales(),
            None,
            &Apariencia::default(),
            &Avanzado::default(),
        )
        .expect("firmar");
        let f = &verify_signatures(dest.to_string_lossy().into_owned()).expect("verificar")[0];
        assert!(
            f.covers_whole_file && f.estado == ESTADO_OK,
            "recién firmado"
        );

        // una revisión detrás: bytes añadidos al final, sin tocar lo firmado
        let con_revision = dir.join("firma-revision-mas.pdf");
        let mut bytes = std::fs::read(&dest).expect("leer firmado");
        bytes.extend_from_slice(b"\n% revision anadida despues de firmar\n");
        std::fs::write(&con_revision, &bytes).expect("escribir");

        let f =
            &verify_signatures(con_revision.to_string_lossy().into_owned()).expect("verificar")[0];
        assert!(
            !f.covers_whole_file,
            "la firma ya no cubre el fichero entero: hay contenido detrás"
        );
        assert!(f.digest_ok, "lo que la firma cubre no ha cambiado");
        assert_eq!(
            f.estado, ESTADO_OK,
            "una revisión detrás no es una manipulación (algoritmo: {})",
            f.algoritmo
        );
        for p in [&src, &dest, &con_revision] {
            std::fs::remove_file(p).ok();
        }
    }

    /// **R20.** El periodo de validez tiene dos extremos y hasta el ciclo 5
    /// solo se miraba uno: un certificado emitido con fecha futura salía
    /// «en vigor».
    #[test]
    fn un_certificado_que_todavia_no_ha_entrado_en_vigor_no_esta_en_vigor() {
        use std::time::{Duration, SystemTime};
        let ahora = SystemTime::now();
        let dia = Duration::from_secs(24 * 3600);
        // en vigor: empezó ayer y acaba mañana
        assert_eq!(
            fuera_de_vigor(ahora - dia, ahora + dia, ahora),
            (false, false)
        );
        // caducado: acabó ayer
        assert_eq!(
            fuera_de_vigor(ahora - 2 * dia, ahora - dia, ahora),
            (true, false)
        );
        // todavía no: empieza mañana
        assert_eq!(
            fuera_de_vigor(ahora + dia, ahora + 2 * dia, ahora),
            (true, true)
        );
    }

    /// **Orden 13 del informe de QA.** Un PDF firmado con **ECDSA de
    /// verdad**, guardado en `fixtures/`: hasta ahora la firma ECDSA se
    /// fabricaba en el propio test, así que el camino que recorre un
    /// documento que llega de fuera —abrirlo del disco y verificarlo— no lo
    /// probaba nadie. El fixture se generó con `cms_a_mano` y el
    /// certificado P-256 de pruebas, y no cambia: sus bytes son los que
    /// verifica este test.
    ///
    /// Es también el fixture del tercer estado: quien quiera comprobar que
    /// «no se ha podido comprobar» no acusa a nadie ya no tiene que
    /// manipular bytes a mano.
    #[test]
    fn el_fixture_firmado_con_ecdsa_se_verifica_al_abrirlo() {
        let fixture =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/firmado_ecdsa.pdf");
        let firmas = verify_signatures(fixture.to_string_lossy().into_owned())
            .expect("verificar el fixture");
        assert_eq!(firmas.len(), 1, "el fixture lleva una firma");
        let f = &firmas[0];
        assert_eq!(f.estado, ESTADO_OK, "algoritmo: {}", f.algoritmo);
        assert!(f.digest_ok, "el documento no ha cambiado desde la firma");
        assert!(f.covers_whole_file);
        assert!(
            f.algoritmo.contains("ECDSA P-256") && f.algoritmo.contains("SHA-256"),
            "{}",
            f.algoritmo
        );
        assert!(
            f.cert_subject.contains("Ada Lovelace"),
            "{}",
            f.cert_subject
        );
        assert_eq!(
            f.confianza,
            crate::confianza::AUTOFIRMADO,
            "el certificado de pruebas es autofirmado"
        );

        // y tocar un byte del cuerpo del fixture sí lo rompe
        let tocado = std::env::temp_dir().join("firma-ecdsa-fixture-tocado.pdf");
        let mut bytes = std::fs::read(&fixture).expect("leer el fixture");
        let i = find_subslice(&bytes, b"stream").expect("un stream") + 20;
        bytes[i] ^= 0xFF;
        std::fs::write(&tocado, &bytes).expect("escribir");
        let f = &verify_signatures(tocado.to_string_lossy().into_owned()).expect("verificar")[0];
        assert!(!f.digest_ok, "cambiar un byte tiene que romper la firma");
        assert_eq!(f.estado, ESTADO_MODIFICADO);
        std::fs::remove_file(&tocado).ok();
    }

    /// **Distinto 5.** Un campo de firma no es un campo que se rellene: con
    /// él en la lista de `get_form_fields`, un PDF que solo lleva una firma
    /// se anunciaba como «este documento se puede rellenar», y encima en el
    /// momento en que el usuario está mirando la firma. Tampoco es un
    /// comentario, así que no sale en el panel.
    #[test]
    fn un_campo_de_firma_no_es_un_campo_de_formulario_ni_un_comentario() {
        let dir = std::env::temp_dir();
        let src = dir.join("firma-campo-src.pdf");
        let dest = dir.join("firma-campo-out.pdf");
        crea_pdf(&["Contrato"], &src);
        let ap = Apariencia {
            rect: Some(crate::Rect {
                x: 60.0,
                y: 500.0,
                w: 220.0,
                h: 90.0,
            }),
            page_index: Some(0),
            signer_name: Some("Jorge".into()),
            signature_png: None,
        };
        sign(
            &src.to_string_lossy(),
            &dest.to_string_lossy(),
            &credenciales(),
            None,
            &ap,
            &Avanzado::default(),
        )
        .expect("firmar");
        let ruta = dest.to_string_lossy().into_owned();

        // la firma está ahí (el widget existe en el fichero)
        assert_eq!(verify_signatures(ruta.clone()).expect("verificar").len(), 1);
        let campos = crate::formularios::get_form_fields(ruta.clone(), 0).expect("campos");
        assert!(
            campos.is_empty(),
            "un campo de firma no se rellena: {} campos",
            campos.len()
        );
        let comentarios =
            crate::anotaciones::get_document_annotations(ruta.clone()).expect("comentarios");
        assert!(
            comentarios.is_empty(),
            "ni es un comentario: {} anotaciones",
            comentarios.len()
        );
        for f in [&src, &dest] {
            std::fs::remove_file(f).ok();
        }
    }

    /// **G1.** La confianza es una pregunta distinta de la validez: el
    /// documento puede estar intacto y la firma cuadrar, y aun así no
    /// haber nadie que responda por el certificado. Vitela firma con uno
    /// autofirmado, y eso es lo que tiene que decir la tarjeta.
    #[test]
    fn la_firma_de_vitela_es_valida_y_su_certificado_no_lo_respalda_nadie() {
        let (dest, _) = pdf_firmado("firma-confianza");
        let f = &verify_signatures(dest.to_string_lossy().into_owned()).expect("verificar")[0];
        assert_eq!(f.estado, ESTADO_OK, "algoritmo: {}", f.algoritmo);
        assert!(f.self_signed);
        assert_eq!(
            f.confianza,
            crate::confianza::AUTOFIRMADO,
            "nadie más responde por el certificado de prueba"
        );
        std::fs::remove_file(&dest).ok();
    }

    /// **G1.** Una cadena de dos —el firmante y su AC en el bolso de la
    /// firma— cuya raíz no está en el almacén del sistema: no es
    /// autofirmada, pero tampoco se sabe quién la emitió. Es el caso de un
    /// PDF que llega de fuera firmado por una AC que la máquina no conoce,
    /// y la respuesta honesta es «no se ha podido comprobar», no «no
    /// válida».
    #[test]
    fn una_cadena_cuya_raiz_no_conoce_el_sistema_sale_desconocida() {
        use rsa::pkcs8::DecodePrivateKey;
        let (dest, bytes) = pdf_firmado("firma-confianza-cadena");
        let ca = x509_cert::Certificate::from_pem(include_str!("../fixtures/test_ca_cert.pem"))
            .expect("AC de prueba");
        let hija = x509_cert::Certificate::from_pem(include_str!("../fixtures/test_hija_cert.pem"))
            .expect("certificado hijo");
        let clave =
            rsa::RsaPrivateKey::from_pkcs8_pem(include_str!("../fixtures/test_hija_key.pem"))
                .expect("clave del hijo");
        let cms = cms_a_mano(
            &digest_del_byterange(&bytes, Hash::Sha256),
            const_oid::db::rfc5912::ID_SHA_256,
            const_oid::db::rfc5912::SHA_256_WITH_RSA_ENCRYPTION,
            vec![ca, hija.clone()],
            &hija,
            |datos| {
                use rsa::signature::{SignatureEncoding, Signer};
                let sk = rsa::pkcs1v15::SigningKey::<Sha256>::new(clave.clone());
                sk.sign(datos).to_vec()
            },
        );
        std::fs::write(&dest, recose(&bytes, &cms)).expect("recoser");
        let f = &verify_signatures(dest.to_string_lossy().into_owned()).expect("verificar")[0];
        assert_eq!(f.estado, ESTADO_OK, "la firma cuadra: {}", f.algoritmo);
        assert!(!f.self_signed, "la emitió su AC, no ella misma");
        assert_eq!(f.confianza, crate::confianza::DESCONOCIDA);
        std::fs::remove_file(&dest).ok();
    }

    /// El certificado de la firma ECDSA de prueba (P-256), y su clave.
    fn cert_ec() -> x509_cert::Certificate {
        x509_cert::Certificate::from_pem(include_str!("../fixtures/test_ec_cert.pem"))
            .expect("certificado EC de prueba")
    }

    fn clave_ec() -> p256::ecdsa::SigningKey {
        use p256::pkcs8::DecodePrivateKey;
        p256::ecdsa::SigningKey::from_pkcs8_pem(include_str!("../fixtures/test_ec_key.pem"))
            .expect("clave EC de prueba")
    }

    /// Los dos rangos que firma el PDF (todo menos el hueco de /Contents) y
    /// dónde está ese hueco. Es lo que escribe `sign`.
    fn hueco_de_contents(bytes: &[u8]) -> (usize, usize) {
        // el hueco es la cadena hexadecimal de tamaño fijo que reserva `sign`
        let i = (0..bytes.len())
            .find(|&i| bytes[i] == b'<' && bytes.get(i + SIG_LEN * 2 + 1) == Some(&b'>'))
            .expect("el hueco de /Contents");
        (i, i + SIG_LEN * 2 + 2)
    }

    /// Sustituye el PKCS#7 de un PDF ya firmado por otro, sin mover un solo
    /// byte: el hueco de `/Contents` tiene tamaño fijo, así que el
    /// `/ByteRange` sigue valiendo y el documento sigue intacto. Así se
    /// fabrica lo que en la vida real llega firmado por otra herramienta.
    fn recose(bytes: &[u8], cms: &[u8]) -> Vec<u8> {
        let (ini, fin) = hueco_de_contents(bytes);
        let hex: String = cms.iter().map(|b| format!("{b:02X}")).collect();
        let hueco = fin - ini - 2;
        assert!(hex.len() <= hueco, "el CMS no cabe en el hueco");
        let mut out = bytes.to_vec();
        let relleno = format!("{hex:0<hueco$}");
        out[ini + 1..fin - 1].copy_from_slice(relleno.as_bytes());
        out
    }

    /// El digest del `/ByteRange` de un PDF firmado, con el hash que se
    /// pida: lo que va dentro del CMS como `messageDigest`.
    fn digest_del_byterange(bytes: &[u8], hash: Hash) -> Vec<u8> {
        let (ini, fin) = hueco_de_contents(bytes);
        hash.digest(&[&bytes[..ini], &bytes[fin..]])
    }

    /// CMS SignedData a mano, para fabricar firmas que Vitela no hace: el
    /// algoritmo de digest, el de firma, el bolso de certificados (en el
    /// orden que se pase) y la firma ya calculada.
    fn cms_a_mano(
        digest: &[u8],
        digest_alg: const_oid::ObjectIdentifier,
        firma_alg: const_oid::ObjectIdentifier,
        certificados: Vec<x509_cert::Certificate>,
        firmante: &x509_cert::Certificate,
        firma_de: impl Fn(&[u8]) -> Vec<u8>,
    ) -> Vec<u8> {
        use cms::signed_data::{SignedData, SignerInfo, SignerInfos};
        use der::asn1::{Any, OctetString, SetOfVec};
        use der::{Decode, Tag, Tagged};
        use x509_cert::attr::Attribute;

        let alg = |oid| AlgorithmIdentifierOwned {
            oid,
            parameters: None,
        };
        let atributo = |oid, valor: Any| {
            let mut valores = SetOfVec::new();
            valores.insert(valor).expect("valor");
            Attribute {
                oid,
                values: valores,
            }
        };
        let content_type = atributo(
            const_oid::db::rfc5911::ID_CONTENT_TYPE,
            Any::new(
                Tag::ObjectIdentifier,
                const_oid::db::rfc5911::ID_DATA.as_bytes(),
            )
            .expect("oid"),
        );
        let message_digest = atributo(
            const_oid::db::rfc5911::ID_MESSAGE_DIGEST,
            Any::new(Tag::OctetString, digest).expect("digest"),
        );
        let mut attrs = SetOfVec::new();
        attrs.insert(content_type).expect("contentType");
        attrs.insert(message_digest).expect("messageDigest");
        let attrs_der = attrs.to_der().expect("atributos");
        let firma = firma_de(&attrs_der);

        let signer = SignerInfo {
            version: cms::content_info::CmsVersion::V1,
            sid: SignerIdentifier::IssuerAndSerialNumber(IssuerAndSerialNumber {
                issuer: firmante.tbs_certificate.issuer.clone(),
                serial_number: firmante.tbs_certificate.serial_number.clone(),
            }),
            digest_alg: alg(digest_alg),
            signed_attrs: Some(attrs),
            signature_algorithm: alg(firma_alg),
            signature: OctetString::new(firma).expect("firma"),
            unsigned_attrs: None,
        };
        let mut bolso = SetOfVec::new();
        for c in certificados {
            bolso
                .insert(CertificateChoices::Certificate(c))
                .expect("certificado");
        }
        let sd = SignedData {
            version: cms::content_info::CmsVersion::V1,
            digest_algorithms: {
                let mut v = SetOfVec::new();
                v.insert(alg(digest_alg)).expect("digest alg");
                v
            },
            encap_content_info: EncapsulatedContentInfo {
                econtent_type: const_oid::db::rfc5911::ID_DATA,
                econtent: None,
            },
            certificates: Some(bolso.into()),
            crls: None,
            signer_infos: SignerInfos(SetOfVec::from_iter([signer]).expect("signer")),
        };
        let der = sd.to_der().expect("SignedData");
        let ci = cms::content_info::ContentInfo {
            content_type: const_oid::db::rfc5911::ID_SIGNED_DATA,
            content: Any::from_der(&der).expect("any"),
        };
        let _ = ci.content.tag();
        ci.to_der().expect("ContentInfo")
    }

    /// Un servidor de tiempo de mentira: escucha en localhost, contesta a
    /// una petición RFC 3161 con un token firmado por nuestro propio
    /// certificado de prueba y se apaga. Devuelve su URL.
    fn tsa_de_mentira(cuando: &str) -> (String, std::thread::JoinHandle<()>) {
        use std::io::{Read, Write};
        let escucha = std::net::TcpListener::bind("127.0.0.1:0").expect("escuchar");
        let url = format!("http://{}/tsr", escucha.local_addr().expect("puerto"));
        let token = token_de_mentira(cuando);
        let hilo = std::thread::spawn(move || {
            let Ok((mut cliente, _)) = escucha.accept() else {
                return;
            };
            // se lee la petición **entera** antes de contestar: cerrar el
            // socket con bytes sin leer manda un RST y el cliente se queda
            // sin respuesta. Lo que mande no se mira: lo que se prueba
            // aquí es que Vitela sabe meter el token en el CMS y volver a
            // leerlo
            let mut peticion = Vec::new();
            let mut buf = [0u8; 1024];
            while let Ok(n) = cliente.read(&mut buf) {
                if n == 0 {
                    break;
                }
                peticion.extend_from_slice(&buf[..n]);
                let entera = peticion
                    .windows(4)
                    .position(|v| v == b"\r\n\r\n")
                    .map(|corte| {
                        let cabecera = String::from_utf8_lossy(&peticion[..corte]).to_lowercase();
                        let largo = cabecera
                            .split("content-length:")
                            .nth(1)
                            .and_then(|t| t.split(['\r', '\n']).next())
                            .and_then(|t| t.trim().parse::<usize>().ok())
                            .unwrap_or(0);
                        peticion.len() >= corte + 4 + largo
                    })
                    .unwrap_or(false);
                if entera {
                    break;
                }
            }
            let respuesta = super::super::tsa::respuesta_de_prueba(&token);
            let cabeceras = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/timestamp-reply\r\n\
                 Content-Length: {}\r\nConnection: close\r\n\r\n",
                respuesta.len()
            );
            let _ = cliente.write_all(cabeceras.as_bytes());
            let _ = cliente.write_all(&respuesta);
            let _ = cliente.flush();
        });
        (url, hilo)
    }

    /// Un `TimeStampToken` de verdad —un CMS con un `TSTInfo` dentro—
    /// firmado con el certificado de prueba.
    fn token_de_mentira(cuando: &str) -> Vec<u8> {
        use der::asn1::{Any, OctetString, SetOfVec};
        use der::{Decode, Tag};
        let cred = credenciales();
        // TSTInfo: versión, política, imprint, serie y la hora
        let tst = crate::tsa::tst_de_prueba(cuando);
        let content = EncapsulatedContentInfo {
            econtent_type: crate::tsa::OID_TSTINFO,
            econtent: Some(Any::new(Tag::OctetString, tst.as_slice()).expect("eContent")),
        };
        let alg = |oid| AlgorithmIdentifierOwned {
            oid,
            parameters: None,
        };
        let signer = cms::signed_data::SignerInfo {
            version: cms::content_info::CmsVersion::V1,
            sid: SignerIdentifier::IssuerAndSerialNumber(IssuerAndSerialNumber {
                issuer: cred.cert.tbs_certificate.issuer.clone(),
                serial_number: cred.cert.tbs_certificate.serial_number.clone(),
            }),
            digest_alg: alg(const_oid::db::rfc5912::ID_SHA_256),
            signed_attrs: None,
            signature_algorithm: alg(const_oid::db::rfc5912::SHA_256_WITH_RSA_ENCRYPTION),
            signature: OctetString::new(vec![0u8; 32]).expect("firma"),
            unsigned_attrs: None,
        };
        let mut bolso = SetOfVec::new();
        bolso
            .insert(CertificateChoices::Certificate(cred.cert.clone()))
            .expect("certificado");
        let sd = cms::signed_data::SignedData {
            version: cms::content_info::CmsVersion::V1,
            digest_algorithms: {
                let mut v = SetOfVec::new();
                v.insert(alg(const_oid::db::rfc5912::ID_SHA_256))
                    .expect("alg");
                v
            },
            encap_content_info: content,
            certificates: Some(bolso.into()),
            crls: None,
            signer_infos: cms::signed_data::SignerInfos(
                SetOfVec::from_iter([signer]).expect("signer"),
            ),
        };
        let der = sd.to_der().expect("SignedData");
        cms::content_info::ContentInfo {
            content_type: const_oid::db::rfc5911::ID_SIGNED_DATA,
            content: Any::from_der(&der).expect("any"),
        }
        .to_der()
        .expect("ContentInfo")
    }

    /// Un respondedor OCSP de mentira en el puerto que dice el certificado
    /// de prueba (`test_ocsp_cert.pem` lleva su AIA apuntando ahí). Como el
    /// servidor de tiempo: se lee la petición entera antes de contestar y
    /// se apaga.
    fn ocsp_de_mentira(cuando: &str) -> std::thread::JoinHandle<()> {
        use std::io::{Read, Write};
        let escucha =
            std::net::TcpListener::bind("127.0.0.1:41960").expect("el puerto del respondedor");
        let cuerpo = crate::ocsp::prueba::respuesta_de_prueba(cuando);
        std::thread::spawn(move || {
            let Ok((mut cliente, _)) = escucha.accept() else {
                return;
            };
            let mut peticion = Vec::new();
            let mut buf = [0u8; 1024];
            while let Ok(n) = cliente.read(&mut buf) {
                if n == 0 {
                    break;
                }
                peticion.extend_from_slice(&buf[..n]);
                let entera = peticion
                    .windows(4)
                    .position(|v| v == b"\r\n\r\n")
                    .map(|corte| {
                        let cabecera = String::from_utf8_lossy(&peticion[..corte]).to_lowercase();
                        let largo = cabecera
                            .split("content-length:")
                            .nth(1)
                            .and_then(|t| t.split(['\r', '\n']).next())
                            .and_then(|t| t.trim().parse::<usize>().ok())
                            .unwrap_or(0);
                        peticion.len() >= corte + 4 + largo
                    })
                    .unwrap_or(false);
                if entera {
                    break;
                }
            }
            let cabeceras = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/ocsp-response\r\n\
                 Content-Length: {}\r\nConnection: close\r\n\r\n",
                cuerpo.len()
            );
            let _ = cliente.write_all(cabeceras.as_bytes());
            let _ = cliente.write_all(&cuerpo);
            let _ = cliente.flush();
        })
    }

    /// **LTV de verdad** (C-5 del ciclo 10). Hasta aquí «LTV» archivaba los
    /// certificados y prometía comprobar la revocación, que es media
    /// promesa. Ahora, al firmar, se le pregunta al respondedor OCSP que
    /// dice el propio certificado si sigue vigente y **la respuesta se
    /// guarda dentro del documento**; al abrirlo, la prueba se lee del
    /// `/DSS` y **no se llama a nadie**.
    #[test]
    fn firmar_con_ltv_archiva_la_prueba_de_que_el_certificado_seguia_vigente() {
        let dir = std::env::temp_dir();
        let src = dir.join("firma-ocsp-src.pdf");
        let dest = dir.join("firma-ocsp-firmado.pdf");
        crea_pdf(&["Contrato con prueba"], &src);
        let cred = credenciales_pem(
            include_str!("../fixtures/test_ocsp_cert.pem"),
            include_str!("../fixtures/test_ocsp_key.pem"),
        )
        .expect("credenciales con AIA");
        let hilo = ocsp_de_mentira("20260910194012Z");

        let informe = sign(
            &src.to_string_lossy(),
            &dest.to_string_lossy(),
            &cred,
            None,
            &Apariencia::default(),
            &Avanzado {
                tsa_url: None,
                ltv: true,
            },
        )
        .expect("firmar con LTV");
        let _ = hilo.join();
        assert!(informe.ltv);
        assert!(
            informe.aviso.is_empty(),
            "con el respondedor contestando no hay nada que avisar: {}",
            informe.aviso
        );

        // la prueba está dentro del documento y se lee sin red
        let firmas = verify_signatures(dest.to_string_lossy().into_owned()).expect("verificar");
        assert_eq!(firmas.len(), 1);
        assert_eq!(
            firmas[0].estado, "ok",
            "archivar la prueba no rompe la firma"
        );
        assert!(
            firmas[0].ltv_archivado,
            "el /DSS tiene que llevar la respuesta del respondedor"
        );
        assert_eq!(
            firmas[0].ltv_fecha, "2026-09-10T19:40:12+00:00",
            "y con ella, cuándo se comprobó"
        );

        // **sin respondedor la firma sale igual y se dice por qué**: la
        // revocación acompaña a la firma, no es una condición para hacerla
        let sin_red = dir.join("firma-ocsp-sin-red.pdf");
        let informe = sign(
            &src.to_string_lossy(),
            &sin_red.to_string_lossy(),
            &cred,
            None,
            &Apariencia::default(),
            &Avanzado {
                tsa_url: None,
                ltv: true,
            },
        )
        .expect("firmar sin respondedor no es un fallo");
        assert!(
            !informe.aviso.is_empty(),
            "hay que decir que falta la prueba"
        );
        assert!(
            !informe.aviso.contains("OCSP") && !informe.aviso.contains("respondedor"),
            "el aviso lo lee una persona: {}",
            informe.aviso
        );
        let firmas = verify_signatures(sin_red.to_string_lossy().into_owned()).expect("verificar");
        assert_eq!(firmas[0].estado, "ok");
        assert!(!firmas[0].ltv_archivado);
        assert!(firmas[0].ltv_fecha.is_empty());

        // y un certificado que no dice dónde preguntar no manda a nadie a
        // ninguna parte: se firma, se archivan los certificados y se dice
        let sin_aia = dir.join("firma-ocsp-sin-aia.pdf");
        let informe = sign(
            &src.to_string_lossy(),
            &sin_aia.to_string_lossy(),
            &credenciales(),
            None,
            &Apariencia::default(),
            &Avanzado {
                tsa_url: None,
                ltv: true,
            },
        )
        .expect("firmar");
        assert!(informe.aviso.contains("no dice dónde"), "{}", informe.aviso);
        assert!(
            !verify_signatures(sin_aia.to_string_lossy().into_owned()).expect("verificar")[0]
                .ltv_archivado
        );

        for f in [&src, &dest, &sin_red, &sin_aia] {
            std::fs::remove_file(f).ok();
        }
    }

    /// **AC-083, alto.** Un documento firmado y luego modificado se
    /// presentaba en **gris** —«no se ha podido comprobar la firma: usa un
    /// tipo de firma que Vitela todavía no sabe leer»— siendo la misma
    /// RSA/SHA-256 que Vitela acababa de escribir. El `/ByteRange` del
    /// fichero recortado seguía diciendo dónde acababa el original, así que
    /// la comprobación se salía antes de leer el CMS y se quedaba con el
    /// «no se sabe» que trae puesto.
    ///
    /// Unos números que ya no caben en el fichero son **prueba** de que el
    /// fichero ha cambiado. El «no se sabe» se guarda para cuando de verdad
    /// no se sabe: un algoritmo que no se conoce o un certificado que falta.
    #[test]
    fn un_documento_firmado_y_luego_tocado_se_dice_modificado_y_no_desconocido() {
        let dir = std::env::temp_dir();
        let src = dir.join("firma-tocado-src.pdf");
        let firmado = dir.join("firma-tocado-firmado.pdf");
        crea_pdf(&["Uno", "Dos", "Tres"], &src);
        sign(
            &src.to_string_lossy(),
            &firmado.to_string_lossy(),
            &credenciales(),
            None,
            &Apariencia::default(),
            &Avanzado::default(),
        )
        .expect("firmar");
        assert_eq!(
            verify_signatures(firmado.to_string_lossy().into_owned()).expect("verificar")[0].estado,
            ESTADO_OK
        );

        // lo que hace cualquiera: abrirlo, quitar una página y guardar. El
        // documento sigue siendo un PDF válido y su /ByteRange dice dónde
        // acababa el de antes
        let info = crate::open_pdf(firmado.to_string_lossy().into_owned(), None, None, None)
            .expect("abrir el firmado");
        crate::paginas::delete_page(info.work_path.clone(), 1).expect("quitar una página");
        let roto = dir.join("firma-tocado-roto.pdf");
        crate::save_pdf(info.work_path.clone(), roto.to_string_lossy().into_owned())
            .expect("guardar");
        crate::close_document(info.work_path).ok();

        let f = &verify_signatures(roto.to_string_lossy().into_owned()).expect("verificar")[0];
        assert_eq!(
            f.estado, ESTADO_MODIFICADO,
            "un /ByteRange que ya no cabe en el fichero es prueba, no duda"
        );
        assert!(!f.documento_intacto);
        assert!(!f.digest_ok);

        for f in [&src, &firmado, &roto] {
            std::fs::remove_file(f).ok();
        }
    }

    /// **El sello de tiempo, de punta a punta** (orden 4.1 del ciclo 9).
    /// La fecha de una firma sin sello es la del reloj del que firmó, y
    /// Vitela la enseñaba como si fuera un hecho. Con sello, quien
    /// responde por la hora es un tercero, y eso es lo que hay que poder
    /// enseñar en la tarjeta.
    #[test]
    fn firmar_con_sello_de_tiempo_lo_mete_en_el_cms_y_se_vuelve_a_leer() {
        let dir = std::env::temp_dir();
        let src = dir.join("firma-tsa-src.pdf");
        let dest = dir.join("firma-tsa-firmado.pdf");
        crea_pdf(&["Contrato con hora"], &src);
        let (url, hilo) = tsa_de_mentira("20260910194012Z");

        let informe = sign(
            &src.to_string_lossy(),
            &dest.to_string_lossy(),
            &credenciales(),
            None,
            &Apariencia::default(),
            &Avanzado {
                tsa_url: Some(url),
                ltv: true,
            },
        )
        .expect("firmar con sello");
        let _ = hilo.join();

        assert!(
            informe.sellada,
            "el informe tiene que decir que va sellada: {}",
            informe.aviso
        );
        // el aviso no puede nombrar el servidor de tiempo: es lo que la
        // interfaz reconoce para preguntar «¿firmar sin sello?», y aquí el
        // sello ha llegado. (Lo que sí dice es que este certificado no
        // lleva dónde comprobar su revocación, que es otra cosa.)
        assert!(
            !informe.aviso.contains("servidor de tiempo"),
            "el sello ha llegado: {}",
            informe.aviso
        );
        let sello = informe.sello.expect("el sello");
        assert_eq!(sello.fecha, "2026-09-10T19:40:12+00:00");
        assert!(!sello.autoridad.is_empty(), "quién ha sellado");

        // y se vuelve a leer del documento: es lo que enseña la tarjeta
        let firmas = verify_signatures(dest.to_string_lossy().into_owned()).expect("verificar");
        assert_eq!(firmas.len(), 1);
        assert_eq!(firmas[0].estado, "ok", "el sello no puede romper la firma");
        let leido = firmas[0].sello_de_tiempo.as_ref().expect("el sello leído");
        assert_eq!(leido.fecha, "2026-09-10T19:40:12+00:00");

        // LTV: los certificados archivados en el /DSS del catálogo
        let texto = String::from_utf8_lossy(&std::fs::read(&dest).expect("leer")).to_string();
        assert!(texto.contains("/DSS"), "sin /DSS no hay nada archivado");

        // **sin servidor, la firma sale igual y se dice por qué**: lo que
        // no puede pasar es que se caiga después de elegir el destino
        let sin_red = dir.join("firma-tsa-sin-red.pdf");
        let informe = sign(
            &src.to_string_lossy(),
            &sin_red.to_string_lossy(),
            &credenciales(),
            None,
            &Apariencia::default(),
            &Avanzado {
                tsa_url: Some("http://127.0.0.1:9/tsr".into()),
                ltv: false,
            },
        )
        .expect("firmar sin sello no es un fallo");
        assert!(!informe.sellada);
        assert!(
            informe.aviso.contains("servidor de tiempo"),
            "el aviso tiene que nombrar el servidor de tiempo para que la \
             interfaz pueda preguntar «¿firmar sin sello?»: {}",
            informe.aviso
        );
        let firmas = verify_signatures(sin_red.to_string_lossy().into_owned()).expect("verificar");
        assert_eq!(firmas[0].estado, "ok");
        assert!(firmas[0].sello_de_tiempo.is_none());

        for f in [&src, &dest, &sin_red] {
            std::fs::remove_file(f).ok();
        }
    }

    /// Un PDF firmado por Vitela, para recoserle otra firma encima.
    fn pdf_firmado(nombre: &str) -> (std::path::PathBuf, Vec<u8>) {
        let dir = std::env::temp_dir();
        let src = dir.join(format!("{nombre}-src.pdf"));
        let dest = dir.join(format!("{nombre}-firmado.pdf"));
        crea_pdf(&["Contrato"], &src);
        sign(
            &src.to_string_lossy(),
            &dest.to_string_lossy(),
            &credenciales(),
            None,
            &Apariencia::default(),
            &Avanzado::default(),
        )
        .expect("firmar");
        let bytes = std::fs::read(&dest).expect("leer");
        std::fs::remove_file(&src).ok();
        (dest, bytes)
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
        let rect = crate::Rect {
            x: 60.0,
            y: 500.0,
            w: 220.0,
            h: 90.0,
        };
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
            &Avanzado::default(),
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
        assert_eq!(f.estado, ESTADO_OK, "algoritmo: {}", f.algoritmo);
        assert!(
            f.algoritmo.contains("RSA-2048 / SHA-256"),
            "{}",
            f.algoritmo
        );
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
            &Avanzado::default(),
        )
        .expect("firmar sin apariencia");
        let f = &verify_signatures(invisible.to_string_lossy().into_owned()).expect("verificar")[0];
        assert!(f.digest_ok && f.covers_whole_file);
        assert!(f.rect.is_none(), "sin rect no hay firma que ver");

        for p in [&src, &dest, &tocado, &invisible] {
            std::fs::remove_file(p).ok();
        }
    }
    /// **H4, el primer paso.** Antes de escribir nada de la segunda firma
    /// hay que saber si lopdf conserva **byte a byte** el documento
    /// anterior al guardar una actualización incremental: si normalizara
    /// los objetos viejos, la primera firma se rompería y todo el trabajo
    /// sobraría.
    ///
    /// `covers_whole_file` deja de ser cierto y eso es lo correcto: hay
    /// bytes detrás que la primera firma no avala. R20 ya enseñó que eso es
    /// neutro y no una manipulación.
    #[test]
    fn guardar_incrementalmente_un_pdf_firmado_no_le_rompe_la_firma() {
        let dir = std::env::temp_dir();
        let src = dir.join("firma-incremental-base.pdf");
        let firmado = dir.join("firma-incremental-firmado.pdf");
        let crecido = dir.join("firma-incremental-crecido.pdf");
        crea_pdf(&["Contrato"], &src);
        sign(
            &src.to_string_lossy(),
            &firmado.to_string_lossy(),
            &credenciales(),
            None,
            &Apariencia::default(),
            &Avanzado::default(),
        )
        .expect("firmar");
        let antes = std::fs::read(&firmado).expect("leer el firmado");

        // guardar incrementalmente SIN tocar nada
        let doc = LoDoc::load(&firmado).expect("cargar");
        let mut inc = lopdf::IncrementalDocument::create_from(antes.clone(), doc);
        let mut out: Vec<u8> = Vec::new();
        inc.save_to(&mut out).expect("guardar incremental");
        std::fs::write(&crecido, &out).expect("escribir");

        assert!(out.len() > antes.len(), "una revisión nueva añade bytes");
        assert_eq!(
            &out[..antes.len()],
            &antes[..],
            "los bytes de la primera revisión tienen que quedarse exactamente donde estaban"
        );

        let f = &verify_signatures(crecido.to_string_lossy().into_owned()).expect("verificar")[0];
        assert_eq!(f.estado, ESTADO_OK, "la firma sigue cuadrando: {f:?}");
        assert!(f.digest_ok);
        assert!(
            !f.covers_whole_file,
            "y ya no cubre el fichero entero, que es justo lo que significa una revisión detrás"
        );
        for p in [&src, &firmado, &crecido] {
            std::fs::remove_file(p).ok();
        }
    }

    /// **R28.** De un DN se enseña el nombre, no el DN. La tarjeta del
    /// panel decía «Emitido por
    /// `CN=AC FNMT Usuarios,OU=Ceres,O=FNMT-RCM,C=ES`» en una línea que
    /// existe precisamente para hablar en llano.
    ///
    /// El caso que hay que acertar es la coma escapada: en RFC 4514 una
    /// coma dentro de un valor va como `\,` y partir por comas a secas
    /// dejaba medio apellido.
    #[test]
    fn del_dn_se_ensena_el_nombre_y_una_coma_escapada_no_lo_parte() {
        assert_eq!(
            nombre_llano("CN=AC FNMT Usuarios,OU=Ceres,O=FNMT-RCM,C=ES"),
            "AC FNMT Usuarios"
        );
        // una coma dentro del CN: un solo componente, con su coma
        assert_eq!(nombre_llano(r"CN=Pérez\, Ada,O=Vitela,C=ES"), "Pérez, Ada");
        // sin CN, el O
        assert_eq!(nombre_llano("OU=Ceres,O=FNMT-RCM,C=ES"), "FNMT-RCM");
        // sin ninguno de los dos, el DN entero antes que una casilla vacía
        assert_eq!(nombre_llano("C=ES"), "C=ES");
        assert_eq!(nombre_llano(""), "");
        // el CN vacío no gana: se sigue buscando algo que decir
        assert_eq!(nombre_llano("CN=,O=Vitela"), "Vitela");
    }

    /// **R28.** Y en la firma de verdad: el nombre llano en `cert_subject`
    /// y `cert_issuer`, el DN completo guardado aparte.
    #[test]
    fn la_tarjeta_de_la_firma_lleva_el_nombre_y_el_dn_va_aparte() {
        let dir = std::env::temp_dir();
        let src = dir.join("firma-dn-llano.pdf");
        let dest = dir.join("firma-dn-llano-out.pdf");
        crea_pdf(&["Contrato"], &src);
        sign(
            &src.to_string_lossy(),
            &dest.to_string_lossy(),
            &credenciales(),
            None,
            &Apariencia::default(),
            &Avanzado::default(),
        )
        .expect("firmar");
        let f = &verify_signatures(dest.to_string_lossy().into_owned()).expect("verificar")[0];
        assert!(
            !f.cert_subject.contains('='),
            "el sujeto sale en llano y no como DN: {}",
            f.cert_subject
        );
        assert!(
            !f.cert_issuer.contains('='),
            "y el emisor también: {}",
            f.cert_issuer
        );
        assert!(
            f.cert_subject_dn.contains("CN="),
            "el DN entero se conserva: {}",
            f.cert_subject_dn
        );
        assert_eq!(f.cert_subject, nombre_llano(&f.cert_subject_dn));
        assert_eq!(f.cert_issuer, nombre_llano(&f.cert_issuer_dn));
        // sin `/Name` en el diccionario de firma, quien firma es ese mismo
        // nombre llano
        assert_eq!(f.name, f.cert_subject);
        // y «autofirmado» lo dice la definición buena (nombre Y firma),
        // no solo que el sujeto y el emisor se llamen igual
        assert!(f.self_signed, "el certificado de prueba es autofirmado");
        for p in [&src, &dest] {
            std::fs::remove_file(p).ok();
        }
    }

    /// Una firma ECDSA P-256 —lo que llevan hoy las firmas cualificadas— es
    /// una firma buena: tiene que salir «ok», no en rojo. Vitela solo sabía
    /// RSA/SHA-256 y daba por manipulado todo lo demás.
    #[test]
    fn una_firma_ecdsa_p256_se_comprueba_y_sale_bien() {
        let (dest, bytes) = pdf_firmado("firma-ecdsa");
        let digest = digest_del_byterange(&bytes, Hash::Sha256);
        let cert = cert_ec();
        let clave = clave_ec();
        let cms = cms_a_mano(
            &digest,
            const_oid::db::rfc5912::ID_SHA_256,
            const_oid::db::rfc5912::ECDSA_WITH_SHA_256,
            vec![cert.clone()],
            &cert,
            |datos| {
                use p256::ecdsa::signature::Signer;
                let firma: p256::ecdsa::Signature = clave.sign(datos);
                firma.to_der().as_bytes().to_vec()
            },
        );
        std::fs::write(&dest, recose(&bytes, &cms)).expect("recoser");
        let f = &verify_signatures(dest.to_string_lossy().into_owned()).expect("verificar")[0];
        assert_eq!(f.estado, ESTADO_OK, "algoritmo: {}", f.algoritmo);
        assert!(f.digest_ok && f.covers_whole_file);
        assert!(f.algoritmo.contains("ECDSA P-256"), "{}", f.algoritmo);
        assert!(
            f.cert_subject.contains("Ada Lovelace"),
            "{}",
            f.cert_subject
        );
        std::fs::remove_file(&dest).ok();
    }

    /// Lo mismo con RSA y SHA-512: el digest del `/ByteRange` hay que
    /// calcularlo con el hash que declara la firma, no siempre con SHA-256.
    #[test]
    fn una_firma_rsa_sha512_se_comprueba_con_su_propio_hash() {
        let (dest, bytes) = pdf_firmado("firma-sha512");
        let digest = digest_del_byterange(&bytes, Hash::Sha512);
        let cred = credenciales();
        let cert = cred.cert.clone();
        let clave = cred.key.clone();
        let cms = cms_a_mano(
            &digest,
            const_oid::db::rfc5912::ID_SHA_512,
            const_oid::db::rfc5912::SHA_512_WITH_RSA_ENCRYPTION,
            vec![cert.clone()],
            &cert,
            |datos| {
                use rsa::signature::{SignatureEncoding, Signer};
                let sk = rsa::pkcs1v15::SigningKey::<sha2::Sha512>::new(clave.clone());
                sk.sign(datos).to_vec()
            },
        );
        std::fs::write(&dest, recose(&bytes, &cms)).expect("recoser");
        let f = &verify_signatures(dest.to_string_lossy().into_owned()).expect("verificar")[0];
        assert_eq!(f.estado, ESTADO_OK, "algoritmo: {}", f.algoritmo);
        assert!(f.algoritmo.contains("SHA-512"), "{}", f.algoritmo);

        // y si le tocan un byte al cuerpo, entonces sí: modificado
        let mut tocado = std::fs::read(&dest).expect("leer");
        let i = find_subslice(&tocado, b"stream").expect("un stream") + 20;
        tocado[i] ^= 0xFF;
        let otro = dest.with_extension("tocado.pdf");
        std::fs::write(&otro, &tocado).expect("escribir");
        let f = &verify_signatures(otro.to_string_lossy().into_owned()).expect("verificar")[0];
        assert_eq!(f.estado, ESTADO_MODIFICADO, "un byte cambiado sí es rojo");
        std::fs::remove_file(&dest).ok();
        std::fs::remove_file(&otro).ok();
    }

    /// El azar del sistema, para el relleno de RSA-PSS (que firma distinto
    /// cada vez).
    struct Entropia;
    impl rsa::rand_core::RngCore for Entropia {
        fn next_u32(&mut self) -> u32 {
            let mut b = [0u8; 4];
            self.fill_bytes(&mut b);
            u32::from_le_bytes(b)
        }
        fn next_u64(&mut self) -> u64 {
            let mut b = [0u8; 8];
            self.fill_bytes(&mut b);
            u64::from_le_bytes(b)
        }
        fn fill_bytes(&mut self, dest: &mut [u8]) {
            getrandom::getrandom(dest).expect("entropía del sistema");
        }
        fn try_fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), rsa::rand_core::Error> {
            self.fill_bytes(dest);
            Ok(())
        }
    }
    impl rsa::rand_core::CryptoRng for Entropia {}
    /// **Certificar** (`/DocMDP`). Firmar dice quién firmó; certificar
    /// dice además **qué se puede cambiar después sin romper la firma**, y
    /// es lo que distingue «firmado» de «esta es la versión buena».
    #[test]
    fn certificar_escribe_el_docmdp_y_solo_la_primera_firma_puede() {
        let dir = std::env::temp_dir();
        let src = dir.join("firma-certificar.pdf");
        let cert = dir.join("firma-certificar-1.pdf");
        crea_pdf(&["Pliego de condiciones"], &src);

        certify(
            &src.to_string_lossy(),
            &cert.to_string_lossy(),
            &credenciales(),
            Some("Esta es la versión buena".into()),
            &Apariencia::default(),
            2,
            &Avanzado::default(),
        )
        .expect("certificar");

        let bytes = std::fs::read(&cert).expect("leer");
        let texto = String::from_utf8_lossy(&bytes);
        assert!(
            texto.contains("/DocMDP"),
            "la firma tiene que llevar su /DocMDP"
        );
        assert!(
            texto.contains("/TransformMethod"),
            "y su método de transformación"
        );
        assert!(
            texto.contains("/Perms"),
            "y el catálogo tiene que señalarla"
        );
        assert!(texto.contains("/P 2"), "con el nivel que se pidió");

        // y sigue siendo una firma como las demás: se comprueba igual
        let firmas = verify_signatures(cert.to_string_lossy().into_owned()).expect("verificar");
        assert_eq!(firmas.len(), 1);
        assert_eq!(firmas[0].estado, "ok");
        assert!(firmas[0].covers_whole_file);
        assert_eq!(firmas[0].reason, "Esta es la versión buena");

        // certificar detrás de otra firma no vale: el /DocMDP avala el
        // documento entero y detrás hay bytes que esta firma no ha visto
        let segunda = dir.join("firma-certificar-2.pdf");
        let e = certify(
            &cert.to_string_lossy(),
            &segunda.to_string_lossy(),
            &otras_credenciales(),
            None,
            &Apariencia::default(),
            2,
            &Avanzado::default(),
        )
        .unwrap_err();
        assert!(e.contains("ya lleva una firma"), "el aviso en llano: {e}");
        assert!(!e.contains("DocMDP"), "nada de jerga: {e}");

        // y un nivel que no existe se dice antes de escribir nada
        assert!(certify(
            &src.to_string_lossy(),
            &segunda.to_string_lossy(),
            &credenciales(),
            None,
            &Apariencia::default(),
            9,
            &Avanzado::default(),
        )
        .unwrap_err()
        .contains("1, 2 o 3"));

        // firmar normal **no** escribe /DocMDP: certificar es otra cosa
        let normal = dir.join("firma-certificar-normal.pdf");
        sign(
            &src.to_string_lossy(),
            &normal.to_string_lossy(),
            &credenciales(),
            None,
            &Apariencia::default(),
            &Avanzado::default(),
        )
        .expect("firmar");
        assert!(
            !String::from_utf8_lossy(&std::fs::read(&normal).expect("leer")).contains("/DocMDP")
        );

        for f in [&src, &cert, &segunda, &normal] {
            std::fs::remove_file(f).ok();
        }
    }

    /// **Certificar tiene que verse después.** Sin esto la función es
    /// invisible en cuanto se cierra el diálogo: la banda de apertura
    /// seguiría diciendo «Firmado por Jorge» donde Acrobat dice
    /// «Certificado por Jorge · se pueden rellenar los formularios».
    #[test]
    fn verificar_dice_si_una_firma_certifica_y_con_que_nivel() {
        let dir = std::env::temp_dir();
        let src = dir.join("firma-certifica-campo.pdf");
        let normal = dir.join("firma-certifica-campo-normal.pdf");
        let cert = dir.join("firma-certifica-campo-cert.pdf");
        let encima = dir.join("firma-certifica-campo-encima.pdf");
        crea_pdf(&["Pliego de condiciones"], &src);

        // una firma normal no certifica nada
        sign(
            &src.to_string_lossy(),
            &normal.to_string_lossy(),
            &credenciales(),
            None,
            &Apariencia::default(),
            &Avanzado::default(),
        )
        .expect("firmar");
        let firmas = verify_signatures(normal.to_string_lossy().into_owned()).expect("verificar");
        assert_eq!(firmas.len(), 1);
        assert_eq!(firmas[0].certifica, None, "firmar no es certificar");

        // certificar a nivel 2 se lee tal cual
        certify(
            &src.to_string_lossy(),
            &cert.to_string_lossy(),
            &credenciales(),
            None,
            &Apariencia::default(),
            2,
            &Avanzado::default(),
        )
        .expect("certificar");
        let firmas = verify_signatures(cert.to_string_lossy().into_owned()).expect("verificar");
        assert_eq!(firmas.len(), 1);
        assert_eq!(firmas[0].certifica, Some(2));

        // y con una firma normal detrás, cada una dice lo suyo: la
        // certificación es de la primera y sigue siéndolo
        sign(
            &cert.to_string_lossy(),
            &encima.to_string_lossy(),
            &otras_credenciales(),
            None,
            &Apariencia::default(),
            &Avanzado::default(),
        )
        .expect("firmar encima");
        let firmas = verify_signatures(encima.to_string_lossy().into_owned()).expect("verificar");
        assert_eq!(firmas.len(), 2);
        assert_eq!(firmas[0].certifica, Some(2));
        assert_eq!(firmas[1].certifica, None);

        for f in [&src, &normal, &cert, &encima] {
            std::fs::remove_file(f).ok();
        }
    }

    /// **H4.** Dos personas firmando el mismo contrato, que es el caso que
    /// hasta el ciclo 5 mandaba al usuario a Acrobat: firmar encima avisaba
    /// y se plantaba, porque Vitela reescribía el fichero entero.
    ///
    /// Ahora la segunda firma va en una **actualización incremental**: los
    /// bytes de antes se quedan donde estaban y el fichero crece por el
    /// final. Las dos firmas comprueban, la primera ya no cubre el fichero
    /// entero —hay una revisión detrás, que es lo normal en un PDF firmado
    /// que sigue vivo— y sigue en `ok`, no en rojo.
    #[test]
    fn dos_personas_pueden_firmar_el_mismo_documento() {
        let dir = std::env::temp_dir();
        let src = dir.join("firma-dos-personas.pdf");
        let una = dir.join("firma-dos-personas-1.pdf");
        let dos = dir.join("firma-dos-personas-2.pdf");
        crea_pdf(&["Contrato de arrendamiento"], &src);
        sign(
            &src.to_string_lossy(),
            &una.to_string_lossy(),
            &credenciales(),
            Some("Conforme".into()),
            &Apariencia::default(),
            &Avanzado::default(),
        )
        .expect("la primera firma");
        let primera = std::fs::read(&una).expect("leer");

        // la segunda, con otro certificado y **visible**: la apariencia se
        // dibuja en la revisión nueva y la geometría de la página se lee de
        // la de atrás
        sign(
            &una.to_string_lossy(),
            &dos.to_string_lossy(),
            &otras_credenciales(),
            Some("También conforme".into()),
            &Apariencia {
                rect: Some(crate::Rect {
                    x: 60.0,
                    y: 500.0,
                    w: 180.0,
                    h: 60.0,
                }),
                page_index: Some(0),
                signer_name: Some("Ada Lovelace".into()),
                signature_png: None,
            },
            &Avanzado::default(),
        )
        .expect("la segunda firma");
        let ambas = std::fs::read(&dos).expect("leer");

        assert_eq!(
            &ambas[..primera.len()],
            &primera[..],
            "la revisión de la primera firma no se toca ni un byte"
        );

        let firmas = verify_signatures(dos.to_string_lossy().into_owned()).expect("verificar");
        assert_eq!(firmas.len(), 2, "dos firmas: {firmas:?}");
        assert_eq!(firmas[0].estado, ESTADO_OK, "la primera: {:?}", firmas[0]);
        assert!(firmas[0].digest_ok);
        assert!(
            !firmas[0].covers_whole_file,
            "la primera ya no cubre el fichero: detrás está la segunda"
        );
        assert_eq!(firmas[1].estado, ESTADO_OK, "la segunda: {:?}", firmas[1]);
        assert!(
            firmas[1].covers_whole_file,
            "la segunda sí cubre el fichero entero"
        );
        assert_ne!(
            firmas[0].cert_subject, firmas[1].cert_subject,
            "las firma dos personas distintas"
        );
        assert_eq!(firmas[0].reason, "Conforme");
        assert_eq!(firmas[1].reason, "También conforme");
        assert!(firmas[0].rect.is_none(), "la primera es invisible");
        assert!(
            firmas[1].rect.is_some(),
            "la segunda lleva su sello dibujado en la revisión nueva"
        );
        assert_eq!(firmas[1].name, "Ada Lovelace");
        // y la página se renderiza con el sello encima
        crate::render_page_png(dos.to_string_lossy().into_owned(), 0, 200, true)
            .expect("render con dos firmas");
        // y el documento se sigue pudiendo abrir y leer
        assert!(
            crate::tests::textos_de(&dos)[0].contains("Contrato de arrendamiento"),
            "el documento sigue leyéndose entero"
        );

        // y una tercera encima sigue sin romper a las dos de antes
        let tres = dir.join("firma-dos-personas-3.pdf");
        sign(
            &dos.to_string_lossy(),
            &tres.to_string_lossy(),
            &credenciales(),
            None,
            &Apariencia::default(),
            &Avanzado::default(),
        )
        .expect("la tercera firma");
        let firmas = verify_signatures(tres.to_string_lossy().into_owned()).expect("verificar");
        assert_eq!(firmas.len(), 3);
        assert!(
            firmas.iter().all(|f| f.estado == ESTADO_OK),
            "las tres siguen cuadrando: {firmas:?}"
        );

        for p in [&src, &una, &dos, &tres] {
            std::fs::remove_file(p).ok();
        }
    }

    /// **R41b (AC-064).** Con dos firmas válidas, el documento está
    /// intacto: la segunda firma no es un «cambio posterior» que la primera
    /// no avale. Acrobat dice «Firmado y todas las firmas son válidas»; la
    /// banda de Vitela decía «la firma es válida, pero hay cambios
    /// posteriores que no avala», y el cambio era la otra firma.
    ///
    /// El veredicto es del **documento** y por eso viaja en cada
    /// `FirmaInfo` con el mismo valor: la UI no tiene que deducirlo de la
    /// primera de la lista, que es justo lo que salía mal.
    #[test]
    fn con_dos_firmas_validas_el_documento_esta_intacto() {
        let dir = std::env::temp_dir();
        let src = dir.join("firma-intacto.pdf");
        let una = dir.join("firma-intacto-1.pdf");
        let dos = dir.join("firma-intacto-2.pdf");
        let tocado = dir.join("firma-intacto-tocado.pdf");
        crea_pdf(&["Contrato"], &src);
        sign(
            &src.to_string_lossy(),
            &una.to_string_lossy(),
            &credenciales(),
            None,
            &Apariencia::default(),
            &Avanzado::default(),
        )
        .expect("la primera firma");
        let firmas = verify_signatures(una.to_string_lossy().into_owned()).expect("verificar");
        assert_eq!(firmas.len(), 1);
        assert!(
            firmas[0].documento_intacto,
            "una firma sola: {:?}",
            firmas[0]
        );

        sign(
            &una.to_string_lossy(),
            &dos.to_string_lossy(),
            &otras_credenciales(),
            None,
            &Apariencia::default(),
            &Avanzado::default(),
        )
        .expect("la segunda firma");
        let firmas = verify_signatures(dos.to_string_lossy().into_owned()).expect("verificar");
        assert_eq!(firmas.len(), 2);
        assert!(
            !firmas[0].covers_whole_file,
            "la primera no cubre el fichero: detrás está la segunda"
        );
        assert!(
            firmas.iter().all(|f| f.documento_intacto),
            "las dos son válidas y nada se ha escrito después: {firmas:?}"
        );

        // y una revisión que NO es una firma sí es un cambio sin avalar
        let base = std::fs::read(&dos).expect("leer");
        let doc = LoDoc::load(&dos).expect("cargar");
        let mut inc = lopdf::IncrementalDocument::create_from(base, doc);
        let id = inc
            .new_document
            .add_object(Object::string_literal("después de firmar"));
        let root = inc
            .get_prev_documents()
            .trailer
            .get(b"Root")
            .and_then(|o| o.as_reference())
            .expect("root");
        inc.opt_clone_object_to_new_document(root).expect("clonar");
        inc.new_document
            .get_object_mut(root)
            .and_then(|o| o.as_dict_mut())
            .expect("catálogo")
            .set("Vitela", Object::Reference(id));
        let mut out = Vec::new();
        inc.save_to(&mut out).expect("guardar");
        std::fs::write(&tocado, &out).expect("escribir");

        let firmas = verify_signatures(tocado.to_string_lossy().into_owned()).expect("verificar");
        assert!(
            firmas.iter().all(|f| f.estado == ESTADO_OK),
            "las dos firmas siguen cuadrando con lo que firmaron: {firmas:?}"
        );
        assert!(
            firmas.iter().all(|f| !f.documento_intacto),
            "pero hay una revisión detrás que ninguna avala: {firmas:?}"
        );

        for p in [&src, &una, &dos, &tocado] {
            std::fs::remove_file(p).ok();
        }
    }

    /// **H4.** Un cambio **entre** las dos firmas es una revisión más, no
    /// una manipulación: la primera firma sigue cuadrando con los bytes que
    /// avaló, y decir «modificado» sería acusar en falso.
    #[test]
    fn un_cambio_entre_las_dos_firmas_no_acusa_a_la_primera() {
        let dir = std::env::temp_dir();
        let src = dir.join("firma-entre-medias.pdf");
        let una = dir.join("firma-entre-medias-1.pdf");
        let tocado = dir.join("firma-entre-medias-tocado.pdf");
        let dos = dir.join("firma-entre-medias-2.pdf");
        crea_pdf(&["Contrato"], &src);
        sign(
            &src.to_string_lossy(),
            &una.to_string_lossy(),
            &credenciales(),
            None,
            &Apariencia::default(),
            &Avanzado::default(),
        )
        .expect("la primera firma");

        // una revisión incremental por medio (lo que hace rellenar un campo)
        let primera = std::fs::read(&una).expect("leer");
        let doc = LoDoc::load(&una).expect("cargar");
        let mut inc = lopdf::IncrementalDocument::create_from(primera.clone(), doc);
        let id = inc
            .new_document
            .add_object(Object::string_literal("una revisión"));
        let root = inc
            .get_prev_documents()
            .trailer
            .get(b"Root")
            .and_then(|o| o.as_reference())
            .expect("root");
        inc.opt_clone_object_to_new_document(root).expect("clonar");
        inc.new_document
            .get_object_mut(root)
            .and_then(|o| o.as_dict_mut())
            .expect("catálogo")
            .set("Vitela", Object::Reference(id));
        let mut out = Vec::new();
        inc.save_to(&mut out).expect("guardar");
        std::fs::write(&tocado, &out).expect("escribir");

        sign(
            &tocado.to_string_lossy(),
            &dos.to_string_lossy(),
            &otras_credenciales(),
            None,
            &Apariencia::default(),
            &Avanzado::default(),
        )
        .expect("la segunda firma");

        let firmas = verify_signatures(dos.to_string_lossy().into_owned()).expect("verificar");
        assert_eq!(firmas.len(), 2);
        assert_eq!(
            firmas[0].estado, ESTADO_OK,
            "la primera sigue cuadrando con lo que firmó: {:?}",
            firmas[0]
        );
        assert_eq!(firmas[1].estado, ESTADO_OK);
        for p in [&src, &una, &tocado, &dos] {
            std::fs::remove_file(p).ok();
        }
    }

    /// El contenido del `/AP /N` del widget de firma de la página `pagina`.
    fn ap_de_la_firma(path: &str) -> String {
        let doc = LoDoc::load(path).expect("cargar");
        for (n, _) in doc.get_pages() {
            let Some(annots) = crate::anotaciones::lista_annots(&doc, n as u16 - 1) else {
                continue;
            };
            for a in annots {
                let Some(annot) = dict_de(&doc, &a) else {
                    continue;
                };
                if annot
                    .get(b"FT")
                    .and_then(|o| o.as_name())
                    .unwrap_or_default()
                    != b"Sig"
                {
                    continue;
                }
                let Ok(ap) = annot.get(b"AP").and_then(|o| o.as_dict()) else {
                    continue;
                };
                let id = ap.get(b"N").and_then(|o| o.as_reference()).expect("/AP /N");
                let stream = doc
                    .get_object(id)
                    .and_then(|o| o.as_stream())
                    .expect("stream");
                let bytes = stream
                    .decompressed_content()
                    .unwrap_or_else(|_| stream.content.clone());
                return String::from_utf8_lossy(&bytes).into_owned();
            }
        }
        panic!("el documento no tiene widget de firma con apariencia");
    }

    /// **Un certificado se enseña por su nombre** (orden 20). Quien cifra
    /// un documento para tres personas tiene que poder comprobar que son
    /// las tres personas, y el nombre del fichero no lo dice.
    #[test]
    fn un_certificado_se_lee_por_su_nombre_y_no_por_el_del_fichero() {
        let ruta = std::env::temp_dir().join("firma-ficha-cert.pem");
        std::fs::write(&ruta, include_str!("../fixtures/test_hija_cert.pem")).expect("escribir");
        let ficha = read_certificate(ruta.to_string_lossy().into_owned()).expect("leer");
        assert_eq!(ficha.nombre, "Firmante de prueba");
        assert!(!ficha.emisor.is_empty(), "y quién responde por él");
        assert!(
            ficha.not_after.starts_with("20"),
            "hasta cuándo vale, en ISO 8601: {}",
            ficha.not_after
        );

        // lo que no es un certificado se dice en llano y sin jerga
        let basura = std::env::temp_dir().join("firma-ficha-basura.pem");
        std::fs::write(&basura, b"esto no es un certificado").expect("escribir");
        let e = read_certificate(basura.to_string_lossy().into_owned()).unwrap_err();
        assert!(e.contains("no es un certificado"), "{e}");

        std::fs::remove_file(&ruta).ok();
        std::fs::remove_file(&basura).ok();
    }

    /// **AC-080.** Un token de sellado de una autoridad de verdad lleva
    /// tres certificados en el bolso y el primero suele ser la raíz, así
    /// que la tarjeta decía «DigiCert Trusted Root G4» donde Acrobat dice
    /// el nombre del respondedor que selló. Quien sella es el que señala
    /// el `SignerIdentifier`, exactamente igual que quien firma.
    #[test]
    fn el_sello_nombra_a_quien_sello_y_no_al_primero_del_bolso() {
        use der::asn1::{Any, OctetString, SetOfVec};
        use der::{Decode, DecodePem, Encode, Tag};

        let firmante =
            x509_cert::Certificate::from_pem(include_str!("../fixtures/test_hija_cert.pem"))
                .expect("el certificado que sella");
        let otros = [
            x509_cert::Certificate::from_pem(include_str!("../fixtures/test_ca_cert.pem"))
                .expect("la raíz"),
            x509_cert::Certificate::from_pem(include_str!("../fixtures/test_cert.pem"))
                .expect("otro más"),
        ];
        let alg = |oid| AlgorithmIdentifierOwned {
            oid,
            parameters: None,
        };
        let signer = cms::signed_data::SignerInfo {
            version: cms::content_info::CmsVersion::V1,
            sid: SignerIdentifier::IssuerAndSerialNumber(IssuerAndSerialNumber {
                issuer: firmante.tbs_certificate.issuer.clone(),
                serial_number: firmante.tbs_certificate.serial_number.clone(),
            }),
            digest_alg: alg(const_oid::db::rfc5912::ID_SHA_256),
            signed_attrs: None,
            signature_algorithm: alg(const_oid::db::rfc5912::SHA_256_WITH_RSA_ENCRYPTION),
            signature: OctetString::new(vec![0u8; 32]).expect("firma"),
            unsigned_attrs: None,
        };
        let mut bolso = SetOfVec::new();
        for c in std::iter::once(&firmante).chain(otros.iter()) {
            bolso
                .insert(CertificateChoices::Certificate(c.clone()))
                .expect("certificado");
        }
        let tst = crate::tsa::tst_de_prueba("20260911084500Z");
        let sd = cms::signed_data::SignedData {
            version: cms::content_info::CmsVersion::V1,
            digest_algorithms: {
                let mut v = SetOfVec::new();
                v.insert(alg(const_oid::db::rfc5912::ID_SHA_256))
                    .expect("alg");
                v
            },
            encap_content_info: EncapsulatedContentInfo {
                econtent_type: crate::tsa::OID_TSTINFO,
                econtent: Some(Any::new(Tag::OctetString, tst.as_slice()).expect("eContent")),
            },
            certificates: Some(bolso.into()),
            crls: None,
            signer_infos: cms::signed_data::SignerInfos(
                SetOfVec::from_iter([signer]).expect("signer"),
            ),
        };
        let der = sd.to_der().expect("SignedData");
        let token = cms::content_info::ContentInfo {
            content_type: const_oid::db::rfc5911::ID_SIGNED_DATA,
            content: Any::from_der(&der).expect("any"),
        }
        .to_der()
        .expect("ContentInfo");

        // el bolso no puede empezar por el que sella, o el test no probaría
        // nada (el orden lo fija el DER del conjunto, no nosotros)
        let leidos = certificados_del_bolso(&sd);
        assert_ne!(
            nombre_llano(&leidos[0].tbs_certificate.subject.to_string()),
            nombre_llano(&firmante.tbs_certificate.subject.to_string()),
            "hace falta un bolso cuyo primero no sea el que sella"
        );

        let sello = crate::tsa::lee_sello(&token).expect("leer el sello");
        assert_eq!(sello.autoridad, "Firmante de prueba");
        assert_eq!(sello.fecha, "2026-09-11T08:45:00+00:00");
    }

    /// **AC-082.** El diálogo enseña la previa «Certificado por Jorge» y en
    /// el PDF se escribía «Firmado por Jorge»: quien lo abría fuera de
    /// Vitela veía una firma normal donde había una certificación, que es
    /// justo lo que distingue «firmado» de «esta es la versión buena».
    #[test]
    fn el_sello_visible_de_una_certificacion_dice_que_certifica() {
        let dir = std::env::temp_dir();
        let src = dir.join("firma-certifica-ap-src.pdf");
        let dest = dir.join("firma-certifica-ap-out.pdf");
        crea_pdf(&["Pliego"], &src);
        let ap = Apariencia {
            rect: Some(crate::Rect {
                x: 60.0,
                y: 500.0,
                w: 240.0,
                h: 90.0,
            }),
            page_index: Some(0),
            signer_name: Some("Jorge Gómez".into()),
            signature_png: None,
        };
        certify(
            &src.to_string_lossy(),
            &dest.to_string_lossy(),
            &credenciales(),
            None,
            &ap,
            2,
            &Avanzado::default(),
        )
        .expect("certificar");
        let contenido = ap_de_la_firma(&dest.to_string_lossy());
        assert!(
            contenido.contains("Certificado por Jorge"),
            "el sello de una certificación tiene que decirlo: {contenido}"
        );
        assert!(
            !contenido.contains("Firmado por"),
            "y no puede decir además que es una firma normal: {contenido}"
        );

        // una firma normal sigue diciendo lo suyo
        let normal = dir.join("firma-certifica-ap-normal.pdf");
        sign(
            &src.to_string_lossy(),
            &normal.to_string_lossy(),
            &credenciales(),
            None,
            &ap,
            &Avanzado::default(),
        )
        .expect("firmar");
        assert!(ap_de_la_firma(&normal.to_string_lossy()).contains("Firmado por Jorge"));

        for f in [&src, &dest, &normal] {
            std::fs::remove_file(f).ok();
        }
    }

    /// El sello de firma de Acrobat pone el motivo debajo del nombre y la
    /// fecha: es lo que explica por qué se firmó, y sin él la firma visible
    /// dice menos que el panel.
    #[test]
    fn la_firma_visible_lleva_el_motivo() {
        let dir = std::env::temp_dir();
        let src = dir.join("firma-motivo-src.pdf");
        let dest = dir.join("firma-motivo-out.pdf");
        crea_pdf(&["Contrato"], &src);
        let ap = Apariencia {
            rect: Some(crate::Rect {
                x: 60.0,
                y: 500.0,
                w: 240.0,
                h: 90.0,
            }),
            page_index: Some(0),
            signer_name: Some("Jorge Gómez".into()),
            signature_png: None,
        };
        sign(
            &src.to_string_lossy(),
            &dest.to_string_lossy(),
            &credenciales(),
            Some("Conforme con el presupuesto".into()),
            &ap,
            &Avanzado::default(),
        )
        .expect("firmar con motivo");
        let contenido = ap_de_la_firma(&dest.to_string_lossy());
        assert!(
            contenido.contains("Firmado por Jorge"),
            "falta el nombre: {contenido}"
        );
        assert!(
            contenido.contains("Motivo: Conforme con el presupuesto"),
            "el motivo no está en la apariencia: {contenido}"
        );

        // sin motivo, la apariencia no inventa una línea vacía
        let sin = dir.join("firma-sin-motivo-out.pdf");
        sign(
            &src.to_string_lossy(),
            &sin.to_string_lossy(),
            &credenciales(),
            None,
            &ap,
            &Avanzado::default(),
        )
        .expect("firmar sin motivo");
        assert!(
            !ap_de_la_firma(&sin.to_string_lossy()).contains("Motivo"),
            "sin motivo no se escribe la línea"
        );
        for p in [&src, &dest, &sin] {
            std::fs::remove_file(p).ok();
        }
    }

    /// RSA-PSS es el otro relleno que se ve en las firmas modernas: se
    /// comprueba igual, no se da por manipulada.
    #[test]
    fn una_firma_rsa_pss_se_comprueba() {
        let (dest, bytes) = pdf_firmado("firma-pss");
        let cred = credenciales();
        let cert = cred.cert.clone();
        let clave = cred.key.clone();
        let cms = cms_a_mano(
            &digest_del_byterange(&bytes, Hash::Sha256),
            const_oid::db::rfc5912::ID_SHA_256,
            const_oid::db::rfc5912::ID_RSASSA_PSS,
            vec![cert.clone()],
            &cert,
            |datos| {
                use rsa::signature::{RandomizedSigner, SignatureEncoding};
                let sk = rsa::pss::SigningKey::<Sha256>::new(clave.clone());
                sk.sign_with_rng(&mut Entropia, datos).to_vec()
            },
        );
        std::fs::write(&dest, recose(&bytes, &cms)).expect("recoser");
        let f = &verify_signatures(dest.to_string_lossy().into_owned()).expect("verificar")[0];
        assert_eq!(f.estado, ESTADO_OK, "algoritmo: {}", f.algoritmo);
        assert!(f.algoritmo.contains("RSA-PSS"), "{}", f.algoritmo);
        std::fs::remove_file(&dest).ok();
    }

    /// Un algoritmo que Vitela no sabe leer no es una manipulación: sale
    /// «desconocido», que la UI enseña en neutro. Un falso «el documento ha
    /// cambiado» sobre un contrato firmado es un error caro.
    #[test]
    fn un_algoritmo_que_no_se_conoce_no_acusa_de_manipulacion() {
        let (dest, bytes) = pdf_firmado("firma-rara");
        let inventado = const_oid::ObjectIdentifier::new_unwrap("1.2.3.4.5.6.7.8");
        let cred = credenciales();
        let cert = cred.cert.clone();
        let clave = cred.key.clone();
        let cms = cms_a_mano(
            &digest_del_byterange(&bytes, Hash::Sha256),
            inventado,
            inventado,
            vec![cert.clone()],
            &cert,
            |datos| {
                use rsa::signature::{SignatureEncoding, Signer};
                let sk = rsa::pkcs1v15::SigningKey::<Sha256>::new(clave.clone());
                sk.sign(datos).to_vec()
            },
        );
        std::fs::write(&dest, recose(&bytes, &cms)).expect("recoser");
        let f = &verify_signatures(dest.to_string_lossy().into_owned()).expect("verificar")[0];
        assert_eq!(f.estado, ESTADO_DESCONOCIDO, "algoritmo: {}", f.algoritmo);
        assert!(!f.digest_ok, "no se ha comprobado nada");
        assert!(f.algoritmo.contains("no reconocido"), "{}", f.algoritmo);
        std::fs::remove_file(&dest).ok();
    }

    /// En una firma cualificada viaja la cadena entera y la CA suele ir la
    /// primera del bolso: el firmante es el que señala el
    /// `SignerIdentifier`, no el primero que se encuentre. Cogiendo el
    /// primero, la tarjeta enseñaba la autoridad en lugar de la persona y
    /// la firma salía por inválida.
    #[test]
    fn el_certificado_del_firmante_no_es_el_primero_del_bolso() {
        let (dest, bytes) = pdf_firmado("firma-cadena");
        let cred = credenciales();
        let cert = cred.cert.clone();
        let clave = cred.key.clone();
        let cms = cms_a_mano(
            &digest_del_byterange(&bytes, Hash::Sha256),
            const_oid::db::rfc5912::ID_SHA_256,
            const_oid::db::rfc5912::SHA_256_WITH_RSA_ENCRYPTION,
            // el certificado de otro (aquí el EC) va DELANTE del firmante
            vec![cert_ec(), cert.clone()],
            &cert,
            |datos| {
                use rsa::signature::{SignatureEncoding, Signer};
                let sk = rsa::pkcs1v15::SigningKey::<Sha256>::new(clave.clone());
                sk.sign(datos).to_vec()
            },
        );
        std::fs::write(&dest, recose(&bytes, &cms)).expect("recoser");
        let f = &verify_signatures(dest.to_string_lossy().into_owned()).expect("verificar")[0];
        assert_eq!(f.estado, ESTADO_OK, "algoritmo: {}", f.algoritmo);
        assert!(
            !f.cert_subject.contains("Ada Lovelace"),
            "la tarjeta enseña el certificado equivocado: {}",
            f.cert_subject
        );
        std::fs::remove_file(&dest).ok();
    }
}
