//! Anotaciones básicas: resaltado, trazo (Ink), nota, listado y borrado.

use crate::{on_pdfium_thread, pdfium, save_and_close, with_doc, with_lopdf, Rect};
use crate::historial::mutacion;
use pdfium_render::prelude::*;
use serde::Serialize;

/// Convierte un rect en coords de UI (origen arriba-izquierda) a PdfRect
/// (origen abajo-izquierda).
pub(crate) fn ui_rect_to_pdf(r: &Rect, page_h: f32) -> PdfRect {
    PdfRect::new(
        PdfPoints::new(page_h - r.y - r.h),
        PdfPoints::new(r.x),
        PdfPoints::new(page_h - r.y),
        PdfPoints::new(r.x + r.w),
    )
}

/// Crea una anotación de resaltado amarillo sobre los rects dados
/// (coords de UI en puntos PDF).
#[tauri::command(async)]
pub fn add_highlight(work_path: String, page_index: u16, rects: Vec<Rect>) -> Result<(), String> {
    if rects.is_empty() {
        return Err("No hay nada que resaltar".into());
    }
    mutacion(work_path, |work_path| on_pdfium_thread(move || {
        let pdfium = pdfium()?;
        let doc = pdfium
            .load_pdf_from_file(&work_path, None)
            .map_err(|e| e.to_string())?;
        let mut page = doc.pages().get(page_index).map_err(|e| e.to_string())?;
        let page_h = page.height().value;
        let mut annot = page
            .annotations_mut()
            .create_highlight_annotation()
            .map_err(|e| e.to_string())?;
        // flag Print: sin él, aplanar (FLAT_PRINT) descarta la anotación
        annot.set_is_printed(true).map_err(|e| e.to_string())?;
        annot
            .set_stroke_color(PdfColor::new(255, 220, 0, 140))
            .map_err(|e| e.to_string())?;
        let left = rects.iter().map(|r| r.x).fold(f32::MAX, f32::min);
        let top = rects.iter().map(|r| r.y).fold(f32::MAX, f32::min);
        let right = rects.iter().map(|r| r.x + r.w).fold(f32::MIN, f32::max);
        let bottom = rects.iter().map(|r| r.y + r.h).fold(f32::MIN, f32::max);
        let envelope = Rect {
            x: left,
            y: top,
            w: right - left,
            h: bottom - top,
        };
        annot
            .set_bounds(ui_rect_to_pdf(&envelope, page_h))
            .map_err(|e| e.to_string())?;
        for r in &rects {
            let pr = ui_rect_to_pdf(r, page_h);
            // Orden del spec (UL, UR, LL, LR): otros visores generan la
            // apariencia a partir de los quads y el orden importa.
            let quad = PdfQuadPoints::new(
                pr.left(),
                pr.top(),
                pr.right(),
                pr.top(),
                pr.left(),
                pr.bottom(),
                pr.right(),
                pr.bottom(),
            );
            annot
                .attachment_points_mut()
                .create_attachment_point_at_end(quad)
                .map_err(|e| e.to_string())?;
        }
        drop(page);
        save_and_close(doc, &work_path)?;
        Ok(())
    }))
}

/// Añade un trazo a mano alzada como anotación Ink con su apariencia
/// (un path dentro de la anotación), de modo que se puede borrar
/// individualmente. Los puntos vienen en coords de UI (puntos PDF,
/// origen arriba-izquierda).
#[tauri::command(async)]
pub fn add_stroke(
    work_path: String,
    page_index: u16,
    points: Vec<[f32; 2]>,
    color: Option<[u8; 4]>,
    width: Option<f32>,
) -> Result<(), String> {
    if points.len() < 2 {
        return Err("Trazo demasiado corto".into());
    }
    let c = color.unwrap_or([226, 61, 61, 255]);
    let w = width.unwrap_or(2.0).clamp(0.5, 12.0);
    mutacion(work_path, |work_path| on_pdfium_thread(move || {
        let pdfium = pdfium()?;
        let doc = pdfium
            .load_pdf_from_file(&work_path, None)
            .map_err(|e| e.to_string())?;
        let mut page = doc.pages().get(page_index).map_err(|e| e.to_string())?;
        let page_h = page.height().value;
        let mut annot = page
            .annotations_mut()
            .create_ink_annotation()
            .map_err(|e| e.to_string())?;
        annot.set_is_printed(true).map_err(|e| e.to_string())?;
        // /C antes de añadir el path (con /AP PDFium ya no deja fijarlo);
        // es lo que lee get_annotations, el color del path no se ve fuera
        annot
            .set_stroke_color(PdfColor::new(c[0], c[1], c[2], c[3]))
            .map_err(|e| e.to_string())?;
        const MARGIN: f32 = 3.0;
        let min_x = points.iter().map(|p| p[0]).fold(f32::MAX, f32::min) - MARGIN;
        let max_x = points.iter().map(|p| p[0]).fold(f32::MIN, f32::max) + MARGIN;
        let min_y = points.iter().map(|p| p[1]).fold(f32::MAX, f32::min) - MARGIN;
        let max_y = points.iter().map(|p| p[1]).fold(f32::MIN, f32::max) + MARGIN;
        annot
            .set_bounds(PdfRect::new(
                PdfPoints::new(page_h - max_y),
                PdfPoints::new(min_x),
                PdfPoints::new(page_h - min_y),
                PdfPoints::new(max_x),
            ))
            .map_err(|e| e.to_string())?;
        let mut path = PdfPagePathObject::new(
            &doc,
            PdfPoints::new(points[0][0]),
            PdfPoints::new(page_h - points[0][1]),
            Some(PdfColor::new(c[0], c[1], c[2], c[3])),
            Some(PdfPoints::new(w)),
            None,
        )
        .map_err(|e| e.to_string())?;
        for p in &points[1..] {
            path.line_to(PdfPoints::new(p[0]), PdfPoints::new(page_h - p[1]))
                .map_err(|e| e.to_string())?;
        }
        annot
            .objects_mut()
            .add_path_object(path)
            .map_err(|e| e.to_string())?;
        drop(page);
        save_and_close(doc, &work_path)?;
        Ok(())
    }))
}

/// Crea una nota (anotación de texto) en el punto dado (coords de UI).
#[tauri::command(async)]
pub fn add_note(
    work_path: String,
    page_index: u16,
    x: f32,
    y: f32,
    text: String,
) -> Result<(), String> {
    if text.trim().is_empty() {
        return Err("La nota está vacía".into());
    }
    mutacion(work_path, |work_path| on_pdfium_thread(move || {
        let pdfium = pdfium()?;
        let doc = pdfium
            .load_pdf_from_file(&work_path, None)
            .map_err(|e| e.to_string())?;
        let mut page = doc.pages().get(page_index).map_err(|e| e.to_string())?;
        let page_h = page.height().value;
        let mut annot = page
            .annotations_mut()
            .create_text_annotation(&text)
            .map_err(|e| e.to_string())?;
        annot.set_is_printed(true).map_err(|e| e.to_string())?;
        const ICON: f32 = 22.0;
        annot
            .set_bounds(ui_rect_to_pdf(
                &Rect {
                    x,
                    y,
                    w: ICON,
                    h: ICON,
                },
                page_h,
            ))
            .map_err(|e| e.to_string())?;
        drop(page);
        save_and_close(doc, &work_path)?;
        Ok(())
    }))
}

#[derive(Serialize, Debug)]
pub struct AnnotationInfo {
    pub index: u16,
    pub kind: String,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub contents: String,
    /// Para resaltados: un rect por línea (los quads), en coords de UI.
    pub rects: Vec<Rect>,
    /// Color de trazo de la anotación (para que la UI pinte los overlays
    /// con el color real, no uno fijo).
    pub color: Option<[u8; 4]>,
}

/// Lista las anotaciones de una página (bounds en coords de UI). La UI las
/// usa para pintar los iconos de nota, los rects de los resaltados (PDFium no
/// genera apariencia automática para Text ni Highlight) y para borrar con clic.
#[tauri::command(async)]
pub fn get_annotations(path: String, page_index: u16) -> Result<Vec<AnnotationInfo>, String> {
    on_pdfium_thread(move || {
        with_doc(&path, |doc| {
            let page = doc.pages().get(page_index).map_err(|e| e.to_string())?;
            let page_h = page.height().value;
            let annotations = page.annotations();
            let mut out = Vec::new();
            for i in 0..annotations.len() {
                let Ok(mut a) = annotations.get(i) else {
                    continue;
                };
                let Ok(b) = a.bounds() else { continue };
                let mut rects = Vec::new();
                {
                    // resaltado, subrayado y tachado guardan sus líneas como
                    // quadpoints; tipos distintos sin trait común
                    macro_rules! lee_quads {
                        ($m:expr) => {
                            if let Some(m) = $m {
                                let points = m.attachment_points_mut();
                                for j in 0..points.len() {
                                    if let Ok(q) = points.get(j) {
                                        rects.push(Rect {
                                            x: q.left().value,
                                            y: page_h - q.top().value,
                                            w: q.right().value - q.left().value,
                                            h: q.top().value - q.bottom().value,
                                        });
                                    }
                                }
                            }
                        };
                    }
                    lee_quads!(a.as_highlight_annotation_mut());
                    lee_quads!(a.as_underline_annotation_mut());
                    lee_quads!(a.as_strikeout_annotation_mut());
                }
                out.push(AnnotationInfo {
                    index: i as u16,
                    kind: format!("{:?}", a.annotation_type()),
                    x: b.left().value,
                    y: page_h - b.top().value,
                    w: b.right().value - b.left().value,
                    h: b.top().value - b.bottom().value,
                    contents: a.contents().unwrap_or_default(),
                    rects,
                    color: None,
                });
            }
            if let Some(colores) = colores_annots_lopdf(&path, page_index) {
                for a in out.iter_mut() {
                    a.color = colores.get(a.index as usize).copied().flatten();
                }
            }
            Ok(out)
        })
    })
}

/// Colores /C (+ /CA como alfa) de las anotaciones de una página leídos con
/// lopdf, alineados por índice con el orden de /Annots (el mismo que recorre
/// PDFium). Es la ÚNICA fuente del color, a propósito: `stroke_color()` de
/// pdfium-render 0.8 castea el handle de la anotación a objeto de página
/// cuando FPDFAnnot_GetColor falla (anotaciones con appearance stream:
/// formas, sellos, Ink, y cualquiera tras un render, porque PDFium genera
/// los /AP en memoria), y en Linux ese cast es un SIGSEGV de toda la app.
pub fn colores_annots_lopdf(path: &str, page_index: u16) -> Option<Vec<Option<[u8; 4]>>> {
    with_lopdf(path, |doc| Ok(colores_annots(doc, page_index))).ok().flatten()
}

pub fn colores_annots(doc: &lopdf::Document, page_index: u16) -> Option<Vec<Option<[u8; 4]>>> {
    use lopdf::Object;
    let page_id = *doc.get_pages().get(&(page_index as u32 + 1))?;
    let page = doc.get_object(page_id).ok()?.as_dict().ok()?;
    let annots = match page.get(b"Annots").ok()? {
        Object::Reference(rid) => doc.get_object(*rid).ok()?.as_array().ok()?.clone(),
        Object::Array(a) => a.clone(),
        _ => return None,
    };
    let num = |o: &Object| -> Option<f32> {
        match o {
            Object::Integer(i) => Some(*i as f32),
            Object::Real(r) => Some(*r),
            _ => None,
        }
    };
    Some(
        annots
            .iter()
            .map(|a| {
                let dict = match a {
                    Object::Reference(rid) => doc.get_object(*rid).ok()?.as_dict().ok()?,
                    Object::Dictionary(d) => d,
                    _ => return None,
                };
                let c = match dict.get(b"C").ok()? {
                    Object::Array(v) => v,
                    _ => return None,
                };
                let alpha = dict
                    .get(b"CA")
                    .ok()
                    .and_then(num)
                    .map(|a| (a * 255.0) as u8)
                    .unwrap_or(255);
                match c.len() {
                    3 => Some([
                        (num(&c[0])? * 255.0) as u8,
                        (num(&c[1])? * 255.0) as u8,
                        (num(&c[2])? * 255.0) as u8,
                        alpha,
                    ]),
                    1 => {
                        let g = (num(&c[0])? * 255.0) as u8;
                        Some([g, g, g, alpha])
                    }
                    _ => None,
                }
            })
            .collect(),
    )
}

/// Elimina la anotación con el índice dado.
#[tauri::command(async)]
pub fn remove_annotation(work_path: String, page_index: u16, annot_index: u16) -> Result<(), String> {
    mutacion(work_path, |work_path| on_pdfium_thread(move || {
        let pdfium = pdfium()?;
        let doc = pdfium
            .load_pdf_from_file(&work_path, None)
            .map_err(|e| e.to_string())?;
        let mut page = doc.pages().get(page_index).map_err(|e| e.to_string())?;
        let annotations = page.annotations_mut();
        let annot = annotations
            .get(annot_index as usize)
            .map_err(|e| e.to_string())?;
        annotations
            .delete_annotation(annot)
            .map_err(|e| e.to_string())?;
        drop(page);
        save_and_close(doc, &work_path)?;
        Ok(())
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine;
    #[allow(unused_imports)]
    use crate::tests::{crea_pdf, textos_de};
    #[allow(unused_imports)]
    use crate::{get_page_sizes, open_pdf, render_page_b64, render_page_png};

    #[test]
    fn trazo_es_visible() {
        let tmp = std::env::temp_dir().join("editor_pdf_test_trazo_vis.pdf");
        crea_pdf(&["Hola"], &tmp);
        let work = tmp.to_string_lossy().into_owned();
        let decode = |b64: String| {
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(b64)
                .unwrap();
            image::load_from_memory(&bytes).unwrap().to_rgba8()
        };
        let antes = decode(render_page_b64(work.clone(), 0, 200).unwrap());
        // trazo horizontal que pasa por (150, 120) pt
        add_stroke(work.clone(), 0, vec![[50.0, 120.0], [250.0, 120.0]], None, None).expect("trazo");
        let despues = decode(render_page_b64(work.clone(), 0, 200).unwrap());
        let px = (150.0f32 * 200.0 / 595.0) as u32;
        let py = (120.0f32 * 200.0 / 595.0) as u32;
        let mut cambiado = false;
        for dy in 0..3 {
            if antes.get_pixel(px, py + dy) != despues.get_pixel(px, py + dy) {
                cambiado = true;
            }
        }
        assert!(cambiado, "el trazo no cambió ningún píxel");
        // el color del trazo tiene que llegar a la UI en get_annotations
        add_stroke(
            work.clone(),
            0,
            vec![[50.0, 200.0], [250.0, 200.0]],
            Some([46, 160, 67, 255]),
            None,
        )
        .expect("trazo con color");
        let annots = get_annotations(work, 0).expect("annots");
        let ultimo = annots.last().expect("hay anotaciones");
        assert_eq!(ultimo.kind, "Ink");
        assert_eq!(ultimo.color, Some([46, 160, 67, 255]));
        std::fs::remove_file(&tmp).ok();
    }

    #[test]
    fn resaltado_devuelve_sus_rects() {
        // PDFium no genera apariencia para Highlight: la UI lo pinta con los
        // rects que devuelve get_annotations. Verificamos ese contrato.
        let tmp = std::env::temp_dir().join("editor_pdf_test_resaltado_rects.pdf");
        crea_pdf(&["Hola"], &tmp);
        let work = tmp.to_string_lossy().into_owned();
        add_highlight(
            work.clone(),
            0,
            vec![
                Rect {
                    x: 50.0,
                    y: 100.0,
                    w: 200.0,
                    h: 14.0,
                },
                Rect {
                    x: 50.0,
                    y: 118.0,
                    w: 120.0,
                    h: 14.0,
                },
            ],
        )
        .expect("resaltar");
        let annots = get_annotations(work.clone(), 0).expect("listar");
        let hl = annots
            .iter()
            .find(|a| a.kind == "Highlight")
            .expect("hay un resaltado");
        assert_eq!(hl.rects.len(), 2, "un rect por línea");
        assert!((hl.rects[0].x - 50.0).abs() < 0.5, "x: {}", hl.rects[0].x);
        assert!((hl.rects[0].y - 100.0).abs() < 0.5, "y: {}", hl.rects[0].y);
        assert!((hl.rects[0].w - 200.0).abs() < 0.5, "w: {}", hl.rects[0].w);
        std::fs::remove_file(&tmp).ok();
    }

    #[test]
    fn anotaciones() {
        let tmp = std::env::temp_dir().join("editor_pdf_test_anotaciones.pdf");
        crea_pdf(&["Hola Mundo"], &tmp);
        let work = tmp.to_string_lossy().into_owned();

        // resaltado + nota
        add_highlight(
            work.clone(),
            0,
            vec![Rect {
                x: 50.0,
                y: 700.0,
                w: 100.0,
                h: 14.0,
            }],
        )
        .expect("resaltar");
        add_note(work.clone(), 0, 200.0, 100.0, "Una nota".into()).expect("añadir nota");
        let annots = get_annotations(work.clone(), 0).expect("listar anotaciones");
        assert_eq!(annots.len(), 2, "anotaciones: {:?}", annots.len());
        let nota = annots.iter().find(|a| a.kind == "Text").expect("nota");
        assert_eq!(nota.contents, "Una nota");

        // trazo como anotación Ink
        add_stroke(
            work.clone(),
            0,
            vec![[10.0, 10.0], [50.0, 40.0], [90.0, 10.0]],
            None,
            None,
        )
        .expect("añadir trazo");
        let annots = get_annotations(work.clone(), 0).expect("listar con trazo");
        assert_eq!(annots.len(), 3);
        let trazo = annots.iter().find(|a| a.kind == "Ink").expect("trazo");

        // borrar la nota y el trazo individualmente
        remove_annotation(work.clone(), 0, nota.index).expect("borrar nota");
        let annots = get_annotations(work.clone(), 0).expect("listar tras borrar");
        assert_eq!(annots.len(), 2);
        let trazo_idx = annots
            .iter()
            .find(|a| a.kind == "Ink")
            .map(|a| a.index)
            .unwrap_or(trazo.index);
        remove_annotation(work.clone(), 0, trazo_idx).expect("borrar trazo");
        assert_eq!(get_annotations(work.clone(), 0).expect("listar").len(), 1);

        // el render con anotaciones no debe fallar
        render_page_b64(work.clone(), 0, 200).expect("render con anotaciones");

        std::fs::remove_file(&tmp).ok();
    }
}
