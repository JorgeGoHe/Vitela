//! Creación de campos de formulario y enlaces (PDFium no los crea: cirugía
//! con lopdf, mismo patrón que el campo de firma de firma.rs). El borrado de
//! campos no se ofrece en v1 (dejaría huérfanos en /Fields); los enlaces son
//! anotaciones normales y se borran con remove_annotation.

use crate::{cirugia, Rect};
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

/// `/Rotate` de la página (heredable, como el MediaBox).
fn rotacion(doc: &LoDoc, page_id: ObjectId) -> u16 {
    let mut actual = page_id;
    for _ in 0..32 {
        let Ok(dict) = doc.get_object(actual).and_then(|o| o.as_dict()) else {
            break;
        };
        if let Ok(r) = dict.get(b"Rotate").and_then(|o| o.as_i64()) {
            return r.rem_euclid(360) as u16;
        }
        match dict.get(b"Parent") {
            Ok(Object::Reference(rid)) => actual = *rid,
            _ => break,
        }
    }
    0
}

/// Geometría de la página para convertir las coordenadas de la UI, con su
/// rotación: sin ella, en una página girada el campo o el enlace caen fuera.
fn geo_pagina(doc: &LoDoc, page_id: ObjectId) -> Result<crate::Geo, String> {
    let mb = media_box(doc, page_id)?;
    Ok(crate::Geo::nueva(&mb, rotacion(doc, page_id)))
}

/// Rect de UI (origen arriba-izquierda) a array Rect PDF de la página dada.
fn rect_pdf(rect: &Rect, geo: &crate::Geo) -> Object {
    let r = geo.ui_rect_a_pdf(rect);
    Object::Array(vec![
        r.left().value.into(),
        r.bottom().value.into(),
        r.right().value.into(),
        r.top().value.into(),
    ])
}

/// Crea un campo de formulario (texto o casilla) en la página.
#[tauri::command(async)]
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
    cirugia(&work_path, move |doc| {
        let page_id = *doc
            .get_pages()
            .get(&(page_index as u32 + 1))
            .ok_or("Página fuera de rango")?;
        let geo = geo_pagina(doc, page_id)?;

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
        widget.set("Rect", rect_pdf(&rect, &geo));
        widget.set("T", Object::string_literal(nombre));
        widget.set("F", 4i64); // Print
        widget.set("DA", Object::string_literal("/Helv 0 Tf 0 g"));
        let mut mk = Dictionary::new();
        mk.set(
            "BC",
            Object::Array(vec![0.into(), 0.into(), 0.into()]),
        );
        if geo.rot != 0 {
            // /R gira el widget al revés que la página para que se lea
            // derecho, que es lo que hace Acrobat
            mk.set("R", Object::Integer(geo.rot as i64));
        }
        widget.set("MK", Object::Dictionary(mk));
        // la apariencia va en el espacio del PDF: con la página rotada, el
        // ancho de la UI es el alto del PDF
        let caja = geo.ui_rect_a_pdf(&rect);
        let (ancho, alto) = (
            caja.right().value - caja.left().value,
            caja.top().value - caja.bottom().value,
        );
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
                            ancho.into(),
                            alto.into(),
                        ]),
                    );
                    d.set("Resources", Object::Dictionary(Dictionary::new()));
                    Stream::new(d, c.as_bytes().to_vec())
                };
                let off_id = doc.add_object(bbox("").clone());
                let aspa = format!(
                    "q 0 g 1.5 w 2 2 m {} {} l S 2 {} m {} 2 l S Q",
                    ancho - 2.0,
                    alto - 2.0,
                    alto - 2.0,
                    ancho - 2.0
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

/// Solo se escriben enlaces web y de correo; sin esquema se asume https.
fn normaliza_uri(u: &str) -> Result<String, String> {
    let u = u.trim();
    let con_esquema = if u.contains(':') { u.to_string() } else { format!("https://{u}") };
    let esquema = con_esquema.split(':').next().unwrap_or("").to_ascii_lowercase();
    if matches!(esquema.as_str(), "http" | "https" | "mailto") {
        Ok(con_esquema)
    } else {
        Err(format!("Solo se admiten enlaces http, https o mailto (no «{esquema}»)"))
    }
}

/// Crea un enlace en la página: a una URL externa o a otra página.
#[tauri::command(async)]
pub fn create_link(
    work_path: String,
    page_index: u16,
    rect: Rect,
    uri: Option<String>,
    dest_page: Option<u16>,
) -> Result<(), String> {
    let uri = uri.filter(|u| !u.trim().is_empty()).map(|u| normaliza_uri(&u)).transpose()?;
    if uri.is_some() == dest_page.is_some() {
        return Err("Indica o una URL o una página de destino (solo una)".into());
    }
    cirugia(&work_path, move |doc| {
        let paginas = doc.get_pages();
        let page_id = *paginas
            .get(&(page_index as u32 + 1))
            .ok_or("Página fuera de rango")?;
        let geo = geo_pagina(doc, page_id)?;
        let mut link = Dictionary::new();
        link.set("Type", Object::Name(b"Annot".to_vec()));
        link.set("Subtype", Object::Name(b"Link".to_vec()));
        link.set("Rect", rect_pdf(&rect, &geo));
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

    /// En una página rotada, el campo y el enlace tienen que quedar donde
    /// se dibujó el área: el `/Rect` vive en el espacio SIN rotar, así que
    /// hay que convertirlo, no volcarlo tal cual.
    #[test]
    fn campos_y_enlaces_en_una_pagina_rotada() {
        let pdf = std::env::temp_dir().join("formularios2-rotada-test.pdf");
        crea_pdf(&["Solicitud"], &pdf);
        let work = pdf.to_string_lossy().to_string();
        crate::paginas::rotate_page(work.clone(), 0).expect("girar 90°");

        let area = Rect { x: 100.0, y: 400.0, w: 150.0, h: 30.0 };
        create_form_field(work.clone(), 0, "text".into(), area.clone(), "nombre".into())
            .expect("crear campo");
        create_link(
            work.clone(),
            0,
            Rect { x: 100.0, y: 200.0, w: 150.0, h: 30.0 },
            Some("https://ejemplo.org".into()),
            None,
        )
        .expect("crear enlace");

        let campo = &crate::formularios::get_form_fields(work.clone(), 0).expect("listar")[0];
        assert!(
            (campo.x - 100.0).abs() < 1.0
                && (campo.y - 400.0).abs() < 1.0
                && (campo.w - 150.0).abs() < 1.0
                && (campo.h - 30.0).abs() < 1.0,
            "el campo se lee en ({},{}) {}×{}",
            campo.x,
            campo.y,
            campo.w,
            campo.h
        );
        let enlace = &crate::documento::get_links(work.clone(), 0).expect("enlaces")[0];
        assert!(
            (enlace.x - 100.0).abs() < 1.0 && (enlace.y - 200.0).abs() < 1.0,
            "el enlace se lee en ({},{})",
            enlace.x,
            enlace.y
        );

        // y en el fichero, el /Rect está dentro de la página sin rotar:
        // (100,400) de la UI con /Rotate 90 es (400,100) en el PDF
        let doc = LoDoc::load(&work).expect("cargar");
        let page_id = *doc.get_pages().get(&1).expect("página 1");
        let annots = doc
            .get_object(page_id)
            .and_then(|o| o.as_dict())
            .and_then(|d| d.get(b"Annots"))
            .and_then(|o| o.as_array())
            .expect("Annots")
            .clone();
        let rid = annots[0].as_reference().expect("referencia");
        let r: Vec<f32> = doc
            .get_object(rid)
            .and_then(|o| o.as_dict())
            .and_then(|d| d.get(b"Rect"))
            .and_then(|o| o.as_array())
            .expect("Rect")
            .iter()
            .map(|o| match o {
                Object::Integer(i) => *i as f32,
                Object::Real(v) => *v,
                _ => 0.0,
            })
            .collect();
        assert!(
            (r[0] - 400.0).abs() < 1.0
                && (r[1] - 100.0).abs() < 1.0
                && (r[2] - 430.0).abs() < 1.0
                && (r[3] - 250.0).abs() < 1.0,
            "/Rect del campo: {r:?}"
        );
        std::fs::remove_file(&pdf).ok();
    }

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
        let campos = crate::formularios::get_form_fields(work.clone(), 0).expect("listar");
        assert_eq!(campos.len(), 1);
        assert_eq!(campos[0].name, "nombre");
        assert_eq!(campos[0].kind, "Text");
        assert!((campos[0].x - 100.0).abs() < 1.0);
        crate::formularios::set_form_text(work.clone(), 0, campos[0].annot_index, "Jorge".into())
            .expect("rellenar");
        let campos = crate::formularios::get_form_fields(work, 0).expect("relistar");
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
        let campos = crate::formularios::get_form_fields(work.clone(), 0).expect("listar");
        assert_eq!(campos.len(), 2);
        let nombres: Vec<&str> = campos.iter().map(|c| c.name.as_str()).collect();
        assert!(nombres.contains(&"acepto") && nombres.contains(&"acepto-2"), "{nombres:?}");
        let idx = campos.iter().find(|c| c.name == "acepto").unwrap().annot_index;
        crate::formularios::set_form_checked(work.clone(), 0, idx, true).expect("marcar");
        let campos = crate::formularios::get_form_fields(work, 0).expect("relistar");
        assert!(campos.iter().find(|c| c.name == "acepto").unwrap().checked);
    }

    #[test]
    fn campo_creado_y_borrado() {
        let pdf = std::env::temp_dir().join("formularios2-borrar-test.pdf");
        crea_pdf(&["Baja"], &pdf);
        let work = pdf.to_string_lossy().to_string();
        create_form_field(
            work.clone(),
            0,
            "text".into(),
            Rect { x: 60.0, y: 200.0, w: 140.0, h: 22.0 },
            "efimero".into(),
        )
        .expect("crear");
        assert_eq!(crate::formularios::get_form_fields(work.clone(), 0).unwrap().len(), 1);
        delete_form_field(work.clone(), "efimero".into()).expect("borrar");
        assert_eq!(crate::formularios::get_form_fields(work.clone(), 0).unwrap().len(), 0);
        assert!(delete_form_field(work, "no-existe".into()).is_err());
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
        // esquemas peligrosos fuera; sin esquema se asume https
        assert!(create_link(work.clone(), 0, r.clone(), Some("file:///etc/passwd".into()), None)
            .is_err());
        assert!(create_link(work.clone(), 0, r.clone(), Some("javascript:alert(1)".into()), None)
            .is_err());
        create_link(work.clone(), 0, r.clone(), Some("ejemplo.org/x".into()), None)
            .expect("sin esquema");
        let links = crate::documento::get_links(work.clone(), 0).expect("listar");
        assert!(links.iter().any(|l| l.uri.as_deref() == Some("https://ejemplo.org/x")));
        // el annot_index de get_links es el mismo que da get_annotations
        let annots = crate::anotaciones::get_annotations(work.clone(), 0).expect("annots");
        let de_annots: Vec<u16> = annots
            .iter()
            .filter(|a| a.kind == "Link")
            .map(|a| a.index)
            .collect();
        let de_links: Vec<u16> = links.iter().map(|l| l.annot_index).collect();
        assert_eq!(de_annots, de_links);
        // borrar el primero con ese índice (es lo que hace la UI)
        crate::anotaciones::remove_annotation(work.clone(), 0, links[0].annot_index)
            .expect("borrar");
        let quedan = crate::documento::get_links(work, 0).expect("relistar");
        assert_eq!(quedan.len(), 2);
        assert!(quedan.iter().all(|l| l.uri != links[0].uri));
    }
}

/// Borra un campo de formulario por nombre: quita el widget de los Annots de
/// su página y la referencia de /Fields del AcroForm.
#[tauri::command(async)]
pub fn delete_form_field(work_path: String, name: String) -> Result<(), String> {
    cirugia(&work_path, move |doc| {
        // localizar el widget por su T
        let widget_id = doc
            .objects
            .iter()
            .find(|(_, o)| {
                o.as_dict()
                    .map(|d| {
                        matches!(d.get(b"Subtype").and_then(|s| s.as_name()), Ok(b"Widget"))
                            && matches!(
                                d.get(b"T"),
                                Ok(Object::String(t, _)) if String::from_utf8_lossy(t) == name
                            )
                    })
                    .unwrap_or(false)
            })
            .map(|(id, _)| *id)
            .ok_or_else(|| format!("No existe el campo «{name}»"))?;
        // quitarlo de los Annots de todas las páginas (directo o referencia)
        let paginas: Vec<ObjectId> = doc.get_pages().values().copied().collect();
        for page_id in paginas {
            let annots_ref = {
                let Ok(page) = doc.get_object(page_id).and_then(|o| o.as_dict()) else {
                    continue;
                };
                match page.get(b"Annots") {
                    Ok(Object::Reference(rid)) => Some(*rid),
                    _ => None,
                }
            };
            if let Some(rid) = annots_ref {
                if let Ok(arr) = doc.get_object_mut(rid).and_then(|o| o.as_array_mut()) {
                    arr.retain(|o| o.as_reference().ok() != Some(widget_id));
                }
            } else if let Ok(page) = doc.get_object_mut(page_id).and_then(|o| o.as_dict_mut()) {
                if let Ok(Object::Array(arr)) = page.get_mut(b"Annots") {
                    arr.retain(|o| o.as_reference().ok() != Some(widget_id));
                }
            }
        }
        // y de /Fields del AcroForm
        let catalog_id = doc
            .trailer
            .get(b"Root")
            .and_then(|o| o.as_reference())
            .map_err(|e| e.to_string())?;
        let form_es_ref = {
            let catalog = doc
                .get_object(catalog_id)
                .and_then(|o| o.as_dict())
                .map_err(|e| e.to_string())?;
            match catalog.get(b"AcroForm") {
                Ok(Object::Reference(rid)) => Some(*rid),
                _ => None,
            }
        };
        let quita = |form: &mut Dictionary| {
            if let Ok(Object::Array(arr)) = form.get_mut(b"Fields") {
                arr.retain(|o| o.as_reference().ok() != Some(widget_id));
            }
        };
        if let Some(rid) = form_es_ref {
            if let Ok(form) = doc.get_object_mut(rid).and_then(|o| o.as_dict_mut()) {
                quita(form);
            }
        } else if let Ok(catalog) = doc.get_object_mut(catalog_id).and_then(|o| o.as_dict_mut()) {
            if let Ok(Object::Dictionary(form)) = catalog.get_mut(b"AcroForm") {
                quita(form);
            }
        }
        Ok(())
    })
}
