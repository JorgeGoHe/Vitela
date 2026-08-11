//! Propiedades y navegación del documento: marcadores (outline), metadatos
//! y enlaces. La lectura usa PDFium; la escritura (outline, metadatos) se
//! hace con lopdf porque PDFium no la expone.

use crate::{on_pdfium_thread, invalidate_doc_cache, with_doc};
use lopdf::{Dictionary, Document as LoDoc, Object, ObjectId, StringFormat};
use pdfium_render::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone)]
pub struct OutlineNode {
    pub title: String,
    pub page_index: Option<u16>,
    pub children: Vec<OutlineNode>,
}

fn nodo_de(b: &PdfBookmark) -> OutlineNode {
    let page_index = b
        .destination()
        .and_then(|d| d.page_index().ok())
        .or_else(|| {
            b.action().and_then(|a| match a {
                PdfAction::LocalDestination(l) => {
                    l.destination().ok().and_then(|d| d.page_index().ok())
                }
                _ => None,
            })
        })
        .map(|i| i as u16);
    let children = b.iter_direct_children().map(|c| nodo_de(&c)).collect();
    OutlineNode {
        title: b.title().unwrap_or_default(),
        page_index,
        children,
    }
}

/// Árbol de marcadores del documento.
#[tauri::command]
pub fn get_outline(path: String) -> Result<Vec<OutlineNode>, String> {
    on_pdfium_thread(move || {
        with_doc(&path, |doc| {
            let mut out = Vec::new();
            let mut actual = doc.bookmarks().root();
            while let Some(b) = actual {
                out.push(nodo_de(&b));
                actual = b.next_sibling();
            }
            Ok(out)
        })
    })
}

/// Codifica un texto como cadena PDF: literal si es ASCII, UTF-16BE con BOM
/// en caso contrario (los acentos en literal UTF-8 se leerían mal).
fn cadena_pdf(text: &str) -> Object {
    if text.is_ascii() {
        Object::string_literal(text)
    } else {
        let mut bytes = vec![0xFE, 0xFF];
        for unit in text.encode_utf16() {
            bytes.extend_from_slice(&unit.to_be_bytes());
        }
        Object::String(bytes, StringFormat::Hexadecimal)
    }
}

/// Reescribe el árbol /Outlines completo con lopdf.
#[tauri::command]
pub fn set_outline(work_path: String, nodes: Vec<OutlineNode>) -> Result<(), String> {
    let mut doc =
        LoDoc::load(&work_path).map_err(|e| format!("No se pudo leer el PDF: {e}"))?;
    let paginas: Vec<ObjectId> = doc.get_pages().values().copied().collect();

    fn construye(
        doc: &mut LoDoc,
        nodes: &[OutlineNode],
        parent: ObjectId,
        paginas: &[ObjectId],
    ) -> Result<(Option<ObjectId>, Option<ObjectId>, i64), String> {
        let mut primero = None;
        let mut anterior: Option<ObjectId> = None;
        let mut total = 0i64;
        for node in nodes {
            let id = doc.add_object(Dictionary::new());
            if primero.is_none() {
                primero = Some(id);
            }
            let (hijo_primero, hijo_ultimo, hijos) =
                construye(doc, &node.children, id, paginas)?;
            let mut d = Dictionary::new();
            d.set("Title", cadena_pdf(&node.title));
            d.set("Parent", Object::Reference(parent));
            if let Some(p) = node.page_index {
                if let Some(page_id) = paginas.get(p as usize) {
                    d.set(
                        "Dest",
                        Object::Array(vec![
                            Object::Reference(*page_id),
                            Object::Name(b"XYZ".to_vec()),
                            Object::Null,
                            Object::Null,
                            Object::Null,
                        ]),
                    );
                }
            }
            if let Some(h) = hijo_primero {
                d.set("First", Object::Reference(h));
            }
            if let Some(h) = hijo_ultimo {
                d.set("Last", Object::Reference(h));
                d.set("Count", hijos);
            }
            if let Some(prev) = anterior {
                d.set("Prev", Object::Reference(prev));
                // encadenar el Next del anterior
                if let Ok(pd) = doc.get_object_mut(prev).and_then(|o| o.as_dict_mut()) {
                    pd.set("Next", Object::Reference(id));
                }
            }
            *doc.get_object_mut(id)
                .and_then(|o| o.as_dict_mut())
                .map_err(|e| e.to_string())? = d;
            anterior = Some(id);
            total += 1 + hijos;
        }
        Ok((primero, anterior, total))
    }

    let outlines_id = doc.add_object(Dictionary::new());
    let (primero, ultimo, total) = construye(&mut doc, &nodes, outlines_id, &paginas)?;
    let mut outlines = Dictionary::new();
    outlines.set("Type", Object::Name(b"Outlines".to_vec()));
    if let Some(p) = primero {
        outlines.set("First", Object::Reference(p));
    }
    if let Some(u) = ultimo {
        outlines.set("Last", Object::Reference(u));
    }
    outlines.set("Count", total);
    *doc.get_object_mut(outlines_id)
        .and_then(|o| o.as_dict_mut())
        .map_err(|e| e.to_string())? = outlines;

    let catalog_id = doc
        .trailer
        .get(b"Root")
        .and_then(|o| o.as_reference())
        .map_err(|e| e.to_string())?;
    doc.get_object_mut(catalog_id)
        .and_then(|o| o.as_dict_mut())
        .map_err(|e| e.to_string())?
        .set("Outlines", Object::Reference(outlines_id));

    doc.save(&work_path)
        .map_err(|e| format!("No se pudo guardar: {e}"))?;
    on_pdfium_thread(invalidate_doc_cache);
    Ok(())
}

#[derive(Serialize, Deserialize, Default)]
pub struct Metadata {
    pub title: String,
    pub author: String,
    pub subject: String,
    pub keywords: String,
    pub creator: String,
    pub producer: String,
}

/// Metadatos del documento (diccionario /Info).
#[tauri::command]
pub fn get_metadata(path: String) -> Result<Metadata, String> {
    on_pdfium_thread(move || {
        with_doc(&path, |doc| {
            let m = doc.metadata();
            let lee = |t: PdfDocumentMetadataTagType| {
                m.get(t).map(|v| v.value().to_string()).unwrap_or_default()
            };
            Ok(Metadata {
                title: lee(PdfDocumentMetadataTagType::Title),
                author: lee(PdfDocumentMetadataTagType::Author),
                subject: lee(PdfDocumentMetadataTagType::Subject),
                keywords: lee(PdfDocumentMetadataTagType::Keywords),
                creator: lee(PdfDocumentMetadataTagType::Creator),
                producer: lee(PdfDocumentMetadataTagType::Producer),
            })
        })
    })
}

/// Escribe título, autor, asunto y palabras clave en /Info (lopdf).
#[tauri::command]
pub fn set_metadata(work_path: String, meta: Metadata) -> Result<(), String> {
    let mut doc =
        LoDoc::load(&work_path).map_err(|e| format!("No se pudo leer el PDF: {e}"))?;
    let mut info = match doc.trailer.get(b"Info") {
        Ok(Object::Reference(rid)) => doc
            .get_object(*rid)
            .ok()
            .and_then(|o| o.as_dict().ok())
            .cloned()
            .unwrap_or_default(),
        Ok(Object::Dictionary(d)) => d.clone(),
        _ => Dictionary::new(),
    };
    for (clave, valor) in [
        ("Title", &meta.title),
        ("Author", &meta.author),
        ("Subject", &meta.subject),
        ("Keywords", &meta.keywords),
    ] {
        if valor.trim().is_empty() {
            info.remove(clave.as_bytes());
        } else {
            info.set(clave, cadena_pdf(valor.trim()));
        }
    }
    let info_id = doc.add_object(info);
    doc.trailer.set("Info", Object::Reference(info_id));
    doc.save(&work_path)
        .map_err(|e| format!("No se pudo guardar: {e}"))?;
    on_pdfium_thread(invalidate_doc_cache);
    Ok(())
}

#[derive(Serialize)]
pub struct LinkInfo {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub uri: Option<String>,
    pub dest_page: Option<u16>,
}

/// Enlaces de una página (bounds en coords de UI) con su destino.
#[tauri::command]
pub fn get_links(path: String, page_index: u16) -> Result<Vec<LinkInfo>, String> {
    on_pdfium_thread(move || {
        with_doc(&path, |doc| {
            let page = doc.pages().get(page_index).map_err(|e| e.to_string())?;
            let page_h = page.height().value;
            let mut out = Vec::new();
            for link in page.links().iter() {
                let Ok(r) = link.rect() else { continue };
                let mut uri = None;
                let mut dest_page = link
                    .destination()
                    .and_then(|d| d.page_index().ok())
                    .map(|i| i as u16);
                if let Some(action) = link.action() {
                    match action {
                        PdfAction::Uri(u) => uri = u.uri().ok(),
                        PdfAction::LocalDestination(l) => {
                            if dest_page.is_none() {
                                dest_page = l
                                    .destination()
                                    .ok()
                                    .and_then(|d| d.page_index().ok())
                                    .map(|i| i as u16);
                            }
                        }
                        _ => {}
                    }
                }
                if uri.is_none() && dest_page.is_none() {
                    continue;
                }
                out.push(LinkInfo {
                    x: r.left().value,
                    y: page_h - r.top().value,
                    w: r.right().value - r.left().value,
                    h: r.top().value - r.bottom().value,
                    uri,
                    dest_page,
                });
            }
            Ok(out)
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::crea_pdf;

    #[test]
    fn outline_ida_y_vuelta() {
        let pdf = std::env::temp_dir().join("documento-outline-test.pdf");
        crea_pdf(&["Uno", "Dos", "Tres"], &pdf);
        let work = pdf.to_string_lossy().to_string();
        assert!(get_outline(work.clone()).expect("sin outline").is_empty());
        set_outline(
            work.clone(),
            vec![
                OutlineNode {
                    title: "Introducción".into(),
                    page_index: Some(0),
                    children: vec![OutlineNode {
                        title: "Sección española: años".into(),
                        page_index: Some(1),
                        children: vec![],
                    }],
                },
                OutlineNode {
                    title: "Final".into(),
                    page_index: Some(2),
                    children: vec![],
                },
            ],
        )
        .expect("escribir outline");
        let leido = get_outline(work).expect("leer outline");
        assert_eq!(leido.len(), 2);
        assert_eq!(leido[0].title, "Introducción");
        assert_eq!(leido[0].page_index, Some(0));
        assert_eq!(leido[0].children.len(), 1);
        assert_eq!(leido[0].children[0].title, "Sección española: años");
        assert_eq!(leido[0].children[0].page_index, Some(1));
        assert_eq!(leido[1].title, "Final");
        assert_eq!(leido[1].page_index, Some(2));
    }

    #[test]
    fn metadatos_ida_y_vuelta() {
        let pdf = std::env::temp_dir().join("documento-meta-test.pdf");
        crea_pdf(&["Página"], &pdf);
        let work = pdf.to_string_lossy().to_string();
        set_metadata(
            work.clone(),
            Metadata {
                title: "Informe años núñez".into(),
                author: "Jorge".into(),
                subject: "Pruebas".into(),
                keywords: "pdf, editor".into(),
                ..Default::default()
            },
        )
        .expect("escribir metadatos");
        let m = get_metadata(work).expect("leer metadatos");
        assert_eq!(m.title, "Informe años núñez");
        assert_eq!(m.author, "Jorge");
        assert_eq!(m.subject, "Pruebas");
        assert_eq!(m.keywords, "pdf, editor");
    }
}
