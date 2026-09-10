//! Autoguardado y recuperación tras un cierre inesperado (paridad #36).
//!
//! La copia de trabajo ya vive en temp y ya sobrevive a un cierre bruto: lo
//! único que faltaba era **el apunte de que existía y no se había
//! guardado**. Eso es este módulo: un `sesion.json` en `DIR_DATOS` con la
//! copia viva, el original del que salió y si tenía cambios sin guardar.
//!
//! Como en Acrobat, esto es silencioso: no hay insignias, ni avisos
//! periódicos, ni ajuste en Preferencias. Solo al arrancar, si el apunte
//! sigue ahí, la UI ofrece recuperar el documento por su nombre; al cerrar
//! bien, el apunte se borra y no vuelve a salir.

use serde::{Deserialize, Serialize};

/// Lo que se apunta de la sesión viva.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Sesion {
    /// El documento del que salió la copia (vacío si nunca se guardó en
    /// ningún sitio: un documento nuevo o combinado).
    pub original_path: String,
    /// La copia de trabajo, en temp, con todos los cambios dentro.
    pub work_path: String,
    /// ¿Había cambios sin guardar cuando se apuntó?
    pub modificado: bool,
    /// Cuándo se apuntó, en ISO 8601.
    pub cuando: String,
    /// Nombre para enseñar en la banda («Tenías cambios sin guardar en
    /// *factura.pdf*»). Se recalcula al leer, no se guarda rancio.
    #[serde(default)]
    pub name: String,
}

/// Fichero del apunte dentro del directorio de datos de la app. `None`
/// mientras el directorio no esté fijado (tests y arranques a medias): la
/// recuperación es una red, nunca un motivo de error.
fn fichero() -> Option<std::path::PathBuf> {
    let dir = crate::firmas_visuales::DIR_DATOS.get()?;
    let _ = std::fs::create_dir_all(dir);
    Some(dir.join("sesion.json"))
}

pub(crate) fn apunta_en(
    fichero: &std::path::Path,
    work_path: &str,
    original_path: &str,
    modificado: bool,
) -> Result<(), String> {
    let sesion = Sesion {
        original_path: original_path.to_string(),
        work_path: work_path.to_string(),
        modificado,
        cuando: chrono::Local::now().to_rfc3339(),
        name: String::new(),
    };
    let json = serde_json::to_string_pretty(&sesion)
        .map_err(|e| crate::mensaje_llano(format!("No se ha podido apuntar la sesión: {e}")))?;
    std::fs::write(fichero, json)
        .map_err(|e| crate::mensaje_llano(format!("No se ha podido apuntar la sesión: {e}")))
}

/// El apunte tal como está en disco, sin comprobar nada.
pub(crate) fn lee_en(fichero: &std::path::Path) -> Option<Sesion> {
    let mut s: Sesion = std::fs::read_to_string(fichero)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())?;
    let de = |ruta: &str| {
        std::path::Path::new(ruta)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
    };
    s.name = de(&s.original_path)
        .or_else(|| de(&s.work_path))
        .unwrap_or_else(|| "documento".into());
    Some(s)
}

/// Lo que hay que ofrecer al arrancar: un apunte **con cambios sin
/// guardar** y cuya copia de trabajo siga en el disco. Si la copia ya no
/// está (temp barrido, otra instancia que la cerró) no hay nada que
/// recuperar y el apunte se retira.
pub(crate) fn recupera_en(fichero: &std::path::Path) -> Option<Sesion> {
    let s = lee_en(fichero)?;
    if !s.modificado || !std::path::Path::new(&s.work_path).is_file() {
        let _ = std::fs::remove_file(fichero);
        return None;
    }
    Some(s)
}

/// La copia de trabajo apuntada, si la hay. La usa el barrido de huérfanos
/// del arranque para no llevarse justo lo que hay que recuperar.
pub(crate) fn copia_apuntada() -> Option<String> {
    let s = lee_en(&fichero()?)?;
    Some(s.work_path)
}

/// La UI apunta el estado del documento vivo: al abrirlo, cuando pasa a
/// «modificado» y tras cada mutación (con su propio retardo, no en cada
/// tecla). Escribir el apunte es escribir un JSON de cuatro líneas: la
/// copia de trabajo ya está en el disco, no se copia nada.
#[tauri::command(async)]
pub fn autosave_state(
    work_path: String,
    original_path: Option<String>,
    modified: bool,
) -> Result<(), String> {
    let Some(f) = fichero() else { return Ok(()) };
    apunta_en(&f, &work_path, &original_path.unwrap_or_default(), modified)
}

/// Se cerró bien: no hay nada que recuperar. Lo llama la UI al cerrar el
/// documento y al salir después de guardar o de descartar, y también
/// «Descartar» en la banda de recuperación.
///
/// `work_path` es **obligatorio** desde el ciclo 7: el apunte se borra solo
/// si es de ese documento. Con varios documentos abiertos, cerrar uno no
/// puede llevarse el trabajo sin guardar de otro, y «bórrame el apunte que
/// haya» deja de ser una orden que alguien pueda querer dar.
///
/// Nunca falla: si no hay apunte, no hay nada que hacer.
#[tauri::command(async)]
pub fn borra_sesion(work_path: String) -> Result<(), String> {
    let Some(f) = fichero() else { return Ok(()) };
    borra_en(&f, Some(&work_path));
    Ok(())
}

pub(crate) fn borra_en(fichero: &std::path::Path, work_path: Option<&str>) {
    if let Some(w) = work_path.filter(|w| !w.is_empty()) {
        // el apunte es de otro documento: no es nuestro, no se toca
        if lee_en(fichero).is_some_and(|s| s.work_path != w) {
            return;
        }
    }
    let _ = std::fs::remove_file(fichero);
}

/// Al arrancar: ¿quedó trabajo sin guardar de la vez anterior? Devuelve el
/// apunte para la banda («Tenías cambios sin guardar en *factura.pdf*»,
/// con Recuperar y Descartar), o `None` si no hay nada que ofrecer.
#[tauri::command(async)]
pub fn recover_session() -> Result<Option<Sesion>, String> {
    let Some(f) = fichero() else { return Ok(None) };
    Ok(recupera_en(&f))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fichero_de_prueba(nombre: &str) -> std::path::PathBuf {
        let f = std::env::temp_dir().join(format!("vitela-test-sesion-{nombre}.json"));
        let _ = std::fs::remove_file(&f);
        f
    }

    /// Un cierre bruto no se puede llevar el trabajo: la copia sigue en
    /// temp y el apunte dice de qué documento era. Cerrando bien, en
    /// cambio, no hay que ofrecer nada.
    #[test]
    fn tras_un_cierre_bruto_se_ofrece_recuperar_y_tras_uno_limpio_no() {
        let f = fichero_de_prueba("bruto");
        let copia = std::env::temp_dir().join("vitela-test-sesion-copia.pdf");
        crate::tests::crea_pdf(&["Factura"], &copia);
        let work = copia.to_string_lossy().into_owned();

        apunta_en(&f, &work, "/Users/jorge/facturas/factura.pdf", true).expect("apuntar");
        let s = recupera_en(&f).expect("hay algo que recuperar");
        assert_eq!(s.work_path, work);
        assert_eq!(s.original_path, "/Users/jorge/facturas/factura.pdf");
        assert!(s.modificado);
        assert_eq!(s.name, "factura.pdf", "la banda dice el nombre del original");
        assert!(!s.cuando.is_empty());

        // guardar deja el documento sin cambios pendientes: nada que ofrecer
        apunta_en(&f, &work, "/Users/jorge/facturas/factura.pdf", false).expect("apuntar");
        assert!(recupera_en(&f).is_none(), "sin cambios no se ofrece nada");

        // y cerrar limpiamente borra el apunte
        apunta_en(&f, &work, "", true).expect("apuntar");
        assert!(recupera_en(&f).is_some());
        let _ = std::fs::remove_file(&f);
        assert!(recupera_en(&f).is_none(), "cerrado limpio, nada que recuperar");
        std::fs::remove_file(&copia).ok();
    }

    /// «Descartar» y cerrar bien borran el apunte; y si el apunte es de
    /// otro documento (otra ventana, otra sesión), no se toca.
    #[test]
    fn borrar_la_sesion_solo_borra_la_suya() {
        let f = fichero_de_prueba("borrar");
        apunta_en(&f, "/tmp/vitela-a.pdf", "/tmp/a.pdf", true).expect("apuntar");
        borra_en(&f, Some("/tmp/vitela-otro.pdf"));
        assert!(f.exists(), "el apunte de otro documento no se toca");
        borra_en(&f, Some("/tmp/vitela-a.pdf"));
        assert!(!f.exists(), "el suyo sí");

        // sin ruta, se borra el que haya (cerrar la app)
        apunta_en(&f, "/tmp/vitela-a.pdf", "/tmp/a.pdf", true).expect("apuntar");
        borra_en(&f, None);
        assert!(!f.exists());
        // y borrar dos veces no es un error
        borra_en(&f, None);
    }

    /// Si la copia de trabajo ya no está (el temp la barrió, u otra
    /// instancia la cerró), no se ofrece recuperar algo que no existe: eso
    /// sería un callejón sin salida.
    #[test]
    fn no_se_ofrece_recuperar_una_copia_que_ya_no_esta() {
        let f = fichero_de_prueba("sin-copia");
        apunta_en(&f, "/tmp/vitela-que-ya-no-esta.pdf", "/tmp/x.pdf", true).expect("apuntar");
        assert!(recupera_en(&f).is_none());
        assert!(!f.exists(), "el apunte inútil se retira solo");
    }

    /// Un apunte corrupto no puede impedir que la app arranque.
    #[test]
    fn un_apunte_corrupto_no_rompe_el_arranque() {
        let f = fichero_de_prueba("corrupto");
        std::fs::write(&f, b"esto no es JSON").expect("escribir basura");
        assert!(lee_en(&f).is_none());
        assert!(recupera_en(&f).is_none());
        std::fs::remove_file(&f).ok();
    }
}
