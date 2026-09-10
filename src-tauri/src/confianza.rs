//! Cadena de confianza de una firma: **quién responde por el certificado**
//! que firmó el documento.
//!
//! Es una pregunta distinta de la que responde `firma.rs`. Allí se
//! comprueba que el documento no ha cambiado y que la firma la hizo la
//! clave del certificado que viaja dentro; aquí se comprueba si ese
//! certificado lo emitió alguien en quien el sistema ya confía. Acrobat las
//! separa igual: la barra azul («todas las firmas son válidas») solo sale
//! cuando el certificado **encadena con una raíz de confianza**, y con la
//! criptografía perfecta pero sin raíz dice «la validez de la firma es
//! desconocida».
//!
//! Las raíces las pone el sistema, no Vitela: en macOS evalúa el propio
//! llavero (`SecTrustEvaluateWithError`, el mismo motor que usa Safari) y
//! en Windows y Linux se leen del almacén nativo y la cadena se comprueba
//! con el mismo código que ya verifica las firmas (RustCrypto), sin
//! traerse un ensamblador nuevo al build de Windows.
//!
//! **No se comprueba la revocación** (ni CRL ni OCSP): un certificado
//! revocado esta mañana sigue encadenando con su raíz. Por eso la etiqueta
//! dice «emitido por una autoridad reconocida» y **nunca** «válida».

#[cfg(any(not(target_os = "macos"), test))]
use der::Decode;
use der::Encode;
use x509_cert::Certificate;

/// El certificado encadena con una raíz del almacén del sistema.
pub const RAIZ_CONOCIDA: &str = "raiz_conocida";
/// Se firmó a sí mismo: nadie más responde por él.
pub const AUTOFIRMADO: &str = "autofirmado";
/// Ni encadena ni es autofirmado: no se sabe quién lo emitió.
pub const DESCONOCIDA: &str = "desconocida";

/// La confianza del certificado del firmante, en una de las tres palabras
/// de arriba. `bolso` son los demás certificados que viajan en la firma
/// (en una firma cualificada suele ir la cadena entera) y `momento` el
/// instante en el que se comprueba: la hora de la firma cuando el CMS la
/// trae, porque un certificado caducado **hoy** era bueno cuando se firmó,
/// que es lo que mira Acrobat.
pub(crate) fn confianza_del_firmante(
    firmante: &Certificate,
    bolso: &[Certificate],
    momento: std::time::SystemTime,
) -> String {
    if respalda_el_sistema(firmante, bolso, momento) {
        RAIZ_CONOCIDA
    } else if es_autofirmado(firmante) {
        AUTOFIRMADO
    } else {
        DESCONOCIDA
    }
    .to_string()
}

/// Sujeto y emisor iguales **y** la firma del certificado la hizo su propia
/// clave: sin lo segundo, cualquiera puede copiar el nombre de una AC.
pub(crate) fn es_autofirmado(cert: &Certificate) -> bool {
    cert.tbs_certificate.subject == cert.tbs_certificate.issuer && firmado_por(cert, cert)
}

/// ¿`emisor` firmó `hijo`? Nombre y firma, las dos cosas.
///
/// De aquí abajo, hasta [`encadena`], es la cadena recorrida a mano: en
/// macOS la recorre el llavero y este código solo se compila fuera… y en
/// los tests, que es donde se comprueba sin depender del almacén de la
/// máquina.
#[cfg(any(not(target_os = "macos"), test))]
fn emite(emisor: &Certificate, hijo: &Certificate) -> bool {
    hijo.tbs_certificate.issuer == emisor.tbs_certificate.subject && firmado_por(hijo, emisor)
}

/// La firma del certificado, comprobada con la clave pública del emisor.
/// Reutiliza el verificador de `firma.rs` (RSA PKCS#1 v1.5, RSA-PSS y
/// ECDSA P-256/P-384, con SHA-256/384/512): un certificado firmado con algo
/// que no sabemos leer no encadena, y eso sale como «desconocida», nunca
/// como una acusación.
fn firmado_por(hijo: &Certificate, emisor: &Certificate) -> bool {
    let (Ok(spki), Ok(tbs)) = (
        emisor.tbs_certificate.subject_public_key_info.to_der(),
        hijo.tbs_certificate.to_der(),
    ) else {
        return false;
    };
    let Some(firma) = hijo.signature.as_bytes() else {
        return false;
    };
    crate::firma::comprueba_firma(hijo.signature_algorithm.oid, &spki, &tbs, firma, None).0
        == Some(true)
}

/// ¿El certificado puede emitir otros? Un certificado de persona con el
/// nombre de su AC no sirve para encadenar: hace falta el
/// `basicConstraints` con `cA: true`.
#[cfg(any(not(target_os = "macos"), test))]
fn es_autoridad(cert: &Certificate) -> bool {
    use x509_cert::ext::pkix::BasicConstraints;
    const BASIC_CONSTRAINTS: const_oid::ObjectIdentifier =
        const_oid::ObjectIdentifier::new_unwrap("2.5.29.19");
    cert.tbs_certificate
        .extensions
        .as_ref()
        .and_then(|exts| exts.iter().find(|e| e.extn_id == BASIC_CONSTRAINTS))
        .and_then(|e| BasicConstraints::from_der(e.extn_value.as_bytes()).ok())
        .map(|bc| bc.ca)
        .unwrap_or(false)
}

/// ¿El certificado estaba en vigor en ese instante?
#[cfg(any(not(target_os = "macos"), test))]
fn vigente_en(cert: &Certificate, momento: std::time::SystemTime) -> bool {
    let v = &cert.tbs_certificate.validity;
    momento >= v.not_before.to_system_time() && momento <= v.not_after.to_system_time()
}

/// **La parte comprobable sin el sistema**: sube del firmante a una raíz
/// pasando por los intermedios del bolso. En cada escalón se exige que el
/// emisor firmara de verdad al hijo, que el intermedio sea una autoridad y
/// que los dos estuvieran en vigor en `momento`.
///
/// La profundidad está acotada: un bolso con dos certificados que se
/// emiten el uno al otro daría vueltas para siempre.
#[cfg(any(not(target_os = "macos"), test))]
pub(crate) fn encadena(
    firmante: &Certificate,
    intermedios: &[Certificate],
    raices: &[Certificate],
    momento: std::time::SystemTime,
) -> bool {
    let mut actual = firmante.clone();
    for _ in 0..8 {
        if !vigente_en(&actual, momento) {
            return false;
        }
        if raices
            .iter()
            .any(|r| vigente_en(r, momento) && emite(r, &actual))
        {
            return true;
        }
        let siguiente = intermedios.iter().find(|i| {
            i.tbs_certificate.subject != actual.tbs_certificate.subject
                && es_autoridad(i)
                && emite(i, &actual)
        });
        match siguiente {
            Some(i) => actual = i.clone(),
            None => return false,
        }
    }
    false
}

/// macOS: lo evalúa el llavero. `SecTrustCreateWithCertificates` con la
/// cadena que venga en la firma como intermedios y la política X.509
/// básica; las anclas las pone el sistema. La fecha de verificación es la
/// de la firma, que es lo que hace que una firma vieja con un certificado
/// ya caducado siga diciendo quién la emitió.
#[cfg(target_os = "macos")]
fn respalda_el_sistema(
    firmante: &Certificate,
    bolso: &[Certificate],
    momento: std::time::SystemTime,
) -> bool {
    use security_framework::certificate::SecCertificate;
    use security_framework::policy::SecPolicy;
    use security_framework::trust::SecTrust;

    let mut ders = vec![firmante.clone()];
    ders.extend(bolso.iter().filter(|c| *c != firmante).cloned());
    let certs: Vec<SecCertificate> = ders
        .iter()
        .filter_map(|c| c.to_der().ok())
        .filter_map(|d| SecCertificate::from_der(&d).ok())
        .collect();
    if certs.is_empty() {
        return false;
    }
    let Ok(mut trust) = SecTrust::create_with_certificates(&certs, &[SecPolicy::create_x509()])
    else {
        return false;
    };
    // CFAbsoluteTime cuenta desde el 1 de enero de 2001, no desde 1970
    if let Ok(desde_1970) = momento.duration_since(std::time::UNIX_EPOCH) {
        let cf = core_foundation::date::CFDate::new(desde_1970.as_secs_f64() - 978_307_200.0);
        let _ = trust.set_trust_verify_date(&cf);
    }
    trust.evaluate_with_error().is_ok()
}

/// Windows y Linux: las raíces del almacén nativo y la cadena a mano.
#[cfg(not(target_os = "macos"))]
fn respalda_el_sistema(
    firmante: &Certificate,
    bolso: &[Certificate],
    momento: std::time::SystemTime,
) -> bool {
    encadena(firmante, bolso, raices_del_sistema(), momento)
}

/// Las raíces del almacén del sistema, leídas una sola vez: son cientos y
/// el usuario puede tener varias firmas en el mismo documento.
#[cfg(not(target_os = "macos"))]
fn raices_del_sistema() -> &'static [Certificate] {
    static RAICES: std::sync::OnceLock<Vec<Certificate>> = std::sync::OnceLock::new();
    RAICES.get_or_init(|| {
        rustls_native_certs::load_native_certs()
            .certs
            .iter()
            .filter_map(|der| Certificate::from_der(der.as_ref()).ok())
            .collect()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, SystemTime};
    use x509_cert::der::DecodePem;

    pub(crate) fn ca() -> Certificate {
        Certificate::from_pem(include_str!("../fixtures/test_ca_cert.pem")).expect("AC de prueba")
    }
    pub(crate) fn hija() -> Certificate {
        Certificate::from_pem(include_str!("../fixtures/test_hija_cert.pem"))
            .expect("certificado hijo de prueba")
    }
    fn autofirmado() -> Certificate {
        Certificate::from_pem(include_str!("../fixtures/test_cert.pem")).expect("autofirmado")
    }
    /// Un instante dentro de la validez de los tres fixtures.
    fn manana() -> SystemTime {
        SystemTime::now() + Duration::from_secs(24 * 3600)
    }

    /// El troceador de cadenas, sin depender del llavero de la máquina:
    /// con la AC como raíz, el certificado hijo encadena; sin ella, no.
    /// Es la parte que se puede fijar en un test (el llavero real no).
    #[test]
    fn la_cadena_sube_hasta_la_raiz_que_se_le_da_y_no_mas() {
        let (ca, hija) = (vec![ca()], hija());
        assert!(
            encadena(&hija, &ca, &ca, manana()),
            "la AC que la emitió, puesta como raíz, tiene que encadenar"
        );
        assert!(
            !encadena(&hija, &ca, &[], manana()),
            "sin ninguna raíz no se encadena con nada"
        );
        assert!(
            !encadena(&hija, &[], &[autofirmado()], manana()),
            "una raíz que no la emitió no vale"
        );
        // fuera de vigencia no encadena aunque la raíz esté
        let hace_diez_anos = SystemTime::now() - Duration::from_secs(10 * 365 * 24 * 3600);
        assert!(!encadena(&hija, &ca, &ca, hace_diez_anos));
    }

    /// Un certificado de persona con el nombre de su AC no sirve para
    /// encadenar: hace falta el `basicConstraints` con `cA: true`.
    #[test]
    fn solo_encadena_por_certificados_de_autoridad() {
        assert!(es_autoridad(&ca()), "la AC de prueba es autoridad");
        assert!(!es_autoridad(&hija()), "el hijo es de firmante, no de AC");
    }

    /// «Autofirmado» es sujeto igual a emisor **y** la firma hecha con su
    /// propia clave: copiar el nombre de una AC en el emisor no basta.
    #[test]
    fn autofirmado_es_el_que_se_firma_a_si_mismo() {
        assert!(es_autofirmado(&autofirmado()));
        assert!(!es_autofirmado(&hija()));
        assert!(es_autofirmado(&ca()), "la AC de prueba es raíz");
    }

    /// El caso de todos los días: el certificado con el que firma Vitela no
    /// lo respalda nadie, y así hay que decirlo.
    #[test]
    fn el_certificado_de_vitela_sale_autofirmado() {
        assert_eq!(
            confianza_del_firmante(&autofirmado(), &[], manana()),
            AUTOFIRMADO
        );
    }

    /// Una cadena de dos con la AC en el bolso pero sin raíz en el almacén
    /// del sistema: no se sabe quién lo emitió, y **no** es autofirmado.
    #[test]
    fn una_cadena_sin_raiz_en_el_sistema_es_desconocida() {
        assert_eq!(
            confianza_del_firmante(&hija(), &[ca()], manana()),
            DESCONOCIDA
        );
    }
}
