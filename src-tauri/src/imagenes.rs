//! Comandos extra de imágenes que no están en lib.rs.

use crate::{on_pdfium_thread, with_doc};
use base64::Engine;
use pdfium_render::prelude::*;

/// Contenido de un objeto de imagen como PNG en base64 (con máscaras y
/// transparencia aplicadas). La UI lo usa como vista previa al arrastrar.
#[tauri::command]
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

        let imgs = crate::get_images(work.clone(), 0).expect("listar imágenes");
        assert_eq!(imgs.len(), 1);
        let b64 = get_image_data(work, 0, imgs[0].object_index).expect("extraer contenido");
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(b64)
            .expect("base64");
        let out = image::load_from_memory(&bytes).expect("PNG válido").to_rgba8();
        let p = out.get_pixel(out.width() / 2, out.height() / 2);
        assert!(p[0] > 150 && p[1] < 100, "esperaba rojo, hay {p:?}");
    }
}
