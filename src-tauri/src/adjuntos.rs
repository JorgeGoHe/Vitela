//! Lo que el PDF lleva dentro y hasta ahora Vitela solo sabía **borrar**:
//! los ficheros adjuntos (`/Names → /EmbeddedFiles`) y las capas
//! (`/OCProperties`). Sanitizar los quitaba sin que el usuario pudiera
//! verlos siquiera.
//!
//! Adjuntos: una factura electrónica española lleva su XML dentro, y un
//! plano suele traer el DWG. Se listan, se guardan al disco y se añaden.
//!
//! **Aviso de alcance de las capas**: PDFium respeta el `/OCProperties /D
//! /OFF` del fichero al renderizar, así que apagar una capa **es escribir
//! en el documento** — no es una vista. Por eso `set_layer_visible` pasa
//! por `mutacion`: cambia el fichero, lo marca como modificado y ⌘Z lo
//! devuelve, y la UI lo dice. Esconderla solo en pantalla exigiría un
//! contexto OCG en el render, que pdfium-render 0.8 no expone.

use crate::{cirugia, on_pdfium_thread, with_lopdf};
use lopdf::{Dictionary, Object, Stream};
use serde::Serialize;

/// Un fichero metido dentro del PDF.
#[derive(Serialize, Debug)]
pub struct Adjunto {
    /// Nombre con el que se guardó, tal como se enseña en la lista.
    pub name: String,
    /// Tamaño en bytes del fichero incrustado (`/Params /Size`, o los
    /// bytes de verdad si no lo declara).
    pub bytes: u64,
    /// Fecha de creación en ISO 8601, vacía si no la trae.
    pub created: String,
    /// La descripción que puso quien lo adjuntó (`/Desc`).
    pub description: String,
}

/// Los ficheros que lleva dentro el documento, en el orden del árbol de
/// nombres (que es el orden alfabético del PDF, el mismo que enseña
/// Acrobat).
#[tauri::command(async)]
pub fn list_attachments(path: String) -> Result<Vec<Adjunto>, String> {
    on_pdfium_thread(move || {
        with_lopdf(&path, |doc| {
            Ok(entradas(doc)
                .into_iter()
                .map(|(nombre, id)| {
                    let spec = doc
                        .get_object(id)
                        .ok()
                        .and_then(|o| o.as_dict().ok())
                        .cloned()
                        .unwrap_or_default();
                    let fichero = fichero_de(doc, &spec);
                    Adjunto {
                        name: nombre_visible(doc, &spec, &nombre),
                        bytes: fichero
                            .as_ref()
                            .map(|(_, bytes)| *bytes)
                            .unwrap_or(0),
                        created: fichero
                            .as_ref()
                            .map(|(fecha, _)| fecha.clone())
                            .unwrap_or_default(),
                        description: spec
                            .get(b"Desc")
                            .map(crate::anotaciones::texto_de_cadena_pdf)
                            .unwrap_or_default(),
                    }
                })
                .collect())
        })
    })
}

/// Guarda un adjunto al disco tal cual está: los mismos bytes que metió
/// quien lo adjuntó, sin recomprimir ni tocar nada.
#[tauri::command(async)]
pub fn save_attachment(path: String, index: u16, dest_path: String) -> Result<u64, String> {
    on_pdfium_thread(move || {
        let bytes = with_lopdf(&path, |doc| {
            let entradas = entradas(doc);
            let (_, id) = entradas
                .get(index as usize)
                .ok_or("Ese adjunto ya no está en el documento")?;
            let spec = doc
                .get_object(*id)
                .and_then(|o| o.as_dict())
                .map_err(|e| e.to_string())?
                .clone();
            let stream_id = stream_de(doc, &spec).ok_or("Ese adjunto no lleva fichero dentro")?;
            let stream = doc
                .get_object(stream_id)
                .and_then(|o| o.as_stream())
                .map_err(|e| e.to_string())?;
            // los adjuntos van comprimidos (/FlateDecode) casi siempre
            Ok(stream
                .decompressed_content()
                .unwrap_or_else(|_| stream.content.clone()))
        })?;
        let n = bytes.len() as u64;
        std::fs::write(&dest_path, bytes).map_err(|e| {
            crate::mensaje_llano(format!("No se ha podido escribir {dest_path}: {e}"))
        })?;
        Ok(n)
    })
}

/// Mete un fichero dentro del PDF, con su nombre, su tamaño y su fecha,
/// como hace «Adjuntar archivo» de Acrobat.
#[tauri::command(async)]
pub fn add_attachment(
    work_path: String,
    file_path: String,
    description: Option<String>,
) -> Result<(), String> {
    let bytes = std::fs::read(&file_path).map_err(|e| {
        crate::mensaje_llano(format!("No se ha podido leer {file_path}: {e}"))
    })?;
    let nombre = std::path::Path::new(&file_path)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .ok_or("Ese fichero no tiene nombre")?;
    let fecha = crate::anotaciones::fecha_pdf_ahora();
    cirugia(&work_path, move |doc| {
        let tam = bytes.len() as i64;
        let mut stream_dict = Dictionary::new();
        stream_dict.set("Type", Object::Name(b"EmbeddedFile".to_vec()));
        let mut params = Dictionary::new();
        params.set("Size", Object::Integer(tam));
        params.set("CreationDate", Object::string_literal(fecha.clone()));
        params.set("ModDate", Object::string_literal(fecha));
        stream_dict.set("Params", Object::Dictionary(params));
        let mut stream = Stream::new(stream_dict, bytes);
        // comprimido, que es lo que espera cualquier visor
        let _ = stream.compress();
        let stream_id = doc.add_object(Object::Stream(stream));

        let mut ef = Dictionary::new();
        ef.set("F", Object::Reference(stream_id));
        let mut spec = Dictionary::new();
        spec.set("Type", Object::Name(b"Filespec".to_vec()));
        spec.set("F", crate::documento::cadena_pdf(&nombre));
        spec.set("UF", crate::documento::cadena_pdf(&nombre));
        spec.set("EF", Object::Dictionary(ef));
        if let Some(d) = description.as_deref().filter(|d| !d.trim().is_empty()) {
            spec.set("Desc", crate::documento::cadena_pdf(d));
        }
        let spec_id = doc.add_object(Object::Dictionary(spec));
        anade_al_arbol(doc, &nombre, spec_id)
    })
}

/// Una capa del documento (un grupo de contenido opcional, `/OCG`).
#[derive(Serialize, Debug)]
pub struct Capa {
    pub name: String,
    /// ¿Se ve ahora mismo? Sale del `/OFF` de la configuración por defecto.
    pub visible: bool,
}

/// Las capas del documento, en el orden en que las declara el catálogo.
#[tauri::command(async)]
pub fn list_layers(path: String) -> Result<Vec<Capa>, String> {
    on_pdfium_thread(move || {
        with_lopdf(&path, |doc| {
            let (ocgs, apagadas) = capas_de(doc);
            Ok(ocgs
                .into_iter()
                .map(|id| Capa {
                    name: doc
                        .get_object(id)
                        .ok()
                        .and_then(|o| o.as_dict().ok())
                        .and_then(|d| d.get(b"Name").ok())
                        .map(crate::anotaciones::texto_de_cadena_pdf)
                        .filter(|n| !n.is_empty())
                        .unwrap_or_else(|| "Capa sin nombre".into()),
                    visible: !apagadas.contains(&id),
                })
                .collect())
        })
    })
}

/// Enciende o apaga una capa escribiendo el `/OFF` de la configuración por
/// defecto. **Cambia el fichero** (ver el aviso de arriba): pasa por
/// `mutacion`, así que deja su paso de deshacer.
#[tauri::command(async)]
pub fn set_layer_visible(work_path: String, index: u16, visible: bool) -> Result<(), String> {
    cirugia(&work_path, move |doc| {
        let (ocgs, mut apagadas) = capas_de(doc);
        let id = *ocgs
            .get(index as usize)
            .ok_or("Esa capa ya no está en el documento")?;
        if visible {
            apagadas.retain(|x| *x != id);
        } else if !apagadas.contains(&id) {
            apagadas.push(id);
        }
        let root = doc.trailer.get(b"Root").and_then(|o| o.as_reference()).map_err(|e| e.to_string())?;
        let oc_ref = doc
            .get_object(root)
            .and_then(|o| o.as_dict())
            .and_then(|d| d.get(b"OCProperties"))
            .map_err(|_| "El documento no tiene capas".to_string())?
            .clone();
        let d_ref = match &oc_ref {
            Object::Reference(id) => doc
                .get_object(*id)
                .and_then(|o| o.as_dict())
                .map_err(|e| e.to_string())?
                .get(b"D")
                .ok()
                .cloned(),
            Object::Dictionary(d) => d.get(b"D").ok().cloned(),
            _ => None,
        };
        let off = Object::Array(apagadas.into_iter().map(Object::Reference).collect());
        match d_ref {
            // la configuración por defecto es un objeto propio
            Some(Object::Reference(did)) => {
                doc.get_object_mut(did)
                    .and_then(|o| o.as_dict_mut())
                    .map_err(|e| e.to_string())?
                    .set("OFF", off);
            }
            // o va en línea dentro de /OCProperties
            _ => {
                let oc_id = match oc_ref {
                    Object::Reference(id) => id,
                    _ => {
                        return Err(
                            "Las capas de este documento no se pueden apagar desde aquí".into()
                        )
                    }
                };
                let oc = doc
                    .get_object_mut(oc_id)
                    .and_then(|o| o.as_dict_mut())
                    .map_err(|e| e.to_string())?;
                let mut d = oc
                    .get(b"D")
                    .and_then(|o| o.as_dict())
                    .cloned()
                    .unwrap_or_default();
                d.set("OFF", off);
                oc.set("D", Object::Dictionary(d));
            }
        }
        Ok(())
    })
}

/// Los `/OCGs` del catálogo y las que están apagadas en la configuración
/// por defecto (`/D /OFF`).
fn capas_de(doc: &lopdf::Document) -> (Vec<lopdf::ObjectId>, Vec<lopdf::ObjectId>) {
    let refs = |o: Option<&Object>| -> Vec<lopdf::ObjectId> {
        let Some(o) = o else { return Vec::new() };
        let arr = match o {
            Object::Array(a) => a.clone(),
            Object::Reference(id) => doc
                .get_object(*id)
                .and_then(|x| x.as_array())
                .cloned()
                .unwrap_or_default(),
            _ => Vec::new(),
        };
        arr.iter()
            .filter_map(|x| match x {
                Object::Reference(id) => Some(*id),
                _ => None,
            })
            .collect()
    };
    let Ok(root) = doc.trailer.get(b"Root").and_then(|o| o.as_reference()) else {
        return (Vec::new(), Vec::new());
    };
    let Some(oc) = doc
        .get_object(root)
        .ok()
        .and_then(|o| o.as_dict().ok())
        .and_then(|d| d.get(b"OCProperties").ok())
        .and_then(|o| dict_de(doc, o))
    else {
        return (Vec::new(), Vec::new());
    };
    let ocgs = refs(oc.get(b"OCGs").ok());
    let apagadas = oc
        .get(b"D")
        .ok()
        .and_then(|o| dict_de(doc, o))
        .map(|d| refs(d.get(b"OFF").ok()))
        .unwrap_or_default();
    (ocgs, apagadas)
}

fn dict_de(doc: &lopdf::Document, o: &Object) -> Option<Dictionary> {
    match o {
        Object::Dictionary(d) => Some(d.clone()),
        Object::Reference(id) => doc.get_object(*id).ok()?.as_dict().ok().cloned(),
        _ => None,
    }
}

/// Las entradas del árbol `/EmbeddedFiles`: (clave, id del `/Filespec`).
/// El árbol de nombres puede venir plano o repartido en `/Kids`.
fn entradas(doc: &lopdf::Document) -> Vec<(String, lopdf::ObjectId)> {
    fn recorre(
        doc: &lopdf::Document,
        nodo: &Object,
        out: &mut Vec<(String, lopdf::ObjectId)>,
        hondo: u8,
    ) {
        if hondo > 8 {
            return;
        }
        let Some(d) = dict_de(doc, nodo) else { return };
        if let Ok(Object::Array(names)) = d.get(b"Names") {
            for par in names.chunks(2) {
                let (Some(clave), Some(valor)) = (par.first(), par.get(1)) else {
                    continue;
                };
                let nombre = crate::anotaciones::texto_de_cadena_pdf(clave);
                match valor {
                    Object::Reference(id) => out.push((nombre, *id)),
                    Object::Dictionary(_) => {}
                    _ => {}
                }
            }
        }
        if let Ok(Object::Array(kids)) = d.get(b"Kids") {
            for k in kids.clone() {
                recorre(doc, &k, out, hondo + 1);
            }
        }
    }
    let Ok(root) = doc.trailer.get(b"Root").and_then(|o| o.as_reference()) else {
        return Vec::new();
    };
    let Some(nombres) = doc
        .get_object(root)
        .ok()
        .and_then(|o| o.as_dict().ok())
        .and_then(|d| d.get(b"Names").ok())
        .and_then(|o| dict_de(doc, o))
    else {
        return Vec::new();
    };
    let Ok(arbol) = nombres.get(b"EmbeddedFiles") else {
        return Vec::new();
    };
    let mut out = Vec::new();
    recorre(doc, arbol, &mut out, 0);
    out
}

/// El nombre que se enseña: el `/UF` (Unicode) o el `/F` del `/Filespec`;
/// si no llevan ninguno, la clave del árbol.
fn nombre_visible(_doc: &lopdf::Document, spec: &Dictionary, clave: &str) -> String {
    for k in [b"UF".as_ref(), b"F".as_ref()] {
        if let Ok(o) = spec.get(k) {
            let n = crate::anotaciones::texto_de_cadena_pdf(o);
            if !n.is_empty() {
                return n;
            }
        }
    }
    clave.to_string()
}

/// El stream del fichero incrustado de un `/Filespec` (`/EF /F`, o `/UF`).
fn stream_de(doc: &lopdf::Document, spec: &Dictionary) -> Option<lopdf::ObjectId> {
    let ef = spec.get(b"EF").ok().and_then(|o| dict_de(doc, o))?;
    for k in [b"F".as_ref(), b"UF".as_ref()] {
        if let Ok(Object::Reference(id)) = ef.get(k) {
            return Some(*id);
        }
    }
    None
}

/// Fecha y tamaño del fichero incrustado, para la fila de la lista.
fn fichero_de(doc: &lopdf::Document, spec: &Dictionary) -> Option<(String, u64)> {
    let id = stream_de(doc, spec)?;
    let stream = doc.get_object(id).ok()?.as_stream().ok()?;
    let params = stream.dict.get(b"Params").ok().and_then(|o| dict_de(doc, o));
    let tam = params
        .as_ref()
        .and_then(|p| p.get(b"Size").ok())
        .and_then(|o| o.as_i64().ok())
        .map(|n| n as u64)
        .unwrap_or_else(|| {
            stream
                .decompressed_content()
                .map(|c| c.len() as u64)
                .unwrap_or(stream.content.len() as u64)
        });
    let fecha = params
        .as_ref()
        .and_then(|p| p.get(b"CreationDate").ok())
        .map(|o| crate::anotaciones::fecha_pdf_a_iso(&crate::anotaciones::texto_de_cadena_pdf(o)))
        .unwrap_or_default();
    Some((fecha, tam))
}

/// Mete un `/Filespec` en el árbol `/EmbeddedFiles`, creándolo si el
/// documento no tenía ninguno. Las claves van ordenadas: el spec lo exige y
/// hay visores que buscan por bisección.
fn anade_al_arbol(
    doc: &mut lopdf::Document,
    nombre: &str,
    spec_id: lopdf::ObjectId,
) -> Result<(), String> {
    let root = doc
        .trailer
        .get(b"Root")
        .and_then(|o| o.as_reference())
        .map_err(|e| e.to_string())?;
    // las que ya había (aplanadas: si el árbol venía con /Kids, se queda
    // en uno solo, que es igual de válido y mucho más simple)
    let mut pares: Vec<(String, lopdf::ObjectId)> = entradas(doc);
    pares.push((nombre.to_string(), spec_id));
    pares.sort_by(|a, b| a.0.cmp(&b.0));
    let names: Vec<Object> = pares
        .into_iter()
        .flat_map(|(n, id)| {
            vec![
                crate::documento::cadena_pdf(&n),
                Object::Reference(id),
            ]
        })
        .collect();
    let mut arbol = Dictionary::new();
    arbol.set("Names", Object::Array(names));
    let arbol_id = doc.add_object(Object::Dictionary(arbol));

    let nombres_ref = doc
        .get_object(root)
        .and_then(|o| o.as_dict())
        .map_err(|e| e.to_string())?
        .get(b"Names")
        .ok()
        .cloned();
    match nombres_ref {
        Some(Object::Reference(id)) => {
            doc.get_object_mut(id)
                .and_then(|o| o.as_dict_mut())
                .map_err(|e| e.to_string())?
                .set("EmbeddedFiles", Object::Reference(arbol_id));
        }
        Some(Object::Dictionary(mut d)) => {
            d.set("EmbeddedFiles", Object::Reference(arbol_id));
            doc.get_object_mut(root)
                .and_then(|o| o.as_dict_mut())
                .map_err(|e| e.to_string())?
                .set("Names", Object::Dictionary(d));
        }
        _ => {
            let mut d = Dictionary::new();
            d.set("EmbeddedFiles", Object::Reference(arbol_id));
            let id = doc.add_object(Object::Dictionary(d));
            doc.get_object_mut(root)
                .and_then(|o| o.as_dict_mut())
                .map_err(|e| e.to_string())?
                .set("Names", Object::Reference(id));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::crea_pdf;

    /// **G4.** Un adjunto entra, se lista con su nombre y su tamaño, y sale
    /// **byte a byte igual** que entró: es lo único que importa de un
    /// fichero que alguien va a abrir con otro programa.
    #[test]
    fn un_adjunto_entra_se_lista_y_sale_igual() {
        let dir = std::env::temp_dir();
        let pdf = dir.join("adjuntos-test.pdf");
        let xml = dir.join("adjuntos-factura.xml");
        let vuelta = dir.join("adjuntos-vuelta.xml");
        crea_pdf(&["Factura"], &pdf);
        let work = pdf.to_string_lossy().into_owned();
        let contenido = b"<factura><total>42</total></factura>".to_vec();
        std::fs::write(&xml, &contenido).expect("escribir xml");

        assert!(list_attachments(work.clone()).expect("listar").is_empty());
        add_attachment(
            work.clone(),
            xml.to_string_lossy().into_owned(),
            Some("La factura en XML".into()),
        )
        .expect("adjuntar");

        let lista = list_attachments(work.clone()).expect("listar");
        assert_eq!(lista.len(), 1, "{lista:?}");
        assert_eq!(lista[0].name, "adjuntos-factura.xml");
        assert_eq!(lista[0].bytes, contenido.len() as u64);
        assert_eq!(lista[0].description, "La factura en XML");
        assert!(!lista[0].created.is_empty(), "la fecha de cuando se metió");

        let n = save_attachment(work.clone(), 0, vuelta.to_string_lossy().into_owned())
            .expect("guardar");
        assert_eq!(n as usize, contenido.len());
        assert_eq!(
            std::fs::read(&vuelta).expect("leer"),
            contenido,
            "el fichero que sale tiene que ser el que entró"
        );

        // y sanitizar lo cuenta, que es de donde venía la asimetría
        let informe = crate::seguridad2::sanitize_pdf(work.clone(), true).expect("ensayo");
        assert_eq!(informe.adjuntos, 1);

        // ⌘Z lo quita
        crate::historial::undo(work.clone()).expect("deshacer");
        assert!(list_attachments(work.clone()).expect("listar").is_empty());

        for f in [&pdf, &xml, &vuelta] {
            std::fs::remove_file(f).ok();
        }
    }

    /// **G4.** Apagar una capa cambia lo que se ve. Ojo: **cambia el
    /// fichero** (PDFium respeta el `/OFF` del documento al renderizar), y
    /// por eso deja su paso de deshacer.
    #[test]
    fn apagar_una_capa_le_quita_la_tinta_a_la_pagina() {
        let dir = std::env::temp_dir();
        let pdf = dir.join("capas-test.pdf");
        crea_pdf(&["Con capa"], &pdf);
        let work = pdf.to_string_lossy().into_owned();
        assert!(list_layers(work.clone()).expect("listar").is_empty());

        // un documento con una capa que envuelve todo el contenido
        crate::cirugia(&work, |doc| {
            let mut ocg = Dictionary::new();
            ocg.set("Type", Object::Name(b"OCG".to_vec()));
            ocg.set("Name", crate::documento::cadena_pdf("Cotas"));
            let ocg_id = doc.add_object(Object::Dictionary(ocg));
            let mut d = Dictionary::new();
            d.set("ON", Object::Array(vec![Object::Reference(ocg_id)]));
            d.set("Order", Object::Array(vec![Object::Reference(ocg_id)]));
            let mut oc = Dictionary::new();
            oc.set("OCGs", Object::Array(vec![Object::Reference(ocg_id)]));
            oc.set("D", Object::Dictionary(d));
            let oc_id = doc.add_object(Object::Dictionary(oc));
            let root = doc.trailer.get(b"Root").and_then(|o| o.as_reference()).unwrap();
            doc.get_object_mut(root)
                .and_then(|o| o.as_dict_mut())
                .unwrap()
                .set("OCProperties", Object::Reference(oc_id));
            // el contenido de la página, marcado como de esa capa
            let page_id = *doc.get_pages().get(&1).unwrap();
            let contenido = doc.get_page_content(page_id).expect("contenido");
            let mut nuevo = b"/OC /Capa0 BDC\n".to_vec();
            nuevo.extend_from_slice(&contenido);
            nuevo.extend_from_slice(b"\nEMC\n");
            doc.change_page_content(page_id, nuevo).expect("contenido");
            let page = doc.get_object_mut(page_id).and_then(|o| o.as_dict_mut()).unwrap();
            let mut props = Dictionary::new();
            props.set("Capa0", Object::Reference(ocg_id));
            let mut recursos = page
                .get(b"Resources")
                .and_then(|o| o.as_dict())
                .cloned()
                .unwrap_or_default();
            recursos.set("Properties", Object::Dictionary(props));
            page.set("Resources", Object::Dictionary(recursos));
            Ok(())
        })
        .expect("preparar el documento con capa");

        let capas = list_layers(work.clone()).expect("listar");
        assert_eq!(capas.len(), 1);
        assert_eq!(capas[0].name, "Cotas");
        assert!(capas[0].visible, "empieza encendida");
        assert!(hay_tinta(&work), "con la capa encendida se ve el texto");

        set_layer_visible(work.clone(), 0, false).expect("apagar");
        let capas = list_layers(work.clone()).expect("listar");
        assert!(!capas[0].visible, "y ahora está apagada");
        assert!(
            !hay_tinta(&work),
            "apagar la capa tiene que quitar su tinta del render"
        );

        // ⌘Z la devuelve: es un cambio del documento, no de la vista
        crate::historial::undo(work.clone()).expect("deshacer");
        assert!(list_layers(work.clone()).expect("listar")[0].visible);
        assert!(hay_tinta(&work));
        std::fs::remove_file(&pdf).ok();
    }

    /// ¿Queda algo pintado en la primera página?
    fn hay_tinta(work: &str) -> bool {
        let png = crate::render_page_png(work.to_string(), 0, 300, true).expect("render");
        let img = image::load_from_memory(&png).expect("PNG").to_rgba8();
        img.pixels().any(|p| p.0[0] < 200 && p.0[1] < 200 && p.0[2] < 200)
    }
}
