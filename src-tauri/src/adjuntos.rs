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
use lopdf::{Dictionary, Document as LoDoc, Object, ObjectId, Stream};
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

/// Quita un adjunto del documento, como «Eliminar» del panel de adjuntos
/// de Acrobat. `index` es la posición en la lista de `list_attachments`.
///
/// No basta con sacar la entrada del árbol de nombres: el `/Filespec` y el
/// stream con los bytes seguirían dentro del fichero, sin nadie que
/// apuntara a ellos —justo lo que `sanitize_pdf` aprendió a no hacer—. Por
/// eso se poda el documento al final. Pasa por `cirugia`, así que deja su
/// paso de deshacer.
#[tauri::command(async)]
pub fn delete_attachment(work_path: String, index: u16) -> Result<(), String> {
    cirugia(&work_path, move |doc| {
        let mut pares = entradas(doc);
        if index as usize >= pares.len() {
            return Err("Ese adjunto ya no está en el documento".into());
        }
        pares.remove(index as usize);
        reescribe_arbol(doc, pares)?;
        // los bytes se van de verdad: quitar la referencia dejaría el
        // fichero incrustado dentro del PDF
        doc.prune_objects();
        Ok(())
    })
}

/// Deja un adjunto en un fichero temporal y devuelve su ruta, para que la
/// UI lo abra con el visor del sistema. Es la acción principal de la fila:
/// el XML de una factura se quiere **ver**, no guardar en el escritorio.
///
/// Cada uno va en su propia carpeta (`vitela-adjunto-…`) para conservar el
/// nombre y la extensión de verdad, que es lo que mira el sistema para
/// elegir con qué programa abrirlo. El barrido de huérfanos del arranque se
/// lleva las de más de 24 h.
#[tauri::command(async)]
pub fn open_attachment(path: String, index: u16) -> Result<String, String> {
    let (nombre, bytes) = on_pdfium_thread(move || {
        with_lopdf(&path, |doc| {
            let entradas = entradas(doc);
            let (clave, id) = entradas
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
            Ok((
                nombre_visible(doc, &spec, clave),
                stream
                    .decompressed_content()
                    .unwrap_or_else(|_| stream.content.clone()),
            ))
        })
    })?;
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let dir = std::env::temp_dir().join(format!("vitela-adjunto-{nanos}"));
    std::fs::create_dir_all(&dir).map_err(|e| {
        crate::mensaje_llano(format!("No se ha podido preparar el adjunto: {e}"))
    })?;
    let destino = dir.join(nombre_seguro(&nombre));
    std::fs::write(&destino, bytes).map_err(|e| {
        crate::mensaje_llano(format!("No se ha podido preparar el adjunto: {e}"))
    })?;
    Ok(destino.to_string_lossy().into_owned())
}

/// El nombre de un adjunto, limpio para usarlo como nombre de fichero: el
/// `/F` de un `/Filespec` lo escribe quien hizo el PDF y puede traer barras
/// o `..`, que fuera de su carpeta escribirían donde no deben.
fn nombre_seguro(nombre: &str) -> String {
    let limpio: String = nombre
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(nombre)
        .chars()
        .filter(|c| !matches!(c, ':' | '*' | '?' | '"' | '<' | '>' | '|' | '\0'))
        .collect();
    let limpio = limpio.trim().trim_matches('.').to_string();
    if limpio.is_empty() {
        "adjunto".into()
    } else {
        limpio
    }
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
        // `/OCProperties` puede venir **en línea** en el catálogo (legal, y
        // frecuente en generadores sencillos): entonces no hay ningún
        // objeto que modificar, así que se promueve a objeto propio. Antes
        // se contestaba «no se pueden apagar desde aquí» —un mensaje que
        // además sugería que había otro sitio donde sí, y no lo hay— y el
        // panel se quedaba con casillas que no hacían nada (AC-056).
        let oc_id = match oc_ref {
            Object::Reference(id) => id,
            Object::Dictionary(d) => {
                let id = doc.add_object(Object::Dictionary(d));
                doc.get_object_mut(root)
                    .and_then(|o| o.as_dict_mut())
                    .map_err(|e| e.to_string())?
                    .set("OCProperties", Object::Reference(id));
                id
            }
            _ => return Err("El documento no tiene capas".into()),
        };
        let d_ref = doc
            .get_object(oc_id)
            .and_then(|o| o.as_dict())
            .map_err(|e| e.to_string())?
            .get(b"D")
            .ok()
            .cloned();
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
    // las que ya había (aplanadas: si el árbol venía con /Kids, se queda
    // en uno solo, que es igual de válido y mucho más simple)
    let mut pares: Vec<(String, lopdf::ObjectId)> = entradas(doc);
    pares.push((nombre.to_string(), spec_id));
    reescribe_arbol(doc, pares)
}

/// Deja el árbol `/EmbeddedFiles` con exactamente estas entradas, ordenadas
/// por clave (el spec lo exige y hay visores que buscan por bisección). Lo
/// usan añadir y borrar.
fn reescribe_arbol(
    doc: &mut lopdf::Document,
    mut pares: Vec<(String, lopdf::ObjectId)>,
) -> Result<(), String> {
    let root = doc
        .trailer
        .get(b"Root")
        .and_then(|o| o.as_reference())
        .map_err(|e| e.to_string())?;
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

    /// **R27.** Los adjuntos se abren y se borran. Hasta ahora Vitela
    /// sabía **añadir** un adjunto y no sabía quitar uno: el que se
    /// equivocaba de fichero tenía que sanitizar el documento entero
    /// —perdiendo metadatos, scripts, capas y formulario— para deshacerlo.
    ///
    /// Borrar el primero tiene que dejar el segundo **entero byte a byte**,
    /// y los bytes del borrado tienen que irse del fichero de verdad.
    #[test]
    fn borrar_un_adjunto_deja_el_otro_entero_y_se_lleva_sus_bytes() {
        let dir = std::env::temp_dir();
        let pdf = dir.join("adjuntos-borrar.pdf");
        let uno = dir.join("adjuntos-uno.xml");
        let dos = dir.join("adjuntos-dos.txt");
        let vuelta = dir.join("adjuntos-borrar-vuelta.txt");
        crea_pdf(&["Con dos adjuntos"], &pdf);
        let work = pdf.to_string_lossy().into_owned();
        let bytes_uno = b"<uno>este se va</uno>".repeat(40);
        let bytes_dos = b"el segundo se queda igual".to_vec();
        std::fs::write(&uno, &bytes_uno).expect("escribir uno");
        std::fs::write(&dos, &bytes_dos).expect("escribir dos");
        add_attachment(work.clone(), uno.to_string_lossy().into_owned(), None).expect("uno");
        add_attachment(work.clone(), dos.to_string_lossy().into_owned(), None).expect("dos");
        let lista = list_attachments(work.clone()).expect("listar");
        assert_eq!(lista.len(), 2, "{lista:?}");
        assert_eq!(lista[0].name, "adjuntos-dos.txt", "van en orden alfabético");

        // se borra el XML (el segundo de la lista, por el orden del árbol)
        delete_attachment(work.clone(), 1).expect("borrar");
        let lista = list_attachments(work.clone()).expect("listar");
        assert_eq!(lista.len(), 1, "queda uno: {lista:?}");
        assert_eq!(lista[0].name, "adjuntos-dos.txt");

        // el que queda sale byte a byte igual que entró
        let n = save_attachment(work.clone(), 0, vuelta.to_string_lossy().into_owned())
            .expect("guardar el que queda");
        assert_eq!(n as usize, bytes_dos.len());
        assert_eq!(std::fs::read(&vuelta).expect("leer"), bytes_dos);

        // y sanitizar cuenta uno, no dos
        let informe = crate::seguridad2::sanitize_pdf(work.clone(), true).expect("ensayo");
        assert_eq!(informe.adjuntos, 1, "{informe:?}");

        // los bytes del borrado ya no están en el fichero
        let crudo = std::fs::read(&work).expect("leer el pdf");
        assert!(
            !crudo.windows(9).any(|w| w == b"este se v"),
            "quitar la referencia no basta: hay que podar el objeto"
        );

        // ⌘Z lo devuelve
        crate::historial::undo(work.clone()).expect("deshacer");
        assert_eq!(list_attachments(work.clone()).expect("listar").len(), 2);

        // y un índice que no existe se dice, no se traga
        assert!(delete_attachment(work.clone(), 9).is_err());

        for f in [&pdf, &uno, &dos, &vuelta] {
            std::fs::remove_file(f).ok();
        }
    }

    /// **R27.** «Abrir» deja el adjunto en un temporal **con su nombre y su
    /// extensión**, que es lo que mira el sistema para elegir programa.
    #[test]
    fn abrir_un_adjunto_lo_deja_en_un_temporal_con_su_nombre() {
        let dir = std::env::temp_dir();
        let pdf = dir.join("adjuntos-abrir.pdf");
        let xml = dir.join("adjuntos-abrir-factura.xml");
        crea_pdf(&["Factura"], &pdf);
        let work = pdf.to_string_lossy().into_owned();
        let contenido = b"<factura><total>42</total></factura>".to_vec();
        std::fs::write(&xml, &contenido).expect("escribir xml");
        add_attachment(work.clone(), xml.to_string_lossy().into_owned(), None).expect("adjuntar");

        let ruta = open_attachment(work.clone(), 0).expect("abrir");
        let ruta = std::path::PathBuf::from(ruta);
        assert_eq!(
            ruta.file_name().unwrap().to_string_lossy(),
            "adjuntos-abrir-factura.xml",
            "el nombre y la extensión son los del adjunto"
        );
        assert_eq!(std::fs::read(&ruta).expect("leer"), contenido);
        assert!(
            ruta.parent()
                .unwrap()
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("vitela-adjunto-"),
            "cada uno en su carpeta, para que el barrido se las lleve"
        );
        assert!(open_attachment(work.clone(), 7).is_err(), "un índice que no existe");

        std::fs::remove_dir_all(ruta.parent().unwrap()).ok();
        for f in [&pdf, &xml] {
            std::fs::remove_file(f).ok();
        }
    }

    /// Un nombre de adjunto lo escribe quien hizo el PDF: puede traer
    /// barras o `..` y escribiría fuera de su carpeta.
    #[test]
    fn el_nombre_del_adjunto_no_se_sale_de_su_carpeta() {
        assert_eq!(nombre_seguro("factura.xml"), "factura.xml");
        assert_eq!(nombre_seguro("../../etc/passwd"), "passwd");
        assert_eq!(nombre_seguro("C:\\Windows\\system32\\a.dll"), "a.dll");
        assert_eq!(nombre_seguro("  ..  "), "adjunto");
        assert_eq!(nombre_seguro(""), "adjunto");
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

    /// **AC-056.** Con `/OCProperties` **en línea** en el catálogo —legal y
    /// frecuente en generadores sencillos— las capas se listaban (así que
    /// el panel pintaba sus casillas) y apagarlas contestaba «las capas de
    /// este documento no se pueden apagar desde aquí», un mensaje que
    /// además sugería que había otro sitio donde sí. Ahora el diccionario
    /// se promueve a objeto propio y se apaga como cualquier otro.
    #[test]
    fn una_capa_se_apaga_tambien_con_ocproperties_en_linea() {
        let dir = std::env::temp_dir();
        let pdf = dir.join("capas-en-linea.pdf");
        crea_pdf(&["Con capa en línea"], &pdf);
        let work = pdf.to_string_lossy().into_owned();

        crate::cirugia(&work, |doc| {
            let mut ocg = Dictionary::new();
            ocg.set("Type", Object::Name(b"OCG".to_vec()));
            ocg.set("Name", crate::documento::cadena_pdf("Cotas"));
            let ocg_id = doc.add_object(Object::Dictionary(ocg));
            // /D en línea DENTRO de un /OCProperties también en línea, que
            // es exactamente el documento del informe de QA
            let mut d = Dictionary::new();
            d.set("ON", Object::Array(vec![Object::Reference(ocg_id)]));
            d.set("Order", Object::Array(vec![Object::Reference(ocg_id)]));
            let mut oc = Dictionary::new();
            oc.set("OCGs", Object::Array(vec![Object::Reference(ocg_id)]));
            oc.set("D", Object::Dictionary(d));
            let root = doc.trailer.get(b"Root").and_then(|o| o.as_reference()).unwrap();
            doc.get_object_mut(root)
                .and_then(|o| o.as_dict_mut())
                .unwrap()
                .set("OCProperties", Object::Dictionary(oc));
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
        .expect("preparar el documento");

        let capas = list_layers(work.clone()).expect("listar");
        assert_eq!(capas.len(), 1, "la capa se lista: {capas:?}");
        assert!(hay_tinta(&work));

        set_layer_visible(work.clone(), 0, false).expect("apagar con /OCProperties en línea");
        assert!(!list_layers(work.clone()).expect("listar")[0].visible);
        assert!(!hay_tinta(&work), "apagarla tiene que quitarle la tinta");

        // y se vuelve a encender, que es lo que hace la casilla al segundo clic
        set_layer_visible(work.clone(), 0, true).expect("encender");
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

// ---------------------------------------------------------------------------
// El adjunto que es un comentario (`/FileAttachment`)
// ---------------------------------------------------------------------------

/// Lado del icono de la chincheta, en puntos. Es el tamaño con el que
/// Acrobat pinta los suyos, y como el del post-it **no se redimensiona**:
/// un icono más grande no dice nada más.
const LADO_CHINCHETA: f32 = 20.0;

/// El `/AP` de la chincheta: PDFium no lo escribe para este subtipo (como
/// no lo escribe para las notas), así que se dibuja a mano en coordenadas
/// locales, con `/BBox 0 0 lado lado`, para que se vea en cualquier visor y
/// se imprima.
fn apariencia_chincheta(doc: &mut LoDoc, color: [f32; 3]) -> ObjectId {
    let l = LADO_CHINCHETA;
    let mut ops = String::new();
    ops.push_str(&format!("{:.3} {:.3} {:.3} rg\n", color[0], color[1], color[2]));
    ops.push_str("0.2 0.2 0.2 RG\n0.8 w\n");
    // la cabeza
    ops.push_str(&format!(
        "{:.2} {:.2} {:.2} {:.2} re\n",
        l * 0.28,
        l * 0.55,
        l * 0.44,
        l * 0.30
    ));
    // el cuello y la aguja
    ops.push_str(&format!(
        "{:.2} {:.2} {:.2} {:.2} re\n",
        l * 0.40,
        l * 0.30,
        l * 0.20,
        l * 0.25
    ));
    ops.push_str("B\n");
    ops.push_str(&format!("{:.2} {:.2} m\n", l * 0.50, l * 0.30));
    ops.push_str(&format!("{:.2} {:.2} l\nS\n", l * 0.50, l * 0.06));

    let mut forma = Dictionary::new();
    forma.set("Type", Object::Name(b"XObject".to_vec()));
    forma.set("Subtype", Object::Name(b"Form".to_vec()));
    forma.set("FormType", 1i64);
    forma.set(
        "BBox",
        Object::Array(vec![0.into(), 0.into(), l.into(), l.into()]),
    );
    forma.set("Resources", Object::Dictionary(Dictionary::new()));
    doc.add_object(Stream::new(forma, ops.into_bytes()))
}

/// **Adjuntar un fichero como comentario**: la chincheta de Acrobat
/// («Comentar ▸ Adjuntar archivo»). El fichero viaja dentro del PDF, en la
/// página y en el punto donde se pincha, y sale en el panel de comentarios
/// como uno más.
///
/// **Es distinto del adjunto del documento** (`add_attachment`): aquel va
/// en el árbol `/Names → /EmbeddedFiles`, no tiene sitio en ninguna página
/// y se abre desde el panel de adjuntos. Este está *en* la página, donde
/// alguien lo puso, y es parte de la revisión.
///
/// `punto` es la esquina de la chincheta en el espacio propio de la página,
/// como el resto de comandos que escriben. El icono no se redimensiona:
/// `transform_annotation` lo mueve, como al post-it.
#[tauri::command(async)]
pub fn add_file_attachment_annotation(
    work_path: String,
    page_index: u16,
    punto: [f32; 2],
    src_path: String,
    author: Option<String>,
) -> Result<(), String> {
    let bytes = std::fs::read(&src_path)
        .map_err(|e| crate::mensaje_llano(format!("No se ha podido leer {src_path}: {e}")))?;
    let nombre = std::path::Path::new(&src_path)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .ok_or("Ese fichero no tiene nombre")?;
    let autor = crate::anotaciones::autor_o_sistema(author);
    let fecha = crate::anotaciones::fecha_pdf_ahora();
    cirugia(&work_path, move |doc| {
        let page_id = *doc
            .get_pages()
            .get(&(page_index as u32 + 1))
            .ok_or("Página fuera de rango")?;
        let geo = crate::formularios2::geo_pagina(doc, page_id)?;
        let caja = geo.ui_rect_a_pdf(&crate::Rect {
            x: punto[0],
            y: punto[1],
            w: LADO_CHINCHETA,
            h: LADO_CHINCHETA,
        });

        let tam = bytes.len() as i64;
        let mut stream_dict = Dictionary::new();
        stream_dict.set("Type", Object::Name(b"EmbeddedFile".to_vec()));
        let mut params = Dictionary::new();
        params.set("Size", Object::Integer(tam));
        params.set("CreationDate", Object::string_literal(fecha.clone()));
        params.set("ModDate", Object::string_literal(fecha.clone()));
        stream_dict.set("Params", Object::Dictionary(params));
        let mut stream = Stream::new(stream_dict, bytes);
        let _ = stream.compress();
        let stream_id = doc.add_object(Object::Stream(stream));

        let mut ef = Dictionary::new();
        ef.set("F", Object::Reference(stream_id));
        let mut spec = Dictionary::new();
        spec.set("Type", Object::Name(b"Filespec".to_vec()));
        spec.set("F", crate::documento::cadena_pdf(&nombre));
        spec.set("UF", crate::documento::cadena_pdf(&nombre));
        spec.set("EF", Object::Dictionary(ef));
        let spec_id = doc.add_object(Object::Dictionary(spec));

        let ap_id = apariencia_chincheta(doc, [0.99, 0.73, 0.18]);
        let mut annot = Dictionary::new();
        annot.set("Type", Object::Name(b"Annot".to_vec()));
        annot.set("Subtype", Object::Name(b"FileAttachment".to_vec()));
        annot.set(
            "Rect",
            Object::Array(vec![
                caja.left().value.into(),
                caja.bottom().value.into(),
                caja.right().value.into(),
                caja.top().value.into(),
            ]),
        );
        annot.set("FS", Object::Reference(spec_id));
        // el nombre del icono del spec; Acrobat ofrece cuatro y la chincheta
        // es el suyo por defecto
        annot.set("Name", Object::Name(b"PushPin".to_vec()));
        // el texto del comentario es el nombre del fichero: es lo que el
        // panel de comentarios tiene que enseñar en su fila
        annot.set("Contents", crate::documento::cadena_pdf(&nombre));
        annot.set("C", Object::Array(vec![0.99.into(), 0.73.into(), 0.18.into()]));
        annot.set("F", 4i64); // Print
        annot.set("T", crate::documento::cadena_pdf(&autor));
        annot.set("CreationDate", Object::string_literal(fecha.clone()));
        annot.set("M", Object::string_literal(fecha));
        let mut ap = Dictionary::new();
        ap.set("N", Object::Reference(ap_id));
        annot.set("AP", Object::Dictionary(ap));
        let annot_id = doc.add_object(annot);
        crate::formularios2::anade_a_annots(doc, page_id, annot_id)
    })
}

/// El nombre y los bytes del fichero que lleva dentro una anotación
/// `/FileAttachment` (la chincheta de «Comentar ▸ Adjuntar archivo»).
///
/// Es **otro** camino que el del adjunto del documento: aquel cuelga del
/// árbol `/Names → /EmbeddedFiles` y este del `/FS` de una anotación de una
/// página. Hasta el ciclo 9 se sabía poner y no se sabía sacar, que es un
/// callejón sin salida con el fichero del usuario dentro.
fn fichero_de_la_chincheta(
    path: &str,
    page_index: u16,
    annot_index: u16,
) -> Result<(String, Vec<u8>), String> {
    let path = path.to_string();
    on_pdfium_thread(move || {
        with_lopdf(&path, |doc| {
            let annots = crate::anotaciones::lista_annots(doc, page_index)
                .ok_or("Esa página no tiene comentarios")?;
            let objeto = annots
                .get(annot_index as usize)
                .ok_or("Ese adjunto ya no está en la página")?;
            let annot = dict_de(doc, objeto).ok_or("Ese comentario ya no está en la página")?;
            if annot.get(b"Subtype").and_then(|o| o.as_name()).unwrap_or_default()
                != b"FileAttachment"
            {
                return Err("Ese comentario no lleva ningún fichero adjunto".into());
            }
            let spec = annot
                .get(b"FS")
                .ok()
                .and_then(|o| dict_de(doc, o))
                .ok_or("Ese adjunto no lleva fichero dentro")?;
            let stream_id = stream_de(doc, &spec).ok_or("Ese adjunto no lleva fichero dentro")?;
            let stream = doc
                .get_object(stream_id)
                .and_then(|o| o.as_stream())
                .map_err(|e| e.to_string())?;
            Ok((
                nombre_visible(doc, &spec, "adjunto"),
                stream
                    .decompressed_content()
                    .unwrap_or_else(|_| stream.content.clone()),
            ))
        })
    })
}

/// Deja en el temporal el fichero de una chincheta y devuelve su ruta, para
/// que la UI lo abra con el visor del sistema: es lo que hace el doble clic
/// en Acrobat. Mismo saneado de nombre y misma carpeta acotada que
/// [`open_attachment`], que es lo que el permiso del opener deja abrir.
#[tauri::command(async)]
pub fn open_page_attachment(
    path: String,
    page_index: u16,
    annot_index: u16,
) -> Result<String, String> {
    let (nombre, bytes) = fichero_de_la_chincheta(&path, page_index, annot_index)?;
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let dir = std::env::temp_dir().join(format!("vitela-adjunto-{nanos}"));
    std::fs::create_dir_all(&dir).map_err(|e| {
        crate::mensaje_llano(format!("No se ha podido preparar el adjunto: {e}"))
    })?;
    let destino = dir.join(nombre_seguro(&nombre));
    std::fs::write(&destino, bytes).map_err(|e| {
        crate::mensaje_llano(format!("No se ha podido preparar el adjunto: {e}"))
    })?;
    Ok(destino.to_string_lossy().into_owned())
}

/// Escribe donde diga la UI el fichero de una chincheta («Guardar
/// adjunto como…» del menú contextual de Acrobat) y devuelve cuántos
/// bytes ha dejado, como [`save_attachment`].
#[tauri::command(async)]
pub fn save_page_attachment(
    path: String,
    page_index: u16,
    annot_index: u16,
    dest_path: String,
) -> Result<u64, String> {
    let (_, bytes) = fichero_de_la_chincheta(&path, page_index, annot_index)?;
    let n = bytes.len() as u64;
    std::fs::write(&dest_path, bytes).map_err(|e| {
        crate::mensaje_llano(format!("No se ha podido escribir {dest_path}: {e}"))
    })?;
    Ok(n)
}

#[cfg(test)]
mod tests_chincheta {
    use super::*;
    use crate::tests::crea_pdf;

    /// **Adjuntar un fichero como comentario.** En Acrobat es «Comentar ▸
    /// Adjuntar archivo»: una chincheta en la página, con el fichero
    /// dentro, que sale en el panel de comentarios como uno más. No es el
    /// adjunto del documento, que no está en ninguna página.
    ///
    /// Y lo que hay que probar de verdad: **borrar la chincheta se lleva
    /// los bytes**. Quitar la anotación y dejar el fichero incrustado
    /// dentro del PDF haría que el documento siguiera pesando lo mismo
    /// después de borrar el adjunto, que es exactamente el defecto que
    /// `sanitize_pdf` aprendió a no tener.
    #[test]
    fn la_chincheta_lleva_el_fichero_dentro_y_borrarla_se_lo_lleva() {
        let pdf = std::env::temp_dir().join("adjuntos-chincheta.pdf");
        crea_pdf(&["Contrato"], &pdf);
        let work = pdf.to_string_lossy().into_owned();
        let factura = std::env::temp_dir().join("adjuntos-chincheta-factura.xml");
        // grande y **sin compresión posible**, para que su peso se note en
        // el fichero: un texto repetido cabría en unos cientos de bytes
        let mut semilla: u32 = 12345;
        let ruido: Vec<u8> = (0..40_000)
            .map(|_| {
                semilla = semilla.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                (semilla >> 24) as u8
            })
            .collect();
        std::fs::write(&factura, &ruido).expect("escribir el adjunto");

        add_file_attachment_annotation(
            work.clone(),
            0,
            [120.0, 200.0],
            factura.to_string_lossy().into_owned(),
            Some("Jorge".into()),
        )
        .expect("adjuntar");
        let con_adjunto = std::fs::metadata(&work).expect("peso").len();

        // sale como comentario, con el nombre del fichero y su autor
        let anots = crate::anotaciones::get_document_annotations(work.clone()).expect("listar");
        assert_eq!(anots.len(), 1, "la chincheta es un comentario: {anots:?}");
        assert_eq!(anots[0].annot.kind, "FileAttachment");
        assert_eq!(anots[0].annot.contents, "adjuntos-chincheta-factura.xml");
        assert_eq!(anots[0].annot.author, "Jorge");
        assert!(!anots[0].annot.modified.is_empty());
        // y no es un adjunto del documento: el panel de adjuntos no cambia
        assert!(
            list_attachments(work.clone()).expect("adjuntos").is_empty(),
            "el adjunto del documento y el comentario son cosas distintas"
        );
        // se ve en el render: PDFium no le escribe la apariencia y por eso
        // se dibuja a mano
        crate::render_page_png(work.clone(), 0, 200, true).expect("render con la chincheta");

        // borrarla se lleva la anotación **y los bytes**
        crate::anotaciones::remove_annotation(work.clone(), 0, anots[0].annot.index)
            .expect("borrar");
        assert!(crate::anotaciones::get_document_annotations(work.clone())
            .expect("listar")
            .is_empty());
        let sin_adjunto = std::fs::metadata(&work).expect("peso").len();
        assert!(
            sin_adjunto + 30_000 < con_adjunto,
            "los bytes del fichero tienen que irse con la chincheta: {con_adjunto} → {sin_adjunto}"
        );

        // un fichero que no está no es una anotación a medias
        assert!(add_file_attachment_annotation(
            work.clone(),
            0,
            [120.0, 200.0],
            "/no/existe/factura.xml".into(),
            None,
        )
        .unwrap_err()
        .contains("factura.xml"));
        std::fs::remove_file(&factura).ok();
        std::fs::remove_file(&pdf).ok();
    }

    /// **La vuelta de la chincheta.** Hasta el ciclo 9 se sabía meter un
    /// fichero en una página y no se sabía sacarlo: el usuario creía
    /// haberlo guardado dentro —lo había hecho— y no podía recuperarlo sin
    /// abrir el PDF en Acrobat. Es el callejón sin salida más literal que
    /// ha tenido la aplicación.
    #[test]
    fn el_fichero_de_una_chincheta_se_abre_y_se_guarda() {
        let pdf = std::env::temp_dir().join("adjuntos-chincheta-sacar.pdf");
        crea_pdf(&["Contrato"], &pdf);
        let work = pdf.to_string_lossy().into_owned();
        let origen = std::env::temp_dir().join("adjuntos-chincheta-sacar-factura.xml");
        let contenido = b"<factura><total>1234,56</total></factura>".to_vec();
        std::fs::write(&origen, &contenido).expect("escribir el adjunto");

        add_file_attachment_annotation(
            work.clone(),
            0,
            [120.0, 200.0],
            origen.to_string_lossy().into_owned(),
            None,
        )
        .expect("adjuntar");

        // abrir deja el fichero en el temporal, con su nombre y su
        // extensión de verdad (es lo que mira el sistema para elegir con
        // qué programa abrirlo) y con los bytes del original
        let ruta = open_page_attachment(work.clone(), 0, 0).expect("abrir");
        let abierto = std::path::Path::new(&ruta);
        assert!(abierto.exists(), "la ruta que se devuelve no existe: {ruta}");
        assert_eq!(
            abierto.file_name().and_then(|n| n.to_str()),
            Some("adjuntos-chincheta-sacar-factura.xml")
        );
        assert_eq!(std::fs::read(abierto).expect("leer"), contenido);

        // y guardar lo escribe donde diga la UI, diciendo cuántos bytes
        let destino = std::env::temp_dir().join("adjuntos-chincheta-guardada.xml");
        let n = save_page_attachment(
            work.clone(),
            0,
            0,
            destino.to_string_lossy().into_owned(),
        )
        .expect("guardar");
        assert_eq!(n as usize, contenido.len());
        assert_eq!(std::fs::read(&destino).expect("leer"), contenido);

        // un comentario que no lleva fichero lo dice en llano, no falla
        // con jerga ni devuelve una ruta vacía
        crate::anotaciones::add_note(
            work.clone(),
            0,
            300.0,
            300.0,
            "Una nota".into(),
            None,
        )
        .expect("nota");
        let e = open_page_attachment(work.clone(), 0, 1).unwrap_err();
        assert!(e.contains("no lleva ningún fichero"), "aviso en llano: {e}");
        let e = save_page_attachment(work.clone(), 0, 9, destino.to_string_lossy().into_owned())
            .unwrap_err();
        assert!(e.contains("ya no está en la página"), "aviso en llano: {e}");

        if let Some(dir) = abierto.parent() {
            std::fs::remove_dir_all(dir).ok();
        }
        std::fs::remove_file(&destino).ok();
        std::fs::remove_file(&origen).ok();
        std::fs::remove_file(&pdf).ok();
    }
}
