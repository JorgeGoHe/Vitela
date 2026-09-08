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
) -> Result<(), String> {
    if page_indices.is_empty() {
        return Err("No hay páginas que extraer".into());
    }
    let range = page_indices
        .iter()
        .map(|i| (i + 1).to_string())
        .collect::<Vec<_>>()
        .join(",");
    on_pdfium_thread(move || {
        with_doc(&work_path, |doc| {
            let mut new_doc = pdfium()?.create_new_pdf().map_err(|e| e.to_string())?;
            new_doc
                .pages_mut()
                .copy_pages_from_document(doc, &range, 0)
                .map_err(|e| e.to_string())?;
            new_doc.save_to_file(&dest_path).map_err(|e| e.to_string())
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[allow(unused_imports)]
    use crate::tests::{crea_pdf, textos_de};
    #[allow(unused_imports)]
    use crate::{get_page_sizes, open_pdf, render_page_b64, render_page_png};

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
