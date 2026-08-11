//! Salida: exportar páginas como imágenes, extraer el texto plano y
//! comprimir el documento recomprimiendo sus imágenes.

use crate::{on_pdfium_thread, pdfium, save_and_close, with_doc};
use pdfium_render::prelude::*;
use serde::Serialize;
use std::io::Cursor;

/// Exporta todas las páginas como PNG o JPEG al directorio dado, a la
/// resolución pedida. Devuelve las rutas escritas.
#[tauri::command]
pub fn export_pages_png(
    path: String,
    dest_dir: String,
    dpi: u16,
    format: String,
) -> Result<Vec<String>, String> {
    let dpi = dpi.clamp(72, 600) as f32;
    on_pdfium_thread(move || {
        with_doc(&path, |doc| {
            let dir = std::path::Path::new(&dest_dir);
            let (ext, fmt) = match format.as_str() {
                "jpeg" | "jpg" => ("jpg", image::ImageFormat::Jpeg),
                _ => ("png", image::ImageFormat::Png),
            };
            let mut out = Vec::new();
            let total = doc.pages().len();
            for i in 0..total {
                let page = doc.pages().get(i).map_err(|e| e.to_string())?;
                let width = (page.width().value / 72.0 * dpi).round() as i32;
                let bitmap = page
                    .render_with_config(
                        &PdfRenderConfig::new()
                            .set_target_width(width)
                            .render_form_data(true)
                            .render_annotations(true),
                    )
                    .map_err(|e| e.to_string())?;
                let dest = dir.join(format!("pagina-{:03}.{ext}", i + 1));
                let img = bitmap.as_image();
                // JPEG no admite alfa
                let img = if fmt == image::ImageFormat::Jpeg {
                    image::DynamicImage::ImageRgb8(img.to_rgb8())
                } else {
                    img
                };
                img.save_with_format(&dest, fmt)
                    .map_err(|e| format!("No se pudo escribir {}: {e}", dest.display()))?;
                out.push(dest.to_string_lossy().into_owned());
            }
            Ok(out)
        })
    })
}

/// Vuelca el texto de todas las páginas a un fichero de texto plano.
#[tauri::command]
pub fn export_text(path: String, dest_path: String) -> Result<(), String> {
    on_pdfium_thread(move || {
        with_doc(&path, |doc| {
            let mut out = String::new();
            let total = doc.pages().len();
            for i in 0..total {
                let page = doc.pages().get(i).map_err(|e| e.to_string())?;
                if i > 0 {
                    out.push_str("\n\n");
                }
                out.push_str(&page.text().map(|t| t.all()).unwrap_or_default());
            }
            std::fs::write(&dest_path, out).map_err(|e| format!("No se pudo escribir: {e}"))
        })
    })
}

#[derive(Serialize)]
pub struct CompressReport {
    pub antes: u64,
    pub despues: u64,
    pub imagenes: u32,
}

/// Comprime el documento recomprimiendo sus imágenes a JPEG con la calidad
/// dada y submuestreando las que superen `max_dpi` respecto a su tamaño en
/// página. Se saltan las imágenes con transparencia (JPEG la perdería) y las
/// rotadas (la reinserción solo maneja imágenes sin rotar).
#[tauri::command]
pub fn compress_pdf(work_path: String, quality: u8, max_dpi: u16) -> Result<CompressReport, String> {
    let quality = quality.clamp(30, 95);
    let max_dpi = max_dpi.clamp(72, 600) as f32;
    let antes = std::fs::metadata(&work_path)
        .map(|m| m.len())
        .unwrap_or(0);
    on_pdfium_thread(move || {
        let pdfium = pdfium()?;
        let doc = pdfium
            .load_pdf_from_file(&work_path, None)
            .map_err(|e| e.to_string())?;
        let mut recomprimidas = 0u32;
        for p in 0..doc.pages().len() {
            let mut page = doc.pages().get(p).map_err(|e| e.to_string())?;
            // recopilar candidatas primero: índice, bounds y píxeles
            struct Candidata {
                index: usize,
                left: f32,
                bottom: f32,
                w: f32,
                h: f32,
                jpeg: Vec<u8>,
            }
            let mut candidatas = Vec::new();
            {
                let objects = page.objects();
                for i in 0..objects.len() {
                    let Ok(obj) = objects.get(i) else { continue };
                    let Some(img_obj) = obj.as_image_object() else {
                        continue;
                    };
                    let Ok(m) = img_obj.matrix() else { continue };
                    // saltar imágenes rotadas o sesgadas
                    if m.b().abs() > 0.01 || m.c().abs() > 0.01 {
                        continue;
                    }
                    let Ok(b) = obj.bounds() else { continue };
                    let w_pts = b.right().value - b.left().value;
                    let h_pts = b.top().value - b.bottom().value;
                    if w_pts < 4.0 || h_pts < 4.0 {
                        continue;
                    }
                    let Ok(raw) = img_obj.get_raw_image() else {
                        continue;
                    };
                    let rgba = raw.to_rgba8();
                    if rgba.pixels().any(|px| px[3] < 250) {
                        continue; // transparencia: JPEG la perdería
                    }
                    let dpi_efectivo = rgba.width() as f32 / (w_pts / 72.0);
                    let objetivo_px = (w_pts / 72.0 * max_dpi).round().max(16.0) as u32;
                    let img = if dpi_efectivo > max_dpi && objetivo_px < rgba.width() {
                        image::DynamicImage::ImageRgba8(rgba).resize(
                            objetivo_px,
                            u32::MAX,
                            image::imageops::FilterType::Lanczos3,
                        )
                    } else {
                        image::DynamicImage::ImageRgba8(rgba)
                    };
                    let mut jpeg = Vec::new();
                    let mut cursor = Cursor::new(&mut jpeg);
                    let mut enc = image::codecs::jpeg::JpegEncoder::new_with_quality(
                        &mut cursor,
                        quality,
                    );
                    if enc
                        .encode_image(&image::DynamicImage::ImageRgb8(img.to_rgb8()))
                        .is_err()
                    {
                        continue;
                    }
                    drop(enc);
                    drop(cursor);
                    candidatas.push(Candidata {
                        index: i,
                        left: b.left().value,
                        bottom: b.bottom().value,
                        w: w_pts,
                        h: h_pts,
                        jpeg,
                    });
                }
            }
            // reemplazar de atrás hacia delante para no desplazar índices
            for c in candidatas.iter().rev() {
                let removed = page
                    .objects_mut()
                    .remove_object_at_index(c.index)
                    .map_err(|e| e.to_string())?;
                // regla del proyecto: no soltar el objeto extraído
                std::mem::forget(removed);
                let mut obj =
                    PdfPageImageObject::new_from_jpeg_reader(&doc, Cursor::new(c.jpeg.clone()))
                        .map_err(|e| e.to_string())?;
                // el objeto nace de 1x1 pt: escalar a su tamaño y colocar
                obj.scale(c.w, c.h).map_err(|e| e.to_string())?;
                obj.translate(PdfPoints::new(c.left), PdfPoints::new(c.bottom))
                    .map_err(|e| e.to_string())?;
                page.objects_mut()
                    .add_image_object(obj)
                    .map_err(|e| e.to_string())?;
                recomprimidas += 1;
            }
            if !candidatas.is_empty() {
                page.regenerate_content().map_err(|e| e.to_string())?;
            }
        }
        save_and_close(doc, &work_path)?;
        let despues = std::fs::metadata(&work_path)
            .map(|m| m.len())
            .unwrap_or(0);
        Ok(CompressReport {
            antes,
            despues,
            imagenes: recomprimidas,
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::crea_pdf;
    use base64::Engine;

    #[test]
    fn exporta_paginas_y_texto() {
        let dir = std::env::temp_dir().join(format!(
            "exportar-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let pdf = dir.join("doc.pdf");
        crea_pdf(&["Uno", "Dos"], &pdf);
        let rutas = export_pages_png(
            pdf.to_string_lossy().to_string(),
            dir.to_string_lossy().to_string(),
            96,
            "png".into(),
        )
        .expect("exportar imágenes");
        assert_eq!(rutas.len(), 2);
        for r in &rutas {
            let img = image::open(r).expect("PNG legible");
            assert!(img.width() > 500);
        }
        let txt = dir.join("doc.txt");
        export_text(
            pdf.to_string_lossy().to_string(),
            txt.to_string_lossy().to_string(),
        )
        .expect("exportar texto");
        let contenido = std::fs::read_to_string(&txt).unwrap();
        assert!(contenido.contains("Uno") && contenido.contains("Dos"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn comprimir_reduce_una_imagen_grande() {
        let dir = std::env::temp_dir();
        let pdf = dir.join("exportar-compress-test.pdf");
        crea_pdf(&["Con foto"], &pdf);
        let work = pdf.to_string_lossy().to_string();
        // foto sintética 1600x1200 con ruido pseudoaleatorio por píxel
        // (incompresible con Flate; el submuestreo + JPEG sí la reduce)
        let mut img = image::RgbaImage::new(1600, 1200);
        for (x, y, p) in img.enumerate_pixels_mut() {
            let h = x
                .wrapping_mul(2654435761)
                .wrapping_add(y.wrapping_mul(2246822519))
                .rotate_left(13)
                .wrapping_mul(2654435761);
            *p = image::Rgba([h as u8, (h >> 8) as u8, (h >> 16) as u8, 255]);
        }
        let mut buf = Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(img)
            .write_to(&mut buf, image::ImageFormat::Png)
            .unwrap();
        let b64 = base64::engine::general_purpose::STANDARD.encode(buf.into_inner());
        // estampada a 300x225 pt → dpi efectivo ~384: debe submuestrear
        crate::firmas_visuales::stamp_signature(work.clone(), 0, b64, 50.0, 200.0, 300.0, 225.0)
            .expect("insertar imagen");

        let informe = compress_pdf(work.clone(), 70, 150).expect("comprimir");
        assert_eq!(informe.imagenes, 1);
        assert!(
            informe.despues < informe.antes / 2,
            "apenas comprime: {} → {}",
            informe.antes,
            informe.despues
        );
        // la imagen sigue ahí con sus bounds
        let imgs = crate::get_images(work, 0).expect("listar");
        assert_eq!(imgs.len(), 1);
        assert!((imgs[0].w - 300.0).abs() < 2.0);
    }
}
