//! Creación de campos de formulario y enlaces (PDFium no los crea: cirugía
//! con lopdf, mismo patrón que el campo de firma de firma.rs). El borrado de
//! campos no se ofrece en v1 (dejaría huérfanos en /Fields); los enlaces son
//! anotaciones normales y se borran con remove_annotation.

use crate::{invalidate_doc_cache, on_pdfium_thread, Rect};
use lopdf::{Dictionary, Document as LoDoc, Object, ObjectId, Stream, StringFormat};

/// MediaBox de una página, buscando en el propio dict o heredado del árbol.
fn media_box(doc: &LoDoc, page_id: ObjectId) -> Result<[f32; 4], String> {
    let mut actual = page_id;
    for _ in 0..32 {
        let dict = doc
            .get_object(actual)
            .and_then(|o| o.as_dict())
            .map_err(|e| e.to_string())?;
        if let Ok(mb) = dict.get(b"MediaBox") {
            let mb = match mb {
                Object::Reference(rid) => doc
                    .get_object(*rid)
                    .and_then(|o| o.as_array())
                    .map_err(|e| e.to_string())?,
                Object::Array(a) => a,
                _ => return Err("MediaBox inválido".into()),
            };
            let v: Vec<f32> = mb
                .iter()
                .map(|o| match o {
                    Object::Integer(i) => *i as f32,
                    Object::Real(r) => *r,
                    _ => 0.0,
                })
                .collect();
            if v.len() == 4 {
                return Ok([v[0], v[1], v[2], v[3]]);
            }
        }
        match dict.get(b"Parent") {
            Ok(Object::Reference(rid)) => actual = *rid,
            _ => break,
        }
    }
    Err("La página no tiene MediaBox".into())
}

/// Añade una anotación al array Annots de la página (directo o referencia).
fn anade_a_annots(doc: &mut LoDoc, page_id: ObjectId, annot_id: ObjectId) -> Result<(), String> {
    let annots_ref = {
        let page = doc
            .get_object(page_id)
            .and_then(|o| o.as_dict())
            .map_err(|e| e.to_string())?;
        match page.get(b"Annots") {
            Ok(Object::Reference(rid)) => Some(*rid),
            _ => None,
        }
    };
    if let Some(rid) = annots_ref {
        doc.get_object_mut(rid)
            .and_then(|o| o.as_array_mut())
            .map_err(|e| e.to_string())?
            .push(Object::Reference(annot_id));
    } else {
        let page = doc
            .get_object_mut(page_id)
            .and_then(|o| o.as_dict_mut())
            .map_err(|e| e.to_string())?;
        match page.get_mut(b"Annots") {
            Ok(Object::Array(arr)) => arr.push(Object::Reference(annot_id)),
            _ => page.set("Annots", Object::Array(vec![Object::Reference(annot_id)])),
        }
    }
    Ok(())
}

/// Carga el PDF con lopdf, ejecuta `f` y guarda con .tmp + rename. El caché
/// de PDFium se invalida ANTES (mantiene el fichero abierto de forma perezosa
/// y en Windows el rename fallaría).
fn cirugia(
    work_path: &str,
    f: impl FnOnce(&mut LoDoc) -> Result<(), String>,
) -> Result<(), String> {
    on_pdfium_thread(invalidate_doc_cache);
    let mut doc =
        LoDoc::load(work_path).map_err(|e| format!("No se pudo leer el PDF: {e}"))?;
    if doc.is_encrypted() {
        return Err("El documento está cifrado: quita la contraseña antes".into());
    }
    f(&mut doc)?;
    let tmp = format!("{work_path}.tmp");
    doc.save(&tmp).map_err(|e| format!("No se pudo guardar: {e}"))?;
    std::fs::rename(&tmp, work_path).map_err(|e| e.to_string())
}

/// Rect de UI (origen arriba-izquierda) a array Rect PDF de la página dada.
fn rect_pdf(rect: &Rect, mb: &[f32; 4]) -> Object {
    let alto = mb[3] - mb[1];
    let x0 = mb[0] + rect.x;
    let y1 = mb[1] + alto - rect.y;
    Object::Array(vec![
        x0.into(),
        (y1 - rect.h).into(),
        (x0 + rect.w).into(),
        y1.into(),
    ])
}

/// Crea un campo de formulario (texto o casilla) en la página.
#[tauri::command]
pub fn create_form_field(
    work_path: String,
    page_index: u16,
    kind: String,
    rect: Rect,
    name: String,
) -> Result<(), String> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err("El campo necesita un nombre".into());
    }
    if rect.w < 8.0 || rect.h < 8.0 {
        return Err("El área del campo es demasiado pequeña".into());
    }
    cirugia(&work_path, |doc| {
        let page_id = *doc
            .get_pages()
            .get(&(page_index as u32 + 1))
            .ok_or("Página fuera de rango")?;
        let mb = media_box(doc, page_id)?;

        // nombres existentes para garantizar unicidad de T
        let existentes: Vec<String> = doc
            .objects
            .values()
            .filter_map(|o| o.as_dict().ok())
            .filter(|d| {
                matches!(
                    d.get(b"Subtype").and_then(|s| s.as_name()),
                    Ok(b"Widget")
                )
            })
            .filter_map(|d| d.get(b"T").ok())
            .filter_map(|t| match t {
                Object::String(b, _) => Some(String::from_utf8_lossy(b).into_owned()),
                _ => None,
            })
            .collect();
        let mut nombre = name.clone();
        let mut n = 2;
        while existentes.contains(&nombre) {
            nombre = format!("{name}-{n}");
            n += 1;
        }

        let mut widget = Dictionary::new();
        widget.set("Type", Object::Name(b"Annot".to_vec()));
        widget.set("Subtype", Object::Name(b"Widget".to_vec()));
        widget.set("Rect", rect_pdf(&rect, &mb));
        widget.set("T", Object::string_literal(nombre));
        widget.set("F", 4i64); // Print
        widget.set("DA", Object::string_literal("/Helv 0 Tf 0 g"));
        let mut mk = Dictionary::new();
        mk.set(
            "BC",
            Object::Array(vec![0.into(), 0.into(), 0.into()]),
        );
        widget.set("MK", Object::Dictionary(mk));
        match kind.as_str() {
            "text" => {
                widget.set("FT", Object::Name(b"Tx".to_vec()));
                widget.set("V", Object::string_literal(""));
            }
            "checkbox" => {
                widget.set("FT", Object::Name(b"Btn".to_vec()));
                widget.set("V", Object::Name(b"Off".to_vec()));
                widget.set("AS", Object::Name(b"Off".to_vec()));
                // /AP con estados Off y Yes: muchos visores no pintan
                // casillas sin apariencia aunque haya NeedAppearances
                let bbox = |c: &str| {
                    let mut d = Dictionary::new();
                    d.set("Type", Object::Name(b"XObject".to_vec()));
                    d.set("Subtype", Object::Name(b"Form".to_vec()));
                    d.set(
                        "BBox",
                        Object::Array(vec![
                            0.into(),
                            0.into(),
                            rect.w.into(),
                            rect.h.into(),
                        ]),
                    );
                    d.set("Resources", Object::Dictionary(Dictionary::new()));
                    Stream::new(d, c.as_bytes().to_vec())
                };
                let off_id = doc.add_object(bbox("").clone());
                let aspa = format!(
                    "q 0 g 1.5 w 2 2 m {} {} l S 2 {} m {} 2 l S Q",
                    rect.w - 2.0,
                    rect.h - 2.0,
                    rect.h - 2.0,
                    rect.w - 2.0
                );
                let yes_id = doc.add_object(bbox(&aspa).clone());
                let mut estados = Dictionary::new();
                estados.set("Off", Object::Reference(off_id));
                estados.set("Yes", Object::Reference(yes_id));
                let mut ap = Dictionary::new();
                ap.set("N", Object::Dictionary(estados));
                widget.set("AP", Object::Dictionary(ap));
            }
            otro => return Err(format!("Tipo de campo desconocido: {otro}")),
        }
        let widget_id = doc.add_object(widget);
        anade_a_annots(doc, page_id, widget_id)?;

        // AcroForm del catálogo: crear o fusionar (sin tocar SigFlags)
        let catalog_id = doc
            .trailer
            .get(b"Root")
            .and_then(|o| o.as_reference())
            .map_err(|e| e.to_string())?;
        let existente: Option<Dictionary> = {
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
        let mut form = existente.unwrap_or_default();
        match form.get_mut(b"Fields") {
            Ok(Object::Array(arr)) => arr.push(Object::Reference(widget_id)),
            _ => form.set("Fields", Object::Array(vec![Object::Reference(widget_id)])),
        }
        form.set("DA", Object::string_literal("/Helv 0 Tf 0 g"));
        form.set("NeedAppearances", Object::Boolean(true));
        let mut helv = Dictionary::new();
        helv.set("Type", Object::Name(b"Font".to_vec()));
        helv.set("Subtype", Object::Name(b"Type1".to_vec()));
        helv.set("BaseFont", Object::Name(b"Helvetica".to_vec()));
        helv.set("Encoding", Object::Name(b"WinAnsiEncoding".to_vec()));
        let mut fuentes = Dictionary::new();
        fuentes.set("Helv", Object::Dictionary(helv));
        let mut dr = Dictionary::new();
        dr.set("Font", Object::Dictionary(fuentes));
        form.set("DR", Object::Dictionary(dr));
        doc.get_object_mut(catalog_id)
            .and_then(|o| o.as_dict_mut())
            .map_err(|e| e.to_string())?
            .set("AcroForm", Object::Dictionary(form));
        Ok(())
    })
}

/// Crea un enlace en la página: a una URL externa o a otra página.
#[tauri::command]
pub fn create_link(
    work_path: String,
    page_index: u16,
    rect: Rect,
    uri: Option<String>,
    dest_page: Option<u16>,
) -> Result<(), String> {
    let uri = uri.filter(|u| !u.trim().is_empty());
    if uri.is_some() == dest_page.is_some() {
        return Err("Indica o una URL o una página de destino (solo una)".into());
    }
    cirugia(&work_path, |doc| {
        let paginas = doc.get_pages();
        let page_id = *paginas
            .get(&(page_index as u32 + 1))
            .ok_or("Página fuera de rango")?;
        let mb = media_box(doc, page_id)?;
        let mut link = Dictionary::new();
        link.set("Type", Object::Name(b"Annot".to_vec()));
        link.set("Subtype", Object::Name(b"Link".to_vec()));
        link.set("Rect", rect_pdf(&rect, &mb));
        link.set(
            "Border",
            Object::Array(vec![0.into(), 0.into(), 0.into()]),
        );
        if let Some(u) = uri {
            let mut a = Dictionary::new();
            a.set("S", Object::Name(b"URI".to_vec()));
            a.set(
                "URI",
                Object::String(u.trim().as_bytes().to_vec(), StringFormat::Literal),
            );
            link.set("A", Object::Dictionary(a));
        } else if let Some(p) = dest_page {
            let destino = *paginas
                .get(&(p as u32 + 1))
                .ok_or("Página de destino fuera de rango")?;
            link.set(
                "Dest",
                Object::Array(vec![
                    Object::Reference(destino),
                    Object::Name(b"XYZ".to_vec()),
                    Object::Null,
                    Object::Null,
                    Object::Null,
                ]),
            );
        }
        let link_id = doc.add_object(link);
        anade_a_annots(doc, page_id, link_id)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::crea_pdf;

    #[test]
    fn campo_de_texto_visible_y_rellenable_por_pdfium() {
        let pdf = std::env::temp_dir().join("formularios2-texto-test.pdf");
        crea_pdf(&["Solicitud"], &pdf);
        let work = pdf.to_string_lossy().to_string();
        create_form_field(
            work.clone(),
            0,
            "text".into(),
            Rect {
                x: 100.0,
                y: 200.0,
                w: 180.0,
                h: 24.0,
            },
            "nombre".into(),
        )
        .expect("crear campo");
        let campos = crate::get_form_fields(work.clone(), 0).expect("listar");
        assert_eq!(campos.len(), 1);
        assert_eq!(campos[0].name, "nombre");
        assert_eq!(campos[0].kind, "Text");
        assert!((campos[0].x - 100.0).abs() < 1.0);
        crate::set_form_text(work.clone(), 0, campos[0].annot_index, "Jorge".into())
            .expect("rellenar");
        let campos = crate::get_form_fields(work, 0).expect("relistar");
        assert_eq!(campos[0].value, "Jorge");
    }

    #[test]
    fn casilla_marcable_y_nombres_unicos() {
        let pdf = std::env::temp_dir().join("formularios2-casilla-test.pdf");
        crea_pdf(&["Consentimiento"], &pdf);
        let work = pdf.to_string_lossy().to_string();
        let r = Rect {
            x: 80.0,
            y: 300.0,
            w: 16.0,
            h: 16.0,
        };
        create_form_field(work.clone(), 0, "checkbox".into(), r.clone(), "acepto".into())
            .expect("crear casilla");
        // mismo nombre otra vez: debe renombrarse a acepto-2
        create_form_field(
            work.clone(),
            0,
            "checkbox".into(),
            Rect { y: 330.0, ..r },
            "acepto".into(),
        )
        .expect("segunda casilla");
        let campos = crate::get_form_fields(work.clone(), 0).expect("listar");
        assert_eq!(campos.len(), 2);
        let nombres: Vec<&str> = campos.iter().map(|c| c.name.as_str()).collect();
        assert!(nombres.contains(&"acepto") && nombres.contains(&"acepto-2"), "{nombres:?}");
        let idx = campos.iter().find(|c| c.name == "acepto").unwrap().annot_index;
        crate::set_form_checked(work.clone(), 0, idx, true).expect("marcar");
        let campos = crate::get_form_fields(work, 0).expect("relistar");
        assert!(campos.iter().find(|c| c.name == "acepto").unwrap().checked);
    }

    #[test]
    fn enlaces_uri_y_pagina_y_borrado() {
        let pdf = std::env::temp_dir().join("formularios2-enlaces-test.pdf");
        crea_pdf(&["Uno", "Dos"], &pdf);
        let work = pdf.to_string_lossy().to_string();
        let r = Rect {
            x: 50.0,
            y: 100.0,
            w: 120.0,
            h: 18.0,
        };
        create_link(
            work.clone(),
            0,
            r.clone(),
            Some("https://ejemplo.es".into()),
            None,
        )
        .expect("enlace uri");
        create_link(work.clone(), 0, Rect { y: 130.0, ..r }, None, Some(1))
            .expect("enlace página");
        let links = crate::documento::get_links(work.clone(), 0).expect("listar");
        assert_eq!(links.len(), 2);
        assert!(links.iter().any(|l| l.uri.as_deref() == Some("https://ejemplo.es")));
        assert!(links.iter().any(|l| l.dest_page == Some(1)));
        // exactamente uno de los dos parámetros
        assert!(create_link(work.clone(), 0, r.clone(), None, None).is_err());
        // borrar el primero vía remove_annotation (es una anotación normal)
        let annots = crate::get_annotations(work.clone(), 0).expect("annots");
        let link_annot = annots.iter().find(|a| a.kind == "Link").expect("hay Link");
        crate::remove_annotation(work.clone(), 0, link_annot.index).expect("borrar");
        assert_eq!(crate::documento::get_links(work, 0).expect("relistar").len(), 1);
    }
}
