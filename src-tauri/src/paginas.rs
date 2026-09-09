//! Gestión de páginas: borrar, rotar, mover, unir y extraer (FPDF_ImportPages).

use crate::{on_pdfium_thread, pdfium, save_and_close, with_doc};
use crate::historial::mutacion;
use pdfium_render::prelude::*;

/// Borra una página y devuelve el nuevo número de páginas.
#[tauri::command(async)]
pub fn delete_page(work_path: String, page_index: u16) -> Result<u16, String> {
    mutacion(work_path, |work_path| on_pdfium_thread(move || {
        let pdfium = pdfium()?;
        let doc = pdfium
            .load_pdf_from_file(&work_path, None)
            .map_err(|e| e.to_string())?;
        let page = doc.pages().get(page_index).map_err(|e| e.to_string())?;
        page.delete().map_err(|e| e.to_string())?;
        let count = doc.pages().len();
        save_and_close(doc, &work_path)?;
        Ok(count)
    }))
}

/// Rota una página 90° en sentido horario (acumulativo).
#[tauri::command(async)]
pub fn rotate_page(work_path: String, page_index: u16) -> Result<(), String> {
    mutacion(work_path, |work_path| on_pdfium_thread(move || {
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
    }))
}

/// Borra varias páginas de una vez. De mayor a menor, para que borrar una no
/// invalide los índices que quedan. Devuelve el nuevo número de páginas.
///
/// Debe llamarse desde el hilo de PDFium y dentro de una `mutacion`: un
/// borrado en lote es UN paso de deshacer, no uno por página.
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
        return Err(format!("La página {} ya no está en el documento", fuera + 1));
    }
    if indices.len() as u16 == total {
        return Err("Un documento no puede quedarse sin páginas".into());
    }
    for i in indices.iter().rev() {
        doc.pages().get(*i).map_err(|e| e.to_string())?.delete().map_err(|e| e.to_string())?;
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
    mutacion(work_path, move |work_path| on_pdfium_thread(move || {
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
    }))
}

/// Mueve una página a otra posición reconstruyendo el documento en el nuevo
/// orden (FPDF_ImportPages respeta el orden del rango dado).
#[tauri::command(async)]
pub fn move_page(work_path: String, from_index: u16, to_index: u16) -> Result<(), String> {
    if from_index == to_index {
        return Ok(());
    }
    mutacion(work_path, |work_path| on_pdfium_thread(move || {
        let pdfium = pdfium()?;
        let doc = pdfium
            .load_pdf_from_file(&work_path, None)
            .map_err(|e| e.to_string())?;
        let count = doc.pages().len();
        if from_index >= count || to_index >= count {
            return Err("Índice de página fuera de rango".into());
        }
        let mut order: Vec<u16> = (0..count).collect();
        let moved = order.remove(from_index as usize);
        order.insert(to_index as usize, moved);
        let range = order
            .iter()
            .map(|i| (i + 1).to_string())
            .collect::<Vec<_>>()
            .join(",");
        let mut new_doc = pdfium.create_new_pdf().map_err(|e| e.to_string())?;
        new_doc
            .pages_mut()
            .copy_pages_from_document(&doc, &range, 0)
            .map_err(|e| e.to_string())?;
        drop(doc);
        save_and_close(new_doc, &work_path)?;
        Ok(())
    }))
}

/// Añade todas las páginas de otro PDF al final y devuelve el nuevo total.
#[tauri::command(async)]
pub fn merge_pdf(work_path: String, other_path: String) -> Result<u16, String> {
    mutacion(work_path, |work_path| on_pdfium_thread(move || {
        let pdfium = pdfium()?;
        let mut doc = pdfium
            .load_pdf_from_file(&work_path, None)
            .map_err(|e| e.to_string())?;
        let other = pdfium
            .load_pdf_from_file(&other_path, None)
            .map_err(|e| e.to_string())?;
        doc.pages_mut().append(&other).map_err(|e| e.to_string())?;
        let count = doc.pages().len();
        drop(other);
        save_and_close(doc, &work_path)?;
        Ok(count)
    }))
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
    // extraer y borrar es UNA operación: un solo paso de deshacer, y si el
    // borrado falla el documento se queda como estaba
    let cuerpo = move |work_path: String| on_pdfium_thread(move || {
        with_doc(&work_path, |doc| {
            let mut new_doc = pdfium()?.create_new_pdf().map_err(|e| e.to_string())?;
            new_doc
                .pages_mut()
                .copy_pages_from_document(doc, &range, 0)
                .map_err(|e| e.to_string())?;
            new_doc.save_to_file(&dest_path).map_err(|e| e.to_string())
        })?;
        if borrar {
            borra_paginas(&work_path, &page_indices)?;
        }
        Ok(())
    });
    if borrar {
        mutacion(work_path, cuerpo)
    } else {
        cuerpo(work_path)
    }
}

#[cfg(test)]
mod tests {
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
        let pasos = |w: &str| crate::historial::history_state(w.to_string()).expect("historial").undo;
        let antes = pasos(&work);

        assert_eq!(delete_pages(work.clone(), vec![0, 2, 4]).expect("borrar"), 3);
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
        assert_eq!(get_page_sizes(work.clone()).expect("tamaños")[1].rotation, 90);
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
        assert!(d[0].contains("Dos") && d[1].contains("Tres"), "destino: {d:?}");
        let o = textos_de(&pdf);
        assert_eq!(o.len(), 2, "el original conserva las que no se extrajeron");
        assert!(o[0].contains("Uno") && o[1].contains("Cuatro"), "origen: {o:?}");
        crate::historial::undo(work).expect("deshacer");
        assert_eq!(textos_de(&pdf).len(), 4, "un ⌘Z devuelve el original entero");

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
}
