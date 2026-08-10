//! Firma visual: estampar una imagen de firma manuscrita (PNG con
//! transparencia) en la página, y gestionar la biblioteca de firmas
//! guardadas en el directorio de datos de la app. La firma estampada es un
//! objeto de imagen normal del content stream: mover, redimensionar y borrar
//! ya funcionan con los comandos de imágenes existentes.

use crate::{on_pdfium_thread, pdfium, save_and_close};
use base64::Engine;
use pdfium_render::prelude::*;
use serde::Serialize;
use tauri::Manager;

#[derive(Serialize)]
pub struct FirmaGuardada {
    pub id: String,
    pub name: String,
    pub png_base64: String,
}

/// Estampa una imagen (base64, normalmente PNG con alfa) en la página con
/// los bounds dados (coords de UI: origen arriba-izquierda, puntos PDF).
#[tauri::command]
pub fn stamp_signature(
    work_path: String,
    page_index: u16,
    png_base64: String,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
) -> Result<(), String> {
    if w <= 1.0 || h <= 1.0 {
        return Err("Tamaño de firma inválido".into());
    }
    on_pdfium_thread(move || {
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(png_base64.trim())
            .map_err(|e| format!("Imagen base64 inválida: {e}"))?;
        let img = image::load_from_memory(&bytes)
            .map_err(|e| format!("No se pudo leer la imagen: {e}"))?;
        let pdfium = pdfium()?;
        let doc = pdfium
            .load_pdf_from_file(&work_path, None)
            .map_err(|e| e.to_string())?;
        let mut page = doc.pages().get(page_index).map_err(|e| e.to_string())?;
        let page_h = page.height().value;
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
    })
}

/// Directorio de firmas guardadas dentro del app data dir.
fn dir_de_firmas(app: &tauri::AppHandle) -> Result<std::path::PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("Sin directorio de datos: {e}"))?
        .join("firmas");
    std::fs::create_dir_all(&dir).map_err(|e| format!("No se pudo crear {}: {e}", dir.display()))?;
    Ok(dir)
}

/// Guarda `name.png` en `dir` con un id único; devuelve la firma guardada.
/// Separado del comando para poder testearlo sin AppHandle.
pub(crate) fn guardar_firma_en(
    dir: &std::path::Path,
    name: &str,
    png_base64: &str,
) -> Result<FirmaGuardada, String> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(png_base64.trim())
        .map_err(|e| format!("Imagen base64 inválida: {e}"))?;
    image::load_from_memory(&bytes).map_err(|e| format!("No es una imagen válida: {e}"))?;
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let id = format!("{nanos}");
    std::fs::write(dir.join(format!("{id}.png")), &bytes)
        .map_err(|e| format!("No se pudo guardar la firma: {e}"))?;
    let limpio = name.trim();
    let limpio = if limpio.is_empty() { "Firma" } else { limpio };
    std::fs::write(dir.join(format!("{id}.txt")), limpio)
        .map_err(|e| format!("No se pudo guardar el nombre: {e}"))?;
    Ok(FirmaGuardada {
        id,
        name: limpio.to_string(),
        png_base64: png_base64.trim().to_string(),
    })
}

/// Lista las firmas de `dir`, más reciente primero.
pub(crate) fn listar_firmas_en(dir: &std::path::Path) -> Result<Vec<FirmaGuardada>, String> {
    let mut out = Vec::new();
    let entries = std::fs::read_dir(dir).map_err(|e| e.to_string())?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("png") {
            continue;
        }
        let Some(id) = path.file_stem().and_then(|s| s.to_str()).map(String::from) else {
            continue;
        };
        let Ok(bytes) = std::fs::read(&path) else { continue };
        let name = std::fs::read_to_string(dir.join(format!("{id}.txt")))
            .unwrap_or_else(|_| "Firma".into());
        out.push(FirmaGuardada {
            id,
            name: name.trim().to_string(),
            png_base64: base64::engine::general_purpose::STANDARD.encode(&bytes),
        });
    }
    out.sort_by(|a, b| b.id.cmp(&a.id));
    Ok(out)
}

/// Importa un fichero de imagen como firma guardada (el diálogo de abrir
/// devuelve una ruta; la lectura se hace aquí, sin plugin fs). Se re-codifica
/// a PNG para conservar la transparencia con un formato único.
#[tauri::command]
pub fn import_signature_file(
    app: tauri::AppHandle,
    image_path: String,
) -> Result<FirmaGuardada, String> {
    let img = image::open(&image_path).map_err(|e| format!("No se pudo leer la imagen: {e}"))?;
    let mut buf = std::io::Cursor::new(Vec::new());
    img.write_to(&mut buf, image::ImageFormat::Png)
        .map_err(|e| format!("No se pudo convertir la imagen: {e}"))?;
    let png_base64 = base64::engine::general_purpose::STANDARD.encode(buf.into_inner());
    let name = std::path::Path::new(&image_path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Firma")
        .to_string();
    guardar_firma_en(&dir_de_firmas(&app)?, &name, &png_base64)
}

/// Guarda una firma reutilizable en la biblioteca del usuario.
#[tauri::command]
pub fn save_stored_signature(
    app: tauri::AppHandle,
    name: String,
    png_base64: String,
) -> Result<FirmaGuardada, String> {
    guardar_firma_en(&dir_de_firmas(&app)?, &name, &png_base64)
}

/// Lista las firmas guardadas (con su PNG en base64 para las miniaturas).
#[tauri::command]
pub fn list_stored_signatures(app: tauri::AppHandle) -> Result<Vec<FirmaGuardada>, String> {
    listar_firmas_en(&dir_de_firmas(&app)?)
}

/// Borra una firma guardada de la biblioteca.
#[tauri::command]
pub fn delete_stored_signature(app: tauri::AppHandle, id: String) -> Result<(), String> {
    if id.contains(['/', '\\', '.']) {
        return Err("Id de firma inválido".into());
    }
    let dir = dir_de_firmas(&app)?;
    std::fs::remove_file(dir.join(format!("{id}.png")))
        .map_err(|e| format!("No se pudo borrar la firma: {e}"))?;
    let _ = std::fs::remove_file(dir.join(format!("{id}.txt")));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::crea_pdf;

    /// PNG 4x4 con la mitad izquierda roja opaca y la derecha transparente.
    fn png_con_alfa_base64() -> String {
        let mut img = image::RgbaImage::new(4, 4);
        for (x, _y, p) in img.enumerate_pixels_mut() {
            *p = if x < 2 {
                image::Rgba([200, 0, 0, 255])
            } else {
                image::Rgba([0, 0, 0, 0])
            };
        }
        let mut buf = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(img)
            .write_to(&mut buf, image::ImageFormat::Png)
            .expect("codificar png");
        base64::engine::general_purpose::STANDARD.encode(buf.into_inner())
    }

    #[test]
    fn estampa_firma_con_bounds_y_alfa() {
        let dir = std::env::temp_dir();
        let pdf = dir.join("firma-visual-test.pdf");
        crea_pdf(&["Documento a firmar"], &pdf);
        let work = pdf.to_string_lossy().to_string();

        stamp_signature(work.clone(), 0, png_con_alfa_base64(), 100.0, 500.0, 180.0, 60.0)
            .expect("estampar firma");

        // la firma aparece como imagen con los bounds pedidos
        let imgs = crate::get_images(work.clone(), 0).expect("listar imágenes");
        assert_eq!(imgs.len(), 1);
        let img = &imgs[0];
        assert!((img.x - 100.0).abs() < 1.0, "x = {}", img.x);
        assert!((img.y - 500.0).abs() < 1.0, "y = {}", img.y);
        assert!((img.w - 180.0).abs() < 1.0, "w = {}", img.w);
        assert!((img.h - 60.0).abs() < 1.0, "h = {}", img.h);

        // el render compone el alfa: la mitad izquierda pinta rojo, la
        // derecha deja el fondo blanco
        let png_b64 = crate::render_page(work, 0, 600).expect("render");
        let png = base64::engine::general_purpose::STANDARD
            .decode(png_b64)
            .expect("decodificar render");
        let rendered = image::load_from_memory(&png).expect("leer render").to_rgba8();
        let escala = 600.0 / 595.28; // página A4 de crea_pdf: 595.28 pt de ancho
        let alto = rendered.height() as f32;
        let py = |y_ui: f32| ((y_ui * escala).min(alto - 1.0)) as u32;
        let izquierda = rendered.get_pixel((145.0 * escala) as u32, py(530.0));
        let derecha = rendered.get_pixel((235.0 * escala) as u32, py(530.0));
        assert!(
            izquierda[0] > 150 && izquierda[1] < 100,
            "esperaba rojo, hay {izquierda:?}"
        );
        assert!(
            derecha[0] > 200 && derecha[1] > 200 && derecha[2] > 200,
            "esperaba fondo blanco, hay {derecha:?}"
        );
    }

    #[test]
    fn biblioteca_de_firmas_guarda_lista_y_borra() {
        let dir = std::env::temp_dir().join(format!(
            "firmas-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();

        let png = png_con_alfa_base64();
        let guardada = guardar_firma_en(&dir, "  Mi firma  ", &png).expect("guardar");
        assert_eq!(guardada.name, "Mi firma");

        let lista = listar_firmas_en(&dir).expect("listar");
        assert_eq!(lista.len(), 1);
        assert_eq!(lista[0].id, guardada.id);
        assert_eq!(lista[0].name, "Mi firma");
        assert_eq!(lista[0].png_base64, png);

        std::fs::remove_file(dir.join(format!("{}.png", guardada.id))).unwrap();
        assert!(listar_firmas_en(&dir).expect("listar").is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
