//! **Sello de tiempo de una autoridad** (RFC 3161), que es lo que
//! convierte «firmado el 10 de septiembre» en un hecho.
//!
//! La fecha de una firma sin sello es **la del reloj del que firmó**: se
//! escribe en el `/M` del diccionario y en el atributo `signingTime` del
//! CMS, y cualquiera puede poner el reloj donde quiera antes de firmar. Un
//! sello de tiempo es una firma de un tercero sobre el hash de la nuestra,
//! diciendo a qué hora la vio.
//!
//! **Sin dependencias nuevas.** El protocolo va sobre HTTP y el token que
//! devuelve la autoridad **está firmado**: TLS no le añadiría integridad
//! ninguna y la petición solo lleva un hash, así que no hay nada privado
//! que proteger. Traerse `rustls` costaría `ring` o `aws-lc-rs`, que en
//! `x86_64-msvc` piden NASM en el runner de Windows —el mismo motivo por
//! el que `confianza.rs` descartó `webpki`—, y el cliente HTTP que hace
//! falta aquí cabe en cuarenta líneas de `std::net`.
//!
//! El DER se escribe y se lee a mano por lo mismo: una `TimeStampReq` son
//! cuatro campos.

use serde::Serialize;

/// El sello de una autoridad de tiempo, tal como se enseña: «Hora sellada
/// por FreeTSA el 10/09/2026 19:40».
#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct SelloDeTiempo {
    /// Cuándo lo selló la autoridad, en ISO 8601.
    pub fecha: String,
    /// Quién responde por esa hora, en llano (el `CN` de su certificado).
    pub autoridad: String,
}

/// Cuánto se espera a la autoridad. Firmar no puede quedarse colgado: si
/// no contesta, se firma sin sello y se dice.
const ESPERA: std::time::Duration = std::time::Duration::from_secs(12);

/// El OID del atributo no firmado que lleva el token:
/// `id-aa-timeStampToken`, 1.2.840.113549.1.9.16.2.14.
pub(crate) const OID_TOKEN: const_oid::ObjectIdentifier =
    const_oid::ObjectIdentifier::new_unwrap("1.2.840.113549.1.9.16.2.14");

/// Un TLV en DER: etiqueta, longitud (en forma larga si hace falta) y
/// contenido.
pub(crate) fn tlv(etiqueta: u8, contenido: &[u8]) -> Vec<u8> {
    let mut out = vec![etiqueta];
    let n = contenido.len();
    if n < 0x80 {
        out.push(n as u8);
    } else {
        let bytes = n.to_be_bytes();
        let primero = bytes
            .iter()
            .position(|b| *b != 0)
            .unwrap_or(bytes.len() - 1);
        let significativos = &bytes[primero..];
        out.push(0x80 | significativos.len() as u8);
        out.extend_from_slice(significativos);
    }
    out.extend_from_slice(contenido);
    out
}

/// Un entero DER sin signo (con el 0x00 delante si el bit alto está
/// puesto, que si no sería negativo).
pub(crate) fn entero(n: u64) -> Vec<u8> {
    let bytes = n.to_be_bytes();
    let primero = bytes
        .iter()
        .position(|b| *b != 0)
        .unwrap_or(bytes.len() - 1);
    let mut v = bytes[primero..].to_vec();
    if v[0] & 0x80 != 0 {
        v.insert(0, 0);
    }
    tlv(0x02, &v)
}

/// La petición de sello sobre el hash dado (`TimeStampReq` del RFC 3161,
/// con SHA-256 y pidiendo el certificado de la autoridad para poder decir
/// quién ha sellado).
fn peticion(hash: &[u8], nonce: u64) -> Vec<u8> {
    // AlgorithmIdentifier de SHA-256, con su NULL
    let sha256 = tlv(
        0x30,
        &[
            tlv(
                0x06,
                &[0x60, 0x86, 0x48, 0x01, 0x65, 0x03, 0x04, 0x02, 0x01],
            ),
            tlv(0x05, &[]),
        ]
        .concat(),
    );
    let imprint = tlv(0x30, &[sha256, tlv(0x04, hash)].concat());
    tlv(
        0x30,
        &[
            entero(1),          // version
            imprint,            // messageImprint
            entero(nonce),      // nonce
            tlv(0x01, &[0xFF]), // certReq: TRUE
        ]
        .concat(),
    )
}

/// Cabecera de un TLV: (etiqueta, dónde empieza el contenido, cuánto mide).
pub(crate) fn cabecera(der: &[u8]) -> Option<(u8, usize, usize)> {
    let etiqueta = *der.first()?;
    let primera = *der.get(1)? as usize;
    if primera < 0x80 {
        return Some((etiqueta, 2, primera));
    }
    let n = primera & 0x7F;
    if n == 0 || n > 4 {
        return None;
    }
    let mut largo = 0usize;
    for i in 0..n {
        largo = (largo << 8) | *der.get(2 + i)? as usize;
    }
    Some((etiqueta, 2 + n, largo))
}

/// Saca el token de la respuesta de la autoridad (`TimeStampResp`): el
/// estado y, detrás, el `ContentInfo` con el sello. Un estado que no sea
/// «concedido» se cuenta en llano.
fn token_de(respuesta: &[u8]) -> Result<Vec<u8>, String> {
    let (_, ini, largo) =
        cabecera(respuesta).ok_or("El servidor de tiempo ha contestado algo que no es un sello")?;
    let dentro = respuesta
        .get(ini..ini + largo)
        .ok_or("La respuesta del servidor de tiempo viene cortada")?;
    let (_, si, slargo) =
        cabecera(dentro).ok_or("El servidor de tiempo ha contestado algo que no es un sello")?;
    let estado = dentro
        .get(si..si + slargo)
        .and_then(|s| cabecera(s).and_then(|(_, i, l)| s.get(i..i + l)))
        .and_then(|v| v.first().copied())
        .unwrap_or(9);
    // 0 = concedido, 1 = concedido con cambios; el resto es una negativa
    if estado > 1 {
        return Err(format!(
            "El servidor de tiempo no ha dado la hora (código {estado})"
        ));
    }
    let resto = dentro
        .get(si + slargo..)
        .filter(|r| !r.is_empty())
        .ok_or("El servidor de tiempo no ha devuelto ningún sello de tiempo")?;
    let (_, ti, tlargo) =
        cabecera(resto).ok_or("El sello de tiempo que ha devuelto el servidor es ilegible")?;
    Ok(resto
        .get(..ti + tlargo)
        .ok_or("El sello de tiempo que ha devuelto el servidor viene cortado")?
        .to_vec())
}

/// Cómo se llama en llano el servidor de sellado en todos los avisos.
const SERVICIO: &str = "servidor de tiempo";

/// Pide el sello a la autoridad. Devuelve el `ContentInfo` del token, tal
/// cual, para meterlo como atributo no firmado del CMS.
pub(crate) fn pide_token(url: &str, firma: &[u8]) -> Result<Vec<u8>, String> {
    use sha2::{Digest, Sha256};
    let hash = Sha256::digest(firma);
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(1);
    let cuerpo = peticion(&hash, nonce);
    let respuesta = post(url, "application/timestamp-query", &cuerpo, SERVICIO)?;
    token_de(&respuesta)
}

/// Lee del token quién ha sellado y cuándo, para poder enseñarlo.
pub(crate) fn lee_sello(token: &[u8]) -> Option<SelloDeTiempo> {
    use der::{Encode, Reader};
    let ci: cms::content_info::ContentInfo = {
        let mut reader = der::SliceReader::new(token).ok()?;
        reader.decode().ok()?
    };
    let sd: cms::signed_data::SignedData = ci.content.decode_as().ok()?;
    // el TSTInfo va dentro del eContent, envuelto en un OCTET STRING
    let econtent = sd.encap_content_info.econtent.as_ref()?.to_der().ok()?;
    let (_, ini, largo) = cabecera(&econtent)?;
    let tst = econtent.get(ini..ini + largo)?;
    let fecha = hora_del_tst(tst)?;
    // **quién selló es el que señala el `SignerIdentifier`, no el primero
    // del bolso** (AC-080): un token de una autoridad de verdad lleva tres
    // certificados y el primero suele ser la raíz, así que la tarjeta
    // decía «DigiCert Trusted Root G4» donde Acrobat dice el nombre del
    // respondedor. Es el mismo error que el ciclo 4 corrigió para el
    // certificado del firmante, sin aplicar aquí
    let bolso = crate::firma::certificados_del_bolso(&sd);
    let autoridad = sd
        .signer_infos
        .0
        .iter()
        .next()
        .and_then(|si| crate::firma::certificado_del_firmante(&bolso, &si.sid))
        .map(|(cert, _)| crate::firma::nombre_llano(&cert.tbs_certificate.subject.to_string()))
        .unwrap_or_default();
    Some(SelloDeTiempo { fecha, autoridad })
}

/// La `genTime` del `TSTInfo`: el primer `GeneralizedTime` de la
/// secuencia. Se devuelve en ISO 8601, como el resto de fechas que salen
/// de aquí.
fn hora_del_tst(tst: &[u8]) -> Option<String> {
    let (_, ini, largo) = cabecera(tst)?;
    let mut resto = tst.get(ini..ini + largo)?;
    while !resto.is_empty() {
        let (etiqueta, i, l) = cabecera(resto)?;
        if etiqueta == 0x18 {
            let texto = std::str::from_utf8(resto.get(i..i + l)?).ok()?;
            return iso_de_generalized(texto);
        }
        resto = resto.get(i + l..)?;
    }
    None
}

/// `20260910194012Z` → `2026-09-10T19:40:12+00:00`.
pub(crate) fn iso_de_generalized(t: &str) -> Option<String> {
    let t = t.trim_end_matches('Z');
    let n = |a: usize, b: usize| t.get(a..b)?.parse::<u32>().ok();
    let fecha = chrono::NaiveDate::from_ymd_opt(n(0, 4)? as i32, n(4, 6)?, n(6, 8)?)?;
    let hora = chrono::NaiveTime::from_hms_opt(n(8, 10)?, n(10, 12)?, n(12, 14).unwrap_or(0))?;
    Some(
        chrono::DateTime::<chrono::Utc>::from_naive_utc_and_offset(
            fecha.and_time(hora),
            chrono::Utc,
        )
        .to_rfc3339(),
    )
}

/// Un POST de HTTP 1.1 con `std::net`, que es todo lo que necesita el
/// RFC 3161 —y también el RFC 6960, que es por lo que lo comparte el OCSP—.
/// Solo `http://`: ver el comentario de arriba.
///
/// `servicio` es cómo se llama en llano lo que hay al otro lado («servidor
/// de tiempo», «servidor de comprobación del certificado»): sale en todos
/// los avisos y quien los lee no sabe qué es un respondedor.
pub(crate) fn post(
    url: &str,
    tipo: &str,
    cuerpo: &[u8],
    servicio: &str,
) -> Result<Vec<u8>, String> {
    use std::io::{Read, Write};
    let sin_esquema = url.strip_prefix("http://").ok_or_else(|| {
        if url.starts_with("https://") {
            format!(
                "Esa dirección del {servicio} va por https y Vitela habla con él por http, que \
                 es como está pensado el protocolo: la respuesta viene firmada. Prueba con la \
                 dirección http:// del mismo servicio"
            )
        } else {
            format!("«{url}» no es la dirección de un {servicio}")
        }
    })?;
    let (autoridad, ruta) = match sin_esquema.find('/') {
        Some(i) => (&sin_esquema[..i], &sin_esquema[i..]),
        None => (sin_esquema, "/"),
    };
    let con_puerto = if autoridad.contains(':') {
        autoridad.to_string()
    } else {
        format!("{autoridad}:80")
    };
    let destino = std::net::ToSocketAddrs::to_socket_addrs(&con_puerto)
        .map_err(|_| format!("No se encuentra el {servicio} «{autoridad}»"))?
        .next()
        .ok_or_else(|| format!("No se encuentra el {servicio} «{autoridad}»"))?;
    let mut conexion = std::net::TcpStream::connect_timeout(&destino, ESPERA)
        .map_err(|_| format!("No se ha podido hablar con el {servicio} «{autoridad}»"))?;
    conexion.set_read_timeout(Some(ESPERA)).ok();
    conexion.set_write_timeout(Some(ESPERA)).ok();
    let cabeceras = format!(
        "POST {ruta} HTTP/1.1\r\nHost: {autoridad}\r\nContent-Type: {tipo}\r\n\
         Content-Length: {}\r\nConnection: close\r\nAccept: */*\r\n\r\n",
        cuerpo.len()
    );
    conexion
        .write_all(cabeceras.as_bytes())
        .and_then(|_| conexion.write_all(cuerpo))
        .map_err(|_| format!("No se ha podido hablar con el {servicio} «{autoridad}»"))?;
    // se lee hasta que el servidor cierra. Un corte por su parte **con la
    // respuesta ya entera** no es un fallo: hay servidores que cierran a
    // lo bruto en cuanto han escrito, y tirar lo que ya ha llegado sería
    // quedarse sin sello por educación
    let mut respuesta = Vec::new();
    let mut trozo = [0u8; 8192];
    loop {
        match conexion.read(&mut trozo) {
            Ok(0) => break,
            Ok(n) => respuesta.extend_from_slice(&trozo[..n]),
            Err(_) if !respuesta.is_empty() => break,
            Err(_) => {
                return Err(format!(
                    "El {servicio} «{autoridad}» no ha contestado a tiempo"
                ))
            }
        }
    }
    let corte = buscar(&respuesta, b"\r\n\r\n")
        .ok_or_else(|| format!("El {servicio} «{autoridad}» ha contestado algo que no es HTTP"))?;
    let cabecera = String::from_utf8_lossy(&respuesta[..corte]).to_string();
    let primera = cabecera.lines().next().unwrap_or_default();
    if !primera.contains(" 200") {
        return Err(format!("El {servicio} ha contestado «{}»", primera.trim()));
    }
    let cuerpo = respuesta[corte + 4..].to_vec();
    if cabecera
        .to_lowercase()
        .contains("transfer-encoding: chunked")
    {
        return Ok(destroza_chunks(&cuerpo));
    }
    Ok(cuerpo)
}

/// Deshace el `chunked` de HTTP 1.1, que algún servidor de sellado usa.
fn destroza_chunks(cuerpo: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < cuerpo.len() {
        let Some(fin) = buscar(&cuerpo[i..], b"\r\n") else {
            break;
        };
        let cabeza = String::from_utf8_lossy(&cuerpo[i..i + fin]).to_string();
        let largo =
            usize::from_str_radix(cabeza.split(';').next().unwrap_or("").trim(), 16).unwrap_or(0);
        i += fin + 2;
        if largo == 0 || i + largo > cuerpo.len() {
            break;
        }
        out.extend_from_slice(&cuerpo[i..i + largo]);
        i += largo + 2;
    }
    out
}

fn buscar(donde: &[u8], que: &[u8]) -> Option<usize> {
    donde.windows(que.len()).position(|v| v == que)
}

/// El OID del contenido de un token: `id-ct-TSTInfo`.
#[cfg(test)]
pub(crate) const OID_TSTINFO: const_oid::ObjectIdentifier =
    const_oid::ObjectIdentifier::new_unwrap("1.2.840.113549.1.9.16.1.4");

/// Un `TSTInfo` con la hora que se pida, para el servidor de tiempo de
/// mentira de los tests: versión, política, imprint, serie y `genTime`.
#[cfg(test)]
pub(crate) fn tst_de_prueba(cuando: &str) -> Vec<u8> {
    let sha256 = tlv(
        0x30,
        &[
            tlv(
                0x06,
                &[0x60, 0x86, 0x48, 0x01, 0x65, 0x03, 0x04, 0x02, 0x01],
            ),
            tlv(0x05, &[]),
        ]
        .concat(),
    );
    let imprint = tlv(0x30, &[sha256, tlv(0x04, &[0u8; 32])].concat());
    tlv(
        0x30,
        &[
            entero(1),
            tlv(0x06, &[0x2A, 0x03, 0x04, 0x05]), // una política cualquiera
            imprint,
            entero(7),
            tlv(0x18, cuando.as_bytes()),
        ]
        .concat(),
    )
}

/// Una `TimeStampResp` con estado «concedido» y el token dentro, para el
/// servidor de mentira de los tests.
#[cfg(test)]
pub(crate) fn respuesta_de_prueba(token: &[u8]) -> Vec<u8> {
    tlv(0x30, &[tlv(0x30, &entero(0)), token.to_vec()].concat())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// La petición que se manda a la autoridad es la del RFC 3161: su
    /// versión, el hash con su algoritmo y la casilla de pedir el
    /// certificado, que es lo que luego deja decir **quién** ha sellado.
    #[test]
    fn la_peticion_de_sello_es_la_del_rfc_3161() {
        let hash = [7u8; 32];
        let der = peticion(&hash, 42);
        assert_eq!(der[0], 0x30, "una SEQUENCE");
        let (_, ini, largo) = cabecera(&der).expect("cabecera");
        assert_eq!(ini + largo, der.len(), "la longitud tiene que cuadrar");
        // el OID de SHA-256 y el hash van dentro
        assert!(
            buscar(
                &der,
                &[0x60, 0x86, 0x48, 0x01, 0x65, 0x03, 0x04, 0x02, 0x01]
            )
            .is_some(),
            "falta el OID de SHA-256"
        );
        assert!(buscar(&der, &hash).is_some(), "falta el hash");
        assert!(buscar(&der, &[0x01, 0x01, 0xFF]).is_some(), "falta certReq");
        // el nonce viaja: es lo que impide que alguien reenvíe un sello
        // viejo como si fuera la respuesta a esta petición
        assert!(buscar(&der, &[0x02, 0x01, 42]).is_some(), "falta el nonce");
    }

    /// Una autoridad que dice que no —o que contesta cualquier cosa— no
    /// puede tirar la firma: se cuenta en llano y se firma sin sello.
    #[test]
    fn una_negativa_de_la_autoridad_se_cuenta_en_llano() {
        // TimeStampResp con estado 2 (rechazado) y sin token
        let estado = tlv(0x30, &entero(2));
        let respuesta = tlv(0x30, &estado);
        let e = token_de(&respuesta).unwrap_err();
        assert!(e.contains("no ha dado la hora"), "{e}");
        assert!(!e.contains("PKIStatus"), "nada de jerga: {e}");

        // concedido pero sin token
        let respuesta = tlv(0x30, &tlv(0x30, &entero(0)));
        assert!(token_de(&respuesta)
            .unwrap_err()
            .contains("no ha devuelto ningún sello"));

        // y basura
        assert!(token_de(b"lo que sea").is_err());
    }

    /// El token se devuelve **tal cual**, que es lo que hay que meter en el
    /// CMS: un `ContentInfo` entero, con su cabecera.
    #[test]
    fn el_token_sale_entero_de_la_respuesta() {
        let token = tlv(0x30, &[tlv(0x06, &[42]), tlv(0x04, &[1, 2, 3])].concat());
        let respuesta = tlv(0x30, &[tlv(0x30, &entero(0)), token.clone()].concat());
        assert_eq!(token_de(&respuesta).expect("token"), token);
    }

    /// Sin red, la firma sale sin sello y el aviso lo dice: nombra al
    /// servidor y no enseña jerga de sockets.
    #[test]
    fn sin_red_se_dice_en_llano_y_no_se_cuelga() {
        // el 9 no tiene nada escuchando en localhost
        let e = pide_token("http://127.0.0.1:9/tsr", b"firma").unwrap_err();
        assert!(
            e.contains("127.0.0.1:9"),
            "tiene que decir con quién no ha podido: {e}"
        );
        for jerga in ["ConnectionRefused", "os error", "Os {"] {
            assert!(!e.contains(jerga), "sale jerga ({jerga}): {e}");
        }
        // https se dice, no se intenta a medias
        let e = pide_token("https://freetsa.org/tsr", b"firma").unwrap_err();
        assert!(e.contains("http://"), "{e}");
    }

    /// La hora del sello sale en ISO 8601, como el resto de fechas que
    /// cruzan a la interfaz.
    #[test]
    fn la_hora_del_sello_sale_en_iso() {
        assert_eq!(
            iso_de_generalized("20260910194012Z").as_deref(),
            Some("2026-09-10T19:40:12+00:00")
        );
        assert_eq!(iso_de_generalized("nada").as_deref(), None);
    }
}
