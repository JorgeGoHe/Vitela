//! Anotaciones nuevas: subrayado/tachado (markup con quadpoints, como el
//! resaltado), formas geométricas (Ink con su path dentro, como los trazos:
//! es la única vía que renderiza sin /AP y se borra como anotación) y sellos
//! (Stamp con borde + texto dentro).

use crate::{on_pdfium_thread, pdfium, save_and_close, ui_rect_to_pdf, Rect};
use pdfium_render::prelude::*;

fn color_de(c: [u8; 4]) -> PdfColor {
    PdfColor::new(c[0], c[1], c[2], c[3])
}

/// Marca de texto sobre los rects dados (coords de UI): resaltado, subrayado
/// o tachado. Igual que `add_highlight` pero con subtipo y color a elegir.
#[tauri::command]
pub fn add_markup(
    work_path: String,
    page_index: u16,
    rects: Vec<Rect>,
    kind: String,
    color: Option<[u8; 4]>,
) -> Result<(), String> {
    if rects.is_empty() {
        return Err("No hay nada que marcar".into());
    }
    on_pdfium_thread(move || {
        let pdfium = pdfium()?;
        let doc = pdfium
            .load_pdf_from_file(&work_path, None)
            .map_err(|e| e.to_string())?;
        let mut page = doc.pages().get(page_index).map_err(|e| e.to_string())?;
        let page_h = page.height().value;
        let left = rects.iter().map(|r| r.x).fold(f32::MAX, f32::min);
        let top = rects.iter().map(|r| r.y).fold(f32::MAX, f32::min);
        let right = rects.iter().map(|r| r.x + r.w).fold(f32::MIN, f32::max);
        let bottom = rects.iter().map(|r| r.y + r.h).fold(f32::MIN, f32::max);
        let envelope = ui_rect_to_pdf(
            &Rect {
                x: left,
                y: top,
                w: right - left,
                h: bottom - top,
            },
            page_h,
        );
        // los tres subtipos comparten API pero son tipos distintos sin trait
        // común para los quadpoints: macro local en vez de duplicar
        macro_rules! configurar {
            ($annot:expr, $default:expr) => {{
                let mut annot = $annot.map_err(|e| e.to_string())?;
                annot
                    .set_stroke_color(color_de(color.unwrap_or($default)))
                    .map_err(|e| e.to_string())?;
                annot.set_bounds(envelope).map_err(|e| e.to_string())?;
                let points = annot.attachment_points_mut();
                for r in &rects {
                    let pr = ui_rect_to_pdf(r, page_h);
                    // orden del spec (UL, UR, LL, LR)
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
                    points
                        .create_attachment_point_at_end(quad)
                        .map_err(|e| e.to_string())?;
                }
            }};
        }
        {
            let annotations = page.annotations_mut();
            match kind.as_str() {
                "highlight" => {
                    configurar!(annotations.create_highlight_annotation(), [255, 220, 0, 140])
                }
                "underline" => {
                    configurar!(annotations.create_underline_annotation(), [46, 160, 67, 255])
                }
                "strikeout" => {
                    configurar!(annotations.create_strikeout_annotation(), [226, 61, 61, 255])
                }
                otro => return Err(format!("Tipo de marca desconocido: {otro}")),
            }
        }
        drop(page);
        save_and_close(doc, &work_path)?;
        Ok(())
    })
}

/// Forma geométrica entre dos puntos (coords de UI): rectángulo, elipse,
/// línea o flecha. Va como anotación Ink con el path dentro para que
/// renderice en cualquier visor y se pueda borrar individualmente.
#[tauri::command]
pub fn add_shape(
    work_path: String,
    page_index: u16,
    kind: String,
    x1: f32,
    y1: f32,
    x2: f32,
    y2: f32,
    stroke: [u8; 4],
    fill: Option<[u8; 4]>,
    stroke_width: f32,
) -> Result<(), String> {
    on_pdfium_thread(move || {
        let pdfium = pdfium()?;
        let doc = pdfium
            .load_pdf_from_file(&work_path, None)
            .map_err(|e| e.to_string())?;
        let mut page = doc.pages().get(page_index).map_err(|e| e.to_string())?;
        let page_h = page.height().value;
        let stroke_color = color_de(stroke);
        let width = PdfPoints::new(stroke_width.max(0.5));
        let fill_color = fill.map(color_de);

        // coords PDF (origen abajo-izquierda)
        let (px1, py1) = (x1, page_h - y1);
        let (px2, py2) = (x2, page_h - y2);
        let bbox = PdfRect::new(
            PdfPoints::new(py1.min(py2)),
            PdfPoints::new(px1.min(px2)),
            PdfPoints::new(py1.max(py2)),
            PdfPoints::new(px1.max(px2)),
        );

        let path = match kind.as_str() {
            "rect" => PdfPagePathObject::new_rect(
                &doc,
                bbox,
                Some(stroke_color),
                Some(width),
                fill_color,
            )
            .map_err(|e| e.to_string())?,
            "ellipse" => PdfPagePathObject::new_ellipse(
                &doc,
                bbox,
                Some(stroke_color),
                Some(width),
                fill_color,
            )
            .map_err(|e| e.to_string())?,
            "line" => PdfPagePathObject::new_line(
                &doc,
                PdfPoints::new(px1),
                PdfPoints::new(py1),
                PdfPoints::new(px2),
                PdfPoints::new(py2),
                stroke_color,
                width,
            )
            .map_err(|e| e.to_string())?,
            "arrow" => {
                let mut p = PdfPagePathObject::new(
                    &doc,
                    PdfPoints::new(px1),
                    PdfPoints::new(py1),
                    Some(stroke_color),
                    Some(width),
                    None,
                )
                .map_err(|e| e.to_string())?;
                p.line_to(PdfPoints::new(px2), PdfPoints::new(py2))
                    .map_err(|e| e.to_string())?;
                // punta: dos segmentos a ±30° de la dirección de la línea
                let ang = (py2 - py1).atan2(px2 - px1);
                let head = (12.0 + stroke_width * 2.0).max(10.0);
                for delta in [std::f32::consts::PI / 6.0, -std::f32::consts::PI / 6.0] {
                    let a = ang + std::f32::consts::PI - delta;
                    p.move_to(PdfPoints::new(px2), PdfPoints::new(py2))
                        .map_err(|e| e.to_string())?;
                    p.line_to(
                        PdfPoints::new(px2 + head * a.cos()),
                        PdfPoints::new(py2 + head * a.sin()),
                    )
                    .map_err(|e| e.to_string())?;
                }
                p
            }
            otro => return Err(format!("Forma desconocida: {otro}")),
        };

        let mut annot = page
            .annotations_mut()
            .create_ink_annotation()
            .map_err(|e| e.to_string())?;
        let margin = stroke_width + 14.0;
        annot
            .set_bounds(PdfRect::new(
                PdfPoints::new(bbox.bottom().value - margin),
                PdfPoints::new(bbox.left().value - margin),
                PdfPoints::new(bbox.top().value + margin),
                PdfPoints::new(bbox.right().value + margin),
            ))
            .map_err(|e| e.to_string())?;
        annot
            .objects_mut()
            .add_path_object(path)
            .map_err(|e| e.to_string())?;
        drop(annot);
        drop(page);
        save_and_close(doc, &work_path)?;
        Ok(())
    })
}

/// Sello de texto (APROBADO, BORRADOR…): anotación Stamp con un borde y el
/// texto dentro, centrado en el punto dado (coords de UI).
#[tauri::command]
pub fn add_stamp(
    work_path: String,
    page_index: u16,
    text: String,
    color: [u8; 4],
    x: f32,
    y: f32,
    font_size: f32,
) -> Result<(), String> {
    let text = text.trim().to_string();
    if text.is_empty() {
        return Err("El sello está vacío".into());
    }
    on_pdfium_thread(move || {
        let pdfium = pdfium()?;
        let mut doc = pdfium
            .load_pdf_from_file(&work_path, None)
            .map_err(|e| e.to_string())?;
        let font = doc.fonts_mut().helvetica_bold();
        let mut page = doc.pages().get(page_index).map_err(|e| e.to_string())?;
        let page_h = page.height().value;
        let size = font_size.clamp(8.0, 96.0);
        // Helvetica Bold en mayúsculas ronda 0.66 em de media por carácter
        let text_w = text.chars().count() as f32 * size * 0.66;
        let pad = size * 0.45;
        let w = text_w + pad * 2.0;
        let h = size + pad * 2.0;
        // centrado en el punto de clic
        let left = x - w / 2.0;
        let bottom = page_h - y - h / 2.0;
        let c = color_de(color);

        let mut annot = page
            .annotations_mut()
            .create_stamp_annotation()
            .map_err(|e| e.to_string())?;
        annot
            .set_bounds(PdfRect::new(
                PdfPoints::new(bottom - 2.0),
                PdfPoints::new(left - 2.0),
                PdfPoints::new(bottom + h + 2.0),
                PdfPoints::new(left + w + 2.0),
            ))
            .map_err(|e| e.to_string())?;
        let border = PdfPagePathObject::new_rect(
            &doc,
            PdfRect::new(
                PdfPoints::new(bottom),
                PdfPoints::new(left),
                PdfPoints::new(bottom + h),
                PdfPoints::new(left + w),
            ),
            Some(c),
            Some(PdfPoints::new((size * 0.09).max(1.2))),
            None,
        )
        .map_err(|e| e.to_string())?;
        let mut texto = PdfPageTextObject::new(&doc, &text, font, PdfPoints::new(size))
            .map_err(|e| e.to_string())?;
        texto.set_fill_color(c).map_err(|e| e.to_string())?;
        // origen del texto = izquierda de la línea base
        texto
            .translate(
                PdfPoints::new(left + pad),
                PdfPoints::new(bottom + pad + size * 0.14),
            )
            .map_err(|e| e.to_string())?;
        annot
            .objects_mut()
            .add_path_object(border)
            .map_err(|e| e.to_string())?;
        annot
            .objects_mut()
            .add_text_object(texto)
            .map_err(|e| e.to_string())?;
        drop(annot);
        drop(page);
        save_and_close(doc, &work_path)?;
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::crea_pdf;
    use base64::Engine;

    fn render_rgba(path: &str) -> image::RgbaImage {
        let png_b64 = crate::render_page(path.to_string(), 0, 600).expect("render");
        let png = base64::engine::general_purpose::STANDARD
            .decode(png_b64)
            .expect("base64");
        image::load_from_memory(&png).expect("PNG").to_rgba8()
    }

    #[test]
    fn subrayado_y_tachado_con_quads() {
        let pdf = std::env::temp_dir().join("anotaciones2-markup-test.pdf");
        crea_pdf(&["Texto marcado"], &pdf);
        let work = pdf.to_string_lossy().to_string();
        let r = vec![Rect {
            x: 50.0,
            y: 690.0,
            w: 120.0,
            h: 16.0,
        }];
        add_markup(work.clone(), 0, r.clone(), "underline".into(), None).expect("subrayar");
        add_markup(work.clone(), 0, r, "strikeout".into(), None).expect("tachar");
        let annots = crate::get_annotations(work, 0).expect("listar");
        let kinds: Vec<&str> = annots.iter().map(|a| a.kind.as_str()).collect();
        assert!(kinds.contains(&"Underline"), "{kinds:?}");
        assert!(kinds.contains(&"Strikeout") || kinds.contains(&"StrikeOut"), "{kinds:?}");
        for a in &annots {
            assert_eq!(a.rects.len(), 1, "quads de {}", a.kind);
            assert!((a.rects[0].x - 50.0).abs() < 1.0);
        }
    }

    #[test]
    fn el_color_sobrevive_al_render() {
        // PDFium genera appearance streams al renderizar y su GetColor deja
        // de responder: el fallback lopdf debe seguir dando el color
        let pdf = std::env::temp_dir().join("anotaciones2-color-render-test.pdf");
        crea_pdf(&["Texto marcado"], &pdf);
        let work = pdf.to_string_lossy().to_string();
        add_markup(
            work.clone(),
            0,
            vec![Rect { x: 50.0, y: 690.0, w: 120.0, h: 16.0 }],
            "strikeout".into(),
            Some([192, 57, 43, 255]),
        )
        .expect("tachar");
        // renderizar con el caché del documento (como hace la UI)
        crate::render_page(work.clone(), 0, 400).expect("render");
        let annots = crate::get_annotations(work, 0).expect("listar");
        assert_eq!(annots.len(), 1);
        assert_eq!(
            annots[0].color,
            Some([192, 57, 43, 255]),
            "el color debe leerse aunque el render haya generado AP"
        );
    }

    #[test]
    fn formas_visibles_en_el_render() {
        let pdf = std::env::temp_dir().join("anotaciones2-shapes-test.pdf");
        crea_pdf(&["Página"], &pdf);
        let work = pdf.to_string_lossy().to_string();
        // rect rojo relleno para poder mirar un píxel interior
        add_shape(
            work.clone(),
            0,
            "rect".into(),
            100.0,
            300.0,
            220.0,
            380.0,
            [200, 0, 0, 255],
            Some([200, 0, 0, 255]),
            2.0,
        )
        .expect("rect");
        add_shape(
            work.clone(),
            0,
            "arrow".into(),
            300.0,
            300.0,
            400.0,
            380.0,
            [0, 0, 200, 255],
            None,
            3.0,
        )
        .expect("flecha");
        let annots = crate::get_annotations(work.clone(), 0).expect("listar");
        assert_eq!(annots.len(), 2);
        let img = render_rgba(&work);
        let escala = 600.0 / 595.28;
        let p = img.get_pixel((160.0 * escala) as u32, (340.0 * escala) as u32);
        assert!(p[0] > 150 && p[1] < 100, "esperaba rojo dentro del rect, hay {p:?}");
    }

    #[test]
    fn sello_renderiza_borde_y_texto() {
        let pdf = std::env::temp_dir().join("anotaciones2-stamp-test.pdf");
        crea_pdf(&["Página"], &pdf);
        let work = pdf.to_string_lossy().to_string();
        add_stamp(
            work.clone(),
            0,
            "APROBADO".into(),
            [200, 30, 30, 255],
            300.0,
            400.0,
            22.0,
        )
        .expect("sello");
        let annots = crate::get_annotations(work.clone(), 0).expect("listar");
        assert_eq!(annots.len(), 1);
        assert_eq!(annots[0].kind, "Stamp");
        let img = render_rgba(&work);
        let escala = 600.0 / 595.28;
        // barrer la zona del sello buscando píxeles rojos (borde o letras)
        let mut rojos = 0;
        for yy in 370..430 {
            for xx in 200..400 {
                let p = img.get_pixel((xx as f32 * escala) as u32, (yy as f32 * escala) as u32);
                if p[0] > 150 && p[1] < 110 && p[2] < 110 {
                    rojos += 1;
                }
            }
        }
        assert!(rojos > 200, "el sello apenas pinta ({rojos} píxeles rojos)");
    }
}
