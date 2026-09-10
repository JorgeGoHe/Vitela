//! El flujo de revisión de un documento entre dos personas: responder a un
//! comentario, ponerle estado y sacar la lista para leerla fuera.
//!
//! Todo se guarda **como lo guarda Acrobat**, no en una clave propia: la
//! gracia de responder a un comentario es que la respuesta se vea en el
//! programa del otro. Una respuesta es una anotación `/Text` con `/IRT`
//! (*in reply to*) apuntando al comentario y `/RT /Reply`; un estado es
//! otra anotación hija con `/RT /StateModel`, `/StateModel /Review` y
//! `/State`, que es exactamente el sitio donde Acrobat lo escribe y lo lee.
//!
//! Las anotaciones de estado **no son comentarios**: `get_annotations` no
//! las lista (como no lista los `/Popup`) y su `/State` sale en el campo
//! `state` del comentario al que pertenecen.

use crate::historial::mutacion;
use crate::{cirugia_en_hilo, on_pdfium_thread};
use lopdf::{Dictionary, Object};

/// Los cuatro estados de revisión del spec, que son los que enseña Acrobat
/// en su desplegable. `""` o `"None"` quitan el estado.
pub const ESTADOS: [&str; 4] = ["Accepted", "Rejected", "Cancelled", "Completed"];

/// Responde a un comentario: crea una anotación `/Text` con `/IRT`
/// apuntando a la original y `/RT /Reply`, con su autor, su fecha y su
/// ventana emergente. Devuelve el índice de la respuesta dentro del
/// `/Annots` de la página, que es el que maneja el resto de comandos.
///
/// La respuesta se coloca donde el comentario al que responde: en el panel
/// se lee anidada bajo su padre (`in_reply_to` de `get_annotations`), y en
/// la página no estorba con un icono en otro sitio.
#[tauri::command(async)]
pub fn reply_annotation(
    work_path: String,
    page_index: u16,
    annot_index: u16,
    text: String,
    author: Option<String>,
) -> Result<u16, String> {
    if text.trim().is_empty() {
        return Err("La respuesta está vacía".into());
    }
    let autor = crate::anotaciones::autor_o_sistema(author);
    let fecha = crate::anotaciones::fecha_pdf_ahora();
    mutacion(work_path, move |work_path| {
        on_pdfium_thread(move || {
            let mut indice = 0u16;
            cirugia_en_hilo(&work_path, |doc| {
                let page_id = *doc
                    .get_pages()
                    .get(&(page_index as u32 + 1))
                    .ok_or("Página fuera de rango")?;
                let padre = crate::anotaciones::annot_id(doc, page_index, annot_index as usize)?;
                let rect = rect_de(doc, padre).ok_or("Ese comentario ya no está")?;
                let mut annot = Dictionary::new();
                annot.set("Type", Object::Name(b"Annot".to_vec()));
                annot.set("Subtype", Object::Name(b"Text".to_vec()));
                annot.set("Name", Object::Name(b"Comment".to_vec()));
                annot.set(
                    "Rect",
                    Object::Array(rect.iter().map(|v| (*v).into()).collect()),
                );
                annot.set("Contents", crate::documento::cadena_pdf(&text));
                annot.set("T", crate::documento::cadena_pdf(&autor));
                annot.set("M", Object::string_literal(fecha.clone()));
                annot.set("CreationDate", Object::string_literal(fecha.clone()));
                annot.set("IRT", Object::Reference(padre));
                annot.set("RT", Object::Name(b"Reply".to_vec()));
                annot.set("F", 4i64); // Print
                let id = doc.add_object(annot);
                crate::formularios2::anade_a_annots(doc, page_id, id)?;
                crate::anotaciones::anade_popup_de(doc, page_index, id)?;
                indice = crate::anotaciones::lista_annots(doc, page_index)
                    .and_then(|lista| {
                        lista
                            .iter()
                            .position(|o| matches!(o, Object::Reference(r) if *r == id))
                    })
                    .ok_or("No se ha podido añadir la respuesta")? as u16;
                Ok(())
            })?;
            Ok(indice)
        })
    })
}

/// Pone (o quita) el estado de revisión de un comentario: «Aceptado»,
/// «Rechazado», «Cancelado» o «Completado», los cuatro de Acrobat.
///
/// El estado no va en el comentario sino en una anotación hija con
/// `/RT /StateModel`, que es donde lo escribe Acrobat: así el revisor que
/// abra el PDF allí lo ve. Se guarda una sola por comentario —la de quien
/// está revisando— y `""` o `"None"` la quitan.
#[tauri::command(async)]
pub fn set_annotation_state(
    work_path: String,
    page_index: u16,
    annot_index: u16,
    state: String,
    author: Option<String>,
) -> Result<(), String> {
    let quitar = state.is_empty() || state == "None";
    if !quitar && !ESTADOS.contains(&state.as_str()) {
        return Err(format!(
            "«{state}» no es un estado de revisión: son {}",
            ESTADOS.join(", ")
        ));
    }
    let autor = crate::anotaciones::autor_o_sistema(author);
    let fecha = crate::anotaciones::fecha_pdf_ahora();
    crate::cirugia(&work_path, move |doc| {
        let page_id = *doc
            .get_pages()
            .get(&(page_index as u32 + 1))
            .ok_or("Página fuera de rango")?;
        let padre = crate::anotaciones::annot_id(doc, page_index, annot_index as usize)?;
        // si ya tenía estado, se reescribe el hijo que lo llevaba
        if let Some(hijo) = hijo_de_estado(doc, page_index, padre) {
            if quitar {
                if let Some(i) = indice_de(doc, page_index, hijo) {
                    crate::anotaciones::quita_annot(doc, page_index, i)?;
                }
                return Ok(());
            }
            let d = doc
                .get_object_mut(hijo)
                .and_then(|o| o.as_dict_mut())
                .map_err(|e| e.to_string())?;
            d.set("State", crate::documento::cadena_pdf(&state));
            d.set("M", Object::string_literal(fecha));
            return Ok(());
        }
        if quitar {
            return Ok(());
        }
        let rect = rect_de(doc, padre).ok_or("Ese comentario ya no está")?;
        let mut annot = Dictionary::new();
        annot.set("Type", Object::Name(b"Annot".to_vec()));
        annot.set("Subtype", Object::Name(b"Text".to_vec()));
        annot.set(
            "Rect",
            Object::Array(rect.iter().map(|v| (*v).into()).collect()),
        );
        // Acrobat deja el texto vacío y pone el estado en /State
        annot.set("Contents", crate::documento::cadena_pdf(""));
        annot.set("T", crate::documento::cadena_pdf(&autor));
        annot.set("M", Object::string_literal(fecha.clone()));
        annot.set("CreationDate", Object::string_literal(fecha));
        annot.set("IRT", Object::Reference(padre));
        annot.set("RT", Object::Name(b"StateModel".to_vec()));
        annot.set("StateModel", crate::documento::cadena_pdf("Review"));
        annot.set("State", crate::documento::cadena_pdf(&state));
        // Hidden: el sello del estado lo pinta el panel, no la página
        annot.set("F", 2i64);
        let id = doc.add_object(annot);
        crate::formularios2::anade_a_annots(doc, page_id, id)
    })
}

/// «Resumen de comentarios»: la lista entera del documento en un fichero de
/// texto, para leerla fuera o mandarla por correo. Por ahora solo en llano;
/// el FDF/XFDF que Acrobat también exporta **se deja para otro ciclo** y se
/// dice aquí para que no se busque.
///
/// `document_name` es **el nombre que se enseña en la cabecera**:
/// `work_path` es siempre la copia de trabajo, y encabezar el fichero que
/// se le manda a
/// alguien con «Comentarios de vitela-doc-1789032860175765000.pdf» no le
/// dice nada a nadie (AC-060). Sin él se deduce del nombre de la copia,
/// que lleva dentro el del original; el nombre del temporal no sale nunca.
///
/// Escribe fuera del documento, así que no muta nada ni deja paso de
/// deshacer.
#[tauri::command(async)]
pub fn export_comments(
    work_path: String,
    dest_path: String,
    document_name: Option<String>,
) -> Result<u32, String> {
    let comentarios = crate::anotaciones::get_document_annotations(work_path.clone())?;
    if comentarios.is_empty() {
        return Err("El documento no tiene comentarios que resumir".into());
    }
    let nombre = nombre_de_documento(document_name.as_deref(), &work_path);
    let mut out = format!("Comentarios de {nombre}\n\n");
    let mut n = 0u32;
    for c in &comentarios {
        // las respuestas van debajo de su comentario, con sangría y **sin
        // repetir la página ni el tipo**, que son los del comentario al que
        // contestan: en un resumen que se lee de arriba abajo eso es ruido,
        // y Acrobat tampoco los repite
        let respuesta = c.annot.in_reply_to.is_some();
        let sangria = if respuesta { "    " } else { "" };
        let mut cabecera: Vec<String> = Vec::new();
        if respuesta {
            cabecera.push("En respuesta".into());
        } else {
            cabecera.push(format!(
                "Página {} · {}",
                c.page_index + 1,
                tipo_en_llano(&c.annot.kind)
            ));
        }
        if !c.annot.author.is_empty() {
            cabecera.push(c.annot.author.clone());
        }
        if !c.annot.modified.is_empty() {
            cabecera.push(fecha_en_espanol(&c.annot.modified));
        }
        if !c.annot.state.is_empty() {
            cabecera.push(estado_en_llano(&c.annot.state).to_string());
        }
        out.push_str(&format!("{sangria}{}\n", cabecera.join(" · ")));
        if !c.annot.contents.trim().is_empty() {
            for linea in c.annot.contents.lines() {
                out.push_str(&format!("{sangria}  {linea}\n"));
            }
        }
        out.push('\n');
        n += 1;
    }
    std::fs::write(&dest_path, out)
        .map_err(|e| crate::mensaje_llano(format!("No se ha podido escribir {dest_path}: {e}")))?;
    Ok(n)
}

/// El nombre del documento para la cabecera del resumen: el que manda la UI
/// (que puede ser la ruta del original entero) o, si no manda ninguno, el
/// que lleva dentro la copia de trabajo — `vitela-<nombre>-<nanos>.pdf`, de
/// donde sale `<nombre>.pdf`—. **El nombre del temporal no se enseña
/// nunca**: no significa nada para quien recibe el fichero.
pub(crate) fn nombre_de_documento(document_name: Option<&str>, path: &str) -> String {
    let solo_fichero = |s: &str| {
        std::path::Path::new(s)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| s.to_string())
    };
    if let Some(n) = document_name.map(str::trim).filter(|n| !n.is_empty()) {
        return solo_fichero(n);
    }
    let fichero = solo_fichero(path);
    if let Some(stem) = fichero
        .strip_prefix("vitela-")
        .and_then(|r| r.strip_suffix(".pdf"))
    {
        if let Some((nombre, nanos)) = stem.rsplit_once('-') {
            if !nombre.is_empty() && !nanos.is_empty() && nanos.bytes().all(|b| b.is_ascii_digit())
            {
                return format!("{nombre}.pdf");
            }
        }
    }
    fichero
}

/// Una fecha ISO 8601 escrita como se escribe en español: «10/09/2026
/// 00:25». El resumen se le manda a una persona, y
/// `2026-09-10T00:25:56+02:00` es la lengua de la máquina.
pub(crate) fn fecha_en_espanol(iso: &str) -> String {
    chrono::DateTime::parse_from_rfc3339(iso)
        .map(|d| d.format("%d/%m/%Y %H:%M").to_string())
        .unwrap_or_else(|_| iso.to_string())
}

/// El tipo de anotación en la lengua del usuario: en el resumen no puede
/// poner «Squiggly».
pub(crate) fn tipo_en_llano(kind: &str) -> &'static str {
    match kind {
        "Text" => "Nota",
        "Highlight" => "Resaltado",
        "Underline" => "Subrayado",
        "StrikeOut" => "Tachado",
        "FreeText" => "Cuadro de texto",
        "Ink" => "Dibujo",
        "Square" => "Rectángulo",
        "Circle" => "Elipse",
        "Line" => "Línea",
        "Stamp" => "Sello",
        _ => "Comentario",
    }
}

/// Los cuatro estados, en español.
pub(crate) fn estado_en_llano(state: &str) -> &'static str {
    match state {
        "Accepted" => "Aceptado",
        "Rejected" => "Rechazado",
        "Cancelled" => "Cancelado",
        "Completed" => "Completado",
        _ => "Sin estado",
    }
}

/// El `/Rect` de una anotación, tal cual.
fn rect_de(doc: &lopdf::Document, id: lopdf::ObjectId) -> Option<[f32; 4]> {
    let v: Vec<f32> = doc
        .get_object(id)
        .ok()?
        .as_dict()
        .ok()?
        .get(b"Rect")
        .ok()?
        .as_array()
        .ok()?
        .iter()
        .filter_map(|o| match o {
            Object::Integer(i) => Some(*i as f32),
            Object::Real(r) => Some(*r),
            _ => None,
        })
        .collect();
    (v.len() == 4).then(|| [v[0], v[1], v[2], v[3]])
}

/// La anotación de estado de un comentario, si la tiene.
fn hijo_de_estado(
    doc: &lopdf::Document,
    page_index: u16,
    padre: lopdf::ObjectId,
) -> Option<lopdf::ObjectId> {
    crate::anotaciones::lista_annots(doc, page_index)?
        .iter()
        .filter_map(|o| match o {
            Object::Reference(r) => Some(*r),
            _ => None,
        })
        .find(|id| {
            let Ok(d) = doc.get_object(*id).and_then(|o| o.as_dict()) else {
                return false;
            };
            d.get(b"IRT").and_then(|o| o.as_reference()).ok() == Some(padre)
                && d.get(b"RT")
                    .and_then(|o| o.as_name())
                    .map(|n| n == b"StateModel")
                    .unwrap_or(false)
        })
}

/// Posición de una anotación dentro del `/Annots` de su página.
fn indice_de(doc: &lopdf::Document, page_index: u16, id: lopdf::ObjectId) -> Option<usize> {
    crate::anotaciones::lista_annots(doc, page_index)?
        .iter()
        .position(|o| matches!(o, Object::Reference(r) if *r == id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::anotaciones::{
        add_note, get_annotations, get_document_annotations, remove_annotation,
    };
    use crate::tests::crea_pdf;

    /// **G5.** Un comentario tiene conversación: se responde, la respuesta
    /// se guarda como la guarda Acrobat (`/IRT` + `/RT /Reply`) y el panel
    /// la puede anidar bajo su padre.
    #[test]
    fn responder_a_un_comentario_deja_la_respuesta_colgando_de_el() {
        let pdf = std::env::temp_dir().join("comentarios-responder.pdf");
        crea_pdf(&["Documento"], &pdf);
        let work = pdf.to_string_lossy().into_owned();
        add_note(
            work.clone(),
            0,
            100.0,
            200.0,
            "¿Esto está bien?".into(),
            Some("Ana".into()),
        )
        .expect("nota");
        let nota = &get_annotations(work.clone(), 0).expect("anotaciones")[0];
        let padre = nota.index;

        let i = reply_annotation(
            work.clone(),
            0,
            padre,
            "Sí, revisado".into(),
            Some("Jorge".into()),
        )
        .expect("responder");

        let annots = get_annotations(work.clone(), 0).expect("anotaciones");
        let respuesta = annots.iter().find(|a| a.index == i).expect("la respuesta");
        assert_eq!(respuesta.contents, "Sí, revisado");
        assert_eq!(respuesta.author, "Jorge");
        assert_eq!(
            respuesta.in_reply_to,
            Some(padre),
            "la respuesta cuelga del comentario, para que el panel la anide"
        );
        assert!(
            annots
                .iter()
                .any(|a| a.index == padre && a.in_reply_to.is_none()),
            "y el comentario original no cuelga de nadie"
        );
        // una respuesta vacía no es una respuesta
        assert!(reply_annotation(work.clone(), 0, padre, "   ".into(), None).is_err());
        std::fs::remove_file(&pdf).ok();
    }

    /// **G5.** El estado se guarda donde lo guarda Acrobat —una anotación
    /// hija con `/RT /StateModel`— y **no es un comentario**: no sale en el
    /// panel, sale en el campo `state` del comentario al que pertenece.
    #[test]
    fn el_estado_va_en_su_hija_y_sale_en_el_comentario() {
        let pdf = std::env::temp_dir().join("comentarios-estado.pdf");
        crea_pdf(&["Documento"], &pdf);
        let work = pdf.to_string_lossy().into_owned();
        add_note(
            work.clone(),
            0,
            100.0,
            200.0,
            "Repasar".into(),
            Some("Ana".into()),
        )
        .expect("nota");
        let padre = get_annotations(work.clone(), 0).expect("anotaciones")[0].index;
        let cuantas = || get_annotations(work.clone(), 0).expect("anotaciones").len();
        let antes = cuantas();

        set_annotation_state(
            work.clone(),
            0,
            padre,
            "Completed".into(),
            Some("Jorge".into()),
        )
        .expect("poner estado");
        assert_eq!(
            cuantas(),
            antes,
            "el estado no es un comentario más en el panel"
        );
        let c = get_annotations(work.clone(), 0)
            .expect("anotaciones")
            .into_iter()
            .find(|a| a.index == padre)
            .expect("el comentario");
        assert_eq!(c.state, "Completed");

        // está escrito en el fichero como lo escribe Acrobat
        let bytes = std::fs::read(&work).expect("leer");
        let texto = String::from_utf8_lossy(&bytes);
        assert!(texto.contains("/StateModel"), "falta el /StateModel");

        // cambiarlo reescribe el mismo hijo, no añade otro
        set_annotation_state(work.clone(), 0, padre, "Rejected".into(), None).expect("cambiar");
        let c = get_annotations(work.clone(), 0)
            .expect("anotaciones")
            .into_iter()
            .find(|a| a.index == padre)
            .expect("el comentario");
        assert_eq!(c.state, "Rejected");
        assert_eq!(cuantas(), antes);

        // y quitarlo lo quita
        set_annotation_state(work.clone(), 0, padre, String::new(), None).expect("quitar");
        let c = get_annotations(work.clone(), 0)
            .expect("anotaciones")
            .into_iter()
            .find(|a| a.index == padre)
            .expect("el comentario");
        assert_eq!(c.state, "", "sin estado");

        // un estado inventado se dice, con los que valen
        let e = set_annotation_state(work.clone(), 0, padre, "Pendiente".into(), None).unwrap_err();
        assert!(e.contains("Accepted"), "{e}");
        std::fs::remove_file(&pdf).ok();
    }

    /// **G5.** Borrar el comentario padre se lleva sus respuestas y su
    /// estado: si no, quedan colgando de un objeto que ya no está y el
    /// panel enseña respuestas sin pregunta.
    #[test]
    fn borrar_un_comentario_se_lleva_sus_respuestas() {
        let pdf = std::env::temp_dir().join("comentarios-borrar-hilo.pdf");
        crea_pdf(&["Documento"], &pdf);
        let work = pdf.to_string_lossy().into_owned();
        add_note(
            work.clone(),
            0,
            100.0,
            200.0,
            "Padre".into(),
            Some("Ana".into()),
        )
        .expect("nota");
        add_note(
            work.clone(),
            0,
            300.0,
            200.0,
            "De otro sitio".into(),
            Some("Ana".into()),
        )
        .expect("otra nota");
        let padre = get_annotations(work.clone(), 0).expect("anotaciones")[0].index;
        reply_annotation(work.clone(), 0, padre, "Una".into(), None).expect("responder");
        reply_annotation(work.clone(), 0, padre, "Dos".into(), None).expect("responder");
        set_annotation_state(work.clone(), 0, padre, "Accepted".into(), None).expect("estado");

        remove_annotation(work.clone(), 0, padre).expect("borrar el padre");
        let quedan = get_annotations(work.clone(), 0).expect("anotaciones");
        assert_eq!(
            quedan.len(),
            1,
            "solo tenía que quedar el comentario de otro sitio: {:?}",
            quedan
                .iter()
                .map(|a| a.contents.clone())
                .collect::<Vec<_>>()
        );
        assert_eq!(quedan[0].contents, "De otro sitio");
        assert!(
            quedan.iter().all(|a| a.in_reply_to.is_none()),
            "no quedan respuestas huérfanas"
        );
        std::fs::remove_file(&pdf).ok();
    }

    /// **G5.** El resumen: la lista entera en llano, con la página, el
    /// autor y el estado, y las respuestas sangradas bajo su comentario.
    /// Nada de «Squiggly» ni de «Accepted» en un fichero que se lee.
    #[test]
    fn el_resumen_de_comentarios_se_lee_sin_saber_de_pdf() {
        let dir = std::env::temp_dir();
        let pdf = dir.join("comentarios-resumen.pdf");
        let txt = dir.join("comentarios-resumen.txt");
        crea_pdf(&["Uno", "Dos"], &pdf);
        let work = pdf.to_string_lossy().into_owned();
        add_note(
            work.clone(),
            0,
            100.0,
            200.0,
            "Falta la fecha".into(),
            Some("Ana".into()),
        )
        .expect("nota");
        let padre = get_annotations(work.clone(), 0).expect("anotaciones")[0].index;
        reply_annotation(
            work.clone(),
            0,
            padre,
            "Corregido".into(),
            Some("Jorge".into()),
        )
        .expect("responder");
        set_annotation_state(work.clone(), 0, padre, "Completed".into(), None).expect("estado");
        add_note(
            work.clone(),
            1,
            100.0,
            200.0,
            "Y aquí también".into(),
            Some("Ana".into()),
        )
        .expect("nota 2");

        let n = export_comments(
            work.clone(),
            txt.to_string_lossy().into_owned(),
            Some("/Users/ana/contratos/contrato.pdf".into()),
        )
        .expect("exportar");
        assert_eq!(n, 3, "dos notas y una respuesta");
        let resumen = std::fs::read_to_string(&txt).expect("leer");
        assert!(resumen.contains("Falta la fecha") && resumen.contains("Y aquí también"));
        assert!(resumen.contains("Página 1") && resumen.contains("Página 2"));
        assert!(resumen.contains("Ana") && resumen.contains("Jorge"));
        assert!(resumen.contains("Completado"), "el estado, en español");
        assert!(!resumen.contains("Completed"), "y no en el del spec");
        assert!(resumen.contains("Nota"), "el tipo, en llano");
        // **AC-060**: la cabecera lleva el nombre del documento, no la ruta
        // ni el temporal
        assert!(
            resumen.starts_with("Comentarios de contrato.pdf"),
            "la cabecera: {}",
            resumen.lines().next().unwrap_or("")
        );
        // **Distinto 4**: la respuesta va sangrada y NO repite la página ni
        // el tipo de su comentario
        assert!(
            resumen.contains("    En respuesta · Jorge"),
            "la respuesta va sangrada y sin repetir la cabecera:\n{resumen}"
        );
        assert_eq!(
            resumen.matches("Página 1").count(),
            1,
            "la página se dice una vez, no también en la respuesta:\n{resumen}"
        );
        // y la fecha en español, no en ISO 8601
        assert!(
            !resumen.contains('T') || !resumen.contains("+0"),
            "la fecha no sale en ISO:\n{resumen}"
        );
        assert!(
            resumen.contains('/'),
            "la fecha va como 10/09/2026 00:25:\n{resumen}"
        );
        // el documento no se ha tocado
        assert_eq!(
            get_document_annotations(work.clone()).expect("anots").len(),
            3
        );
        for f in [&pdf, &txt] {
            std::fs::remove_file(f).ok();
        }
    }

    /// **AC-060.** Sin nombre de documento, la cabecera lo saca de la copia
    /// de trabajo (`vitela-<nombre>-<nanos>.pdf`): el nombre del temporal,
    /// con sus diecinueve dígitos, no le dice nada a quien recibe el
    /// fichero.
    #[test]
    fn la_cabecera_del_resumen_no_ensena_nunca_el_nombre_del_temporal() {
        assert_eq!(
            nombre_de_documento(None, "/tmp/vitela-doc-1789032860175765000.pdf"),
            "doc.pdf"
        );
        assert_eq!(
            nombre_de_documento(None, "/tmp/vitela-contrato de alquiler-17890.pdf"),
            "contrato de alquiler.pdf"
        );
        // el que manda la UI gana, y si manda la ruta entera se queda el
        // nombre
        assert_eq!(
            nombre_de_documento(Some("/Users/ana/contrato.pdf"), "/tmp/vitela-doc-1.pdf"),
            "contrato.pdf"
        );
        assert_eq!(
            nombre_de_documento(Some("  "), "/tmp/factura.pdf"),
            "factura.pdf"
        );
        // un fichero que no es una copia de trabajo se enseña tal cual
        assert_eq!(
            nombre_de_documento(None, "/tmp/vitela-sin-nanos.pdf"),
            "vitela-sin-nanos.pdf"
        );
        assert_eq!(nombre_de_documento(None, "/tmp/factura.pdf"), "factura.pdf");
    }

    /// La fecha del resumen se escribe como se escribe en español.
    #[test]
    fn la_fecha_del_resumen_va_en_espanol() {
        assert_eq!(
            fecha_en_espanol("2026-09-10T00:25:56+02:00"),
            "10/09/2026 00:25"
        );
        // lo que no se sepa leer se deja tal cual antes que inventarlo
        assert_eq!(fecha_en_espanol(""), "");
        assert_eq!(fecha_en_espanol("ayer"), "ayer");
    }
}
