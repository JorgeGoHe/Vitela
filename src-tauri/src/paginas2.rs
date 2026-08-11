//! Gestión de páginas ampliada: página en blanco, duplicar, insertar otro
//! PDF en una posición, recorte, marca de agua y encabezados/pies con
//! numeración.

use crate::{on_pdfium_thread, pdfium, save_and_close, Rect};
use pdfium_render::prelude::*;

/// Inserta una página en blanco en `index`, del mismo tamaño que la página
/// vecina (o A4 si el documento está vacío). Devuelve el nuevo total.
#[tauri::command]
pub fn add_blank_page(work_path: String, index: u16) -> Result<u16, String> {
    on_pdfium_thread(move || {
        let pdfium = pdfium()?;
        let mut doc = pdfium
            .load_pdf_from_file(&work_path, None)
            .map_err(|e| e.to_string())?;
        let count = doc.pages().len();
        let size = doc
            .pages()
            .get(index.min(count.saturating_sub(1)))
            .map(|p| PdfPagePaperSize::Custom(p.width(), p.height()))
            .unwrap_or_else(|_| PdfPagePaperSize::a4());
        doc.pages_mut()
            .create_page_at_index(size, index.min(count))
            .map_err(|e| e.to_string())?;
        let nuevo = doc.pages().len();
        save_and_close(doc, &work_path)?;
        Ok(nuevo)
    })
}

/// Duplica la página dada (la copia queda justo después). Devuelve el total.
#[tauri::command]
pub fn duplicate_page(work_path: String, page_index: u16) -> Result<u16, String> {
    on_pdfium_thread(move || {
        let pdfium = pdfium()?;
        let mut doc = pdfium
            .load_pdf_from_file(&work_path, None)
            .map_err(|e| e.to_string())?;
        // segundo handle del mismo fichero, solo lectura, en el mismo hilo
        let origen = pdfium
            .load_pdf_from_file(&work_path, None)
            .map_err(|e| e.to_string())?;
        doc.pages_mut()
            .copy_pages_from_document(
                &origen,
                &format!("{}", page_index + 1),
                page_index + 1,
            )
            .map_err(|e| e.to_string())?;
        drop(origen);
        let nuevo = doc.pages().len();
        save_and_close(doc, &work_path)?;
        Ok(nuevo)
    })
}

/// Inserta todas las páginas de otro PDF en la posición dada. Devuelve el
/// total resultante (generaliza `merge_pdf`, que solo añade al final).
#[tauri::command]
pub fn insert_pdf_at(work_path: String, other_path: String, index: u16) -> Result<u16, String> {
    on_pdfium_thread(move || {
        let pdfium = pdfium()?;
        let mut doc = pdfium
            .load_pdf_from_file(&work_path, None)
            .map_err(|e| e.to_string())?;
        let other = pdfium
            .load_pdf_from_file(&other_path, None)
            .map_err(|e| e.to_string())?;
        let rango = format!("1-{}", other.pages().len());
        let index = index.min(doc.pages().len());
        doc.pages_mut()
            .copy_pages_from_document(&other, &rango, index)
            .map_err(|e| e.to_string())?;
        drop(other);
        let nuevo = doc.pages().len();
        save_and_close(doc, &work_path)?;
        Ok(nuevo)
    })
}

/// Recorta una página (o todas) al rect dado en coords de UI. En vez de
/// fijar solo el CropBox (que desplazaría el origen y desalinearía todas las
/// coordenadas de la UI), se normaliza: se traslada el contenido y las
/// anotaciones y se reescriben MediaBox y CropBox a (0,0,w,h). El contenido
/// fuera del área no se elimina (solo deja de mostrarse), como en Acrobat.
#[tauri::command]
pub fn crop_page(
    work_path: String,
    page_index: u16,
    rect: Rect,
    all_pages: bool,
) -> Result<(), String> {
    if rect.w < 24.0 || rect.h < 24.0 {
        return Err("El área de recorte es demasiado pequeña".into());
    }
    on_pdfium_thread(move || {
        let pdfium = pdfium()?;
        let doc = pdfium
            .load_pdf_from_file(&work_path, None)
            .map_err(|e| e.to_string())?;
        let indices: Vec<u16> = if all_pages {
            (0..doc.pages().len()).collect()
        } else {
            vec![page_index]
        };
        for i in indices {
            let mut page = doc.pages().get(i).map_err(|e| e.to_string())?;
            let page_w = page.width().value;
            let page_h = page.height().value;
            // rect en coords PDF de esta página, dentro de sus límites
            let x0 = rect.x.clamp(0.0, page_w - 1.0);
            let y_top = rect.y.clamp(0.0, page_h - 1.0);
            let w = rect.w.min(page_w - x0);
            let h = rect.h.min(page_h - y_top);
            let y0 = page_h - y_top - h; // borde inferior en coords PDF
            {
                let objects = page.objects_mut();
                for j in 0..objects.len() {
                    if let Ok(mut obj) = objects.get(j) {
                        obj.translate(PdfPoints::new(-x0), PdfPoints::new(-y0))
                            .map_err(|e| e.to_string())?;
                    }
                }
            }
            {
                let annotations = page.annotations_mut();
                for j in 0..annotations.len() {
                    let Ok(mut a) = annotations.get(j) else { continue };
                    if let Ok(b) = a.bounds() {
                        let _ = a.set_bounds(PdfRect::new(
                            PdfPoints::new(b.bottom().value - y0),
                            PdfPoints::new(b.left().value - x0),
                            PdfPoints::new(b.top().value - y0),
                            PdfPoints::new(b.right().value - x0),
                        ));
                    }
                    macro_rules! desplaza_quads {
                        ($m:expr) => {
                            if let Some(m) = $m {
                                let points = m.attachment_points_mut();
                                for k in 0..points.len() {
                                    if let Ok(q) = points.get(k) {
                                        let _ = points.set_attachment_point_at_index(
                                            k,
                                            PdfQuadPoints::new(
                                                PdfPoints::new(q.left().value - x0),
                                                PdfPoints::new(q.top().value - y0),
                                                PdfPoints::new(q.right().value - x0),
                                                PdfPoints::new(q.top().value - y0),
                                                PdfPoints::new(q.left().value - x0),
                                                PdfPoints::new(q.bottom().value - y0),
                                                PdfPoints::new(q.right().value - x0),
                                                PdfPoints::new(q.bottom().value - y0),
                                            ),
                                        );
                                    }
                                }
                            }
                        };
                    }
                    desplaza_quads!(a.as_highlight_annotation_mut());
                    desplaza_quads!(a.as_underline_annotation_mut());
                    desplaza_quads!(a.as_strikeout_annotation_mut());
                }
            }
            let caja = PdfRect::new(
                PdfPoints::new(0.0),
                PdfPoints::new(0.0),
                PdfPoints::new(h),
                PdfPoints::new(w),
            );
            page.boundaries_mut()
                .set_media(caja)
                .map_err(|e| e.to_string())?;
            page.boundaries_mut()
                .set_crop(caja)
                .map_err(|e| e.to_string())?;
            page.regenerate_content().map_err(|e| e.to_string())?;
        }
        save_and_close(doc, &work_path)?;
        Ok(())
    })
}

/// Ancho estimado de un texto en Helvetica Bold (media ~0.6 em por carácter).
fn ancho_estimado(text: &str, size: f32) -> f32 {
    text.chars().count() as f32 * size * 0.6
}

/// Marca de agua de texto en todas las páginas (diagonal ascendente u
/// horizontal, centrada). Va como contenido de página: no se puede quitar
/// selectivamente después de guardar.
#[tauri::command]
pub fn add_watermark(
    work_path: String,
    text: String,
    font_size: f32,
    color: [u8; 4],
    diagonal: bool,
) -> Result<(), String> {
    let text = text.trim().to_string();
    if text.is_empty() {
        return Err("La marca de agua está vacía".into());
    }
    on_pdfium_thread(move || {
        let pdfium = pdfium()?;
        let mut doc = pdfium
            .load_pdf_from_file(&work_path, None)
            .map_err(|e| e.to_string())?;
        let font = doc.fonts_mut().helvetica_bold();
        let size = font_size.clamp(12.0, 200.0);
        let c = PdfColor::new(color[0], color[1], color[2], color[3]);
        for i in 0..doc.pages().len() {
            let mut page = doc.pages().get(i).map_err(|e| e.to_string())?;
            let page_w = page.width().value;
            let page_h = page.height().value;
            let mut obj = PdfPageTextObject::new(&doc, &text, font, PdfPoints::new(size))
                .map_err(|e| e.to_string())?;
            obj.set_fill_color(c).map_err(|e| e.to_string())?;
            let w = ancho_estimado(&text, size);
            // centro del texto antes de transformar (baseline en el origen)
            let (cx, cy) = (w / 2.0, size * 0.35);
            let (cx2, cy2) = if diagonal {
                obj.rotate_counter_clockwise_degrees(45.0)
                    .map_err(|e| e.to_string())?;
                let r = std::f32::consts::FRAC_1_SQRT_2;
                (cx * r - cy * r, cx * r + cy * r)
            } else {
                (cx, cy)
            };
            obj.translate(
                PdfPoints::new(page_w / 2.0 - cx2),
                PdfPoints::new(page_h / 2.0 - cy2),
            )
            .map_err(|e| e.to_string())?;
            page.objects_mut()
                .add_text_object(obj)
                .map_err(|e| e.to_string())?;
            page.regenerate_content().map_err(|e| e.to_string())?;
        }
        save_and_close(doc, &work_path)?;
        Ok(())
    })
}

/// Encabezado y pie en todas las páginas, con tres huecos por zona
/// (izquierda/centro/derecha). Plantillas: `{n}` número de página, `{total}`
/// total, `{fecha}` fecha de hoy. Numerar páginas = pie centro con `{n}`.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn add_header_footer(
    work_path: String,
    header_left: Option<String>,
    header_center: Option<String>,
    header_right: Option<String>,
    footer_left: Option<String>,
    footer_center: Option<String>,
    footer_right: Option<String>,
    font_size: f32,
) -> Result<(), String> {
    let zonas = [
        &header_left,
        &header_center,
        &header_right,
        &footer_left,
        &footer_center,
        &footer_right,
    ];
    if zonas.iter().all(|z| match z {
        Some(s) => s.trim().is_empty(),
        None => true,
    }) {
        return Err("No hay ningún texto que añadir".into());
    }
    on_pdfium_thread(move || {
        let pdfium = pdfium()?;
        let mut doc = pdfium
            .load_pdf_from_file(&work_path, None)
            .map_err(|e| e.to_string())?;
        let font = doc.fonts_mut().helvetica();
        let size = font_size.clamp(6.0, 24.0);
        let total = doc.pages().len();
        let fecha = chrono::Local::now().format("%d/%m/%Y").to_string();
        const MARGEN_X: f32 = 36.0;
        for i in 0..total {
            let mut page = doc.pages().get(i).map_err(|e| e.to_string())?;
            let page_w = page.width().value;
            let page_h = page.height().value;
            let y_header = page_h - 28.0;
            let y_footer = 20.0;
            let piezas: [(&Option<String>, u8, f32); 6] = [
                (&header_left, 0, y_header),
                (&header_center, 1, y_header),
                (&header_right, 2, y_header),
                (&footer_left, 0, y_footer),
                (&footer_center, 1, y_footer),
                (&footer_right, 2, y_footer),
            ];
            for (texto, alineacion, y) in piezas {
                let Some(t) = texto else { continue };
                let t = t
                    .replace("{n}", &format!("{}", i + 1))
                    .replace("{total}", &format!("{total}"))
                    .replace("{fecha}", &fecha);
                if t.trim().is_empty() {
                    continue;
                }
                let mut obj = PdfPageTextObject::new(&doc, &t, font, PdfPoints::new(size))
                    .map_err(|e| e.to_string())?;
                obj.set_fill_color(PdfColor::new(60, 60, 60, 255))
                    .map_err(|e| e.to_string())?;
                let w = ancho_estimado(&t, size);
                let x = match alineacion {
                    0 => MARGEN_X,
                    1 => (page_w - w) / 2.0,
                    _ => page_w - MARGEN_X - w,
                };
                obj.translate(PdfPoints::new(x), PdfPoints::new(y))
                    .map_err(|e| e.to_string())?;
                page.objects_mut()
                    .add_text_object(obj)
                    .map_err(|e| e.to_string())?;
            }
            page.regenerate_content().map_err(|e| e.to_string())?;
        }
        save_and_close(doc, &work_path)?;
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::crea_pdf;

    fn textos(path: &str) -> Vec<String> {
        on_pdfium_thread({
            let path = path.to_string();
            move || {
                let pdfium = pdfium().expect("pdfium");
                let doc = pdfium.load_pdf_from_file(&path, None).expect("abrir");
                doc.pages()
                    .iter()
                    .map(|p| p.text().map(|t| t.all()).unwrap_or_default())
                    .collect()
            }
        })
    }

    #[test]
    fn pagina_en_blanco_duplicar_e_insertar() {
        let dir = std::env::temp_dir();
        let pdf = dir.join("paginas2-gestion-test.pdf");
        let otro = dir.join("paginas2-otro-test.pdf");
        crea_pdf(&["Uno", "Dos"], &pdf);
        crea_pdf(&["Extra"], &otro);
        let work = pdf.to_string_lossy().to_string();

        assert_eq!(add_blank_page(work.clone(), 1).expect("en blanco"), 3);
        // ahora: Uno, (blanco), Dos
        assert_eq!(textos(&work)[1].trim(), "");

        assert_eq!(duplicate_page(work.clone(), 0).expect("duplicar"), 4);
        // ahora: Uno, Uno, (blanco), Dos
        let t = textos(&work);
        assert!(t[0].contains("Uno") && t[1].contains("Uno"), "{t:?}");

        let total = insert_pdf_at(work.clone(), otro.to_string_lossy().to_string(), 1)
            .expect("insertar");
        assert_eq!(total, 5);
        // ahora: Uno, Extra, Uno, (blanco), Dos
        let t = textos(&work);
        assert!(t[1].contains("Extra"), "{t:?}");
        assert!(t[4].contains("Dos"), "{t:?}");
    }

    #[test]
    fn recorte_normaliza_el_origen() {
        let pdf = std::env::temp_dir().join("paginas2-crop-test.pdf");
        crea_pdf(&["Hola"], &pdf);
        let work = pdf.to_string_lossy().to_string();
        // el texto de crea_pdf está en (50, 700) coords PDF → UI y ≈ 92-106
        crop_page(
            work.clone(),
            0,
            Rect {
                x: 30.0,
                y: 60.0,
                w: 300.0,
                h: 120.0,
            },
            false,
        )
        .expect("recortar");
        let pt = crate::get_page_text(work.clone(), 0).expect("texto");
        assert!((pt.width - 300.0).abs() < 1.0, "ancho {}", pt.width);
        assert!((pt.height - 120.0).abs() < 1.0, "alto {}", pt.height);
        // el primer glifo se movió con el recorte: x ≈ 50-30 = 20
        let c = pt.chars.first().expect("glifos");
        assert!((c.x - 20.0).abs() < 3.0, "x del glifo {}", c.x);
        assert!(
            c.y > 0.0 && c.y < 120.0,
            "y del glifo fuera del área: {}",
            c.y
        );
        // y el render respeta el área nueva
        let png_b64 = crate::render_page(work, 0, 300).expect("render");
        assert!(!png_b64.is_empty());
    }

    #[test]
    fn marca_de_agua_y_numeracion() {
        let pdf = std::env::temp_dir().join("paginas2-marca-test.pdf");
        crea_pdf(&["Uno", "Dos"], &pdf);
        let work = pdf.to_string_lossy().to_string();
        add_watermark(
            work.clone(),
            "BORRADOR".into(),
            60.0,
            [200, 30, 30, 90],
            true,
        )
        .expect("marca de agua");
        add_header_footer(
            work.clone(),
            None,
            Some("Informe {fecha}".into()),
            None,
            None,
            Some("{n} / {total}".into()),
            None,
            10.0,
        )
        .expect("pie");
        let t = textos(&work);
        assert!(t[0].contains("BORRADOR"), "{:?}", t[0]);
        assert!(t[1].contains("BORRADOR"));
        assert!(t[0].contains("1 / 2"), "{:?}", t[0]);
        assert!(t[1].contains("2 / 2"));
        assert!(t[0].contains("Informe"));
    }
}
