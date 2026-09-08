//! Imágenes: listar, insertar, mover/redimensionar, reemplazar, borrar y
//! extraer el contenido de un objeto de imagen.

use crate::{on_pdfium_thread, pdfium, save_and_close, with_doc};
use crate::historial::mutacion;
use serde::Serialize;
use base64::Engine;
use pdfium_render::prelude::*;

/// Contenido de un objeto de imagen como PNG en base64 (con máscaras y
/// transparencia aplicadas). La UI lo usa como vista previa al arrastrar.
#[tauri::command(async)]
pub fn get_image_data(path: String, page_index: u16, object_index: u32) -> Result<String, String> {
    on_pdfium_thread(move || {
        with_doc(&path, |doc| {
            let page = doc.pages().get(page_index).map_err(|e| e.to_string())?;
            let obj = page
                .objects()
                .get(object_index as usize)
                .map_err(|e| e.to_string())?;
            let img_obj = obj.as_image_object().ok_or("No es una imagen")?;
            let img = img_obj
                .get_processed_image(doc)
                .map_err(|e| e.to_string())?;
            let mut buf = std::io::Cursor::new(Vec::new());
            img.write_to(&mut buf, image::ImageFormat::Png)
                .map_err(|e| format!("No se pudo codificar la imagen: {e}"))?;
            Ok(base64::engine::general_purpose::STANDARD.encode(buf.into_inner()))
        })
    })
}

#[derive(Serialize)]
pub struct ImageInfo {
    pub object_index: u32,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// Lista los objetos de imagen de una página (bounds en coords de UI).
#[tauri::command(async)]
pub fn get_images(path: String, page_index: u16) -> Result<Vec<ImageInfo>, String> {
    on_pdfium_thread(move || {
        with_doc(&path, |doc| {
            let page = doc.pages().get(page_index).map_err(|e| e.to_string())?;
            let page_h = page.height().value;
            let objects = page.objects();
            let mut out = Vec::new();
            for i in 0..objects.len() {
                let Ok(obj) = objects.get(i) else { continue };
                if obj.as_image_object().is_none() {
                    continue;
                }
                let Ok(b) = obj.bounds() else { continue };
                out.push(ImageInfo {
                    object_index: i as u32,
                    x: b.left().value,
                    y: page_h - b.top().value,
                    w: b.right().value - b.left().value,
                    h: b.top().value - b.bottom().value,
                });
            }
            Ok(out)
        })
    })
}

/// Inserta una imagen (png/jpg/webp…) con su tamaño natural a 72 dpi,
/// limitado a caber en la página. El punto dado (coords de UI) es la esquina
/// superior izquierda.
#[tauri::command(async)]
pub fn add_image(
    work_path: String,
    page_index: u16,
    image_path: String,
    x: f32,
    y: f32,
) -> Result<(), String> {
    mutacion(work_path, |work_path| on_pdfium_thread(move || {
        let img =
            image::open(&image_path).map_err(|e| format!("No se pudo leer la imagen: {e}"))?;
        let pdfium = pdfium()?;
        let doc = pdfium
            .load_pdf_from_file(&work_path, None)
            .map_err(|e| e.to_string())?;
        let mut page = doc.pages().get(page_index).map_err(|e| e.to_string())?;
        let page_w = page.width().value;
        let page_h = page.height().value;
        let mut w = img.width() as f32;
        let mut h = img.height() as f32;
        let max_w = page_w * 0.6;
        if w > max_w {
            let f = max_w / w;
            w *= f;
            h *= f;
        }
        if h > page_h * 0.8 {
            let f = page_h * 0.8 / h;
            w *= f;
            h *= f;
        }
        let mut obj =
            PdfPageImageObject::new_with_size(&doc, &img, PdfPoints::new(w), PdfPoints::new(h))
                .map_err(|e| e.to_string())?;
        obj.translate(PdfPoints::new(x), PdfPoints::new(page_h - y - h))
            .map_err(|e| e.to_string())?;
        page.objects_mut()
            .add_image_object(obj)
            .map_err(|e| e.to_string())?;
        page.regenerate_content().map_err(|e| e.to_string())?;
        drop(page);
        save_and_close(doc, &work_path)?;
        Ok(())
    }))
}

/// Mueve y/o redimensiona una imagen a los bounds dados (coords de UI).
/// Válido para imágenes sin rotación.
#[tauri::command(async)]
pub fn transform_image(
    work_path: String,
    page_index: u16,
    object_index: u32,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
) -> Result<(), String> {
    if w <= 1.0 || h <= 1.0 {
        return Err("Tamaño de imagen inválido".into());
    }
    mutacion(work_path, |work_path| on_pdfium_thread(move || {
        let pdfium = pdfium()?;
        let doc = pdfium
            .load_pdf_from_file(&work_path, None)
            .map_err(|e| e.to_string())?;
        let mut page = doc.pages().get(page_index).map_err(|e| e.to_string())?;
        let page_h = page.height().value;
        let mut obj = page
            .objects_mut()
            .get(object_index as usize)
            .map_err(|e| e.to_string())?;
        if obj.as_image_object().is_none() {
            return Err("No es una imagen".into());
        }
        let b = obj.bounds().map_err(|e| e.to_string())?;
        let old_w = b.right().value - b.left().value;
        let old_h = b.top().value - b.bottom().value;
        if old_w > 0.0 && old_h > 0.0 {
            obj.scale(w / old_w, h / old_h).map_err(|e| e.to_string())?;
        }
        let b2 = obj.bounds().map_err(|e| e.to_string())?;
        let dx = x - b2.left().value;
        let dy = (page_h - y - h) - b2.bottom().value;
        obj.translate(PdfPoints::new(dx), PdfPoints::new(dy))
            .map_err(|e| e.to_string())?;
        drop(obj);
        page.regenerate_content().map_err(|e| e.to_string())?;
        drop(page);
        save_and_close(doc, &work_path)?;
        Ok(())
    }))
}

/// Reemplaza el contenido de una imagen manteniendo posición y tamaño.
#[tauri::command(async)]
pub fn replace_image(
    work_path: String,
    page_index: u16,
    object_index: u32,
    image_path: String,
) -> Result<(), String> {
    mutacion(work_path, |work_path| on_pdfium_thread(move || {
        let img =
            image::open(&image_path).map_err(|e| format!("No se pudo leer la imagen: {e}"))?;
        let pdfium = pdfium()?;
        let doc = pdfium
            .load_pdf_from_file(&work_path, None)
            .map_err(|e| e.to_string())?;
        let mut page = doc.pages().get(page_index).map_err(|e| e.to_string())?;
        let (left, bottom, w, h) = {
            let obj = page
                .objects()
                .get(object_index as usize)
                .map_err(|e| e.to_string())?;
            if obj.as_image_object().is_none() {
                return Err("No es una imagen".into());
            }
            let b = obj.bounds().map_err(|e| e.to_string())?;
            (
                b.left().value,
                b.bottom().value,
                b.right().value - b.left().value,
                b.top().value - b.bottom().value,
            )
        };
        let removed = page
            .objects_mut()
            .remove_object_at_index(object_index as usize)
            .map_err(|e| e.to_string())?;
        // ver nota en delete_text_block: soltar el objeto extraído casca
        std::mem::forget(removed);
        let mut obj =
            PdfPageImageObject::new_with_size(&doc, &img, PdfPoints::new(w), PdfPoints::new(h))
                .map_err(|e| e.to_string())?;
        obj.translate(PdfPoints::new(left), PdfPoints::new(bottom))
            .map_err(|e| e.to_string())?;
        page.objects_mut()
            .add_image_object(obj)
            .map_err(|e| e.to_string())?;
        page.regenerate_content().map_err(|e| e.to_string())?;
        drop(page);
        save_and_close(doc, &work_path)?;
        Ok(())
    }))
}

/// Elimina una imagen de la página.
#[tauri::command(async)]
pub fn delete_image(work_path: String, page_index: u16, object_index: u32) -> Result<(), String> {
    mutacion(work_path, |work_path| on_pdfium_thread(move || {
        let pdfium = pdfium()?;
        let doc = pdfium
            .load_pdf_from_file(&work_path, None)
            .map_err(|e| e.to_string())?;
        let mut page = doc.pages().get(page_index).map_err(|e| e.to_string())?;
        {
            let obj = page
                .objects()
                .get(object_index as usize)
                .map_err(|e| e.to_string())?;
            if obj.as_image_object().is_none() {
                return Err("No es una imagen".into());
            }
        }
        let removed = page
            .objects_mut()
            .remove_object_at_index(object_index as usize)
            .map_err(|e| e.to_string())?;
        // ver nota en delete_text_block: soltar el objeto extraído casca
        std::mem::forget(removed);
        page.regenerate_content().map_err(|e| e.to_string())?;
        drop(page);
        save_and_close(doc, &work_path)?;
        Ok(())
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::crea_pdf;

    #[test]
    fn extrae_contenido_de_imagen_estampada() {
        let dir = std::env::temp_dir();
        let pdf = dir.join("imagenes-get-data-test.pdf");
        crea_pdf(&["Página con imagen"], &pdf);
        let work = pdf.to_string_lossy().to_string();

        // PNG 4x4 rojo opaco
        let mut img = image::RgbaImage::new(4, 4);
        for (_, _, p) in img.enumerate_pixels_mut() {
            *p = image::Rgba([210, 10, 10, 255]);
        }
        let mut buf = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(img)
            .write_to(&mut buf, image::ImageFormat::Png)
            .expect("codificar png");
        let png_b64 = base64::engine::general_purpose::STANDARD.encode(buf.into_inner());
        crate::firmas_visuales::stamp_signature(work.clone(), 0, png_b64, 50.0, 50.0, 80.0, 80.0)
            .expect("estampar");

        let imgs = get_images(work.clone(), 0).expect("listar imágenes");
        assert_eq!(imgs.len(), 1);
        let b64 = get_image_data(work, 0, imgs[0].object_index).expect("extraer contenido");
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(b64)
            .expect("base64");
        let out = image::load_from_memory(&bytes).expect("PNG válido").to_rgba8();
        let p = out.get_pixel(out.width() / 2, out.height() / 2);
        assert!(p[0] > 150 && p[1] < 100, "esperaba rojo, hay {p:?}");
    }

    #[allow(unused_imports)]
    use crate::{render_page_b64, tests::textos_de};

    #[test]
    fn imagenes() {
        let dir = std::env::temp_dir();
        let tmp = dir.join("editor_pdf_test_imagenes.pdf");
        let png = dir.join("editor_pdf_test_imagen.png");
        let png2 = dir.join("editor_pdf_test_imagen2.png");
        crea_pdf(&["Con imagen"], &tmp);
        let work = tmp.to_string_lossy().into_owned();

        // dos PNGs pequeños de colores distintos
        image::RgbaImage::from_pixel(80, 40, image::Rgba([200, 30, 30, 255]))
            .save(&png)
            .expect("crear png");
        image::RgbaImage::from_pixel(40, 40, image::Rgba([30, 30, 200, 255]))
            .save(&png2)
            .expect("crear png2");

        // insertar en (100, 200): tamaño natural 80x40 pt
        add_image(
            work.clone(),
            0,
            png.to_string_lossy().into_owned(),
            100.0,
            200.0,
        )
        .expect("añadir imagen");
        let imgs = get_images(work.clone(), 0).expect("listar imágenes");
        assert_eq!(imgs.len(), 1, "imágenes: {}", imgs.len());
        let im = &imgs[0];
        assert!(
            (im.x - 100.0).abs() < 2.0 && (im.y - 200.0).abs() < 2.0,
            "posición: ({}, {})",
            im.x,
            im.y
        );
        assert!(
            (im.w - 80.0).abs() < 2.0 && (im.h - 40.0).abs() < 2.0,
            "tamaño: {}x{}",
            im.w,
            im.h
        );

        // mover y redimensionar
        transform_image(work.clone(), 0, im.object_index, 50.0, 300.0, 160.0, 80.0)
            .expect("transformar");
        let imgs = get_images(work.clone(), 0).expect("relistar");
        let im = &imgs[0];
        assert!(
            (im.x - 50.0).abs() < 2.0 && (im.y - 300.0).abs() < 2.0,
            "posición tras mover: ({}, {})",
            im.x,
            im.y
        );
        assert!(
            (im.w - 160.0).abs() < 2.0 && (im.h - 80.0).abs() < 2.0,
            "tamaño tras redimensionar: {}x{}",
            im.w,
            im.h
        );

        // reemplazar manteniendo bounds
        replace_image(
            work.clone(),
            0,
            im.object_index,
            png2.to_string_lossy().into_owned(),
        )
        .expect("reemplazar");
        let imgs = get_images(work.clone(), 0).expect("listar tras reemplazo");
        assert_eq!(imgs.len(), 1);
        assert!(
            (imgs[0].w - 160.0).abs() < 2.0 && (imgs[0].h - 80.0).abs() < 2.0,
            "bounds tras reemplazo: {}x{}",
            imgs[0].w,
            imgs[0].h
        );

        // eliminar
        delete_image(work.clone(), 0, imgs[0].object_index).expect("eliminar");
        assert!(get_images(work.clone(), 0)
            .expect("listar final")
            .is_empty());

        render_page_b64(work.clone(), 0, 200).expect("render tras imágenes");
        for f in [&tmp, &png, &png2] {
            std::fs::remove_file(f).ok();
        }
    }
}
