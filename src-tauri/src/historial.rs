//! Deshacer/rehacer por instantáneas de la copia de trabajo: antes de cada
//! mutación se copia el fichero entero a `<work>.snap<n>` y deshacer
//! intercambia la copia actual por la instantánea. Es mucho más barato que
//! invertir cada operación (en APFS `fs::copy` es un clon) y cubre todos
//! los comandos por igual. Tope `MAX_PASOS`.
//!
//! Todo el acceso a disco pasa por el hilo de PDFium tras invalidar el
//! caché, para que ningún handle perezoso tenga abierto el fichero que se
//! va a renombrar (en Windows fallaría) y para serializar con el resto de
//! comandos. Regla: TODO comando que escriba `work_path` envuelve su cuerpo
//! en [`mutacion`].

use crate::{invalidate_doc_cache, mensaje_llano, on_pdfium_thread, with_doc};
use serde::Serialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{LazyLock, Mutex};

pub(crate) const MAX_PASOS: usize = 20;
/// Por encima de este tamaño no se toman instantáneas (el documento queda
/// sin deshacer, la UI ve undo = 0) para no llenar el disco en Windows,
/// donde la copia es real.
const MAX_BYTES: u64 = 512 * 1024 * 1024;

/// Un paso: la instantánea del fichero **y lo que del documento no vive en
/// el fichero**. Hoy eso es solo la protección puesta esperando a Guardar
/// (ver «Protección» en CLAUDE.md), que se anota en un mapa aparte porque
/// la copia de trabajo no puede ir cifrada. Sin guardarla aquí, deshacer
/// «Quitar la contraseña…» devolvía el fichero y no la contraseña, y el
/// documento se guardaba en claro sin que nadie lo hubiera pedido.
struct Paso {
    snap: PathBuf,
    proteccion: Option<crate::seguridad::Proteccion>,
}

#[derive(Default)]
struct Historial {
    /// De más antigua a más reciente.
    deshacer: Vec<Paso>,
    rehacer: Vec<Paso>,
    seq: u64,
}

static HISTORIAL: LazyLock<Mutex<HashMap<String, Historial>>> = LazyLock::new(Default::default);

#[derive(Serialize, Debug, Clone, Copy, PartialEq, Eq)]
pub struct HistoryState {
    pub undo: u16,
    pub redo: u16,
    /// Páginas del documento tras la operación (la UI lo pasa a su refresco
    /// general, porque deshacer puede cambiar el número de páginas).
    pub page_count: u16,
}

fn con<R>(work_path: &str, f: impl FnOnce(&mut Historial) -> R) -> R {
    let mut mapa = HISTORIAL.lock().unwrap_or_else(|e| e.into_inner());
    f(mapa.entry(work_path.to_string()).or_default())
}

/// Directorio de las instantáneas (dentro del temp, para que el barrido de
/// arranque lo limpie entero).
pub(crate) fn directorio() -> PathBuf {
    std::env::temp_dir().join("vitela-historial")
}

fn nombre_base(work_path: &str) -> String {
    std::path::Path::new(work_path)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "documento".into())
}

fn ruta_instantanea(work_path: &str, seq: u64) -> PathBuf {
    directorio().join(format!("{}.snap{seq}", nombre_base(work_path)))
}

/// Borra del disco cualquier instantánea de ese documento, también las de
/// procesos anteriores (tests con nombres de fixture fijos).
pub(crate) fn borra_instantaneas_en_disco(work_path: &str) {
    let prefijo = format!("{}.snap", nombre_base(work_path));
    if let Ok(entradas) = std::fs::read_dir(directorio()) {
        for e in entradas.flatten() {
            if e.file_name().to_string_lossy().starts_with(&prefijo) {
                let _ = std::fs::remove_file(e.path());
            }
        }
    }
}

/// Toma una instantánea del estado actual (antes de mutar). Vacía la pila
/// de rehacer: una mutación nueva abre otra rama.
fn empuja(work_path: &str) -> Result<(), String> {
    let work_path = work_path.to_string();
    on_pdfium_thread(move || {
        invalidate_doc_cache(&work_path);
        let tam = std::fs::metadata(&work_path)
            .map_err(crate::mensaje_llano)?
            .len();
        if tam > MAX_BYTES {
            return Ok(());
        }
        std::fs::create_dir_all(directorio()).map_err(|e| {
            crate::mensaje_llano(format!(
                "No se ha podido preparar la carpeta de deshacer: {e}"
            ))
        })?;
        con(&work_path, |h| {
            let snap = ruta_instantanea(&work_path, h.seq);
            h.seq += 1;
            std::fs::copy(&work_path, &snap).map_err(|e| {
                crate::mensaje_llano(format!("No se ha podido guardar el paso de deshacer: {e}"))
            })?;
            for viejo in h.rehacer.drain(..) {
                let _ = std::fs::remove_file(viejo.snap);
            }
            h.deshacer.push(Paso {
                snap,
                proteccion: crate::seguridad::proteccion_de(&work_path),
            });
            Ok(())
        })
    })
}

/// Recorta la pila al tope. Se hace tras el éxito de la mutación, no al
/// empujar: si la mutación falla se retira la instantánea nueva y el paso
/// antiguo tiene que seguir ahí.
fn recorta(work_path: &str) {
    con(work_path, |h| {
        while h.deshacer.len() > MAX_PASOS {
            let _ = std::fs::remove_file(h.deshacer.remove(0).snap);
        }
    });
}

/// Descarta la instantánea más reciente (mutación fallida o agrupación).
fn descarta_ultimo(work_path: &str) {
    con(work_path, |h| {
        if let Some(p) = h.deshacer.pop() {
            let _ = std::fs::remove_file(p.snap);
        }
    });
}

/// Retira el paso de deshacer de una mutación que ha salido bien y **no ha
/// cambiado nada**: la goma que pasa por donde no había trazo, por ejemplo.
/// Ofrecer un ⌘Z que no hace nada visible es peor que no ofrecerlo.
pub(crate) fn retira_paso(work_path: &str) {
    descarta_ultimo(work_path);
}

/// Envuelve una mutación de la copia de trabajo: toma la instantánea, ejecuta
/// `f` (que recibe la misma ruta, para que el cuerpo del comando no cambie)
/// y, si falla, retira la instantánea (el disco no ha cambiado porque todos
/// los escritores son atómicos).
pub(crate) fn mutacion<R>(
    work_path: String,
    f: impl FnOnce(String) -> Result<R, String>,
) -> Result<R, String> {
    empuja(&work_path)?;
    match f(work_path.clone()) {
        Ok(r) => {
            recorta(&work_path);
            Ok(r)
        }
        Err(e) => {
            descarta_ultimo(&work_path);
            // único sitio donde se traduce el error de una mutación
            Err(mensaje_llano(e))
        }
    }
}

fn estado(work_path: &str) -> Result<HistoryState, String> {
    let (undo, redo) = con(work_path, |h| {
        (h.deshacer.len() as u16, h.rehacer.len() as u16)
    });
    let page_count = with_doc(work_path, |d| Ok(d.pages().len()))?;
    Ok(HistoryState {
        undo,
        redo,
        page_count,
    })
}

/// Sustituye la copia de trabajo por `origen` guardando la actual en la pila
/// contraria. Primero se copia (nunca hay un instante sin `work_path`) y
/// luego se renombra encima, con el caché invalidado.
fn intercambia(work_path: &str, hacia_atras: bool) -> Result<HistoryState, String> {
    invalidate_doc_cache(work_path);
    let origen = con(work_path, |h| {
        let pila = if hacia_atras {
            &mut h.deshacer
        } else {
            &mut h.rehacer
        };
        pila.pop()
    })
    .ok_or(if hacia_atras {
        "Nada que deshacer"
    } else {
        "Nada que rehacer"
    })?;
    let actual = con(work_path, |h| {
        let p = ruta_instantanea(work_path, h.seq);
        h.seq += 1;
        p
    });
    std::fs::copy(work_path, &actual).map_err(|e| e.to_string())?;
    if let Err(e) = std::fs::rename(&origen.snap, work_path) {
        let _ = std::fs::remove_file(&actual);
        // devolver la instantánea a su pila: no se ha perdido nada
        con(work_path, |h| {
            if hacia_atras {
                h.deshacer.push(origen)
            } else {
                h.rehacer.push(origen)
            }
        });
        return Err(format!("No se ha podido restaurar el documento: {e}"));
    }
    // la protección viaja con el paso: deshacer «Quitar la contraseña…»
    // tiene que devolver también la contraseña
    let vuelve = Paso {
        snap: actual,
        proteccion: crate::seguridad::proteccion_de(work_path),
    };
    crate::seguridad::repon_proteccion(work_path, origen.proteccion);
    con(work_path, |h| {
        if hacia_atras {
            h.rehacer.push(vuelve)
        } else {
            h.deshacer.push(vuelve)
        }
    });
    estado(work_path)
}

/// Deshace la última mutación. Devuelve el estado del historial.
#[tauri::command(async)]
pub fn undo(work_path: String) -> Result<HistoryState, String> {
    on_pdfium_thread(move || intercambia(&work_path, true)).map_err(mensaje_llano)
}

/// Rehace la última mutación deshecha.
#[tauri::command(async)]
pub fn redo(work_path: String) -> Result<HistoryState, String> {
    on_pdfium_thread(move || intercambia(&work_path, false)).map_err(mensaje_llano)
}

/// Pasos disponibles en cada dirección.
#[tauri::command(async)]
pub fn history_state(work_path: String) -> Result<HistoryState, String> {
    on_pdfium_thread(move || estado(&work_path)).map_err(mensaje_llano)
}

/// Funde los últimos `steps` pasos en uno solo (para que una acción de la UI
/// hecha con varios comandos se deshaga de golpe): se conserva la
/// instantánea más antigua del grupo, que es el estado anterior a todos.
#[tauri::command(async)]
pub fn squash_history(work_path: String, steps: u16) -> Result<HistoryState, String> {
    on_pdfium_thread(move || {
        let sobran = con(&work_path, |h| {
            let n = (steps as usize).min(h.deshacer.len());
            let quitar: Vec<Paso> = if n > 1 {
                let desde = h.deshacer.len() - n + 1;
                h.deshacer.drain(desde..).collect()
            } else {
                vec![]
            };
            quitar
        });
        for p in sobran {
            let _ = std::fs::remove_file(p.snap);
        }
        estado(&work_path)
    })
}

/// Borra todas las instantáneas del documento y olvida su historial (al
/// cerrar el documento).
pub(crate) fn limpia(work_path: &str) {
    let h = {
        let mut mapa = HISTORIAL.lock().unwrap_or_else(|e| e.into_inner());
        mapa.remove(work_path)
    };
    if let Some(h) = h {
        for p in h.deshacer.into_iter().chain(h.rehacer) {
            let _ = std::fs::remove_file(p.snap);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::{crea_pdf, textos_de};
    use crate::{anotaciones, busqueda, paginas, seguridad};

    fn instantaneas(work: &str) -> usize {
        let prefijo = format!("{}.snap", nombre_base(work));
        std::fs::read_dir(directorio())
            .map(|d| {
                d.filter_map(|e| e.ok())
                    .filter(|e| e.file_name().to_string_lossy().starts_with(&prefijo))
                    .count()
            })
            .unwrap_or(0)
    }

    fn fixture(nombre: &str, paginas: &[&str]) -> String {
        let pdf = std::env::temp_dir().join(format!("historial-{nombre}.pdf"));
        crea_pdf(paginas, &pdf);
        let work = pdf.to_string_lossy().to_string();
        limpia(&work);
        work
    }

    #[test]
    fn deshacer_borrado_de_pagina_restaura_recuento() {
        let work = fixture("paginas", &["Uno", "Dos", "Tres"]);
        assert_eq!(paginas::delete_page(work.clone(), 0).unwrap(), 2);
        assert_eq!(
            history_state(work.clone()).unwrap(),
            HistoryState {
                undo: 1,
                redo: 0,
                page_count: 2
            }
        );
        let e = undo(work.clone()).unwrap();
        assert_eq!(
            e,
            HistoryState {
                undo: 0,
                redo: 1,
                page_count: 3
            }
        );
        assert_eq!(textos_de(std::path::Path::new(&work))[0], "Uno");
        let e = redo(work.clone()).unwrap();
        assert_eq!(
            e,
            HistoryState {
                undo: 1,
                redo: 0,
                page_count: 2
            }
        );
        assert_eq!(textos_de(std::path::Path::new(&work))[0], "Dos");
        // el caché se invalidó: el render y el texto ven el estado nuevo
        assert!(!busqueda::get_page_text(work.clone(), 0)
            .unwrap()
            .chars
            .is_empty());
        assert!(undo(work.clone()).is_ok());
        assert!(undo(work.clone()).is_err(), "sin pasos debe fallar");
        limpia(&work);
        assert_eq!(instantaneas(&work), 0);
    }

    #[test]
    fn anotacion_deshacer_rehacer_y_rama_nueva() {
        let work = fixture("anotacion", &["Página"]);
        anotaciones::add_note(work.clone(), 0, 100.0, 100.0, "hola".into(), None).unwrap();
        assert_eq!(
            anotaciones::get_annotations(work.clone(), 0).unwrap().len(),
            1
        );
        undo(work.clone()).unwrap();
        assert_eq!(
            anotaciones::get_annotations(work.clone(), 0).unwrap().len(),
            0
        );
        redo(work.clone()).unwrap();
        assert_eq!(
            anotaciones::get_annotations(work.clone(), 0).unwrap().len(),
            1
        );
        undo(work.clone()).unwrap();
        // una mutación nueva descarta la rama de rehacer y sus ficheros
        anotaciones::add_stroke(
            work.clone(),
            0,
            vec![[10.0, 10.0], [50.0, 50.0]],
            None,
            None,
            None,
        )
        .unwrap();
        let e = history_state(work.clone()).unwrap();
        assert_eq!((e.undo, e.redo), (1, 0));
        assert_eq!(instantaneas(&work), 1);
        limpia(&work);
    }

    #[test]
    fn tope_de_pasos_y_mutacion_fallida() {
        let work = fixture("tope", &["Página"]);
        for _ in 0..(MAX_PASOS + 5) {
            paginas::rotate_page(work.clone(), 0).unwrap();
        }
        assert_eq!(
            history_state(work.clone()).unwrap().undo as usize,
            MAX_PASOS
        );
        assert_eq!(instantaneas(&work), MAX_PASOS);
        // una mutación que falla no deja paso ni fichero
        assert!(anotaciones::remove_annotation(work.clone(), 0, 99).is_err());
        assert_eq!(
            history_state(work.clone()).unwrap().undo as usize,
            MAX_PASOS
        );
        assert_eq!(instantaneas(&work), MAX_PASOS);
        for _ in 0..MAX_PASOS {
            undo(work.clone()).unwrap();
        }
        assert!(undo(work.clone()).is_err());
        limpia(&work);
        assert_eq!(instantaneas(&work), 0);
    }

    #[test]
    fn dry_run_no_crea_paso() {
        let work = fixture("dryrun", &["Página"]);
        let r = crate::Rect {
            x: 40.0,
            y: 130.0,
            w: 200.0,
            h: 30.0,
        };
        seguridad::redact_area(work.clone(), 0, r, true).unwrap();
        crate::paginas2::remove_marginal_text(work.clone(), "header".into(), true).unwrap();
        assert_eq!(history_state(work.clone()).unwrap().undo, 0);
        limpia(&work);
    }

    #[test]
    fn cirugia_y_metadatos_pasan_por_historial() {
        let work = fixture("cirugia", &["Página"]);
        let r = crate::Rect {
            x: 50.0,
            y: 50.0,
            w: 120.0,
            h: 20.0,
        };
        crate::formularios2::create_form_field(
            work.clone(),
            0,
            "text".into(),
            r,
            "campo".into(),
            None,
            None,
            None,
            None,
        )
        .unwrap();
        assert_eq!(
            crate::formularios::get_form_fields(work.clone(), 0)
                .unwrap()
                .len(),
            1
        );
        crate::documento::set_metadata(
            work.clone(),
            crate::documento::Metadata {
                title: "T".into(),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(history_state(work.clone()).unwrap().undo, 2);
        undo(work.clone()).unwrap();
        assert_eq!(
            crate::documento::get_metadata(work.clone()).unwrap().title,
            ""
        );
        undo(work.clone()).unwrap();
        assert_eq!(
            crate::formularios::get_form_fields(work.clone(), 0)
                .unwrap()
                .len(),
            0
        );
        redo(work.clone()).unwrap();
        assert_eq!(
            crate::formularios::get_form_fields(work.clone(), 0)
                .unwrap()
                .len(),
            1
        );
        limpia(&work);
    }

    #[test]
    fn agrupar_pasos() {
        let work = fixture("agrupar", &["Página"]);
        let antes = crate::get_page_sizes(work.clone()).unwrap()[0].width;
        paginas::rotate_page(work.clone(), 0).unwrap();
        paginas::rotate_page(work.clone(), 0).unwrap();
        paginas::rotate_page(work.clone(), 0).unwrap();
        let e = squash_history(work.clone(), 3).unwrap();
        assert_eq!((e.undo, e.redo), (1, 0));
        assert_eq!(instantaneas(&work), 1);
        undo(work.clone()).unwrap();
        assert_eq!(crate::get_page_sizes(work.clone()).unwrap()[0].width, antes);
        assert_eq!(history_state(work.clone()).unwrap().undo, 0);
        limpia(&work);
    }

    #[test]
    fn un_comando_por_modulo_deja_paso() {
        let work = fixture("modulos", &["Texto base"]);
        let mut esperados = 0;
        let mut cuenta = |work: &str| {
            esperados += 1;
            assert_eq!(history_state(work.to_string()).unwrap().undo, esperados);
        };
        crate::texto::add_text_block(
            work.clone(),
            0,
            60.0,
            400.0,
            "Nuevo".into(),
            12.0,
            None,
            None,
            None,
            None,
            None,
        )
        .unwrap();
        cuenta(&work);
        crate::anotaciones2::add_stamp(
            work.clone(),
            0,
            "OK".into(),
            [200, 0, 0, 255],
            200.0,
            200.0,
            20.0,
            None,
            None,
        )
        .unwrap();
        cuenta(&work);
        crate::paginas2::add_blank_page(work.clone(), 1).unwrap();
        cuenta(&work);
        // foto con ruido para que compress_pdf tenga algo que reducir de
        // verdad (si no reduce devuelve Err y no deja paso)
        let mut foto = image::RgbaImage::new(1600, 1200);
        for (x, y, p) in foto.enumerate_pixels_mut() {
            let h = x
                .wrapping_mul(2654435761)
                .wrapping_add(y.wrapping_mul(2246822519))
                .rotate_left(13)
                .wrapping_mul(2654435761);
            *p = image::Rgba([h as u8, (h >> 8) as u8, (h >> 16) as u8, 255]);
        }
        let mut buf = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(foto)
            .write_to(&mut buf, image::ImageFormat::Png)
            .unwrap();
        let b64 = {
            use base64::Engine;
            base64::engine::general_purpose::STANDARD.encode(buf.into_inner())
        };
        crate::firmas_visuales::stamp_signature(work.clone(), 0, b64, 50.0, 300.0, 300.0, 225.0)
            .unwrap();
        cuenta(&work);
        seguridad::flatten_pdf(work.clone()).unwrap();
        cuenta(&work);
        crate::exportar::compress_pdf(work.clone(), 60, 72, None, None, None).unwrap();
        cuenta(&work);
        limpia(&work);
    }
}
