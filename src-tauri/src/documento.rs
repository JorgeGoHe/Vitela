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
    Destino {
        page_index,
        top,
        zoom,
    }
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
                        let zoom = node.zoom.filter(|z| *z > 0.0).map(Object::Real);
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
pub(crate) fn trae_encrypt(path: &str) -> bool {
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

/// Deja constancia de que este fichero lo ha escrito Vitela, como hace
/// cualquier editor.
///
/// **Va en `/Producer`, no en `/Creator`** (AC-095). En el spec `/Producer`
/// es «quién ha producido este fichero» y `/Creator` «con qué se escribió
/// el original», que es un dato del usuario: escribir ahí «Vitela»
/// borraba el «Microsoft Word» del documento que alguien nos había
/// mandado, y Propiedades pasaba a decir «Aplicación: Vitela» para
/// cualquier PDF guardado una vez. Solo se pone `/Creator` cuando el
/// documento no trae ninguno.
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
    info.set(
        "Producer",
        cadena_pdf(&format!("Vitela {}", env!("CARGO_PKG_VERSION"))),
    );
    let sin_creador = match info.get(b"Creator") {
        Ok(Object::String(b, _)) => b.is_empty(),
        Ok(_) => false,
        Err(_) => true,
    };
    if sin_creador {
        info.set("Creator", cadena_pdf("Vitela"));
    }
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
                let Ok(annot) = annotations.get(i) else {
                    continue;
                };
                let Some(link) = annot.as_link_annotation().and_then(|l| l.link().ok()) else {
                    continue;
                };
                let Ok(r) = link.rect() else { continue };
                let mut uri = None;
                let mut dest_page = link.destination().and_then(|d| d.page_index().ok());
                if let Some(action) = link.action() {
                    match action {
                        PdfAction::Uri(u) => uri = u.uri().ok(),
                        PdfAction::LocalDestination(l) if dest_page.is_none() => {
                            dest_page = l.destination().ok().and_then(|d| d.page_index().ok());
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
            crate::historial::history_state(ruta.clone())
                .expect("historial")
                .undo
                == 0,
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
        assert_eq!(
            hijo.zoom, None,
            "sin zoom se deja el que haya, como Acrobat"
        );

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
        assert!(
            matches!(dest[2], Object::Null),
            "el `left` se deja como está"
        );
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
        assert_eq!(
            leido[2].page_index,
            Some(1),
            "el destino con nombre se resuelve"
        );
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

// ---------------------------------------------------------------------------
// Etiquetas de página (`/PageLabels`): «i, ii, iii, 1, 2, 3…»
// ---------------------------------------------------------------------------

/// Un tramo de numeración, como el diálogo «Numerar páginas» de Acrobat
/// («Organizar páginas ▸ Más ▸ Numerar páginas»): desde qué página empieza,
/// con qué estilo se cuenta, qué prefijo lleva y con qué número arranca.
///
/// **Las claves van en snake_case** cuando llegan de la interfaz: Tauri solo
/// pasa a snake_case los argumentos de primer nivel del comando, no las de
/// una estructura anidada (lo mismo que el `props` de `create_form_field`).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct RangoEtiqueta {
    /// Primera página del tramo, contando desde 0.
    pub desde: u16,
    /// `"arabigo"` (1, 2, 3), `"romano"` (I, II, III), `"romano_min"`
    /// (i, ii, iii), `"letra"` (A, B, … AA), `"letra_min"` (a, b, … aa) o
    /// `"ninguno"` (solo el prefijo, que es como se marcan las cubiertas y
    /// las separatas).
    pub estilo: String,
    /// Lo que va delante del número («Anexo », «A-»). Puede ir solo.
    #[serde(default)]
    pub prefijo: String,
    /// Con qué número empieza a contar el tramo. Uno por defecto, como el
    /// spec y como Acrobat.
    #[serde(default = "una")]
    pub empieza_en: u32,
}

fn una() -> u32 {
    1
}

/// Lo que sabe el documento de su numeración: los tramos tal como están
/// escritos y **la etiqueta ya compuesta de cada página**, en orden.
///
/// Las dos cosas a la vez a propósito: la píldora y las miniaturas enseñan
/// la etiqueta, y si la interfaz tuviera que componerla habría dos
/// implementaciones de los números romanos —una aquí, para escribir, y otra
/// allí, para enseñar— que tarde o temprano dirían cosas distintas.
#[derive(Serialize, Debug, Default, PartialEq)]
pub struct EtiquetasPaginas {
    pub rangos: Vec<RangoEtiqueta>,
    /// Una entrada por página. Vacía si el documento no trae
    /// `/PageLabels`: entonces la página se llama por su número físico y no
    /// hay nada que enseñar entre paréntesis.
    pub etiquetas: Vec<String>,
}

/// El estilo del spec (`/S`) que corresponde a cada nombre nuestro. `None`
/// es «sin parte numérica», que en el PDF se escribe **no poniendo `/S`**.
fn estilo_pdf(estilo: &str) -> Option<&'static str> {
    match estilo {
        "arabigo" => Some("D"),
        "romano" => Some("R"),
        "romano_min" => Some("r"),
        "letra" => Some("A"),
        "letra_min" => Some("a"),
        _ => None,
    }
}

/// Y al revés, para leer lo que trae un PDF de fuera.
fn estilo_nuestro(s: Option<&str>) -> String {
    match s {
        Some("D") => "arabigo",
        Some("R") => "romano",
        Some("r") => "romano_min",
        Some("A") => "letra",
        Some("a") => "letra_min",
        _ => "ninguno",
    }
    .to_string()
}

/// Números romanos hasta 3999; por encima, el número tal cual (un PDF de
/// cuatro mil páginas numeradas en romano no existe, y devolver «MMMM…» no
/// ayudaría a nadie).
fn romano(mut n: u32) -> String {
    if n == 0 || n > 3999 {
        return n.to_string();
    }
    const TABLA: [(u32, &str); 13] = [
        (1000, "M"),
        (900, "CM"),
        (500, "D"),
        (400, "CD"),
        (100, "C"),
        (90, "XC"),
        (50, "L"),
        (40, "XL"),
        (10, "X"),
        (9, "IX"),
        (5, "V"),
        (4, "IV"),
        (1, "I"),
    ];
    let mut out = String::new();
    for (valor, letras) in TABLA {
        while n >= valor {
            out.push_str(letras);
            n -= valor;
        }
    }
    out
}

/// Letras como las escribe el spec: A…Z, luego AA…ZZ, luego AAA…ZZZ. No es
/// base 26 (no hay «AB»): la letra se repite.
fn letras(n: u32) -> String {
    if n == 0 {
        return String::new();
    }
    let i = (n - 1) % 26;
    let veces = ((n - 1) / 26 + 1) as usize;
    let letra = (b'A' + i as u8) as char;
    std::iter::repeat_n(letra, veces).collect()
}

/// La etiqueta de la página `pagina` (0-based) según el tramo en el que cae.
fn etiqueta_de(rango: &RangoEtiqueta, pagina: u16) -> String {
    let ordinal = rango.empieza_en + (pagina.saturating_sub(rango.desde)) as u32;
    let numero = match rango.estilo.as_str() {
        "arabigo" => ordinal.to_string(),
        "romano" => romano(ordinal),
        "romano_min" => romano(ordinal).to_lowercase(),
        "letra" => letras(ordinal),
        "letra_min" => letras(ordinal).to_lowercase(),
        _ => String::new(),
    };
    format!("{}{}", rango.prefijo, numero)
}

/// Compone la etiqueta de cada página a partir de los tramos.
fn etiquetas_de(rangos: &[RangoEtiqueta], paginas: u16) -> Vec<String> {
    if rangos.is_empty() {
        return Vec::new();
    }
    (0..paginas)
        .map(|p| match rangos.iter().rev().find(|r| r.desde <= p) {
            Some(r) => etiqueta_de(r, p),
            // páginas antes del primer tramo: el spec dice que no tienen
            // etiqueta, y su número físico es lo único honesto
            None => (p + 1).to_string(),
        })
        .collect()
}

/// Recorre el árbol de números del `/PageLabels` (normalmente un `/Nums`
/// plano, pero el spec admite `/Kids`) y devuelve sus pares.
fn recorre_nums(doc: &LoDoc, nodo: &Dictionary, out: &mut Vec<RangoEtiqueta>, hondo: u8) {
    if hondo > 16 {
        return; // un árbol con un ciclo no puede colgar la app
    }
    if let Ok(nums) = nodo.get(b"Nums").and_then(|o| resuelve(doc, o).as_array()) {
        let mut i = 0;
        while i + 1 < nums.len() {
            let desde = match resuelve(doc, &nums[i]).as_i64() {
                Ok(n) if n >= 0 => n as u16,
                _ => {
                    i += 2;
                    continue;
                }
            };
            if let Ok(d) = resuelve(doc, &nums[i + 1]).as_dict() {
                out.push(RangoEtiqueta {
                    desde,
                    estilo: estilo_nuestro(
                        d.get(b"S")
                            .and_then(|o| o.as_name())
                            .ok()
                            .and_then(|n| std::str::from_utf8(n).ok()),
                    ),
                    prefijo: d
                        .get(b"P")
                        .and_then(|o| o.as_str())
                        .map(|s| String::from_utf8_lossy(s).into_owned())
                        .unwrap_or_default(),
                    empieza_en: d.get(b"St").and_then(|o| o.as_i64()).unwrap_or(1).max(1) as u32,
                });
            }
            i += 2;
        }
    }
    if let Ok(kids) = nodo.get(b"Kids").and_then(|o| resuelve(doc, o).as_array()) {
        for kid in kids {
            if let Ok(d) = resuelve(doc, kid).as_dict() {
                recorre_nums(doc, &d.clone(), out, hondo + 1);
            }
        }
    }
}

/// Sigue una referencia hasta el objeto, o devuelve el objeto tal cual.
fn resuelve<'a>(doc: &'a LoDoc, o: &'a Object) -> &'a Object {
    match o {
        Object::Reference(id) => doc.get_object(*id).unwrap_or(o),
        otro => otro,
    }
}

/// La numeración del documento: los tramos y la etiqueta de cada página.
///
/// Un documento sin `/PageLabels` devuelve las dos listas vacías, que es lo
/// que hay que contestar: no es un error, es un PDF que no numera sus
/// páginas y en el que la página 2 se llama «2».
#[tauri::command(async)]
pub fn get_page_labels(path: String) -> Result<EtiquetasPaginas, String> {
    on_pdfium_thread(move || {
        let paginas = with_doc(&path, |doc| Ok(doc.pages().len()))?;
        crate::with_lopdf(&path, |doc| {
            let mut rangos = Vec::new();
            if let Ok(catalog) = doc.catalog() {
                if let Ok(labels) = catalog.get(b"PageLabels") {
                    if let Ok(d) = resuelve(doc, labels).as_dict() {
                        recorre_nums(doc, &d.clone(), &mut rangos, 0);
                    }
                }
            }
            rangos.sort_by_key(|r| r.desde);
            rangos.dedup_by_key(|r| r.desde);
            let etiquetas = etiquetas_de(&rangos, paginas);
            Ok(EtiquetasPaginas { rangos, etiquetas })
        })
    })
}

/// Escribe la numeración: un tramo por cada cambio de estilo, como el
/// diálogo de Acrobat. Con la lista vacía se quita el `/PageLabels` y el
/// documento vuelve a llamar a sus páginas por su número físico.
///
/// El primer tramo tiene que empezar en la página 0: el spec no sabe qué
/// hacer con las páginas anteriores al primero, y un visor que se encuentre
/// una las numera como le parece.
#[tauri::command(async)]
pub fn set_page_labels(work_path: String, rangos: Vec<RangoEtiqueta>) -> Result<(), String> {
    let mut rangos = rangos;
    rangos.sort_by_key(|r| r.desde);
    if let Some(primero) = rangos.first() {
        if primero.desde != 0 {
            return Err("La numeración tiene que empezar en la primera página".into());
        }
    }
    if rangos.windows(2).any(|p| p[0].desde == p[1].desde) {
        return Err("Hay dos tramos que empiezan en la misma página".into());
    }
    cirugia(&work_path, move |doc| {
        let catalog_id = doc
            .trailer
            .get(b"Root")
            .and_then(|o| o.as_reference())
            .map_err(|e| e.to_string())?;
        if rangos.is_empty() {
            doc.get_object_mut(catalog_id)
                .and_then(|o| o.as_dict_mut())
                .map_err(|e| e.to_string())?
                .remove(b"PageLabels");
            return Ok(());
        }
        let paginas = doc.get_pages().len() as u16;
        if rangos.iter().any(|r| r.desde >= paginas) {
            return Err("Hay un tramo que empieza fuera del documento".into());
        }
        let mut nums: Vec<Object> = Vec::new();
        for r in &rangos {
            let mut d = Dictionary::new();
            if let Some(s) = estilo_pdf(&r.estilo) {
                d.set("S", Object::Name(s.as_bytes().to_vec()));
            }
            if !r.prefijo.is_empty() {
                d.set("P", cadena_pdf(&r.prefijo));
            }
            if r.empieza_en != 1 {
                d.set("St", Object::Integer(r.empieza_en.max(1) as i64));
            }
            nums.push(Object::Integer(r.desde as i64));
            nums.push(Object::Dictionary(d));
        }
        let mut arbol = Dictionary::new();
        arbol.set("Nums", Object::Array(nums));
        let arbol_id = doc.add_object(arbol);
        doc.get_object_mut(catalog_id)
            .and_then(|o| o.as_dict_mut())
            .map_err(|e| e.to_string())?
            .set("PageLabels", Object::Reference(arbol_id));
        Ok(())
    })
}

#[cfg(test)]
mod tests_etiquetas {
    use super::*;
    use crate::tests::crea_pdf;

    /// **Etiquetas de página.** En Acrobat («Organizar páginas ▸ Más ▸
    /// Numerar páginas») la portada y el índice se numeran en romano y el
    /// cuerpo empieza otra vez en 1, y lo que se ve es que la píldora dice
    /// «ii» y no «2». Se escribe, se vuelve a leer y tiene que decir lo
    /// mismo: es el contrato con la interfaz, que enseña esas etiquetas.
    #[test]
    fn dos_tramos_de_numeracion_se_escriben_y_se_vuelven_a_leer() {
        let pdf = std::env::temp_dir().join("documento-etiquetas.pdf");
        crea_pdf(&["Portada", "Índice", "Uno", "Dos", "Tres"], &pdf);
        let work = pdf.to_string_lossy().into_owned();

        // sin /PageLabels no hay nada que enseñar, y eso no es un error
        let sin = get_page_labels(work.clone()).expect("leer");
        assert_eq!(
            sin,
            EtiquetasPaginas::default(),
            "un PDF liso no numera nada"
        );

        let rangos = vec![
            RangoEtiqueta {
                desde: 0,
                estilo: "romano_min".into(),
                prefijo: String::new(),
                empieza_en: 1,
            },
            RangoEtiqueta {
                desde: 2,
                estilo: "arabigo".into(),
                prefijo: String::new(),
                empieza_en: 1,
            },
        ];
        set_page_labels(work.clone(), rangos.clone()).expect("numerar");
        let leidas = get_page_labels(work.clone()).expect("leer");
        assert_eq!(leidas.rangos, rangos, "vuelve lo mismo que se escribió");
        assert_eq!(leidas.etiquetas, vec!["i", "ii", "1", "2", "3"]);

        // un prefijo, un arranque distinto y un tramo sin número: los tres
        // sitios donde Acrobat deja escribir algo raro
        let rangos = vec![
            RangoEtiqueta {
                desde: 0,
                estilo: "ninguno".into(),
                prefijo: "Cubierta".into(),
                empieza_en: 1,
            },
            RangoEtiqueta {
                desde: 1,
                estilo: "letra".into(),
                prefijo: "Anexo ".into(),
                empieza_en: 25,
            },
        ];
        set_page_labels(work.clone(), rangos.clone()).expect("numerar otra vez");
        let leidas = get_page_labels(work.clone()).expect("leer");
        assert_eq!(leidas.rangos, rangos);
        assert_eq!(
            leidas.etiquetas,
            vec!["Cubierta", "Anexo Y", "Anexo Z", "Anexo AA", "Anexo BB"],
            "las letras se repiten al pasar de la Z, como dice el spec"
        );

        // quitar la numeración devuelve el documento a sus números físicos
        set_page_labels(work.clone(), Vec::new()).expect("quitar");
        assert_eq!(
            get_page_labels(work.clone()).expect("leer"),
            EtiquetasPaginas::default()
        );

        // y ⌘Z devuelve la que había
        crate::historial::undo(work.clone()).expect("deshacer");
        assert_eq!(get_page_labels(work.clone()).expect("leer").rangos, rangos);

        // lo que no se puede escribir se dice antes de escribirlo
        assert!(set_page_labels(
            work.clone(),
            vec![RangoEtiqueta {
                desde: 1,
                estilo: "arabigo".into(),
                prefijo: String::new(),
                empieza_en: 1,
            }]
        )
        .unwrap_err()
        .contains("primera página"));
        assert!(set_page_labels(
            work.clone(),
            vec![
                RangoEtiqueta {
                    desde: 0,
                    estilo: "arabigo".into(),
                    prefijo: String::new(),
                    empieza_en: 1,
                },
                RangoEtiqueta {
                    desde: 99,
                    estilo: "romano".into(),
                    prefijo: String::new(),
                    empieza_en: 1,
                }
            ]
        )
        .unwrap_err()
        .contains("fuera del documento"));
        std::fs::remove_file(&pdf).ok();
    }

    /// **Propiedades del documento.** «¿Por qué este PDF pesa 40 MB?» y
    /// «¿por qué en tu ordenador se ve con otra letra?» se contestan aquí,
    /// y por eso las fuentes con su tipo y su incrustación no son un
    /// detalle técnico: son la respuesta. Es la pantalla de ⌘D de Acrobat.
    #[test]
    fn la_ficha_del_documento_dice_las_fuentes_el_tamano_y_lo_que_deja_hacer() {
        let pdf = std::env::temp_dir().join("documento-ficha.pdf");
        crea_pdf(&["Una página", "Y otra"], &pdf);
        let work = pdf.to_string_lossy().into_owned();

        let ficha = get_document_info(work.clone()).expect("ficha");
        assert_eq!(ficha.page_count, 2);
        assert!(ficha.bytes > 0, "el peso es el del fichero");
        assert!(
            ficha.version.starts_with("1."),
            "versión del PDF: {}",
            ficha.version
        );
        assert!(
            (ficha.page_width - 595.0).abs() < 2.0,
            "A4: {}",
            ficha.page_width
        );
        assert!(ficha.paginas_iguales, "las dos páginas miden lo mismo");
        assert!(!ficha.formulario);
        assert_eq!(ficha.firmas, 0);
        assert!(
            !ficha.fuentes.is_empty(),
            "el texto tiene que usar alguna fuente"
        );
        assert!(
            ficha
                .fuentes
                .iter()
                .all(|f| !f.nombre.is_empty() && f.tipo != "desconocida"),
            "cada fuente con su nombre y su tipo: {:?}",
            ficha.fuentes
        );
        // sin cifrar y sin protección puesta, el documento lo deja todo
        assert!(!ficha.cifrado && !ficha.proteccion_pendiente);
        assert_eq!(ficha.permisos, crate::seguridad::Permisos::default());

        // un campo y una firma se cuentan aparte: un PDF que solo lleva una
        // firma no es «un documento que se puede rellenar»
        crate::formularios2::create_form_field(
            work.clone(),
            0,
            "text".into(),
            crate::Rect {
                x: 80.0,
                y: 200.0,
                w: 160.0,
                h: 20.0,
            },
            "nombre".into(),
            None,
            None,
            None,
            None,
        )
        .expect("campo");
        let ficha = get_document_info(work.clone()).expect("ficha");
        assert!(ficha.formulario, "ahora sí se puede rellenar");

        // y la protección puesta esperando a Guardar se cuenta como lo que
        // es: todavía no cifrado, pero lo estará
        crate::seguridad::encrypt_pdf(
            work.clone(),
            None,
            "secreta".into(),
            None,
            Some(crate::seguridad::Permisos {
                imprimir: true,
                copiar: false,
                editar: false,
            }),
        )
        .expect("proteger al guardar");
        let ficha = get_document_info(work.clone()).expect("ficha");
        assert!(
            ficha.cifrado && ficha.proteccion_pendiente,
            "protegido, aunque el fichero todavía no lo esté: {ficha:?}"
        );
        let p = ficha.permisos;
        assert!(p.imprimir && !p.copiar && !p.editar, "{p:?}");
        std::fs::remove_file(&pdf).ok();
    }

    /// Los números que se enseñan, uno a uno. Un romano mal escrito no lo
    /// canta nadie hasta que alguien imprime el índice.
    #[test]
    fn los_romanos_y_las_letras_se_escriben_como_dice_el_spec() {
        for (n, esperado) in [
            (1, "I"),
            (4, "IV"),
            (9, "IX"),
            (14, "XIV"),
            (1987, "MCMLXXXVII"),
        ] {
            assert_eq!(romano(n), esperado, "{n}");
        }
        // fuera de rango se dice el número, que es lo único que no engaña
        assert_eq!(romano(0), "0");
        assert_eq!(romano(4000), "4000");
        for (n, esperado) in [(1, "A"), (26, "Z"), (27, "AA"), (52, "ZZ"), (53, "AAA")] {
            assert_eq!(letras(n), esperado, "{n}");
        }
    }
}

// ---------------------------------------------------------------------------
// Propiedades del documento: lo que Acrobat enseña en ⌘D
// ---------------------------------------------------------------------------

/// Una fuente del documento, como la lista Acrobat en «Propiedades ▸
/// Fuentes»: el nombre sin el prefijo del subconjunto, qué clase de fuente
/// es y **si viaja dentro del fichero**.
#[derive(Serialize, Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct FuenteInfo {
    pub nombre: String,
    /// «TrueType», «Type 1», «Type 3» o «compuesta» (Type 0), en llano.
    pub tipo: String,
    /// Con `false`, el visor la sustituye por otra parecida: el documento se
    /// ve distinto en otro ordenador, y es la respuesta a media docena de
    /// preguntas que empiezan por «esto no me sale como a ti».
    pub incrustada: bool,
    /// Solo van los glifos que se usan (el prefijo `ABCDEF+` del spec). Es
    /// lo normal y es lo que hace que un PDF de 40 MB no pese 400.
    pub subconjunto: bool,
}

/// Qué deja hacer el documento. Sale de la máscara `/P` del spec, la misma
/// que compone `encrypt_pdf`.
#[derive(Serialize, Debug, Default, PartialEq, Eq)]
pub struct SeguridadInfo {
    /// El fichero **en el disco** va cifrado.
    pub cifrado: bool,
    /// Hay protección puesta esperando a Guardar (ver «Protección»): el
    /// documento todavía no está cifrado, pero lo estará.
    pub pendiente: bool,
    /// Qué deja hacer la máscara `/P`.
    pub permisos: crate::seguridad::Permisos,
}

/// La ficha entera de un documento: lo que Acrobat reparte por las cuatro
/// pestañas de ⌘D.
#[derive(Serialize, Debug)]
pub struct DocumentoInfo {
    pub page_count: u16,
    pub bytes: u64,
    /// La versión del PDF («1.7»), que es lo que decide qué entiende un
    /// visor viejo.
    pub version: String,
    /// Tamaño de la primera página **ya rotado**, en puntos.
    pub page_width: f32,
    pub page_height: f32,
    /// Con `false`, el documento mezcla tamaños de página y la interfaz lo
    /// dice: «210 × 297 mm (la primera; hay más tamaños)».
    pub paginas_iguales: bool,
    /// El documento se puede rellenar. Los campos `/Sig` no cuentan: un PDF
    /// que solo lleva una firma no es un formulario.
    pub formulario: bool,
    /// Cuántos campos de firma hay.
    pub firmas: u16,
    /// El documento está protegido: o el fichero lleva `/Encrypt` en el
    /// disco, o hay protección puesta esperando a Guardar. Las dos cosas se
    /// cuentan igual porque para quien lo lee significan lo mismo; cuál de
    /// las dos es lo dice `proteccion_pendiente`.
    pub cifrado: bool,
    /// Todavía no está cifrado en el disco, pero lo estará al guardar.
    pub proteccion_pendiente: bool,
    /// Qué deja hacer: la máscara `/P` del spec, en llano.
    pub permisos: crate::seguridad::Permisos,
    pub fuentes: Vec<FuenteInfo>,
    /// Cuándo se creó y cuándo se modificó, en ISO 8601, del `/Info` del
    /// documento (`/CreationDate` y `/ModDate`). Vacías si no lo dice: un
    /// PDF no tiene por qué llevar fecha, y ponerle la del fichero sería
    /// contar la del disco como si fuera la del documento.
    pub creado: String,
    pub modificado: String,
    /// Con qué se hizo (el `/Producer`, y si no el `/Creator`), que es la
    /// línea «Aplicación» de las propiedades de Acrobat.
    pub aplicacion: String,
}

/// Un texto del `/Info` del documento, o vacío.
fn texto_del_info(doc: &LoDoc, clave: &[u8]) -> String {
    doc.trailer
        .get(b"Info")
        .ok()
        .and_then(|o| match o {
            Object::Reference(id) => doc.get_object(*id).ok(),
            otro => Some(otro),
        })
        .and_then(|o| o.as_dict().ok())
        .and_then(|d| d.get(clave).ok())
        .map(crate::anotaciones::texto_de_cadena_pdf)
        .unwrap_or_default()
}

/// Una fecha del `/Info` (`D:YYYYMMDDHHmmSS…`) en ISO 8601, que es como
/// viajan todas las fechas a la interfaz.
fn fecha_del_info(doc: &LoDoc, clave: &[u8]) -> String {
    crate::anotaciones::fecha_pdf_a_iso(&texto_del_info(doc, clave))
}

/// El nombre de una fuente sin el prefijo de subconjunto (`ABCDEF+Arial`),
/// y si lo llevaba.
fn nombre_de_fuente(base: &str) -> (String, bool) {
    match base.split_once('+') {
        Some((prefijo, resto))
            if prefijo.len() == 6 && prefijo.bytes().all(|b| b.is_ascii_uppercase()) =>
        {
            (resto.to_string(), true)
        }
        _ => (base.to_string(), false),
    }
}

/// El `/Subtype` de una fuente, dicho como lo dice Acrobat.
fn tipo_de_fuente(subtype: &str) -> String {
    match subtype {
        "TrueType" => "TrueType",
        "Type1" | "MMType1" => "Type 1",
        "Type3" => "Type 3",
        "Type0" => "compuesta",
        _ => "desconocida",
    }
    .to_string()
}

/// Las fuentes que usa el documento, sin repetir. Se recorren los recursos
/// de cada página: una fuente que no está en ningún `/Resources` no se usa,
/// aunque el fichero la lleve dentro.
fn fuentes_del_documento(doc: &LoDoc) -> Vec<FuenteInfo> {
    let mut out: Vec<FuenteInfo> = Vec::new();
    for (_, page_id) in doc.get_pages() {
        let Ok((propios, heredados)) = doc.get_page_resources(page_id) else {
            continue;
        };
        let mut dicts: Vec<Dictionary> = propios.cloned().into_iter().collect();
        for id in heredados {
            if let Ok(d) = doc.get_object(id).and_then(|o| o.as_dict()) {
                dicts.push(d.clone());
            }
        }
        for recursos in dicts {
            let Ok(fuentes) = recursos.get(b"Font").map(|o| resuelve(doc, o)) else {
                continue;
            };
            let Ok(fuentes) = fuentes.as_dict() else {
                continue;
            };
            for (_, obj) in fuentes.iter() {
                let Ok(f) = resuelve(doc, obj).as_dict() else {
                    continue;
                };
                let subtype = f
                    .get(b"Subtype")
                    .and_then(|o| o.as_name())
                    .ok()
                    .map(|n| String::from_utf8_lossy(n).into_owned())
                    .unwrap_or_default();
                let base = f
                    .get(b"BaseFont")
                    .and_then(|o| o.as_name())
                    .ok()
                    .map(|n| String::from_utf8_lossy(n).into_owned())
                    .unwrap_or_else(|| "sin nombre".into());
                // en una Type 0 el descriptor cuelga de la fuente
                // descendiente, no de ella
                let descriptor = match f.get(b"DescendantFonts").map(|o| resuelve(doc, o)) {
                    Ok(Object::Array(a)) if !a.is_empty() => resuelve(doc, &a[0])
                        .as_dict()
                        .ok()
                        .and_then(|d| d.get(b"FontDescriptor").ok().map(|o| resuelve(doc, o))),
                    _ => f.get(b"FontDescriptor").ok().map(|o| resuelve(doc, o)),
                };
                let incrustada = descriptor
                    .and_then(|d| d.as_dict().ok())
                    .map(|d| {
                        [b"FontFile".as_slice(), b"FontFile2", b"FontFile3"]
                            .iter()
                            .any(|k| d.has(k))
                    })
                    // una Type 3 lleva los glifos dibujados en el propio
                    // documento: no hay fichero que incrustar y no se
                    // sustituye por ninguna otra
                    .unwrap_or(subtype == "Type3");
                let (nombre, subconjunto) = nombre_de_fuente(&base);
                let ficha = FuenteInfo {
                    nombre,
                    tipo: tipo_de_fuente(&subtype),
                    incrustada,
                    subconjunto,
                };
                if !out.contains(&ficha) {
                    out.push(ficha);
                }
            }
        }
    }
    out.sort();
    out
}

/// La ficha del documento: número y tamaño de páginas, peso, versión del
/// PDF, formulario, firmas, **las fuentes con su tipo y si van incrustadas**
/// y el resumen de seguridad.
///
/// Es la pantalla que en Acrobat contesta «¿por qué este PDF pesa 40 MB?» y
/// «¿por qué en tu ordenador se ve con otra letra?», y por eso las fuentes
/// no son un detalle técnico: son la respuesta.
///
/// Solo lectura. La seguridad que se dice es la del **fichero de esta
/// ruta** más la protección que esté puesta esperando a Guardar: la copia
/// de trabajo nunca va cifrada (si lo fuera, PDFium pediría la contraseña
/// en cada render), así que lo honesto es decir lo que se escribirá.
#[tauri::command(async)]
pub fn get_document_info(path: String) -> Result<DocumentoInfo, String> {
    let bytes = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
    let proteccion = crate::seguridad::permisos_puestos(&path);
    on_pdfium_thread(move || {
        let (page_count, page_width, page_height, paginas_iguales) = with_doc(&path, |doc| {
            let paginas = doc.pages();
            let n = paginas.len();
            let primera = paginas.get(0).ok();
            let (w, h) = primera
                .as_ref()
                .map(|p| (p.width().value, p.height().value))
                .unwrap_or((0.0, 0.0));
            let iguales = (0..n)
                .filter_map(|i| paginas.get(i).ok())
                .all(|p| (p.width().value - w).abs() < 1.0 && (p.height().value - h).abs() < 1.0);
            Ok((n, w, h, iguales))
        })?;
        crate::with_lopdf(&path, |doc| {
            let catalogo = doc.catalog().ok();
            let acroform = catalogo
                .and_then(|c| c.get(b"AcroForm").ok())
                .map(|o| resuelve(doc, o))
                .and_then(|o| o.as_dict().ok());
            let campos = acroform
                .and_then(|f| f.get(b"Fields").ok())
                .map(|o| resuelve(doc, o))
                .and_then(|o| o.as_array().ok());
            let mut firmas = 0u16;
            let mut otros = 0u16;
            for c in campos.map(|v| v.as_slice()).unwrap_or_default() {
                let Ok(d) = resuelve(doc, c).as_dict() else {
                    continue;
                };
                match d.get(b"FT").and_then(|o| o.as_name()) {
                    Ok(t) if t == b"Sig" => firmas += 1,
                    _ => otros += 1,
                }
            }
            Ok(DocumentoInfo {
                page_count,
                bytes,
                version: doc.version.clone(),
                page_width,
                page_height,
                paginas_iguales,
                formulario: otros > 0,
                firmas,
                cifrado: proteccion.cifrado || proteccion.pendiente,
                proteccion_pendiente: proteccion.pendiente,
                permisos: proteccion.permisos,
                fuentes: fuentes_del_documento(doc),
                creado: fecha_del_info(doc, b"CreationDate"),
                modificado: fecha_del_info(doc, b"ModDate"),
                aplicacion: {
                    let p = texto_del_info(doc, b"Producer");
                    if p.is_empty() {
                        texto_del_info(doc, b"Creator")
                    } else {
                        p
                    }
                },
            })
        })
    })
}

/// **La vista inicial** del documento (Acrobat: la cuarta pestaña de las
/// propiedades, ⌘D): cómo se abre este PDF en cualquier visor —por qué
/// página, con qué zoom, con qué disposición y con qué panel desplegado—.
///
/// Es la única parte de las propiedades que además **se escribe**, y vive
/// en el catálogo: `/OpenAction` (la página y el zoom), `/PageLayout` (la
/// disposición) y `/PageMode` (el panel).
#[derive(Serialize, Deserialize, Debug, Default, PartialEq)]
pub struct VistaInicial {
    /// Página de arranque (0 = la primera). `None` es «lo que decida el
    /// visor», que es lo que hace un PDF sin `/OpenAction`.
    #[serde(default)]
    pub page_index: Option<u16>,
    /// A qué altura de la página se llega, en el espacio propio de la
    /// página con el origen arriba-izquierda, como el `top` de un marcador.
    #[serde(default)]
    pub top: Option<f32>,
    /// El zoom, 1,0 = 100 %. `None` con `ajuste` en `"zoom"` significa
    /// «déjalo como está», que es el `null` del spec.
    #[serde(default)]
    pub zoom: Option<f32>,
    /// Cómo se encaja la página: `"zoom"` (`/XYZ`, el zoom manda),
    /// `"pagina"` (`/Fit`), `"ancho"` (`/FitH`), `"alto"` (`/FitV`) o `""`
    /// si el documento no dice nada.
    #[serde(default)]
    pub ajuste: String,
    /// Disposición: `"una"`, `"continuo"`, `"dos"`, `"dos-continuo"` o
    /// `""`. Son las cuatro de la píldora de Vitela y las cuatro de
    /// Acrobat.
    #[serde(default)]
    pub disposicion: String,
    /// Panel desplegado al abrir: `"ninguno"`, `"marcadores"`,
    /// `"miniaturas"`, `"adjuntos"`, `"capas"`, `"pantalla-completa"` o
    /// `""`.
    #[serde(default)]
    pub panel: String,
    /// ¿Abre con el panel de marcadores desplegado? Es la casilla del
    /// diálogo, que es la única forma de `panel` que la gente usa: sin
    /// ella habría que enseñar los seis `/PageMode` del spec para que
    /// alguien marcara el que ya quería.
    #[serde(default)]
    pub marcadores: Option<bool>,
}

/// `/PageLayout` del spec ↔ las cuatro disposiciones de la app.
fn disposicion_nuestra(nombre: &[u8]) -> String {
    match nombre {
        b"SinglePage" => "una",
        b"OneColumn" => "continuo",
        b"TwoPageLeft" | b"TwoPageRight" => "dos",
        b"TwoColumnLeft" | b"TwoColumnRight" => "dos-continuo",
        _ => "",
    }
    .to_string()
}

fn disposicion_pdf(nuestra: &str) -> Option<&'static str> {
    Some(match nuestra {
        "una" => "SinglePage",
        "continuo" => "OneColumn",
        "dos" => "TwoPageLeft",
        "dos-continuo" => "TwoColumnLeft",
        _ => return None,
    })
}

/// `/PageMode` del spec ↔ el panel que la app despliega.
fn panel_nuestro(nombre: &[u8]) -> String {
    match nombre {
        b"UseNone" => "ninguno",
        b"UseOutlines" => "marcadores",
        b"UseThumbs" => "miniaturas",
        b"UseAttachments" => "adjuntos",
        b"UseOC" => "capas",
        b"FullScreen" => "pantalla-completa",
        _ => "",
    }
    .to_string()
}

fn panel_pdf(nuestro: &str) -> Option<&'static str> {
    Some(match nuestro {
        "ninguno" => "UseNone",
        "marcadores" => "UseOutlines",
        "miniaturas" => "UseThumbs",
        "adjuntos" => "UseAttachments",
        "capas" => "UseOC",
        "pantalla-completa" => "FullScreen",
        _ => return None,
    })
}

/// Lee la vista inicial del catálogo. Un documento que no dice nada
/// devuelve la ficha vacía —no un error—: es un PDF que deja decidir al
/// visor, que es el caso de casi todos.
#[tauri::command(async)]
pub fn get_open_action(path: String) -> Result<VistaInicial, String> {
    on_pdfium_thread(move || {
        crate::with_lopdf(&path, |doc| {
            let paginas: Vec<ObjectId> = doc.get_pages().values().copied().collect();
            let mut out = VistaInicial::default();
            let Ok(catalog) = doc.catalog() else {
                return Ok(out);
            };
            if let Ok(nombre) = catalog.get(b"PageLayout").and_then(|o| o.as_name()) {
                out.disposicion = disposicion_nuestra(nombre);
            }
            if let Ok(nombre) = catalog.get(b"PageMode").and_then(|o| o.as_name()) {
                out.panel = panel_nuestro(nombre);
            }
            out.marcadores = Some(out.panel == "marcadores");
            if let Ok(accion) = catalog.get(b"OpenAction") {
                // el `/OpenAction` es un destino o una acción `/GoTo`, las
                // dos formas de escribir lo mismo: `destino_de` ya las
                // entiende las dos, así que se le pasa como si fuera un
                // marcador
                let mut nodo = Dictionary::new();
                nodo.set("Dest", accion.clone());
                let d = destino_de(doc, &nodo, &paginas);
                out.page_index = d.page_index;
                out.top = d.top;
                out.zoom = d.zoom;
                out.ajuste = ajuste_de(doc, accion).unwrap_or_default();
            }
            Ok(out)
        })
    })
}

/// El modo de encaje del destino (`/XYZ`, `/Fit`, `/FitH`, `/FitV`…) en el
/// vocabulario de la app.
fn ajuste_de(doc: &LoDoc, accion: &Object) -> Option<String> {
    let arr = resuelve_dest(doc, accion, 0).or_else(|| {
        let a = dict_de(doc, accion)?;
        resuelve_dest(doc, a.get(b"D").ok()?, 0)
    })?;
    let modo = arr.get(1)?.as_name().ok()?;
    Some(
        match modo {
            b"XYZ" => "zoom",
            b"Fit" | b"FitB" => "pagina",
            b"FitH" | b"FitBH" => "ancho",
            b"FitV" | b"FitBV" => "alto",
            b"FitR" => "pagina",
            _ => "",
        }
        .to_string(),
    )
}

/// Escribe la vista inicial. Los campos vacíos **quitan** lo que hubiera:
/// una ficha vacía devuelve el documento a «lo que decida el visor», que es
/// lo que hace el «Predeterminado» de Acrobat.
#[tauri::command(async)]
pub fn set_open_action(work_path: String, vista: VistaInicial) -> Result<(), String> {
    cirugia(&work_path, move |doc| {
        let paginas: Vec<ObjectId> = doc.get_pages().values().copied().collect();
        let destino = match vista.page_index {
            Some(p) => {
                let page_id = *paginas
                    .get(p as usize)
                    .ok_or("Esa página no está en el documento")?;
                // el `top` vuelve a coordenadas del papel, como en los
                // marcadores; `left` se queda en `null` («déjalo como
                // está»): la vista inicial no fija la columna
                let top = vista.top.and_then(|y| {
                    let geo = crate::formularios2::geo_pagina(doc, page_id).ok()?;
                    Some(Object::Real(geo.ui_a_pdf(0.0, y).1))
                });
                Some(Object::Array(match vista.ajuste.as_str() {
                    "pagina" => vec![Object::Reference(page_id), Object::Name(b"Fit".to_vec())],
                    "ancho" => vec![
                        Object::Reference(page_id),
                        Object::Name(b"FitH".to_vec()),
                        top.unwrap_or(Object::Null),
                    ],
                    "alto" => vec![
                        Object::Reference(page_id),
                        Object::Name(b"FitV".to_vec()),
                        Object::Null,
                    ],
                    _ => vec![
                        Object::Reference(page_id),
                        Object::Name(b"XYZ".to_vec()),
                        Object::Null,
                        top.unwrap_or(Object::Null),
                        vista
                            .zoom
                            .filter(|z| *z > 0.0)
                            .map(Object::Real)
                            .unwrap_or(Object::Null),
                    ],
                }))
            }
            None => None,
        };
        let catalog_id = doc
            .trailer
            .get(b"Root")
            .and_then(|o| o.as_reference())
            .map_err(|e| e.to_string())?;
        let catalog = doc
            .get_object_mut(catalog_id)
            .and_then(|o| o.as_dict_mut())
            .map_err(|e| e.to_string())?;
        match destino {
            Some(d) => catalog.set("OpenAction", d),
            None => {
                catalog.remove(b"OpenAction");
            }
        }
        match disposicion_pdf(&vista.disposicion) {
            Some(n) => catalog.set("PageLayout", Object::Name(n.as_bytes().to_vec())),
            None => {
                catalog.remove(b"PageLayout");
            }
        }
        // la casilla manda sobre el nombre del modo: es lo que la
        // interfaz enseña y lo que el usuario ha marcado
        let panel = match vista.marcadores {
            Some(true) => "marcadores".to_string(),
            Some(false) if vista.panel == "marcadores" || vista.panel.is_empty() => {
                "ninguno".to_string()
            }
            _ => vista.panel.clone(),
        };
        match panel_pdf(&panel) {
            Some(n) => catalog.set("PageMode", Object::Name(n.as_bytes().to_vec())),
            None => {
                catalog.remove(b"PageMode");
            }
        }
        Ok(())
    })
}

#[cfg(test)]
mod tests_vista_inicial {
    use super::*;
    use crate::tests::crea_pdf;

    /// **R56 — «Vista inicial».** La cuarta pestaña de ⌘D en Acrobat, y la
    /// única parte de las propiedades que además se escribe: cómo se abre
    /// este documento **en cualquier visor**, no solo en Vitela.
    #[test]
    fn la_vista_inicial_se_lee_se_escribe_y_se_quita() {
        let pdf = std::env::temp_dir().join("documento-vista-inicial.pdf");
        crea_pdf(&["Uno", "Dos", "Tres"], &pdf);
        let work = pdf.to_string_lossy().into_owned();

        // un PDF que no dice nada devuelve la ficha vacía, no un error
        let vacia = get_open_action(work.clone()).expect("leer");
        assert_eq!(vacia.page_index, None);
        assert!(vacia.ajuste.is_empty() && vacia.disposicion.is_empty() && vacia.panel.is_empty());
        assert_eq!(vacia.marcadores, Some(false));

        set_open_action(
            work.clone(),
            VistaInicial {
                page_index: Some(2),
                top: Some(120.0),
                zoom: Some(1.5),
                ajuste: "zoom".into(),
                disposicion: "dos".into(),
                panel: "marcadores".into(),
                marcadores: None,
            },
        )
        .expect("escribir");
        let leida = get_open_action(work.clone()).expect("releer");
        assert_eq!(leida.page_index, Some(2));
        assert_eq!(leida.zoom, Some(1.5));
        assert_eq!(leida.ajuste, "zoom");
        assert_eq!(leida.disposicion, "dos");
        assert_eq!(leida.panel, "marcadores");
        assert_eq!(
            leida.marcadores,
            Some(true),
            "la casilla del diálogo dice lo mismo que el modo"
        );
        assert!(
            leida.top.map(|t| (t - 120.0).abs() < 0.5).unwrap_or(false),
            "el top vuelve donde estaba: {:?}",
            leida.top
        );

        // «ajustar al ancho» es otro modo del spec y se lee como tal
        set_open_action(
            work.clone(),
            VistaInicial {
                page_index: Some(0),
                ajuste: "ancho".into(),
                ..Default::default()
            },
        )
        .expect("escribir ancho");
        let leida = get_open_action(work.clone()).expect("releer");
        assert_eq!(leida.ajuste, "ancho");
        assert_eq!(leida.page_index, Some(0));

        // la casilla manda: sin ella el usuario tendría que elegir entre
        // los seis `/PageMode` del spec para decir «que abra por los
        // marcadores»
        set_open_action(
            work.clone(),
            VistaInicial {
                page_index: Some(0),
                marcadores: Some(false),
                ..Default::default()
            },
        )
        .expect("sin marcadores");
        let leida = get_open_action(work.clone()).expect("releer");
        assert_eq!(leida.marcadores, Some(false));
        assert_eq!(leida.panel, "ninguno");

        // y la ficha vacía devuelve el documento a «lo que decida el visor»
        set_open_action(work.clone(), VistaInicial::default()).expect("quitar");
        let vacia = get_open_action(work.clone()).expect("releer");
        assert_eq!(vacia.page_index, None);
        assert!(vacia.disposicion.is_empty() && vacia.panel.is_empty());

        // una página que no está se dice antes de escribir nada
        assert!(set_open_action(
            work.clone(),
            VistaInicial {
                page_index: Some(9),
                ..Default::default()
            },
        )
        .unwrap_err()
        .contains("no está en el documento"));
        std::fs::remove_file(&pdf).ok();
    }
}
