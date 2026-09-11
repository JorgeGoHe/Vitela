//! Gestión de páginas: borrar, rotar, mover, unir y extraer (FPDF_ImportPages).

use crate::historial::mutacion;
use crate::{on_pdfium_thread, pdfium, save_and_close};
use pdfium_render::prelude::*;

/// Borra una página y devuelve el nuevo número de páginas.
#[tauri::command(async)]
pub fn delete_page(work_path: String, page_index: u16) -> Result<u16, String> {
    mutacion(work_path, |work_path| {
        on_pdfium_thread(move || {
            let pdfium = pdfium()?;
            let doc = pdfium
                .load_pdf_from_file(&work_path, None)
                .map_err(|e| e.to_string())?;
            let page = doc.pages().get(page_index).map_err(|e| e.to_string())?;
            page.delete().map_err(|e| e.to_string())?;
            let count = doc.pages().len();
            save_and_close(doc, &work_path)?;
            Ok(count)
        })
    })
}

/// Rota una página 90° en sentido horario (acumulativo).
#[tauri::command(async)]
pub fn rotate_page(work_path: String, page_index: u16) -> Result<(), String> {
    mutacion(work_path, |work_path| {
        on_pdfium_thread(move || {
            let pdfium = pdfium()?;
            let doc = pdfium
                .load_pdf_from_file(&work_path, None)
                .map_err(|e| e.to_string())?;
            let mut page = doc.pages().get(page_index).map_err(|e| e.to_string())?;
            let next = match page.rotation().unwrap_or(PdfPageRenderRotation::None) {
                PdfPageRenderRotation::None => PdfPageRenderRotation::Degrees90,
                PdfPageRenderRotation::Degrees90 => PdfPageRenderRotation::Degrees180,
                PdfPageRenderRotation::Degrees180 => PdfPageRenderRotation::Degrees270,
                PdfPageRenderRotation::Degrees270 => PdfPageRenderRotation::None,
            };
            page.set_rotation(next);
            drop(page);
            save_and_close(doc, &work_path)?;
            Ok(())
        })
    })
}

/// Borra varias páginas de una vez. De mayor a menor, para que borrar una no
/// invalide los índices que quedan. Devuelve el nuevo número de páginas.
///
/// Debe llamarse desde el hilo de PDFium y dentro de una `mutacion`: un
/// borrado en lote es UN paso de deshacer, no uno por página.
/// **AC-049.** Todo lo que puede salir mal en una extracción, comprobado
/// **antes** de escribir el primer fichero: que las páginas siguen ahí, que
/// llevárselas del original no lo deja sin ninguna y que en la carpeta de
/// destino se puede escribir de verdad.
///
/// Antes se escribían los ficheros y solo después saltaba el error, así que
/// «extraer todas y eliminarlas» dejaba al usuario con unos PDF que no
/// había llegado a pedir del todo y el documento intacto. Acrobat lo dice
/// antes de tocar el disco.
fn revisa_extraccion(
    total: u16,
    page_indices: &[u16],
    carpeta: &std::path::Path,
    borrar: bool,
) -> Result<(), String> {
    let mut indices: Vec<u16> = page_indices.to_vec();
    indices.sort_unstable();
    indices.dedup();
    if let Some(fuera) = indices.iter().find(|i| **i >= total) {
        return Err(format!(
            "La página {} ya no está en el documento",
            fuera + 1
        ));
    }
    if borrar && indices.len() as u16 >= total {
        return Err(
            "No se pueden extraer todas las páginas y borrarlas del original: un documento \
             no puede quedarse sin páginas. Quita la marca de «Eliminar las páginas del \
             original» o deja alguna fuera"
                .into(),
        );
    }
    carpeta_escribible(carpeta)
}

/// ¿Se puede escribir en esa carpeta? Se comprueba escribiendo, que es la
/// única manera que no miente (permisos, disco lleno, volumen de solo
/// lectura, carpeta que ya no está).
fn carpeta_escribible(dir: &std::path::Path) -> Result<(), String> {
    if !dir.is_dir() {
        return Err(format!(
            "La carpeta {} ya no está: elige otra",
            dir.display()
        ));
    }
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let prueba = dir.join(format!(".vitela-prueba-{nanos}"));
    std::fs::write(&prueba, b"").map_err(|_| {
        format!(
            "No se puede escribir en {}: elige otra carpeta",
            dir.display()
        )
    })?;
    let _ = std::fs::remove_file(&prueba);
    Ok(())
}

fn borra_paginas(work_path: &str, page_indices: &[u16]) -> Result<u16, String> {
    let pdfium = pdfium()?;
    let doc = pdfium
        .load_pdf_from_file(work_path, None)
        .map_err(|e| e.to_string())?;
    let total = doc.pages().len();
    let mut indices: Vec<u16> = page_indices.to_vec();
    indices.sort_unstable();
    indices.dedup();
    if let Some(fuera) = indices.iter().find(|i| **i >= total) {
        return Err(format!(
            "La página {} ya no está en el documento",
            fuera + 1
        ));
    }
    if indices.len() as u16 == total {
        return Err("Un documento no puede quedarse sin páginas".into());
    }
    for i in indices.iter().rev() {
        doc.pages()
            .get(*i)
            .map_err(|e| e.to_string())?
            .delete()
            .map_err(|e| e.to_string())?;
    }
    let count = doc.pages().len();
    save_and_close(doc, work_path)?;
    Ok(count)
}

/// Borra varias páginas en una sola mutación (un solo ⌘Z las devuelve) y
/// devuelve el nuevo número de páginas.
#[tauri::command(async)]
pub fn delete_pages(work_path: String, page_indices: Vec<u16>) -> Result<u16, String> {
    if page_indices.is_empty() {
        return Err("No hay páginas que eliminar".into());
    }
    mutacion(work_path, move |work_path| {
        on_pdfium_thread(move || borra_paginas(&work_path, &page_indices))
    })
}

/// Gira varias páginas en una sola mutación. `quarter_turns` es el número de
/// cuartos de vuelta con signo (±1, ±2, ±3): en negativo gira en sentido
/// antihorario, que es la vuelta atrás que Acrobat tiene y Vitela no tenía.
#[tauri::command(async)]
pub fn rotate_pages(
    work_path: String,
    page_indices: Vec<u16>,
    quarter_turns: i8,
) -> Result<(), String> {
    if page_indices.is_empty() {
        return Err("No hay páginas que girar".into());
    }
    let cuartos = (quarter_turns as i32).rem_euclid(4) as u8;
    if cuartos == 0 {
        return Ok(()); // vuelta entera: nada que hacer, ni paso de historial
    }
    mutacion(work_path, move |work_path| {
        on_pdfium_thread(move || {
            let pdfium = pdfium()?;
            let doc = pdfium
                .load_pdf_from_file(&work_path, None)
                .map_err(|e| e.to_string())?;
            let total = doc.pages().len();
            for i in &page_indices {
                if *i >= total {
                    return Err(format!("La página {} ya no está en el documento", i + 1));
                }
                let mut page = doc.pages().get(*i).map_err(|e| e.to_string())?;
                let actual = match page.rotation().unwrap_or(PdfPageRenderRotation::None) {
                    PdfPageRenderRotation::None => 0u8,
                    PdfPageRenderRotation::Degrees90 => 1,
                    PdfPageRenderRotation::Degrees180 => 2,
                    PdfPageRenderRotation::Degrees270 => 3,
                };
                page.set_rotation(match (actual + cuartos) % 4 {
                    1 => PdfPageRenderRotation::Degrees90,
                    2 => PdfPageRenderRotation::Degrees180,
                    3 => PdfPageRenderRotation::Degrees270,
                    _ => PdfPageRenderRotation::None,
                });
            }
            save_and_close(doc, &work_path)?;
            Ok(())
        })
    })
}

/// Mueve una página a otra posición **reordenando el árbol de páginas en
/// el sitio** (AC-093).
///
/// Antes reconstruía el documento: creaba uno vacío con `create_new_pdf()`
/// y le copiaba las páginas. Eso copia las páginas y **deja atrás el
/// catálogo entero** —marcadores, `/PageLabels`, `/Names →
/// /EmbeddedFiles`, `/OpenAction`, `/PageLayout`, `/PageMode`, `/AcroForm`
/// y `/OCProperties`—, así que subir una página en el panel se llevaba por
/// delante la numeración, los adjuntos, la vista inicial y el formulario,
/// sin decir nada. Era la única de las nueve operaciones de páginas que
/// reconstruía desde cero.
///
/// Reordenar es reescribir el `/Kids`, que es lo único que dice en qué
/// orden van las páginas: no se toca ni una anotación, no hay que esquivar
/// el ciclo `/Popup` ↔ `/Parent` de AC-046 y es más rápido.
#[tauri::command(async)]
pub fn move_page(work_path: String, from_index: u16, to_index: u16) -> Result<(), String> {
    if from_index == to_index {
        return Ok(());
    }
    crate::cirugia(&work_path.clone(), move |doc| {
        let paginas: Vec<lopdf::ObjectId> = doc.get_pages().into_values().collect();
        let count = paginas.len();
        if from_index as usize >= count || to_index as usize >= count {
            return Err("Índice de página fuera de rango".into());
        }
        let mut orden = paginas;
        let movida = orden.remove(from_index as usize);
        orden.insert(to_index as usize, movida);
        reordena_paginas(doc, &orden)
    })
}

/// Las claves que una página **hereda** de los nodos de arriba del árbol.
/// Antes de aplanar hay que bajarlas a cada página, o una perdería su
/// tamaño o su giro por el camino.
const HEREDADOS: [&[u8]; 4] = [b"Resources", b"MediaBox", b"CropBox", b"Rotate"];

/// Deja el árbol de páginas con las páginas en el orden dado, colgando
/// todas de la raíz. Lo demás del documento no se toca.
pub(crate) fn reordena_paginas(
    doc: &mut lopdf::Document,
    orden: &[lopdf::ObjectId],
) -> Result<(), String> {
    use lopdf::Object;
    let raiz = doc
        .catalog()
        .and_then(|c| c.get(b"Pages"))
        .and_then(|o| o.as_reference())
        .map_err(|_| "Este documento no tiene árbol de páginas".to_string())?;
    for id in orden {
        for clave in HEREDADOS {
            let tiene = doc
                .get_object(*id)
                .and_then(|o| o.as_dict())
                .map(|d| d.get(clave).is_ok())
                .unwrap_or(false);
            if tiene {
                continue;
            }
            let Some(valor) = hereda_de_arriba(doc, *id, clave) else {
                continue;
            };
            if let Ok(d) = doc.get_object_mut(*id).and_then(|o| o.as_dict_mut()) {
                d.set(String::from_utf8_lossy(clave).into_owned(), valor);
            }
        }
    }
    let intermedios = nodos_intermedios(doc, raiz);
    for id in orden {
        if let Ok(d) = doc.get_object_mut(*id).and_then(|o| o.as_dict_mut()) {
            d.set("Parent", Object::Reference(raiz));
        }
    }
    let kids: Vec<Object> = orden.iter().map(|id| Object::Reference(*id)).collect();
    let cuantas = kids.len() as i64;
    let d = doc
        .get_object_mut(raiz)
        .and_then(|o| o.as_dict_mut())
        .map_err(|_| "Este documento no tiene árbol de páginas".to_string())?;
    d.set("Kids", Object::Array(kids));
    d.set("Count", cuantas);
    // los nodos intermedios se quedan sin nadie que los mire
    for id in intermedios {
        doc.objects.remove(&id);
    }
    Ok(())
}

/// El primer valor de `clave` que hay subiendo por los `/Parent`, sin
/// contar la propia página.
fn hereda_de_arriba(
    doc: &lopdf::Document,
    page_id: lopdf::ObjectId,
    clave: &[u8],
) -> Option<lopdf::Object> {
    let mut actual = page_id;
    for _ in 0..32 {
        let padre = doc
            .get_object(actual)
            .and_then(|o| o.as_dict())
            .and_then(|d| d.get(b"Parent"))
            .and_then(|o| o.as_reference())
            .ok()?;
        if let Ok(v) = doc
            .get_object(padre)
            .and_then(|o| o.as_dict())
            .and_then(|d| d.get(clave))
        {
            return Some(v.clone());
        }
        actual = padre;
    }
    None
}

/// Los nodos `/Pages` que cuelgan de la raíz, que es lo que queda huérfano
/// al aplanar el árbol. Con corte de ciclos, como todos los recorridos.
fn nodos_intermedios(doc: &lopdf::Document, raiz: lopdf::ObjectId) -> Vec<lopdf::ObjectId> {
    let mut out = Vec::new();
    let mut pila = vec![raiz];
    let mut vistos = vec![raiz];
    while let Some(id) = pila.pop() {
        let Ok(Ok(kids)) = doc
            .get_object(id)
            .and_then(|o| o.as_dict())
            .map(|d| d.get(b"Kids").and_then(|o| o.as_array()))
        else {
            continue;
        };
        for kid in kids {
            let Ok(hijo) = kid.as_reference() else {
                continue;
            };
            if vistos.contains(&hijo) {
                continue;
            }
            vistos.push(hijo);
            let es_nodo = doc
                .get_object(hijo)
                .and_then(|o| o.as_dict())
                .map(|d| matches!(d.get(b"Type").and_then(|o| o.as_name()), Ok(b"Pages")))
                .unwrap_or(false);
            if es_nodo {
                out.push(hijo);
                pila.push(hijo);
            }
        }
    }
    out
}

/// Añade todas las páginas de otro PDF al final y devuelve el nuevo total.
#[tauri::command(async)]
pub fn merge_pdf(work_path: String, other_path: String) -> Result<u16, String> {
    mutacion(work_path, |work_path| {
        on_pdfium_thread(move || {
            let pdfium = pdfium()?;
            let mut doc = pdfium
                .load_pdf_from_file(&work_path, None)
                .map_err(|e| e.to_string())?;
            // AC-046: importar de una copia sin las ventanas de las notas
            // (el par /Popup ↔ /Parent es un ciclo y mata a FPDF_ImportPages)
            let fuente = crate::anotaciones::fuente_importable(&other_path);
            let other = pdfium
                .load_pdf_from_file(fuente.ruta(), None)
                .map_err(|e| e.to_string())?;
            doc.pages_mut().append(&other).map_err(|e| e.to_string())?;
            let count = doc.pages().len();
            drop(other);
            save_and_close(doc, &work_path)?;
            crate::anotaciones::remata_importacion(&work_path)?;
            Ok(count)
        })
    })
}

/// Extrae las páginas indicadas (índices base 0) a un PDF nuevo.
#[tauri::command(async)]
pub fn extract_pages(
    work_path: String,
    page_indices: Vec<u16>,
    dest_path: String,
    delete_after: Option<bool>,
) -> Result<(), String> {
    if page_indices.is_empty() {
        return Err("No hay páginas que extraer".into());
    }
    let borrar = delete_after.unwrap_or(false);
    let range = page_indices
        .iter()
        .map(|i| (i + 1).to_string())
        .collect::<Vec<_>>()
        .join(",");
    let page_indices_revisar = page_indices.clone();
    // extraer y borrar es UNA operación: un solo paso de deshacer, y si el
    // borrado falla el documento se queda como estaba
    let cuerpo = move |work_path: String| {
        on_pdfium_thread(move || {
            {
                // AC-049: todo lo que puede fallar, antes de escribir nada
                let total = crate::with_doc(&work_path, |doc| Ok(doc.pages().len()))?;
                let carpeta = std::path::Path::new(&dest_path)
                    .parent()
                    .unwrap_or(std::path::Path::new("."))
                    .to_path_buf();
                revisa_extraccion(total, &page_indices_revisar, &carpeta, borrar)?;
                // AC-046: importar de una copia sin las ventanas de las notas
                // (el par /Popup ↔ /Parent es un ciclo y mata a FPDF_ImportPages)
                let fuente = crate::anotaciones::fuente_importable(&work_path);
                let doc = pdfium()?
                    .load_pdf_from_file(fuente.ruta(), None)
                    .map_err(crate::mensaje_llano)?;
                let mut new_doc = pdfium()?.create_new_pdf().map_err(|e| e.to_string())?;
                new_doc
                    .pages_mut()
                    .copy_pages_from_document(&doc, &range, 0)
                    .map_err(|e| e.to_string())?;
                new_doc.save_to_file(&dest_path).map_err(|e| {
                    crate::mensaje_llano(format!("No se ha podido escribir {dest_path}: {e}"))
                })?;
            }
            crate::anotaciones::remata_importacion(&dest_path)?;
            if borrar {
                borra_paginas(&work_path, &page_indices)?;
            }
            Ok(())
        })
    };
    if borrar {
        mutacion(work_path, cuerpo)
    } else {
        cuerpo(work_path)
    }
}

/// «Un fichero por página»: extrae cada página pedida a su propio PDF en
/// `dest_dir` (`pagina-N.pdf`, con N el número de la página en el original)
/// y, si se pide, las borra del documento. Devuelve las rutas escritas.
///
/// Es UNA operación: la UI hacía una llamada por página más un borrado
/// aparte, así que un fallo a mitad dejaba ficheros escritos, el documento
/// intacto y nadie avisado. Aquí se escriben todos primero —si uno falla no
/// se ha tocado el documento— y el borrado va dentro de la misma mutación:
/// un solo paso de deshacer.
#[tauri::command(async)]
pub fn extract_each_page(
    work_path: String,
    page_indices: Vec<u16>,
    dest_dir: String,
    delete_after: Option<bool>,
) -> Result<Vec<String>, String> {
    if page_indices.is_empty() {
        return Err("No hay páginas que extraer".into());
    }
    let borrar = delete_after.unwrap_or(false);
    let cuerpo = move |work_path: String| {
        let indices = page_indices.clone();
        let dir = dest_dir.clone();
        let work = work_path.clone();
        let escritos: Vec<String> = on_pdfium_thread(move || {
            // AC-049: todo lo que puede fallar, antes de escribir nada
            let total = crate::with_doc(&work, |doc| Ok(doc.pages().len()))?;
            revisa_extraccion(total, &indices, std::path::Path::new(&dir), borrar)?;
            // AC-046: importar de una copia sin las ventanas de las notas
            // (el par /Popup ↔ /Parent es un ciclo y mata a FPDF_ImportPages)
            let fuente = crate::anotaciones::fuente_importable(&work);
            let doc = pdfium()?
                .load_pdf_from_file(fuente.ruta(), None)
                .map_err(crate::mensaje_llano)?;
            let mut escritos = Vec::new();
            for i in &indices {
                let destino = std::path::Path::new(&dir).join(format!("pagina-{}.pdf", i + 1));
                let mut nuevo = pdfium()?.create_new_pdf().map_err(|e| e.to_string())?;
                nuevo
                    .pages_mut()
                    .copy_pages_from_document(&doc, &(i + 1).to_string(), 0)
                    .map_err(|e| e.to_string())?;
                nuevo.save_to_file(&destino).map_err(|e| {
                    crate::mensaje_llano(format!(
                        "No se ha podido escribir {}: {e}",
                        destino.display()
                    ))
                })?;
                let escrito = destino.to_string_lossy().into_owned();
                crate::anotaciones::remata_importacion(&escrito)?;
                escritos.push(escrito);
            }
            Ok::<Vec<String>, String>(escritos)
        })?;
        if borrar {
            let indices = page_indices.clone();
            on_pdfium_thread(move || borra_paginas(&work_path, &indices))?;
        }
        Ok(escritos)
    };
    if borrar {
        mutacion(work_path, cuerpo)
    } else {
        cuerpo(work_path)
    }
}

#[cfg(test)]
mod tests {

    /// Lo que cuelga del catálogo y que ninguna operación de páginas puede
    /// llevarse por delante: marcador, numeración, adjunto, vista inicial y
    /// formulario. Devuelve cuántos de los cinco siguen ahí.
    fn supervivientes(work: &str) -> Vec<&'static str> {
        let mut vivos = Vec::new();
        if !crate::documento::get_outline(work.to_string())
            .expect("marcadores")
            .is_empty()
        {
            vivos.push("marcador");
        }
        if !crate::documento::get_page_labels(work.to_string())
            .expect("etiquetas")
            .rangos
            .is_empty()
        {
            vivos.push("numeracion");
        }
        if !crate::adjuntos::list_attachments(work.to_string())
            .expect("adjuntos")
            .is_empty()
        {
            vivos.push("adjunto");
        }
        if crate::documento::get_open_action(work.to_string())
            .expect("vista")
            .page_index
            .is_some()
        {
            vivos.push("vista");
        }
        let doc = lopdf::Document::load(work).expect("cargar");
        if doc
            .catalog()
            .ok()
            .and_then(|c| c.get(b"AcroForm").ok())
            .is_some()
        {
            vivos.push("formulario");
        }
        vivos
    }

    /// Un documento de ocho páginas con las cinco cosas colgando del
    /// catálogo, listo para pasarle por encima una operación de páginas.
    fn documento_con_de_todo(nombre: &str) -> String {
        let dir = std::env::temp_dir();
        let pdf = dir.join(nombre);
        let textos: Vec<String> = (1..=8).map(|i| format!("Página {i}")).collect();
        let refs: Vec<&str> = textos.iter().map(|s| s.as_str()).collect();
        crate::tests::crea_pdf(&refs, &pdf);
        let work = pdf.to_string_lossy().into_owned();
        crate::documento::set_outline(
            work.clone(),
            vec![crate::documento::OutlineNode {
                title: "Capítulo 1".into(),
                page_index: Some(0),
                top: None,
                zoom: None,
                children: Vec::new(),
            }],
        )
        .expect("marcador");
        crate::documento::set_page_labels(
            work.clone(),
            vec![crate::documento::RangoEtiqueta {
                desde: 0,
                estilo: "romano_min".into(),
                prefijo: String::new(),
                empieza_en: 1,
            }],
        )
        .expect("numeración");
        let nota = dir.join(format!("{nombre}-nota.txt"));
        std::fs::write(&nota, b"una nota").expect("nota");
        crate::adjuntos::add_attachment(work.clone(), nota.to_string_lossy().into_owned(), None)
            .expect("adjunto");
        crate::documento::set_open_action(
            work.clone(),
            crate::documento::VistaInicial {
                page_index: Some(1),
                top: None,
                zoom: None,
                ajuste: "pagina".into(),
                disposicion: "continuo".into(),
                panel: String::new(),
                marcadores: None,
            },
        )
        .expect("vista inicial");
        crate::formularios2::create_form_field(
            work.clone(),
            0,
            "text".into(),
            crate::Rect {
                x: 60.0,
                y: 300.0,
                w: 180.0,
                h: 22.0,
            },
            "nombre".into(),
            None,
            None,
            None,
            None,
        )
        .expect("campo");
        assert_eq!(
            supervivientes(&work).len(),
            5,
            "el documento de partida tiene que llevar las cinco cosas"
        );
        work
    }

    /// **AC-093, crítico.** Subir una página en el panel destruía los
    /// marcadores, la numeración, los adjuntos, la vista inicial y el
    /// formulario: `move_page` reconstruía el documento con
    /// `create_new_pdf()` y el catálogo entero se quedaba en el viejo. ⌘Z
    /// lo devolvía —es una instantánea—, pero quien reordenaba y guardaba
    /// lo perdía para siempre y sin aviso.
    ///
    /// El test recorre **las nueve operaciones de páginas**, no solo la que
    /// falló: son las nueve las que tienen que dejar el documento entero.
    #[test]
    fn ninguna_operacion_de_paginas_se_lleva_lo_que_cuelga_del_catalogo() {
        let dir = std::env::temp_dir();
        let otro = dir.join("paginas-catalogo-otro.pdf");
        crate::tests::crea_pdf(&["Uno", "Dos"], &otro);
        let o = otro.to_string_lossy().into_owned();

        type Operacion = (&'static str, Box<dyn Fn(&str)>);
        let ops: Vec<Operacion> = vec![
            (
                "move_page",
                Box::new(|w: &str| super::move_page(w.to_string(), 0, 3).expect("mover")),
            ),
            (
                "delete_page",
                Box::new(|w: &str| {
                    super::delete_page(w.to_string(), 2).expect("borrar");
                }),
            ),
            (
                "rotate_page",
                Box::new(|w: &str| super::rotate_page(w.to_string(), 1).expect("girar")),
            ),
            (
                "duplicate_page",
                Box::new(|w: &str| {
                    crate::paginas2::duplicate_page(w.to_string(), 1).expect("duplicar");
                }),
            ),
            (
                "add_blank_page",
                Box::new(|w: &str| {
                    crate::paginas2::add_blank_page(w.to_string(), 1).expect("en blanco");
                }),
            ),
            (
                "merge_pdf",
                Box::new(move |w: &str| {
                    super::merge_pdf(w.to_string(), o.clone()).expect("unir");
                }),
            ),
            (
                "delete_pages",
                Box::new(|w: &str| {
                    super::delete_pages(w.to_string(), vec![5, 6]).expect("borrar lote");
                }),
            ),
            (
                "rotate_pages",
                Box::new(|w: &str| {
                    super::rotate_pages(w.to_string(), vec![0, 1], 1).expect("girar lote");
                }),
            ),
            (
                "extract_pages",
                Box::new(|w: &str| {
                    let fuera = std::env::temp_dir().join("paginas-catalogo-extraidas.pdf");
                    super::extract_pages(
                        w.to_string(),
                        vec![6, 7],
                        fuera.to_string_lossy().into_owned(),
                        Some(true),
                    )
                    .expect("extraer");
                    std::fs::remove_file(&fuera).ok();
                }),
            ),
        ];

        for (nombre, op) in ops {
            let work = documento_con_de_todo(&format!("paginas-catalogo-{nombre}.pdf"));
            op(&work);
            let vivos = supervivientes(&work);
            assert_eq!(
                vivos.len(),
                5,
                "«{nombre}» se ha llevado por delante lo que cuelga del catálogo; \
                 sobreviven {vivos:?}"
            );
            std::fs::remove_file(&work).ok();
        }
        std::fs::remove_file(&otro).ok();
    }

    /// Y reordenar reordena de verdad: la página que se sube queda donde se
    /// suelta y las demás se corren, con su contenido.
    #[test]
    fn mover_una_pagina_la_deja_donde_se_suelta() {
        let dir = std::env::temp_dir();
        let pdf = dir.join("paginas-mover-orden.pdf");
        crate::tests::crea_pdf(&["Uno", "Dos", "Tres", "Cuatro"], &pdf);
        let work = pdf.to_string_lossy().into_owned();
        let texto = |i: u16| {
            crate::busqueda::get_page_text(work.clone(), i)
                .expect("texto")
                .chars
                .iter()
                .map(|c| c.ch.as_str())
                .collect::<String>()
        };
        super::move_page(work.clone(), 0, 2).expect("mover");
        assert_eq!(texto(0), "Dos");
        assert_eq!(texto(1), "Tres");
        assert_eq!(texto(2), "Uno");
        assert_eq!(texto(3), "Cuatro");
        // y al revés
        super::move_page(work.clone(), 2, 0).expect("devolver");
        assert_eq!(texto(0), "Uno");
        assert_eq!(texto(3), "Cuatro");
        std::fs::remove_file(&pdf).ok();
    }

    /// **AC-049.** «Extraer y eliminar del original» con **todas** las
    /// páginas escribía los ficheros y solo después daba el error, así que
    /// el usuario se quedaba con unos PDF que no había llegado a pedir del
    /// todo y el documento intacto. Ahora se comprueba antes de tocar el
    /// disco, como en Acrobat: la carpeta queda vacía.
    #[test]
    fn extraer_y_eliminar_todas_falla_antes_de_escribir_nada() {
        let dir = std::env::temp_dir();
        let pdf = dir.join("ac049-extraer.pdf");
        crea_pdf(&["Una", "Dos", "Tres"], &pdf);
        let work = pdf.to_string_lossy().into_owned();
        let carpeta = dir.join("ac049-salida");
        std::fs::remove_dir_all(&carpeta).ok();
        std::fs::create_dir_all(&carpeta).expect("carpeta");

        let e = extract_each_page(
            work.clone(),
            vec![0, 1, 2],
            carpeta.to_string_lossy().into_owned(),
            Some(true),
        )
        .unwrap_err();
        assert!(
            e.contains("no puede quedarse sin páginas"),
            "el aviso tiene que explicar qué pasa y cómo salir: {e}"
        );
        assert!(
            e.contains("Eliminar las páginas del original"),
            "y nombrar la casilla que hay que quitar: {e}"
        );
        assert_eq!(
            std::fs::read_dir(&carpeta).expect("leer carpeta").count(),
            0,
            "no se puede haber escrito ni un fichero"
        );

        // el mismo caso con «extraer a un solo PDF»
        let destino = carpeta.join("todas.pdf");
        let e = extract_pages(
            work.clone(),
            vec![0, 1, 2],
            destino.to_string_lossy().into_owned(),
            Some(true),
        )
        .unwrap_err();
        assert!(e.contains("no puede quedarse sin páginas"), "{e}");
        assert!(!destino.exists(), "tampoco se ha escrito el PDF");

        // una carpeta que ya no está se dice antes, no a mitad
        let e = extract_each_page(
            work.clone(),
            vec![0],
            dir.join("ac049-no-existe").to_string_lossy().into_owned(),
            None,
        )
        .unwrap_err();
        assert!(e.contains("ya no está"), "{e}");

        // y sin la casilla, extraerlas todas sigue valiendo
        extract_each_page(
            work.clone(),
            vec![0, 1, 2],
            carpeta.to_string_lossy().into_owned(),
            None,
        )
        .expect("extraer sin borrar");
        assert_eq!(std::fs::read_dir(&carpeta).expect("leer").count(), 3);

        std::fs::remove_dir_all(&carpeta).ok();
        std::fs::remove_file(&pdf).ok();
    }

    /// **AC-046 (crítico).** Importar una página que lleva una nota adhesiva
    /// mataba el proceso entero con `SIGSEGV`: la nota y su ventana `/Popup`
    /// se apuntan la una a la otra —un ciclo legal que escriben Acrobat y
    /// Vitela— y `FPDF_ImportPages` copia el grafo de anotaciones
    /// recursivamente hasta reventar la pila. Se llevaba por delante el
    /// trabajo sin guardar y, en la app empaquetada, cerraba Vitela.
    ///
    /// Este test pasa una nota por **todos** los comandos que importan
    /// páginas. Si alguno vuelve a caer no falla: mata el proceso de los
    /// tests, que es exactamente el aviso que hacía falta y que ningún ciclo
    /// había dado.
    #[test]
    fn importar_una_pagina_con_una_nota_no_mata_el_proceso() {
        let dir = std::env::temp_dir();
        let con_nota = dir.join("ac046-con-nota.pdf");
        crea_pdf(&["Con nota", "Segunda"], &con_nota);
        let cn = con_nota.to_string_lossy().into_owned();
        crate::anotaciones::add_note(
            cn.clone(),
            0,
            100.0,
            200.0,
            "Una nota".into(),
            Some("Ana".into()),
        )
        .expect("nota");

        // un documento aparte al que importarla
        let destino = dir.join("ac046-destino.pdf");
        crea_pdf(&["Destino"], &destino);
        let d = destino.to_string_lossy().into_owned();

        // 1. merge_pdf (Añadir PDF…)
        assert_eq!(merge_pdf(d.clone(), cn.clone()).expect("merge_pdf"), 3);
        // la nota llega y sigue siendo UNA anotación (el popup no cuenta)
        let notas = crate::anotaciones::get_annotations(d.clone(), 1).expect("anotaciones");
        assert_eq!(notas.len(), 1, "la nota importada: {notas:?}");
        assert_eq!(notas[0].contents, "Una nota");
        assert!(
            tiene_popup(&d, 1),
            "la ventana emergente se repone tras importar: sin ella la nota \
             pierde su post-it en Acrobat y en Vista Previa"
        );

        // 2. merge_many (Combinar ficheros…)
        assert_eq!(
            crate::paginas2::merge_many(d.clone(), vec![cn.clone()], None).expect("merge_many"),
            5
        );
        // 3. insert_pdf_at
        assert_eq!(
            crate::paginas2::insert_pdf_at(d.clone(), cn.clone(), 0, None).expect("insert_pdf_at"),
            7
        );
        // 4. duplicate_page sobre la página que lleva la nota
        assert_eq!(
            crate::paginas2::duplicate_page(d.clone(), 0).expect("duplicate_page"),
            8
        );
        // 5. replace_pages
        crate::paginas2::replace_pages(d.clone(), vec![7], cn.clone(), None)
            .expect("replace_pages");
        // 6. move_page
        move_page(d.clone(), 0, 3).expect("move_page");
        // 7. extract_pages a un fichero nuevo
        let extraido = dir.join("ac046-extraido.pdf");
        extract_pages(
            d.clone(),
            vec![3],
            extraido.to_string_lossy().into_owned(),
            None,
        )
        .expect("extract_pages");
        assert!(
            tiene_popup(&extraido.to_string_lossy(), 0),
            "y en el extraído también"
        );
        // 8. extract_each_page y split_pdf, que escriben varios ficheros
        let carpeta = dir.join("ac046-sueltas");
        std::fs::create_dir_all(&carpeta).expect("carpeta");
        extract_each_page(
            d.clone(),
            vec![3],
            carpeta.to_string_lossy().into_owned(),
            None,
        )
        .expect("extract_each_page");
        crate::paginas2::split_pdf(
            d.clone(),
            carpeta.to_string_lossy().into_owned(),
            "cada".into(),
            Some(2),
        )
        .expect("split_pdf");

        std::fs::remove_dir_all(&carpeta).ok();
        for f in [&con_nota, &destino, &extraido] {
            std::fs::remove_file(f).ok();
        }
    }

    /// ¿La primera nota de esa página tiene su ventana emergente?
    fn tiene_popup(path: &str, page_index: u16) -> bool {
        let doc = lopdf::Document::load(path).expect("leer el PDF");
        let Some(annots) = crate::anotaciones::lista_annots(&doc, page_index) else {
            return false;
        };
        annots.iter().any(|a| {
            let lopdf::Object::Reference(rid) = a else {
                return false;
            };
            let Ok(d) = doc.get_object(*rid).and_then(|o| o.as_dict()) else {
                return false;
            };
            d.get(b"Subtype")
                .and_then(|s| s.as_name())
                .map(|n| n == b"Text")
                .unwrap_or(false)
                && d.has(b"Popup")
        })
    }

    use super::*;
    #[allow(unused_imports)]
    use crate::tests::{crea_pdf, textos_de};
    #[allow(unused_imports)]
    use crate::{get_page_sizes, open_pdf, render_page_b64, render_page_png};

    /// Acrobat actúa sobre la selección entera de golpe: borrar tres
    /// páginas es un solo ⌘Z, no tres.
    #[test]
    fn borrar_y_girar_en_lote_son_un_solo_paso() {
        let pdf = std::env::temp_dir().join("editor_pdf_test_lote.pdf");
        crea_pdf(&["Uno", "Dos", "Tres", "Cuatro", "Cinco", "Seis"], &pdf);
        let work = pdf.to_string_lossy().into_owned();
        let pasos = |w: &str| {
            crate::historial::history_state(w.to_string())
                .expect("historial")
                .undo
        };
        let antes = pasos(&work);

        assert_eq!(
            delete_pages(work.clone(), vec![0, 2, 4]).expect("borrar"),
            3
        );
        let t = textos_de(&pdf);
        assert_eq!(t.len(), 3);
        assert!(
            t[0].contains("Dos") && t[1].contains("Cuatro") && t[2].contains("Seis"),
            "quedan: {t:?}"
        );
        assert_eq!(pasos(&work), antes + 1, "un borrado en lote es un paso");
        crate::historial::undo(work.clone()).expect("deshacer");
        assert_eq!(textos_de(&pdf).len(), 6, "un solo ⌘Z devuelve las tres");

        // girar al revés: la vuelta atrás que Acrobat tiene
        rotate_pages(work.clone(), vec![1], -1).expect("girar antihorario");
        let sizes = get_page_sizes(work.clone()).expect("tamaños");
        assert_eq!(sizes[1].rotation, 270);
        assert_eq!(sizes[0].rotation, 0, "solo gira lo seleccionado");
        // dos cuartos más: 270 + 180 = 90
        rotate_pages(work.clone(), vec![1], 2).expect("girar media vuelta");
        assert_eq!(
            get_page_sizes(work.clone()).expect("tamaños")[1].rotation,
            90
        );
        // una vuelta entera no cambia nada ni deja paso de historial
        let pasos_ahora = pasos(&work);
        rotate_pages(work.clone(), vec![1], 4).expect("vuelta entera");
        assert_eq!(pasos(&work), pasos_ahora);

        // borrarlas todas no puede dejar un documento sin páginas
        assert!(delete_pages(work.clone(), (0..6).collect()).is_err());

        std::fs::remove_file(&pdf).ok();
    }

    /// «Extraer páginas…» con «Eliminar las páginas del original» marcada:
    /// el destino se queda el rango y el origen lo pierde, todo en la misma
    /// operación.
    #[test]
    fn extraer_puede_llevarse_las_paginas_del_original() {
        let dir = std::env::temp_dir();
        let pdf = dir.join("editor_pdf_test_extraer_mover.pdf");
        let destino = dir.join("editor_pdf_test_extraer_mover_dest.pdf");
        crea_pdf(&["Uno", "Dos", "Tres", "Cuatro"], &pdf);
        let work = pdf.to_string_lossy().into_owned();

        extract_pages(
            work.clone(),
            vec![1, 2],
            destino.to_string_lossy().into_owned(),
            Some(true),
        )
        .expect("extraer y borrar");

        let d = textos_de(&destino);
        assert_eq!(d.len(), 2);
        assert!(
            d[0].contains("Dos") && d[1].contains("Tres"),
            "destino: {d:?}"
        );
        let o = textos_de(&pdf);
        assert_eq!(o.len(), 2, "el original conserva las que no se extrajeron");
        assert!(
            o[0].contains("Uno") && o[1].contains("Cuatro"),
            "origen: {o:?}"
        );
        crate::historial::undo(work).expect("deshacer");
        assert_eq!(
            textos_de(&pdf).len(),
            4,
            "un ⌘Z devuelve el original entero"
        );

        for f in [&pdf, &destino] {
            std::fs::remove_file(f).ok();
        }
    }

    #[test]
    fn gestion_de_paginas() {
        let dir = std::env::temp_dir();
        let doc_a = dir.join("editor_pdf_test_paginas_a.pdf");
        let doc_b = dir.join("editor_pdf_test_paginas_b.pdf");
        let extraido = dir.join("editor_pdf_test_paginas_extra.pdf");
        crea_pdf(&["Uno", "Dos", "Tres"], &doc_a);
        crea_pdf(&["Cuatro"], &doc_b);
        let work = doc_a.to_string_lossy().into_owned();

        // mover: [Uno, Dos, Tres] -> [Dos, Uno, Tres]
        move_page(work.clone(), 0, 1).expect("mover página");
        let t = textos_de(&doc_a);
        assert!(t[0].contains("Dos") && t[1].contains("Uno"), "orden: {t:?}");

        // borrar la primera: -> [Uno, Tres]
        let count = delete_page(work.clone(), 0).expect("borrar página");
        assert_eq!(count, 2);

        // unir doc_b: -> [Uno, Tres, Cuatro]
        let count =
            merge_pdf(work.clone(), doc_b.to_string_lossy().into_owned()).expect("unir PDFs");
        assert_eq!(count, 3);
        let t = textos_de(&doc_a);
        assert!(t[2].contains("Cuatro"), "tras unir: {t:?}");

        // rotar la primera página 90°
        rotate_page(work.clone(), 0).expect("rotar página");

        // extraer la última a un PDF nuevo
        extract_pages(
            work.clone(),
            vec![2],
            extraido.to_string_lossy().into_owned(),
            None,
        )
        .expect("extraer página");
        let t = textos_de(&extraido);
        assert_eq!(t.len(), 1);
        assert!(t[0].contains("Cuatro"), "extraído: {t:?}");

        for f in [&doc_a, &doc_b, &extraido] {
            std::fs::remove_file(f).ok();
        }
    }

    /// «Un fichero por página» era una llamada a `extract_pages` por página
    /// más un `delete_pages` aparte: si algo fallaba a mitad quedaban
    /// ficheros escritos y el documento intacto, sin decirlo. Es una sola
    /// operación con un solo paso de deshacer.
    #[test]
    fn un_fichero_por_pagina_es_una_sola_operacion() {
        let pdf = std::env::temp_dir().join("editor_pdf_test_por_pagina.pdf");
        crea_pdf(&["Uno", "Dos", "Tres", "Cuatro"], &pdf);
        let work = pdf.to_string_lossy().into_owned();
        let dir = std::env::temp_dir().join("vitela-por-pagina-test");
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).expect("carpeta");
        let pasos = |w: &str| {
            crate::historial::history_state(w.to_string())
                .expect("historial")
                .undo
        };
        let antes = pasos(&work);

        let escritos = extract_each_page(
            work.clone(),
            vec![0, 1, 2],
            dir.to_string_lossy().into_owned(),
            Some(true),
        )
        .expect("un fichero por página");

        assert_eq!(escritos.len(), 3, "un fichero por página pedida");
        for (n, ruta) in escritos.iter().enumerate() {
            let t = textos_de(std::path::Path::new(ruta));
            assert_eq!(t.len(), 1, "cada fichero lleva una página");
            let esperado = ["Uno", "Dos", "Tres"][n];
            assert!(t[0].contains(esperado), "{ruta} debería llevar {esperado}");
        }
        assert_eq!(textos_de(&pdf).len(), 1, "las extraídas se han borrado");
        assert_eq!(pasos(&work), antes + 1, "extraer y borrar es UN paso");

        crate::historial::undo(work.clone()).expect("deshacer");
        assert_eq!(textos_de(&pdf).len(), 4, "⌘Z devuelve el documento entero");

        // y si el destino no vale, no se escribe nada a medias
        let fallo = extract_each_page(
            work.clone(),
            vec![0, 1],
            dir.join("no-existe").to_string_lossy().into_owned(),
            Some(true),
        );
        assert!(fallo.is_err(), "una carpeta que no existe tiene que fallar");
        assert_eq!(
            textos_de(&pdf).len(),
            4,
            "el documento se queda como estaba"
        );

        std::fs::remove_dir_all(&dir).ok();
        std::fs::remove_file(&pdf).ok();
    }
}
