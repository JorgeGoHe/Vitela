//! Propiedades y navegación del documento: marcadores (outline), metadatos
//! y enlaces. La lectura usa PDFium; la escritura (outline, metadatos) se
//! hace con lopdf porque PDFium no la expone.

use crate::{cirugia, on_pdfium_thread, with_doc};
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
        });
    let children = b.iter_direct_children().map(|c| nodo_de(&c)).collect();
    OutlineNode {
        title: b.title().unwrap_or_default(),
        page_index,
        children,
    }
}

/// Árbol de marcadores del documento.
#[tauri::command(async)]
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
pub(crate) fn cadena_pdf(text: &str) -> Object {
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
#[tauri::command(async)]
pub fn set_outline(work_path: String, nodes: Vec<OutlineNode>) -> Result<(), String> {
    cirugia(&work_path, move |doc| {
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
        let (primero, ultimo, total) = construye(doc, &nodes, outlines_id, &paginas)?;
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
        Ok(())
    })
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

/// Lo que se puede decir de un PDF sin abrirlo: cuántas páginas trae, qué
/// ocupa y si está protegido.
#[derive(Serialize, Debug)]
pub struct PdfInfo {
    pub page_count: u16,
    /// Tamaño del fichero en bytes (la UI lo pasa a KB o MB).
    pub bytes: u64,
    /// Lleva contraseña: abrirlo pedirá una.
    pub encrypted: bool,
}

/// Ficha rápida de un PDF **sin abrirlo**: no crea copia de trabajo, no
/// toca el historial y no pasa por PDFium. Es lo que necesita la rejilla de
/// combinar («12 páginas · 1,4 MB» por fila, para no ordenar a ciegas
/// ficheros elegidos por el nombre), y también la lista de recientes para
/// marcar los que están protegidos.
///
/// De un documento cifrado se dice lo que se sabe sin la contraseña: que lo
/// está y lo que ocupa. Nunca se pide contraseña desde aquí.
#[tauri::command(async)]
pub fn pdf_info(path: String) -> Result<PdfInfo, String> {
    let bytes = std::fs::metadata(&path)
        .map(|m| m.len())
        .map_err(|e| crate::mensaje_llano(format!("No se ha podido leer «{path}»: {e}")))?;
    match LoDoc::load(&path) {
        Ok(doc) => Ok(PdfInfo {
            page_count: doc.get_pages().len() as u16,
            bytes,
            encrypted: doc.is_encrypted(),
        }),
        // un PDF con contraseña no se deja leer entero, y eso ya es
        // información: lo que no se puede es fallar y no decir nada
        Err(e) if trae_encrypt(&path) => {
            let _ = e;
            Ok(PdfInfo {
                page_count: 0,
                bytes,
                encrypted: true,
            })
        }
        Err(e) => Err(crate::mensaje_llano(format!(
            "No se ha podido leer el PDF: {e}"
        ))),
    }
}

/// ¿Trae el fichero un diccionario `/Encrypt`? Es lo único que se puede
/// mirar cuando el documento no se deja parsear.
fn trae_encrypt(path: &str) -> bool {
    std::fs::read(path)
        .map(|b| b.windows(8).any(|w| w == b"/Encrypt"))
        .unwrap_or(false)
}

/// Metadatos del documento (diccionario /Info).
#[tauri::command(async)]
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

/// Deja `/Creator (Vitela)` en el `/Info` del documento: quien lo abra
/// después sabe con qué se escribió, como hace cualquier editor.
pub(crate) fn marca_creador(doc: &mut lopdf::Document) {
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
    info.set("Creator", cadena_pdf("Vitela"));
    let info_id = doc.add_object(info);
    doc.trailer.set("Info", Object::Reference(info_id));
}

/// Escribe título, autor, asunto y palabras clave en /Info (lopdf).
#[tauri::command(async)]
pub fn set_metadata(work_path: String, meta: Metadata) -> Result<(), String> {
    cirugia(&work_path, move |doc| {
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
        Ok(())
    })
}

#[derive(Serialize)]
pub struct LinkInfo {
    /// Índice de la anotación en la página, el que entiende
    /// `remove_annotation` (para que la UI pueda borrar el enlace).
    pub annot_index: u16,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub uri: Option<String>,
    pub dest_page: Option<u16>,
}

/// Enlaces de una página (bounds en coords de UI) con su destino.
#[tauri::command(async)]
pub fn get_links(path: String, page_index: u16) -> Result<Vec<LinkInfo>, String> {
    on_pdfium_thread(move || {
        with_doc(&path, |doc| {
            let page = doc.pages().get(page_index).map_err(|e| e.to_string())?;
            let geo = crate::Geo::de_pagina(&page);
            let mut out = Vec::new();
            // se recorren las anotaciones (no `page.links()`) para poder
            // devolver el índice que entiende `remove_annotation`
            let annotations = page.annotations();
            for i in 0..annotations.len() {
                let Ok(annot) = annotations.get(i) else { continue };
                let Some(link) = annot.as_link_annotation().and_then(|l| l.link().ok()) else {
                    continue;
                };
                let Ok(r) = link.rect() else { continue };
                let mut uri = None;
                let mut dest_page = link
                    .destination()
                    .and_then(|d| d.page_index().ok());
                if let Some(action) = link.action() {
                    match action {
                        PdfAction::Uri(u) => uri = u.uri().ok(),
                        PdfAction::LocalDestination(l) if dest_page.is_none() => {
                            dest_page = l
                                .destination()
                                .ok()
                                .and_then(|d| d.page_index().ok());
                        }
                        _ => {}
                    }
                }
                if uri.is_none() && dest_page.is_none() {
                    continue;
                }
                let caja = geo.pdf_rect_a_ui(&r);
                out.push(LinkInfo {
                    annot_index: i as u16,
                    x: caja.x,
                    y: caja.y,
                    w: caja.w,
                    h: caja.h,
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

    /// La rejilla de combinar ordena ficheros elegidos por el nombre: hay
    /// que poder decir cuántas páginas trae y qué ocupa cada uno **sin
    /// abrirlos** (sin copia de trabajo, sin historial y sin pedir
    /// contraseña).
    #[test]
    fn la_ficha_de_un_pdf_se_lee_sin_abrirlo() {
        let pdf = std::env::temp_dir().join("documento-pdfinfo-test.pdf");
        crea_pdf(&["Uno", "Dos", "Tres"], &pdf);
        let ruta = pdf.to_string_lossy().into_owned();
        let info = pdf_info(ruta.clone()).expect("ficha");
        assert_eq!(info.page_count, 3);
        assert_eq!(info.bytes, std::fs::metadata(&pdf).expect("tamaño").len());
        assert!(info.bytes > 0);
        assert!(!info.encrypted);
        // no ha dejado copia de trabajo ni paso de deshacer
        assert!(
            crate::historial::history_state(ruta.clone()).expect("historial").undo == 0,
            "pdf_info no puede tocar el historial"
        );

        // uno protegido dice que lo está, sin pedir la contraseña
        let cifrado = std::env::temp_dir().join("documento-pdfinfo-cifrado.pdf");
        crate::seguridad::encrypt_pdf(
            ruta.clone(),
            Some(cifrado.to_string_lossy().into_owned()),
            "secreto".into(),
            None,
            None,
        )
        .expect("cifrar");
        let info = pdf_info(cifrado.to_string_lossy().into_owned()).expect("ficha del cifrado");
        assert!(info.encrypted, "un PDF con contraseña es «protegido»");
        assert!(info.bytes > 0);

        // y un fichero que no está lo dice en llano
        let err = pdf_info("/no/existe/ni-esta.pdf".into()).unwrap_err();
        assert!(
            err.contains("no se encuentra") || err.contains("No se encuentra"),
            "error poco claro: {err}"
        );
        assert!(!err.contains("os error"), "jerga en el error: {err}");
        std::fs::remove_file(&pdf).ok();
        std::fs::remove_file(&cifrado).ok();
    }

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
        // guardado atómico: no queda el temporal de la cirugía
        assert!(!std::path::Path::new(&format!("{work}.tmp")).exists());
        let m = get_metadata(work).expect("leer metadatos");
        assert_eq!(m.title, "Informe años núñez");
        assert_eq!(m.author, "Jorge");
        assert_eq!(m.subject, "Pruebas");
        assert_eq!(m.keywords, "pdf, editor");
    }
}
