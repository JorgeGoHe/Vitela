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
    /// Altura del destino dentro de la página, **en el espacio propio de la
    /// página** y con el origen arriba a la izquierda (la UI convierte con
    /// la `rotation` de `get_page_sizes`, como con `get_text_blocks`).
    ///
    /// Es el `top` del `/XYZ` del spec: sin él, seguir un marcador lleva al
    /// principio de la página y no a la línea donde se puso, que es lo que
    /// hace Acrobat. `None` cuando el destino no lo dice (`/Fit`) o cuando
    /// dice «déjalo como está» (`null`).
    #[serde(default)]
    pub top: Option<f32>,
    /// Ampliación del destino (1,0 = 100 %), el tercer número del `/XYZ`.
    /// `None` cuando el destino no la fija —que en el spec es 0 o `null`— y
    /// entonces se conserva la que haya puesta, como en Acrobat.
    #[serde(default)]
    pub zoom: Option<f32>,
    pub children: Vec<OutlineNode>,
}

/// Un destino de marcador ya resuelto: en qué página cae y con qué vista.
#[derive(Default)]
struct Destino {
    page_index: Option<u16>,
    top: Option<f32>,
    zoom: Option<f32>,
}

/// Árbol de marcadores del documento, con el destino fino de cada uno.
///
/// Se lee con **lopdf** y no con los `bookmarks()` de PDFium: pdfium-render
/// 0.8 da la página del destino pero no los parámetros de vista (`top` y
/// `zoom`), que es justo lo que distingue «ir a la página 12» de «volver a
/// donde estaba». Y escribir el árbol ya era lopdf, así que las dos mitades
/// vuelven a hablar el mismo idioma.
#[tauri::command(async)]
pub fn get_outline(path: String) -> Result<Vec<OutlineNode>, String> {
    on_pdfium_thread(move || crate::with_lopdf(&path, |doc| Ok(lee_outline(doc))))
}

/// Recorre `/Outlines` de un documento y devuelve el árbol.
fn lee_outline(doc: &LoDoc) -> Vec<OutlineNode> {
    let Some(primero) = doc
        .catalog()
        .ok()
        .and_then(|c| c.get(b"Outlines").ok())
        .and_then(|o| dict_de(doc, o))
        .and_then(|d| d.get(b"First").ok())
        .and_then(|o| o.as_reference().ok())
    else {
        return Vec::new();
    };
    let paginas: Vec<ObjectId> = doc.get_pages().values().copied().collect();
    let mut vistos = std::collections::HashSet::new();
    hermanos(doc, primero, &paginas, &mut vistos, 0)
}

/// Un marcador y todos sus hermanos, con sus hijos. `vistos` corta los
/// ciclos: un `/Next` que apunte hacia atrás colgaría la app, y un PDF de
/// fuera puede traerlo.
fn hermanos(
    doc: &LoDoc,
    primero: ObjectId,
    paginas: &[ObjectId],
    vistos: &mut std::collections::HashSet<ObjectId>,
    nivel: usize,
) -> Vec<OutlineNode> {
    let mut out = Vec::new();
    if nivel > 32 {
        return out;
    }
    let mut actual = Some(primero);
    while let Some(id) = actual {
        if !vistos.insert(id) {
            break;
        }
        let Ok(d) = doc.get_object(id).and_then(|o| o.as_dict()) else {
            break;
        };
        let destino = destino_de(doc, d, paginas);
        let children = d
            .get(b"First")
            .ok()
            .and_then(|o| o.as_reference().ok())
            .map(|h| hermanos(doc, h, paginas, vistos, nivel + 1))
            .unwrap_or_default();
        out.push(OutlineNode {
            title: d
                .get(b"Title")
                .map(crate::anotaciones::texto_de_cadena_pdf)
                .unwrap_or_default(),
            page_index: destino.page_index,
            top: destino.top,
            zoom: destino.zoom,
            children,
        });
        actual = d.get(b"Next").ok().and_then(|o| o.as_reference().ok());
    }
    out
}

/// El destino de un marcador: del `/Dest` (array, nombre o cadena) o del
/// `/A` con `/S /GoTo`, que es la otra forma de escribir lo mismo.
fn destino_de(doc: &LoDoc, nodo: &Dictionary, paginas: &[ObjectId]) -> Destino {
    let crudo = nodo.get(b"Dest").ok().cloned().or_else(|| {
        let a = dict_de(doc, nodo.get(b"A").ok()?)?;
        let es_goto = a
            .get(b"S")
            .and_then(|o| o.as_name())
            .map(|n| n == b"GoTo")
            .unwrap_or(false);
        if !es_goto {
            return None;
        }
        a.get(b"D").ok().cloned()
    });
    let Some(arr) = crudo.and_then(|o| resuelve_dest(doc, &o, 0)) else {
        return Destino::default();
    };
    let page_index = match arr.first() {
        Some(Object::Reference(id)) => paginas.iter().position(|p| p == id).map(|i| i as u16),
        Some(Object::Integer(n)) if *n >= 0 => Some(*n as u16),
        _ => None,
    };
    let num = |i: usize| match arr.get(i) {
        Some(Object::Integer(v)) => Some(*v as f32),
        Some(Object::Real(v)) => Some(*v),
        _ => None,
    };
    let modo = arr
        .get(1)
        .and_then(|o| o.as_name().ok())
        .map(|n| n.to_vec())
        .unwrap_or_default();
    // los modos del spec que dicen a qué altura se llega; `/Fit`, `/FitV`,
    // `/FitB` y `/FitBV` no dicen ninguna y se leen sin `top`, sin romperse
    let (top_pdf, zoom) = match modo.as_slice() {
        b"XYZ" => (num(3), num(4).filter(|z| *z > 0.0)),
        b"FitH" | b"FitBH" => (num(2), None),
        b"FitR" => (num(5), None),
        _ => (None, None),
    };
    // el destino va en coordenadas del papel; la UI trabaja con el origen
    // arriba a la izquierda, en el espacio propio de la página
    let top = top_pdf.and_then(|y| {
        let idx = page_index? as usize;
        let page_id = *paginas.get(idx)?;
        let geo = crate::formularios2::geo_pagina(doc, page_id).ok()?;
        Some(geo.pdf_a_ui(0.0, y).1)
    });
    Destino { page_index, top, zoom }
}

/// Un destino puede ser el array, o el nombre de uno del árbol
/// `/Names /Dests` (o del `/Dests` viejo del catálogo). Devuelve el array.
fn resuelve_dest(doc: &LoDoc, obj: &Object, vuelta: usize) -> Option<Vec<Object>> {
    if vuelta > 8 {
        return None;
    }
    match obj {
        Object::Array(a) => Some(a.clone()),
        Object::Reference(id) => resuelve_dest(doc, doc.get_object(*id).ok()?, vuelta + 1),
        // un destino con nombre puede llevar el array dentro de un /D
        Object::Dictionary(d) => resuelve_dest(doc, d.get(b"D").ok()?, vuelta + 1),
        Object::Name(n) => por_nombre(doc, n).and_then(|o| resuelve_dest(doc, &o, vuelta + 1)),
        Object::String(b, _) => por_nombre(doc, b).and_then(|o| resuelve_dest(doc, &o, vuelta + 1)),
        _ => None,
    }
}

/// Busca un destino con nombre: primero en el árbol `/Names /Dests` (PDF
/// 1.2 en adelante) y después en el `/Dests` del catálogo, que es como se
/// escribían antes y sigue habiendo documentos así.
fn por_nombre(doc: &LoDoc, nombre: &[u8]) -> Option<Object> {
    let catalog = doc.catalog().ok()?;
    if let Some(names) = catalog.get(b"Names").ok().and_then(|o| dict_de(doc, o)) {
        if let Some(dests) = names.get(b"Dests").ok().and_then(|o| dict_de(doc, o)) {
            if let Some(v) = en_arbol_de_nombres(doc, dests, nombre, 0) {
                return Some(v);
            }
        }
    }
    let viejo = catalog.get(b"Dests").ok().and_then(|o| dict_de(doc, o))?;
    viejo.get(nombre).ok().cloned()
}

/// Un árbol de nombres del spec: hojas con `/Names [clave valor …]` y ramas
/// con `/Kids`. No se aprovecha que están ordenados: son pocos y buscar de
/// cabo a rabo no se nota.
fn en_arbol_de_nombres(
    doc: &LoDoc,
    nodo: &Dictionary,
    nombre: &[u8],
    nivel: usize,
) -> Option<Object> {
    if nivel > 16 {
        return None;
    }
    if let Ok(names) = nodo.get(b"Names").and_then(|o| o.as_array()) {
        for par in names.chunks(2) {
            let [clave, valor] = par else { continue };
            let coincide = match clave {
                Object::String(b, _) => b.as_slice() == nombre,
                Object::Name(n) => n.as_slice() == nombre,
                _ => false,
            };
            if coincide {
                return Some(valor.clone());
            }
        }
    }
    if let Ok(kids) = nodo.get(b"Kids").and_then(|o| o.as_array()) {
        for k in kids {
            let hijo = dict_de(doc, k)?;
            if let Some(v) = en_arbol_de_nombres(doc, hijo, nombre, nivel + 1) {
                return Some(v);
            }
        }
    }
    None
}

/// Un diccionario, esté por referencia o en línea.
fn dict_de<'a>(doc: &'a LoDoc, obj: &'a Object) -> Option<&'a Dictionary> {
    match obj {
        Object::Dictionary(d) => Some(d),
        Object::Reference(id) => doc.get_object(*id).ok()?.as_dict().ok(),
        _ => None,
    }
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

/// Reescribe el árbol `/Outlines` completo con lopdf.
///
/// Cada marcador guarda su destino como `/XYZ left top zoom`, que es lo que
/// escribe Acrobat: volver a un marcador devuelve **la vista** donde se
/// puso —altura y ampliación—, no el principio de la página. `top` llega en
/// el espacio propio de la página con el origen arriba a la izquierda, como
/// el resto de los comandos que escriben, y aquí se voltea. Sin `top` ni
/// `zoom` se escribe `null`, que en el spec es «déjalo como está».
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
                        // `/XYZ left top zoom`: `left` se queda en `null`
                        // («déjalo como está») porque un marcador no fija la
                        // columna, y `top` vuelve a coordenadas del papel
                        let top = node.top.and_then(|y| {
                            let geo = crate::formularios2::geo_pagina(doc, *page_id).ok()?;
                            Some(Object::Real(geo.ui_a_pdf(0.0, y).1))
                        });
                        let zoom = node
                            .zoom
                            .filter(|z| *z > 0.0)
                            .map(Object::Real);
                        d.set(
                            "Dest",
                            Object::Array(vec![
                                Object::Reference(*page_id),
                                Object::Name(b"XYZ".to_vec()),
                                Object::Null,
                                top.unwrap_or(Object::Null),
                                zoom.unwrap_or(Object::Null),
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
                    top: None,
                    zoom: None,
                    children: vec![OutlineNode {
                        title: "Sección española: años".into(),
                        page_index: Some(1),
                        top: None,
                    zoom: None,
                    children: vec![],
                    }],
                },
                OutlineNode {
                    title: "Final".into(),
                    page_index: Some(2),
                    top: None,
                    zoom: None,
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

    /// **H8.** En Acrobat el destino de un marcador guarda **zoom y
    /// posición** (`/XYZ left top zoom`): volver a un marcador devuelve la
    /// vista exacta, no el principio de la página. Aquí el destino se
    /// escribía siempre `/XYZ null null null`, así que un marcador puesto
    /// en la cláusula tercera llevaba al encabezado.
    #[test]
    fn los_marcadores_guardan_la_altura_y_el_zoom_del_destino() {
        let pdf = std::env::temp_dir().join("documento-outline-destino.pdf");
        crea_pdf(&["Uno", "Dos"], &pdf);
        let work = pdf.to_string_lossy().to_string();
        set_outline(
            work.clone(),
            vec![OutlineNode {
                title: "Cláusula tercera".into(),
                page_index: Some(0),
                top: Some(120.0),
                zoom: Some(1.5),
                children: vec![OutlineNode {
                    title: "Apartado a)".into(),
                    page_index: Some(1),
                    top: Some(300.5),
                    zoom: None,
                    children: vec![],
                }],
            }],
        )
        .expect("escribir el árbol con destino fino");

        let leido = get_outline(work.clone()).expect("releer");
        assert_eq!(leido.len(), 1);
        assert_eq!(leido[0].title, "Cláusula tercera");
        assert_eq!(leido[0].page_index, Some(0));
        assert!(
            leido[0].top.is_some_and(|t| (t - 120.0).abs() < 0.01),
            "la altura vuelve igual: {:?}",
            leido[0].top
        );
        assert!(
            leido[0].zoom.is_some_and(|z| (z - 1.5).abs() < 0.01),
            "el zoom vuelve igual: {:?}",
            leido[0].zoom
        );
        let hijo = &leido[0].children[0];
        assert_eq!(hijo.title, "Apartado a)");
        assert_eq!(hijo.page_index, Some(1));
        assert!(hijo.top.is_some_and(|t| (t - 300.5).abs() < 0.01));
        assert_eq!(hijo.zoom, None, "sin zoom se deja el que haya, como Acrobat");

        // el destino que se escribe es el del spec, con la `y` del papel:
        // 120 pt desde arriba en una A4 son 842 − 120 = 722 desde abajo
        let doc = LoDoc::load(&work).expect("releer con lopdf");
        let dest = doc
            .catalog()
            .and_then(|c| c.get(b"Outlines"))
            .ok()
            .and_then(|o| dict_de(&doc, o))
            .and_then(|d| d.get(b"First").ok())
            .and_then(|o| o.as_reference().ok())
            .and_then(|id| doc.get_object(id).ok())
            .and_then(|o| o.as_dict().ok())
            .and_then(|d| d.get(b"Dest").ok())
            .and_then(|o| o.as_array().ok())
            .expect("el /Dest del primer marcador")
            .clone();
        assert!(
            matches!(&dest[1], Object::Name(n) if n == b"XYZ"),
            "el destino tiene que ser /XYZ: {dest:?}"
        );
        assert!(matches!(dest[2], Object::Null), "el `left` se deja como está");
        let Object::Real(top) = dest[3] else {
            panic!("el `top` tiene que ir escrito: {dest:?}")
        };
        assert!((top - 722.0).abs() < 1.0, "la `y` del papel: {top}");
        std::fs::remove_file(&pdf).ok();
    }

    /// **H8.** Un destino que viene de fuera puede no decir la altura
    /// (`/Fit`, «la página entera») o decirla sin zoom (`/FitH`), y puede
    /// venir por su nombre en vez de por su array. Ninguno de esos casos
    /// puede romper la lectura del árbol: se leen sin `top` o sin `zoom` y
    /// ya está.
    #[test]
    fn un_destino_ajeno_sin_zoom_se_lee_sin_romperse() {
        let pdf = std::env::temp_dir().join("documento-outline-ajeno.pdf");
        crea_pdf(&["Uno", "Dos"], &pdf);
        let work = pdf.to_string_lossy().to_string();
        set_outline(
            work.clone(),
            vec![
                OutlineNode {
                    title: "Con /Fit".into(),
                    page_index: Some(0),
                    top: Some(100.0),
                    zoom: Some(2.0),
                    children: vec![],
                },
                OutlineNode {
                    title: "Con /FitH".into(),
                    page_index: Some(1),
                    top: Some(100.0),
                    zoom: Some(2.0),
                    children: vec![],
                },
                OutlineNode {
                    title: "Por su nombre".into(),
                    page_index: Some(1),
                    top: None,
                    zoom: None,
                    children: vec![],
                },
            ],
        )
        .expect("escribir el árbol");

        // se reescriben a mano los tres destinos, como los escribiría otro
        // programa: /Fit, /FitH 700 y un destino con nombre
        {
            let mut doc = LoDoc::load(&work).expect("releer");
            let paginas: Vec<ObjectId> = doc.get_pages().values().copied().collect();
            let mut id = doc
                .catalog()
                .and_then(|c| c.get(b"Outlines"))
                .ok()
                .and_then(|o| dict_de(&doc, o))
                .and_then(|d| d.get(b"First").ok())
                .and_then(|o| o.as_reference().ok())
                .expect("el primer marcador");
            let destinos = [
                Object::Array(vec![
                    Object::Reference(paginas[0]),
                    Object::Name(b"Fit".to_vec()),
                ]),
                Object::Array(vec![
                    Object::Reference(paginas[1]),
                    Object::Name(b"FitH".to_vec()),
                    Object::Real(700.0),
                ]),
                Object::Name(b"capitulo1".to_vec()),
            ];
            // el destino con nombre, en el árbol /Names /Dests del catálogo
            let mut hoja_d = Dictionary::new();
            hoja_d.set(
                "Names",
                Object::Array(vec![
                    Object::string_literal("capitulo1"),
                    Object::Array(vec![
                        Object::Reference(paginas[1]),
                        Object::Name(b"XYZ".to_vec()),
                        Object::Null,
                        Object::Real(500.0),
                        Object::Real(0.75),
                    ]),
                ]),
            );
            let hoja = doc.add_object(hoja_d);
            let mut dests_d = Dictionary::new();
            dests_d.set("Kids", Object::Array(vec![Object::Reference(hoja)]));
            let dests = doc.add_object(dests_d);
            let mut names_d = Dictionary::new();
            names_d.set("Dests", Object::Reference(dests));
            let names = doc.add_object(names_d);
            let catalog_id = doc.trailer.get(b"Root").unwrap().as_reference().unwrap();
            doc.get_object_mut(catalog_id)
                .unwrap()
                .as_dict_mut()
                .unwrap()
                .set("Names", Object::Reference(names));
            for destino in destinos {
                let d = doc.get_object_mut(id).unwrap().as_dict_mut().unwrap();
                d.set("Dest", destino);
                let siguiente = d.get(b"Next").ok().and_then(|o| o.as_reference().ok());
                let Some(n) = siguiente else { break };
                id = n;
            }
            doc.save(&work).expect("guardar a mano");
        }

        let leido = get_outline(work.clone()).expect("leer un árbol de fuera");
        assert_eq!(leido.len(), 3);
        assert_eq!(leido[0].page_index, Some(0));
        assert_eq!(leido[0].top, None, "/Fit no dice a qué altura se llega");
        assert_eq!(leido[0].zoom, None);
        assert_eq!(leido[1].page_index, Some(1));
        assert!(
            leido[1].top.is_some_and(|t| (t - 142.0).abs() < 1.0),
            "/FitH 700 en una A4 son 142 pt desde arriba: {:?}",
            leido[1].top
        );
        assert_eq!(leido[1].zoom, None, "/FitH no lleva zoom");
        assert_eq!(leido[2].page_index, Some(1), "el destino con nombre se resuelve");
        assert!(leido[2].top.is_some_and(|t| (t - 342.0).abs() < 1.0));
        assert!(leido[2].zoom.is_some_and(|z| (z - 0.75).abs() < 0.01));
        std::fs::remove_file(&pdf).ok();
    }

    /// **H8 en una página girada.** El destino vive en el espacio **propio**
    /// de la página (la caja sin rotar), como los bloques de texto y las
    /// imágenes: su respuesta no puede depender del `/Rotate`, o al girar
    /// la página los marcadores se irían 246 pt (AC-014).
    #[test]
    fn el_destino_de_un_marcador_no_depende_del_giro_de_la_pagina() {
        let pdf = std::env::temp_dir().join("documento-outline-girada.pdf");
        crea_pdf(&["Uno", "Dos"], &pdf);
        let work = pdf.to_string_lossy().to_string();
        set_outline(
            work.clone(),
            vec![OutlineNode {
                title: "En la página girada".into(),
                page_index: Some(0),
                top: Some(200.0),
                zoom: Some(1.0),
                children: vec![],
            }],
        )
        .expect("escribir el marcador");
        let antes = get_outline(work.clone()).expect("leer");
        assert!(
            antes[0].top.is_some_and(|t| (t - 200.0).abs() < 0.01),
            "sin destino que comparar el resto del test no prueba nada: {:?}",
            antes[0].top
        );

        crate::paginas::rotate_page(work.clone(), 0).expect("girar 90°");
        let despues = get_outline(work.clone()).expect("leer con la página girada");
        assert_eq!(
            despues[0].top, antes[0].top,
            "el destino se lee en el espacio propio de la página, no en el de la vista"
        );
        assert_eq!(despues[0].page_index, Some(0));

        // y escribirlo con la página ya girada devuelve el mismo número
        set_outline(work.clone(), despues.clone()).expect("reescribir");
        let vuelta = get_outline(work.clone()).expect("releer");
        assert!(
            vuelta[0].top.is_some_and(|t| (t - 200.0).abs() < 0.01),
            "ida y vuelta con la página girada: {:?}",
            vuelta[0].top
        );
        std::fs::remove_file(&pdf).ok();
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
