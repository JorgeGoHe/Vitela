//! Firma visual: estampar una imagen de firma manuscrita (PNG con
//! transparencia) en la página, y gestionar la biblioteca de firmas
//! guardadas en el directorio de datos de la app. La firma estampada es un
//! objeto de imagen normal del content stream: mover, redimensionar y borrar
//! ya funcionan con los comandos de imágenes existentes.

use crate::{on_pdfium_thread, pdfium, save_and_close};
use crate::historial::mutacion;
use base64::Engine;
use pdfium_render::prelude::*;
use serde::Serialize;

#[derive(Serialize, Debug)]
pub struct FirmaGuardada {
    pub id: String,
    pub name: String,
    pub png_base64: String,
    /// **Para qué es esta imagen** (R58): `"firma"`, `"iniciales"` o
    /// `"sello"`. Hasta el ciclo 8 la distinción vivía en el
    /// `localStorage` de la interfaz, porque el backend guardaba imágenes
    /// por nombre y no sabía de ranuras: limpiar el almacenamiento del
    /// navegador embebido dejaba las imágenes puestas y la ranura perdida.
    /// Ahora es un campo de la biblioteca, que es donde vive.
    pub ranura: String,
}

/// Las tres ranuras de la biblioteca. La de por defecto es la firma, que
/// es lo que había antes de que hubiera ranuras: una imagen guardada por
/// una versión anterior sigue siendo una firma.
pub(crate) const RANURAS: [&str; 3] = ["firma", "iniciales", "sello"];
pub(crate) const RANURA_DEFECTO: &str = "firma";

/// Valida la ranura que llega de la UI. Una ranura que no existe es un
/// error en llano, no una imagen guardada donde nadie la va a buscar.
fn ranura_valida(ranura: Option<&str>) -> Result<String, String> {
    let r = ranura.map(str::trim).filter(|r| !r.is_empty());
    match r {
        None => Ok(RANURA_DEFECTO.to_string()),
        Some(r) if RANURAS.contains(&r) => Ok(r.to_string()),
        Some(otra) => Err(format!(
            "«{otra}» no es una ranura de la biblioteca: son firma, iniciales o sello"
        )),
    }
}

/// La ranura guardada de una imagen, o la de por defecto.
fn ranura_de(dir: &std::path::Path, id: &str) -> String {
    std::fs::read_to_string(dir.join(format!("{id}.ranura")))
        .map(|s| s.trim().to_string())
        .ok()
        .filter(|s| RANURAS.contains(&s.as_str()))
        .unwrap_or_else(|| RANURA_DEFECTO.to_string())
}

/// Estampa una imagen (base64, normalmente PNG con alfa) en la página con
/// los bounds dados (coords de UI: origen arriba-izquierda, puntos PDF).
#[tauri::command(async)]
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
    mutacion(work_path, |work_path| on_pdfium_thread(move || {
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(png_base64.trim())
            .map_err(|e| format!("Imagen base64 inválida: {e}"))?;
        let img = image::load_from_memory(&bytes)
            .map_err(|e| format!("No se ha podido leer la imagen: {e}"))?;
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
    }))
}

/// Directorio de datos de la app. Lo fija el setup de Tauri (app_data_dir)
/// o el puente de QA (temp). Así los comandos no necesitan AppHandle y el
/// puente de desarrollo puede llamarlos igual que Tauri.
pub(crate) static DIR_DATOS: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();

/// Migración del renombre a Vitela: el identifier pasó de
/// com.jorge.editorpdf a com.jorge.vitela y con él cambió app_data_dir.
/// Si el directorio antiguo tiene firmas y el nuevo aún no, se copian
/// (el antiguo se deja intacto por si hay que volver atrás).
pub(crate) fn migrar_datos_antiguos(nuevo: &std::path::Path) {
    let Some(padre) = nuevo.parent() else { return };
    let viejo = padre.join("com.jorge.editorpdf").join("firmas");
    let destino = nuevo.join("firmas");
    if !viejo.is_dir() || destino.exists() {
        return;
    }
    if std::fs::create_dir_all(&destino).is_err() {
        return;
    }
    if let Ok(entradas) = std::fs::read_dir(&viejo) {
        for e in entradas.flatten() {
            let _ = std::fs::copy(e.path(), destino.join(e.file_name()));
        }
    }
}

/// Directorio de firmas guardadas dentro del directorio de datos.
fn dir_de_firmas() -> Result<std::path::PathBuf, String> {
    let dir = DIR_DATOS
        .get()
        .ok_or("Directorio de datos no inicializado")?
        .join("firmas");
    std::fs::create_dir_all(&dir).map_err(|e| {
        crate::mensaje_llano(format!("No se ha podido crear {}: {e}", dir.display()))
    })?;
    Ok(dir)
}

/// Guarda `name.png` en `dir` con un id único; devuelve la firma guardada.
/// Separado del comando para poder testearlo sin AppHandle.
pub(crate) fn guardar_firma_en(
    dir: &std::path::Path,
    name: &str,
    png_base64: &str,
    ranura: Option<&str>,
) -> Result<FirmaGuardada, String> {
    let ranura = ranura_valida(ranura)?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(png_base64.trim())
        .map_err(|e| format!("Imagen base64 inválida: {e}"))?;
    image::load_from_memory(&bytes).map_err(|e| format!("No es una imagen válida: {e}"))?;
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let id = format!("{nanos}");
    std::fs::write(dir.join(format!("{id}.png")), &bytes).map_err(|e| {
        crate::mensaje_llano(format!("No se ha podido guardar la firma: {e}"))
    })?;
    let limpio = name.trim();
    let limpio = if limpio.is_empty() { "Firma" } else { limpio };
    std::fs::write(dir.join(format!("{id}.txt")), limpio).map_err(|e| {
        crate::mensaje_llano(format!("No se ha podido guardar el nombre: {e}"))
    })?;
    std::fs::write(dir.join(format!("{id}.ranura")), &ranura).map_err(|e| {
        crate::mensaje_llano(format!("No se ha podido guardar la ranura: {e}"))
    })?;
    Ok(FirmaGuardada {
        id,
        name: limpio.to_string(),
        png_base64: png_base64.trim().to_string(),
        ranura,
    })
}

/// Lista las firmas de `dir`, más reciente primero.
pub(crate) fn listar_firmas_en(dir: &std::path::Path) -> Result<Vec<FirmaGuardada>, String> {
    let mut out = Vec::new();
    let entries = std::fs::read_dir(dir).map_err(|e| {
        crate::mensaje_llano(format!("No se ha podido leer la biblioteca de firmas: {e}"))
    })?;
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
        let ranura = ranura_de(dir, &id);
        out.push(FirmaGuardada {
            id,
            name: name.trim().to_string(),
            png_base64: base64::engine::general_purpose::STANDARD.encode(&bytes),
            ranura,
        });
    }
    out.sort_by(|a, b| b.id.cmp(&a.id));
    Ok(out)
}

/// Importa un fichero de imagen como firma guardada (el diálogo de abrir
/// devuelve una ruta; la lectura se hace aquí, sin plugin fs). Se re-codifica
/// a PNG para conservar la transparencia con un formato único.
#[tauri::command(async)]
pub fn import_signature_file(
    image_path: String,
    ranura: Option<String>,
) -> Result<FirmaGuardada, String> {
    let img = image::open(&image_path).map_err(|e| {
        crate::mensaje_llano(format!("No se ha podido leer la imagen: {e}"))
    })?;
    let mut buf = std::io::Cursor::new(Vec::new());
    img.write_to(&mut buf, image::ImageFormat::Png).map_err(|e| {
        crate::mensaje_llano(format!("No se ha podido convertir la imagen a PNG: {e}"))
    })?;
    let png_base64 = base64::engine::general_purpose::STANDARD.encode(buf.into_inner());
    let name = std::path::Path::new(&image_path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Firma")
        .to_string();
    guardar_firma_en(&dir_de_firmas()?, &name, &png_base64, ranura.as_deref())
}

/// Guarda una imagen reutilizable en la biblioteca del usuario: una firma
/// manuscrita, unas iniciales o un sello, según la `ranura`.
#[tauri::command(async)]
pub fn save_stored_signature(
    name: String,
    png_base64: String,
    ranura: Option<String>,
) -> Result<FirmaGuardada, String> {
    guardar_firma_en(&dir_de_firmas()?, &name, &png_base64, ranura.as_deref())
}

/// Cambia la ranura de una imagen ya guardada («esto son mis iniciales, no
/// mi firma»), sin volver a subirla.
#[tauri::command(async)]
pub fn set_signature_slot(id: String, ranura: String) -> Result<(), String> {
    if id.contains(['/', '\\', '.']) {
        return Err("Id de firma inválido".into());
    }
    let ranura = ranura_valida(Some(&ranura))?;
    let dir = dir_de_firmas()?;
    if !dir.join(format!("{id}.png")).exists() {
        return Err("Esa imagen ya no está en la biblioteca".into());
    }
    std::fs::write(dir.join(format!("{id}.ranura")), &ranura).map_err(|e| {
        crate::mensaje_llano(format!("No se ha podido guardar la ranura: {e}"))
    })
}

/// Lista las firmas guardadas (con su PNG en base64 para las miniaturas).
#[tauri::command(async)]
pub fn list_stored_signatures() -> Result<Vec<FirmaGuardada>, String> {
    listar_firmas_en(&dir_de_firmas()?)
}

/// Borra una firma guardada de la biblioteca.
#[tauri::command(async)]
pub fn delete_stored_signature(id: String) -> Result<(), String> {
    if id.contains(['/', '\\', '.']) {
        return Err("Id de firma inválido".into());
    }
    let dir = dir_de_firmas()?;
    std::fs::remove_file(dir.join(format!("{id}.png"))).map_err(|e| {
        crate::mensaje_llano(format!("No se ha podido borrar la firma: {e}"))
    })?;
    let _ = std::fs::remove_file(dir.join(format!("{id}.txt")));
    let _ = std::fs::remove_file(dir.join(format!("{id}.ranura")));
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
        let imgs = crate::imagenes::get_images(work.clone(), 0).expect("listar imágenes");
        assert_eq!(imgs.len(), 1);
        let img = &imgs[0];
        assert!((img.x - 100.0).abs() < 1.0, "x = {}", img.x);
        assert!((img.y - 500.0).abs() < 1.0, "y = {}", img.y);
        assert!((img.w - 180.0).abs() < 1.0, "w = {}", img.w);
        assert!((img.h - 60.0).abs() < 1.0, "h = {}", img.h);

        // el render compone el alfa: la mitad izquierda pinta rojo, la
        // derecha deja el fondo blanco
        let png = crate::render_page_png(work, 0, 600, true).expect("render");
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
        let guardada = guardar_firma_en(&dir, "  Mi firma  ", &png, None).expect("guardar");
        assert_eq!(guardada.name, "Mi firma");
        // sin decir ranura, una firma: es lo que había antes de que
        // hubiera ranuras
        assert_eq!(guardada.ranura, "firma");

        let lista = listar_firmas_en(&dir).expect("listar");
        assert_eq!(lista.len(), 1);
        assert_eq!(lista[0].id, guardada.id);
        assert_eq!(lista[0].name, "Mi firma");
        assert_eq!(lista[0].png_base64, png);
        assert_eq!(lista[0].ranura, "firma");

        std::fs::remove_file(dir.join(format!("{}.png", guardada.id))).unwrap();
        assert!(listar_firmas_en(&dir).expect("listar").is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// **R58 — la ranura vive en la biblioteca.** Cuáles de las imágenes
    /// guardadas son iniciales lo recordaba el `localStorage` de la
    /// interfaz, porque el backend guardaba imágenes por nombre y no sabía
    /// de ranuras: limpiar el almacenamiento del navegador embebido dejaba
    /// las imágenes puestas y la distinción perdida. Y con la galería de
    /// sellos son tres ranuras, no dos.
    #[test]
    fn la_biblioteca_recuerda_para_que_es_cada_imagen() {
        let dir = std::env::temp_dir().join(format!(
            "firmas-ranura-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let png = png_con_alfa_base64();

        let firma = guardar_firma_en(&dir, "Mi firma", &png, None).expect("firma");
        let iniciales =
            guardar_firma_en(&dir, "JG", &png, Some("iniciales")).expect("iniciales");
        let sello = guardar_firma_en(&dir, "Aprobado", &png, Some("sello")).expect("sello");
        assert_eq!(iniciales.ranura, "iniciales");
        assert_eq!(sello.ranura, "sello");

        let lista = listar_firmas_en(&dir).expect("listar");
        let ranura_de_id = |id: &str| {
            lista
                .iter()
                .find(|f| f.id == id)
                .map(|f| f.ranura.clone())
                .expect("la imagen tiene que estar en la lista")
        };
        assert_eq!(ranura_de_id(&firma.id), "firma");
        assert_eq!(ranura_de_id(&iniciales.id), "iniciales");
        assert_eq!(ranura_de_id(&sello.id), "sello");

        // una ranura que no existe se dice, no se guarda a medias
        assert!(guardar_firma_en(&dir, "X", &png, Some("rubrica"))
            .unwrap_err()
            .contains("firma, iniciales o sello"));

        // una imagen de una versión anterior (sin fichero de ranura) sigue
        // siendo una firma
        std::fs::remove_file(dir.join(format!("{}.ranura", iniciales.id))).unwrap();
        let lista = listar_firmas_en(&dir).expect("listar");
        assert_eq!(
            lista.iter().find(|f| f.id == iniciales.id).unwrap().ranura,
            "firma"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn migra_firmas_del_identifier_antiguo() {
        let raiz = std::env::temp_dir().join(format!(
            "vitela-migracion-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let viejo = raiz.join("com.jorge.editorpdf").join("firmas");
        std::fs::create_dir_all(&viejo).unwrap();
        std::fs::write(viejo.join("1.png"), b"png").unwrap();
        std::fs::write(viejo.join("1.txt"), "Mi firma").unwrap();

        let nuevo = raiz.join("com.jorge.vitela");
        migrar_datos_antiguos(&nuevo);
        assert_eq!(
            std::fs::read(nuevo.join("firmas").join("1.png")).unwrap(),
            b"png"
        );
        assert_eq!(
            std::fs::read_to_string(nuevo.join("firmas").join("1.txt")).unwrap(),
            "Mi firma"
        );
        // el directorio antiguo queda intacto
        assert!(viejo.join("1.png").exists());

        // una segunda llamada no pisa lo que ya hay en el nuevo
        std::fs::write(nuevo.join("firmas").join("1.txt"), "Editada").unwrap();
        migrar_datos_antiguos(&nuevo);
        assert_eq!(
            std::fs::read_to_string(nuevo.join("firmas").join("1.txt")).unwrap(),
            "Editada"
        );
        let _ = std::fs::remove_dir_all(&raiz);
    }
}
