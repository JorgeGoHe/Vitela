//! **Comprobar en línea si el certificado sigue vigente** (OCSP, RFC 6960),
//! y archivar la prueba dentro del documento.
//!
//! Dos decisiones, y las dos son de postura del proyecto:
//!
//! - **Se pregunta al firmar, nunca al abrir.** Vitela no llama a nadie
//!   cuando abres un documento, y eso no cambia. La respuesta del
//!   respondedor se guarda en el `/DSS /OCSPs` del catálogo, así que dentro
//!   de años se puede seguir enseñando quién dijo qué y cuándo **sin salir
//!   a la red**.
//! - **Si no contesta, se firma igual.** La revocación es una prueba que
//!   acompaña a la firma, no una condición para hacerla: tirar la firma
//!   porque un servidor de un tercero está caído sería lo peor que podría
//!   pasar después de elegir dónde guardar. Se dice en el aviso y ya está.
//!
//! Como el sello de tiempo, esto va **sin crates nuevas**: el DER se
//! escribe y se lee a mano, el POST lo pone `tsa::post` y la petición solo
//! lleva hashes. SHA-1 no es una elección: es el `CertID` del RFC, que es
//! lo único que casan los respondedores de verdad.

use crate::tsa::{cabecera, tlv};

/// `id-ad-ocsp`, el método de acceso que dice dónde preguntar: la extensión
/// «Authority Information Access» del certificado.
const OID_AD_OCSP: &[u8] = &[0x2B, 0x06, 0x01, 0x05, 0x05, 0x07, 0x30, 0x01];

/// `id-pe-authorityInfoAccess`, 1.3.6.1.5.5.7.1.1.
const OID_AIA: const_oid::ObjectIdentifier =
    const_oid::ObjectIdentifier::new_unwrap("1.3.6.1.5.5.7.1.1");

/// Cómo se llama en llano lo que hay al otro lado, para los avisos.
pub(crate) const SERVICIO: &str = "servidor de comprobación del certificado";

/// Los TLV que hay dentro de un contenedor DER.
fn hijos(der: &[u8]) -> Vec<(u8, &[u8])> {
    let mut out = Vec::new();
    let Some((_, ini, largo)) = cabecera(der) else {
        return out;
    };
    let Some(mut resto) = der.get(ini..ini + largo) else {
        return out;
    };
    while !resto.is_empty() {
        let Some((etiqueta, i, l)) = cabecera(resto) else {
            break;
        };
        let Some(contenido) = resto.get(i..i + l) else {
            break;
        };
        out.push((etiqueta, contenido));
        let Some(siguiente) = resto.get(i + l..) else {
            break;
        };
        resto = siguiente;
    }
    out
}

/// **Dónde preguntar por este certificado**: la URL del respondedor OCSP
/// que el propio certificado lleva escrita en su AIA. Sin ella no hay a
/// quién preguntar y no se pregunta —muchos certificados de prueba y
/// algunos de verdad no la traen—.
pub(crate) fn url_de(cert: &x509_cert::Certificate) -> Option<String> {
    let extensiones = cert.tbs_certificate.extensions.as_ref()?;
    let aia = extensiones.iter().find(|e| e.extn_id == OID_AIA)?;
    // AuthorityInfoAccessSyntax ::= SEQUENCE OF AccessDescription
    for (_, descripcion) in hijos(aia.extn_value.as_bytes()) {
        let mut metodo: Option<&[u8]> = None;
        let mut sitio: Option<&[u8]> = None;
        let envuelto = tlv(0x30, descripcion);
        for (etiqueta, contenido) in hijos(&envuelto) {
            match etiqueta {
                0x06 => metodo = Some(contenido),
                // uniformResourceIdentifier: [6] IMPLICIT IA5String
                0x86 => sitio = Some(contenido),
                _ => {}
            }
        }
        if metodo == Some(OID_AD_OCSP) {
            if let Some(url) = sitio.and_then(|b| std::str::from_utf8(b).ok()) {
                return Some(url.to_string());
            }
        }
    }
    None
}

/// El `CertID` del RFC 6960: qué certificado se pregunta, dicho con
/// hashes del emisor y el número de serie. El emisor hace falta entero:
/// se hashean su nombre y su clave pública, no el certificado.
fn cert_id(
    cert: &x509_cert::Certificate,
    emisor: &x509_cert::Certificate,
) -> Result<Vec<u8>, String> {
    use der::Encode;
    use sha1::{Digest, Sha1};
    let nombre = emisor
        .tbs_certificate
        .subject
        .to_der()
        .map_err(|_| "No se ha podido leer el emisor del certificado".to_string())?;
    let clave = emisor
        .tbs_certificate
        .subject_public_key_info
        .subject_public_key
        .raw_bytes();
    let serie = cert
        .tbs_certificate
        .serial_number
        .to_der()
        .map_err(|_| "No se ha podido leer el número de serie del certificado".to_string())?;
    // AlgorithmIdentifier de SHA-1, con su NULL: 1.3.14.3.2.26
    let sha1_alg = tlv(
        0x30,
        &[tlv(0x06, &[0x2B, 0x0E, 0x03, 0x02, 0x1A]), tlv(0x05, &[])].concat(),
    );
    Ok(tlv(
        0x30,
        &[
            sha1_alg,
            tlv(0x04, &Sha1::digest(&nombre)),
            tlv(0x04, &Sha1::digest(clave)),
            serie,
        ]
        .concat(),
    ))
}

/// La petición entera: `OCSPRequest ::= SEQUENCE { tbsRequest }`, con una
/// sola pregunta dentro. Sin nonce a propósito: hay respondedores que
/// devuelven respuestas precocinadas y rechazan la petición con nonce, y
/// aquí la prueba se archiva, no se usa para decidir nada en el momento.
fn peticion(cert_id: &[u8]) -> Vec<u8> {
    let request = tlv(0x30, cert_id);
    let lista = tlv(0x30, &request);
    let tbs = tlv(0x30, &lista);
    tlv(0x30, &tbs)
}

/// **Pregunta al respondedor** y devuelve su respuesta en DER, tal cual,
/// para archivarla. Una respuesta que no sea «concedida» se cuenta en
/// llano: archivar un error no es archivar una prueba.
pub(crate) fn pide(
    url: &str,
    cert: &x509_cert::Certificate,
    emisor: &x509_cert::Certificate,
) -> Result<Vec<u8>, String> {
    let cuerpo = peticion(&cert_id(cert, emisor)?);
    let respuesta = crate::tsa::post(url, "application/ocsp-request", &cuerpo, SERVICIO)?;
    match estado(&respuesta) {
        Some(0) => Ok(respuesta),
        Some(6) => Err(format!(
            "El {SERVICIO} no responde por este certificado: la prueba de que sigue \
             vigente no se puede archivar"
        )),
        _ => Err(format!(
            "El {SERVICIO} ha contestado algo que Vitela no sabe leer: el documento se \
             firma igual, pero sin esa prueba dentro"
        )),
    }
}

/// El `responseStatus` de la respuesta (0 = concedida).
fn estado(respuesta: &[u8]) -> Option<u8> {
    hijos(respuesta)
        .iter()
        .find(|(etiqueta, _)| *etiqueta == 0x0A)
        .and_then(|(_, v)| v.first().copied())
}

/// **Cuándo se hizo la comprobación** (`producedAt`), en ISO 8601. Es la
/// fecha que enseña la tarjeta de la firma, y sale del documento: leerla
/// no toca la red.
pub(crate) fn fecha_de(respuesta: &[u8]) -> Option<String> {
    // OCSPResponse → [0] responseBytes → SEQUENCE { responseType, response }
    let bytes = hijos(respuesta)
        .into_iter()
        .find(|(etiqueta, _)| *etiqueta == 0xA0)
        .map(|(_, v)| v.to_vec())?;
    let dentro = tlv(0xA0, &bytes);
    let basica = hijos(&dentro)
        .into_iter()
        .find(|(etiqueta, _)| *etiqueta == 0x30)
        .map(|(_, v)| tlv(0x30, v))?;
    let basica_der = hijos(&basica)
        .into_iter()
        .find(|(etiqueta, _)| *etiqueta == 0x04)
        .map(|(_, v)| v.to_vec())?;
    // BasicOCSPResponse → tbsResponseData → producedAt
    let datos = hijos(&basica_der)
        .into_iter()
        .find(|(etiqueta, _)| *etiqueta == 0x30)
        .map(|(_, v)| tlv(0x30, v))?;
    hijos(&datos)
        .into_iter()
        .find(|(etiqueta, _)| *etiqueta == 0x18)
        .and_then(|(_, v)| std::str::from_utf8(v).ok())
        .and_then(crate::tsa::iso_de_generalized)
}

#[cfg(test)]
pub(crate) mod prueba {
    use super::*;

    /// Una `OCSPResponse` de mentira con estado «concedida», un
    /// `BasicOCSPResponse` dentro y la hora que se pida, para el
    /// respondedor de los tests. Se escribe a mano, como el `TSTInfo` del
    /// sello: lo que prueba el test es que Vitela sabe pedirla, guardarla
    /// dentro del PDF y volver a leerle la fecha.
    pub(crate) fn respuesta_de_prueba(cuando: &str) -> Vec<u8> {
        // ResponseData ::= SEQUENCE { [1] responderID, producedAt, responses }
        let responder = tlv(0xA1, &tlv(0x30, &[]));
        let producido = tlv(0x18, cuando.as_bytes());
        let respuestas = tlv(0x30, &[]);
        let datos = tlv(0x30, &[responder, producido, respuestas].concat());
        let alg = tlv(
            0x30,
            &[tlv(0x06, &[0x2B, 0x0E, 0x03, 0x02, 0x1A]), tlv(0x05, &[])].concat(),
        );
        let firma = tlv(0x03, &[0x00, 0x01, 0x02, 0x03]);
        let basica = tlv(0x30, &[datos, alg, firma].concat());
        // responseBytes ::= [0] SEQUENCE { responseType, response }
        let tipo = tlv(
            0x06,
            &[0x2B, 0x06, 0x01, 0x05, 0x05, 0x07, 0x30, 0x01, 0x01],
        );
        let bytes = tlv(0xA0, &tlv(0x30, &[tipo, tlv(0x04, &basica)].concat()));
        tlv(0x30, &[tlv(0x0A, &[0x00]), bytes].concat())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **Dónde preguntar sale del propio certificado** (AIA). Un
    /// certificado que no lo dice no se pregunta a ningún sitio: no hay
    /// dirección por defecto que valga.
    #[test]
    fn la_direccion_del_respondedor_sale_del_certificado() {
        use der::DecodePem;
        let con = x509_cert::Certificate::from_pem(include_str!("../fixtures/test_ocsp_cert.pem"))
            .expect("certificado con AIA");
        assert_eq!(
            url_de(&con).as_deref(),
            Some("http://127.0.0.1:41960/ocsp"),
            "la URL del respondedor va escrita en el certificado"
        );
        let sin = x509_cert::Certificate::from_pem(include_str!("../fixtures/test_cert.pem"))
            .expect("certificado sin AIA");
        assert_eq!(url_de(&sin), None, "sin AIA no hay a quién preguntar");
    }

    /// La fecha de la prueba se lee de la respuesta archivada, que es lo
    /// que enseña la tarjeta: **sin salir a la red**.
    #[test]
    fn la_fecha_de_la_prueba_sale_de_la_respuesta_guardada() {
        let der = prueba::respuesta_de_prueba("20260910194012Z");
        assert_eq!(estado(&der), Some(0), "la respuesta va como concedida");
        assert_eq!(fecha_de(&der).as_deref(), Some("2026-09-10T19:40:12+00:00"));
    }
}
