//! Autoguardado y recuperación tras un cierre inesperado (paridad #36).
//!
//! La copia de trabajo ya vive en temp y ya sobrevive a un cierre bruto: lo
//! único que faltaba era **el apunte de que existía y no se había
//! guardado**. Eso es este módulo: un `sesion.json` en `DIR_DATOS` con una
//! entrada por documento vivo —la copia, el original del que salió y si
//! tenía cambios sin guardar—.
//!
//! **Una lista, no un apunte** (ciclo 8). Con pestañas hay varios
//! documentos abiertos a la vez, y con un solo apunte cambiar de pestaña
//! sobrescribía el de la anterior: tres documentos con cambios y un cierre
//! bruto devolvían **uno**, el último que se tocó. La entrada se indexa por
//! copia de trabajo, así que cada comando toca la suya y deja las demás en
//! paz. El fichero viejo —un objeto suelto— se lee como una lista de uno:
//! quien actualice la app no pierde lo que tuviera a medias.
//!
//! Como en Acrobat, esto es silencioso: no hay insignias, ni avisos
//! periódicos, ni ajuste en Preferencias. Solo al arrancar, si queda algún
//! apunte, la UI ofrece recuperar los documentos por su nombre; al cerrar
//! bien, el apunte de ese documento se borra y no vuelve a salir.

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

/// El fichero por dentro: una lista con versión. Un objeto suelto —el
/// formato de antes del ciclo 8— también se lee, como una lista de uno.
#[derive(Serialize, Deserialize, Debug, Default)]
struct Apuntes {
    /// Sin `serde(default)` **a propósito**: es lo que distingue el
    /// formato nuevo del viejo. Con un defecto, el objeto suelto de antes
    /// del ciclo 8 se leía como una lista vacía y el trabajo de quien
    /// actualizara la app se perdía en silencio.
    sesiones: Vec<Sesion>,
}

/// Todo lo que hay apuntado, sin comprobar nada. Entiende los dos formatos:
/// el `{ "sesiones": [...] }` de ahora y el objeto suelto de antes.
pub(crate) fn lee_todo(fichero: &std::path::Path) -> Vec<Apunte> {
    let Ok(texto) = std::fs::read_to_string(fichero) else {
        return Vec::new();
    };
    let mut sesiones = match serde_json::from_str::<Apuntes>(&texto) {
        Ok(a) => a.sesiones,
        // el formato viejo: un solo documento, que es una lista de uno
        Err(_) => match serde_json::from_str::<Sesion>(&texto) {
            Ok(s) => vec![s],
            Err(_) => return Vec::new(),
        },
    };
    for s in &mut sesiones {
        let de = |ruta: &str| {
            std::path::Path::new(ruta)
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
        };
        s.name = de(&s.original_path)
            .or_else(|| de(&s.work_path))
            .unwrap_or_else(|| "documento".into());
    }
    sesiones
}

/// Alias para leerse mejor donde se usa: cada elemento es el apunte de un
/// documento.
pub(crate) type Apunte = Sesion;

/// Escribe la lista entera; con la lista vacía, el fichero se va (no hay
/// nada que recuperar y un fichero de cero apuntes es basura que alguien
/// tendrá que interpretar algún día).
fn escribe(fichero: &std::path::Path, sesiones: Vec<Sesion>) -> Result<(), String> {
    if sesiones.is_empty() {
        let _ = std::fs::remove_file(fichero);
        return Ok(());
    }
    let json = serde_json::to_string_pretty(&Apuntes { sesiones })
        .map_err(|e| crate::mensaje_llano(format!("No se ha podido apuntar la sesión: {e}")))?;
    std::fs::write(fichero, json)
        .map_err(|e| crate::mensaje_llano(format!("No se ha podido apuntar la sesión: {e}")))
}

/// Apunta **un** documento: actualiza su entrada y deja las demás como
/// estaban. Con pestañas, apuntar el de delante no puede borrar el de
/// detrás.
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
    let mut sesiones = lee_todo(fichero);
    for s in &mut sesiones {
        s.name = String::new(); // se recalcula al leer, no se guarda rancio
    }
    match sesiones.iter().position(|s| s.work_path == work_path) {
        Some(i) => sesiones[i] = sesion,
        None => sesiones.push(sesion),
    }
    escribe(fichero, sesiones)
}

/// Lo que hay que ofrecer al arrancar: los apuntes **con cambios sin
/// guardar** cuya copia de trabajo siga en el disco. Si la copia ya no está
/// (temp barrido, otra instancia que la cerró) no hay nada que recuperar y
/// ese apunte se retira; los demás se quedan.
pub(crate) fn recupera_en(fichero: &std::path::Path) -> Vec<Sesion> {
    let todas = lee_todo(fichero);
    let (vivas, muertas): (Vec<Sesion>, Vec<Sesion>) = todas
        .into_iter()
        .partition(|s| s.modificado && std::path::Path::new(&s.work_path).is_file());
    if !muertas.is_empty() {
        // las que ya no se pueden recuperar no se vuelven a ofrecer
        let _ = escribe(fichero, vivas.clone());
    }
    vivas
}

/// Las copias de trabajo apuntadas. Las usa el barrido de huérfanos del
/// arranque para no llevarse justo lo que hay que recuperar: **todas**, no
/// la primera. Con tres documentos apuntados, proteger uno y barrer los
/// otros dos es peor que no barrer nada.
pub(crate) fn copias_apuntadas() -> Vec<String> {
    let Some(f) = fichero() else { return Vec::new() };
    lee_todo(&f).into_iter().map(|s| s.work_path).collect()
}

/// La UI apunta el estado del documento vivo: al abrirlo, cuando pasa a
/// «modificado» y tras cada mutación (con su propio retardo, no en cada
/// tecla). Escribir el apunte es escribir un JSON de cuatro líneas: la
/// copia de trabajo ya está en el disco, no se copia nada.
///
/// Toca **solo la entrada de esa copia de trabajo**: con varias pestañas
/// abiertas, el autoguardado de la de delante no puede borrar lo que tenga
/// sin guardar la de detrás.
#[tauri::command(async)]
pub fn autosave_state(
    work_path: String,
    original_path: Option<String>,
    modified: bool,
) -> Result<(), String> {
    let Some(f) = fichero() else { return Ok(()) };
    apunta_en(&f, &work_path, &original_path.unwrap_or_default(), modified)
}

/// Se cerró bien: no hay nada que recuperar **de ese documento**. Lo llama
/// la UI al cerrar una pestaña y al salir después de guardar o de
/// descartar, y también «Descartar» en la banda de recuperación.
///
/// `work_path` es **obligatorio** desde el ciclo 7: se quita la entrada de
/// ese documento y las demás se quedan. Con varios documentos abiertos,
/// cerrar uno no puede llevarse el trabajo sin guardar de otro, y «bórrame
/// el apunte que haya» deja de ser una orden que alguien pueda querer dar.
///
/// Obligatorio quiere decir **que no admite `null`**: Tauri no sabe
/// deserializar un `String` desde `null`, así que una llamada sin la copia
/// de trabajo no borra nada y devuelve error. Eso lo vigila desde el ciclo 8
/// el cuarto aserto del test cruzado (`OPCIONALES_INDEBIDOS`, en
/// `puente_dev`), porque un envoltorio que declare este parámetro opcional
/// deja el apunte puesto después de guardar y la app ofrece recuperar un
/// documento que ya estaba a salvo.
///
/// Nunca falla: si no hay apunte, no hay nada que hacer.
#[tauri::command(async)]
pub fn borra_sesion(work_path: String) -> Result<(), String> {
    let Some(f) = fichero() else { return Ok(()) };
    borra_en(&f, Some(&work_path));
    Ok(())
}

/// Quita el apunte de una copia de trabajo; sin ruta, los quita todos (que
/// es cerrar la app entera). Con la lista vacía, el fichero se va.
pub(crate) fn borra_en(fichero: &std::path::Path, work_path: Option<&str>) {
    let quedan: Vec<Sesion> = match work_path.filter(|w| !w.is_empty()) {
        Some(w) => lee_todo(fichero).into_iter().filter(|s| s.work_path != w).collect(),
        None => Vec::new(),
    };
    let _ = escribe(fichero, quedan);
}

/// Al arrancar: ¿quedó trabajo sin guardar de la vez anterior? Devuelve un
/// apunte por documento —posiblemente ninguno— para la banda («Tenías
/// cambios sin guardar en **3 documentos**», con Recuperar y Descartar).
///
/// Es una **lista** desde el ciclo 8: con pestañas, un cierre bruto puede
/// dejar varios documentos con cambios, y devolver solo el último que se
/// tocó es prometer una red que no está. Recuperar los abre cada uno en su
/// pestaña.
#[tauri::command(async)]
pub fn recover_session() -> Result<Vec<Sesion>, String> {
    let Some(f) = fichero() else { return Ok(Vec::new()) };
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
        let ofrecidas = recupera_en(&f);
        assert_eq!(ofrecidas.len(), 1, "hay algo que recuperar");
        let s = &ofrecidas[0];
        assert_eq!(s.work_path, work);
        assert_eq!(s.original_path, "/Users/jorge/facturas/factura.pdf");
        assert!(s.modificado);
        assert_eq!(s.name, "factura.pdf", "la banda dice el nombre del original");
        assert!(!s.cuando.is_empty());

        // guardar deja el documento sin cambios pendientes: nada que ofrecer
        apunta_en(&f, &work, "/Users/jorge/facturas/factura.pdf", false).expect("apuntar");
        assert!(recupera_en(&f).is_empty(), "sin cambios no se ofrece nada");

        // y cerrar limpiamente borra el apunte
        apunta_en(&f, &work, "", true).expect("apuntar");
        assert_eq!(recupera_en(&f).len(), 1);
        let _ = std::fs::remove_file(&f);
        assert!(recupera_en(&f).is_empty(), "cerrado limpio, nada que recuperar");
        std::fs::remove_file(&copia).ok();
    }

    /// **H6b.** Con pestañas hay varios documentos abiertos y el
    /// autoguardado va del que está delante: apuntar uno **no puede** borrar
    /// el apunte de otro. Con un solo apunte, tres documentos con cambios y
    /// un cierre bruto devolvían uno, y los otros dos se perdían sin que
    /// nadie lo dijera. Una red de seguridad en la que no se puede confiar
    /// del todo es peor que ninguna.
    #[test]
    fn apuntar_un_documento_no_borra_el_apunte_de_otro() {
        let f = fichero_de_prueba("varios");
        apunta_en(&f, "/tmp/vitela-a.pdf", "/tmp/a.pdf", true).expect("apuntar a");
        apunta_en(&f, "/tmp/vitela-b.pdf", "/tmp/b.pdf", true).expect("apuntar b");
        apunta_en(&f, "/tmp/vitela-c.pdf", "/tmp/c.pdf", false).expect("apuntar c");

        let todos = lee_todo(&f);
        assert_eq!(todos.len(), 3, "tres documentos, tres apuntes: {todos:?}");
        assert_eq!(
            todos.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(),
            vec!["a.pdf", "b.pdf", "c.pdf"]
        );

        // volver a la primera pestaña actualiza **su** entrada, no añade otra
        apunta_en(&f, "/tmp/vitela-a.pdf", "/tmp/a.pdf", false).expect("reapuntar a");
        let todos = lee_todo(&f);
        assert_eq!(todos.len(), 3, "la entrada se actualiza, no se duplica");
        assert!(!todos[0].modificado, "y es la suya la que cambia");
        assert!(todos[1].modificado, "la de al lado se queda como estaba");

        // cerrar una pestaña se lleva la suya y solo la suya
        borra_en(&f, Some("/tmp/vitela-b.pdf"));
        let quedan = lee_todo(&f);
        assert_eq!(quedan.len(), 2);
        assert!(!quedan.iter().any(|s| s.work_path == "/tmp/vitela-b.pdf"));

        // y cerrar la app entera los quita todos
        borra_en(&f, None);
        assert!(!f.exists(), "sin apuntes, el fichero se va");
    }

    /// Solo se ofrece lo que se puede recuperar de verdad: con cambios y
    /// con la copia todavía en el disco. Lo demás se retira **sin tocar a
    /// los vecinos**.
    #[test]
    fn se_ofrecen_los_que_tienen_cambios_y_copia_viva() {
        let f = fichero_de_prueba("vivos");
        let copia = std::env::temp_dir().join("vitela-test-sesion-viva.pdf");
        crate::tests::crea_pdf(&["Contrato"], &copia);
        let viva = copia.to_string_lossy().into_owned();

        apunta_en(&f, &viva, "/tmp/contrato.pdf", true).expect("con cambios y copia");
        apunta_en(&f, "/tmp/vitela-fantasma.pdf", "/tmp/x.pdf", true).expect("sin copia");
        apunta_en(&f, &viva.replace(".pdf", "-2.pdf"), "/tmp/y.pdf", false).expect("guardado");

        let ofrecidas = recupera_en(&f);
        assert_eq!(ofrecidas.len(), 1, "solo uno se puede recuperar: {ofrecidas:?}");
        assert_eq!(ofrecidas[0].work_path, viva);
        // los apuntes inútiles se retiran solos, y el bueno se queda
        let quedan = lee_todo(&f);
        assert_eq!(quedan.len(), 1);
        assert_eq!(quedan[0].work_path, viva);
        std::fs::remove_file(&copia).ok();
        std::fs::remove_file(&f).ok();
    }

    /// «Descartar» y cerrar bien borran el apunte; y el de otro documento
    /// (otra ventana, otra sesión) no se toca.
    #[test]
    fn borrar_la_sesion_solo_borra_la_suya() {
        let f = fichero_de_prueba("borrar");
        apunta_en(&f, "/tmp/vitela-a.pdf", "/tmp/a.pdf", true).expect("apuntar");
        borra_en(&f, Some("/tmp/vitela-otro.pdf"));
        assert!(f.exists(), "el apunte de otro documento no se toca");
        borra_en(&f, Some("/tmp/vitela-a.pdf"));
        assert!(!f.exists(), "el suyo sí");

        // sin ruta, se borran los que haya (cerrar la app)
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
        assert!(recupera_en(&f).is_empty());
        assert!(!f.exists(), "el apunte inútil se retira solo");
    }

    /// **H6b.** Quien actualiza la app puede tener un `sesion.json` del
    /// formato viejo —un objeto suelto— con trabajo dentro. Se lee como una
    /// lista de uno: perder el trabajo de alguien al actualizar sería el
    /// peor momento posible para estrenar un formato.
    #[test]
    fn el_apunte_del_formato_viejo_se_lee_como_una_lista_de_uno() {
        let f = fichero_de_prueba("formato-viejo");
        std::fs::write(
            &f,
            br#"{"original_path":"/tmp/viejo.pdf","work_path":"/tmp/vitela-viejo.pdf",
                 "modificado":true,"cuando":"2026-09-09T19:40:00+02:00"}"#,
        )
        .expect("escribir el formato viejo");
        let todos = lee_todo(&f);
        assert_eq!(todos.len(), 1, "un objeto suelto es una lista de uno");
        assert_eq!(todos[0].work_path, "/tmp/vitela-viejo.pdf");
        assert_eq!(todos[0].name, "viejo.pdf");
        assert!(todos[0].modificado);

        // y al apuntar el segundo documento, el fichero pasa al formato
        // nuevo sin perder al primero
        apunta_en(&f, "/tmp/vitela-nuevo.pdf", "/tmp/nuevo.pdf", true).expect("apuntar");
        assert_eq!(lee_todo(&f).len(), 2);
        std::fs::remove_file(&f).ok();
    }

    /// Un apunte corrupto no puede impedir que la app arranque.
    #[test]
    fn un_apunte_corrupto_no_rompe_el_arranque() {
        let f = fichero_de_prueba("corrupto");
        std::fs::write(&f, b"esto no es JSON").expect("escribir basura");
        assert!(lee_todo(&f).is_empty());
        assert!(recupera_en(&f).is_empty());
        std::fs::remove_file(&f).ok();
    }
}
