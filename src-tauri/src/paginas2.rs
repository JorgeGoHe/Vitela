//! Gestión de páginas ampliada: página en blanco, duplicar, insertar otro
//! PDF en una posición, recorte, marca de agua y encabezados/pies con
//! numeración.

use crate::{on_pdfium_thread, pdfium, save_and_close, Rect};
use crate::historial::mutacion;
use pdfium_render::prelude::*;

/// Inserta una página en blanco en `index`, del mismo tamaño que la página
/// vecina (o A4 si el documento está vacío). Devuelve el nuevo total.
#[tauri::command(async)]
pub fn add_blank_page(work_path: String, index: u16) -> Result<u16, String> {
    mutacion(work_path, |work_path| on_pdfium_thread(move || {
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
    }))
}

/// Duplica la página dada (la copia queda justo después). Devuelve el total.
#[tauri::command(async)]
pub fn duplicate_page(work_path: String, page_index: u16) -> Result<u16, String> {
    mutacion(work_path, |work_path| on_pdfium_thread(move || {
        let pdfium = pdfium()?;
        let mut doc = pdfium
            .load_pdf_from_file(&work_path, None)
            .map_err(|e| e.to_string())?;
        // segundo handle del mismo fichero, solo lectura, en el mismo hilo
        // AC-046: importar de una copia sin las ventanas de las notas
        // (el par /Popup ↔ /Parent es un ciclo y mata a FPDF_ImportPages)
        let fuente = crate::anotaciones::fuente_importable(&work_path);
        let origen = pdfium
            .load_pdf_from_file(fuente.ruta(), None)
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
        crate::anotaciones::repon_popups_en(&work_path)?;
        Ok(nuevo)
    }))
}

/// Inserta todas las páginas de otro PDF en la posición dada. Devuelve el
/// total resultante (generaliza `merge_pdf`, que solo añade al final).
#[tauri::command(async)]
pub fn insert_pdf_at(work_path: String, other_path: String, index: u16) -> Result<u16, String> {
    mutacion(work_path, |work_path| on_pdfium_thread(move || {
        let pdfium = pdfium()?;
        let mut doc = pdfium
            .load_pdf_from_file(&work_path, None)
            .map_err(|e| e.to_string())?;
        // AC-046: importar de una copia sin las ventanas de las notas
        // (el par /Popup ↔ /Parent es un ciclo y mata a FPDF_ImportPages)
        let fuente = crate::anotaciones::fuente_importable(&other_path);
        let other = pdfium
            .load_pdf_from_file(fuente.ruta(), None)
            .map_err(|e| e.to_string())?;
        let rango = format!("1-{}", other.pages().len());
        let index = index.min(doc.pages().len());
        doc.pages_mut()
            .copy_pages_from_document(&other, &rango, index)
            .map_err(|e| e.to_string())?;
        drop(other);
        let nuevo = doc.pages().len();
        save_and_close(doc, &work_path)?;
        crate::anotaciones::repon_popups_en(&work_path)?;
        Ok(nuevo)
    }))
}

/// Recorta una página (o todas) al rect dado en coords de UI. En vez de
/// fijar solo el CropBox (que desplazaría el origen y desalinearía todas las
/// coordenadas de la UI), se normaliza: se traslada el contenido y las
/// anotaciones y se reescriben MediaBox y CropBox a (0,0,w,h). El contenido
/// fuera del área no se elimina (solo deja de mostrarse), como en Acrobat.
#[tauri::command(async)]
pub fn crop_page(
    work_path: String,
    page_index: u16,
    rect: Rect,
    all_pages: bool,
) -> Result<(), String> {
    if rect.w < 24.0 || rect.h < 24.0 {
        return Err("El área de recorte es demasiado pequeña".into());
    }
    mutacion(work_path, |work_path| on_pdfium_thread(move || {
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
    }))
}

/// Ancho estimado de un texto en Helvetica Bold (media ~0.6 em por carácter).
fn ancho_estimado(text: &str, size: f32) -> f32 {
    text.chars().count() as f32 * size * 0.6
}

/// Opacidad por defecto de la marca de agua: la de Acrobat.
const OPACIDAD_MARCA: f32 = 0.3;

/// Las páginas sobre las que trabaja un marginal: las que se pidan o, sin
/// lista, todas. Los índices fuera del documento se ignoran (la UI puede
/// mandar un rango escrito a mano).
fn paginas_pedidas(total: u16, pedidas: &Option<Vec<u16>>) -> Vec<u16> {
    match pedidas {
        Some(v) => {
            let mut v: Vec<u16> = v.iter().copied().filter(|i| *i < total).collect();
            v.sort_unstable();
            v.dedup();
            v
        }
        None => (0..total).collect(),
    }
}

/// Dónde cae el centro del marginal dentro de la página, según la celda del
/// grid 3×3 ("nw".."se", vacío = centro) y las semiextensiones del objeto ya
/// girado, para que no se salga por ningún lado.
fn centro_en_celda(pos: &str, page_w: f32, page_h: f32, hw: f32, hh: f32) -> (f32, f32) {
    let (mx, my) = (page_w * 0.08, page_h * 0.08);
    let tx = if pos.contains('w') {
        (mx + hw).min(page_w / 2.0)
    } else if pos.contains('e') {
        (page_w - mx - hw).max(page_w / 2.0)
    } else {
        page_w / 2.0
    };
    let ty = if pos.contains('n') {
        (page_h - my - hh).max(page_h / 2.0)
    } else if pos.contains('s') {
        (my + hh).min(page_h / 2.0)
    } else {
        page_h / 2.0
    };
    (tx, ty)
}

/// Marca de agua de texto **o de imagen**, en todas las páginas o solo en
/// las que se pidan. `position` es una celda de un grid 3×3 ("nw".."se",
/// `None` = centro), `rotation` los grados antihorarios (sin ella, 45° si
/// `diagonal`, 0 si no) y `opacity` la transparencia (0,3 por defecto, la de
/// Acrobat).
///
/// Va como contenido de página; para poder quitarla después el alpha del
/// texto se limita a 240 (así `remove_marginal_text` la reconoce por
/// translucidez aunque no esté rotada). La imagen se incrusta con su alfa ya
/// multiplicado por la opacidad, que es lo que deja el `/SMask` puesto.
#[tauri::command(async)]
#[allow(clippy::too_many_arguments)]
pub fn add_watermark(
    work_path: String,
    text: String,
    font_size: f32,
    color: [u8; 4],
    diagonal: bool,
    position: Option<String>,
    page_indices: Option<Vec<u16>>,
    image_png: Option<String>,
    opacity: Option<f32>,
    rotation: Option<f32>,
) -> Result<(), String> {
    let text = text.trim().to_string();
    let imagen = match image_png {
        Some(b64) => {
            use base64::Engine;
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(b64.split(',').next_back().unwrap_or_default())
                .map_err(|_| "La imagen de la marca de agua no se ha podido leer")?;
            Some(
                image::load_from_memory(&bytes)
                    .map_err(|e| format!("La imagen de la marca de agua no vale: {e}"))?,
            )
        }
        None => None,
    };
    if text.is_empty() && imagen.is_none() {
        return Err("La marca de agua está vacía".into());
    }
    let pos = position.unwrap_or_else(|| "c".into());
    let opacidad = opacity.unwrap_or(OPACIDAD_MARCA).clamp(0.05, 1.0);
    let giro = rotation.unwrap_or(if diagonal { 45.0 } else { 0.0 });
    mutacion(work_path, move |work_path| on_pdfium_thread(move || {
        let pdfium = pdfium()?;
        let mut doc = pdfium
            .load_pdf_from_file(&work_path, None)
            .map_err(|e| e.to_string())?;
        let font = doc.fonts_mut().helvetica_bold();
        let size = font_size.clamp(12.0, 200.0);
        // la opacidad viene en su propio parámetro: el color llega opaco y
        // el alfa sale de `opacity` (tope 240 para que
        // `remove_marginal_text` siga reconociendo la marca por translúcida)
        let alpha = (opacidad * 255.0).round().clamp(1.0, 240.0) as u8;
        let c = PdfColor::new(color[0], color[1], color[2], alpha);
        // la imagen lleva la opacidad en su propio alfa: así PDFium le
        // escribe el /SMask y se ve translúcida en cualquier visor
        let imagen = imagen.as_ref().map(|img| {
            let mut rgba = img.to_rgba8();
            for p in rgba.pixels_mut() {
                p.0[3] = (p.0[3] as f32 * opacidad).round() as u8;
            }
            image::DynamicImage::ImageRgba8(rgba)
        });
        let (cos, sin) = {
            let r = giro.to_radians();
            (r.cos(), r.sin())
        };
        for i in paginas_pedidas(doc.pages().len(), &page_indices) {
            let mut page = doc.pages().get(i).map_err(|e| e.to_string())?;
            let page_w = page.width().value;
            let page_h = page.height().value;
            // ancho y alto del objeto sin girar, y su centro
            let (w, h, cx, cy) = match &imagen {
                Some(img) => {
                    let (iw, ih) = (img.width() as f32, img.height() as f32);
                    let escala = ((page_w * 0.5) / iw).min((page_h * 0.5) / ih);
                    let (w, h) = (iw * escala, ih * escala);
                    (w, h, w / 2.0, h / 2.0)
                }
                None => {
                    let w = ancho_estimado(&text, size);
                    // el texto tiene la línea base en el origen
                    (w, size, w / 2.0, size * 0.35)
                }
            };
            // el centro, después del giro alrededor del origen
            let (cx2, cy2) = (cx * cos - cy * sin, cx * sin + cy * cos);
            let (hw, hh) = (
                (w * cos.abs() + h * sin.abs()) / 2.0,
                (w * sin.abs() + h * cos.abs()) / 2.0,
            );
            let (tx, ty) = centro_en_celda(&pos, page_w, page_h, hw, hh);
            match &imagen {
                Some(img) => {
                    let mut obj = PdfPageImageObject::new_with_size(
                        &doc,
                        img,
                        PdfPoints::new(w),
                        PdfPoints::new(h),
                    )
                    .map_err(|e| e.to_string())?;
                    if giro != 0.0 {
                        obj.rotate_counter_clockwise_degrees(giro)
                            .map_err(|e| e.to_string())?;
                    }
                    obj.translate(PdfPoints::new(tx - cx2), PdfPoints::new(ty - cy2))
                        .map_err(|e| e.to_string())?;
                    page.objects_mut()
                        .add_image_object(obj)
                        .map_err(|e| e.to_string())?;
                }
                None => {
                    let mut obj = PdfPageTextObject::new(&doc, &text, font, PdfPoints::new(size))
                        .map_err(|e| e.to_string())?;
                    obj.set_fill_color(c).map_err(|e| e.to_string())?;
                    if giro != 0.0 {
                        obj.rotate_counter_clockwise_degrees(giro)
                            .map_err(|e| e.to_string())?;
                    }
                    obj.translate(PdfPoints::new(tx - cx2), PdfPoints::new(ty - cy2))
                        .map_err(|e| e.to_string())?;
                    page.objects_mut()
                        .add_text_object(obj)
                        .map_err(|e| e.to_string())?;
                }
            }
            page.regenerate_content().map_err(|e| e.to_string())?;
        }
        save_and_close(doc, &work_path)?;
        Ok(())
    }))
}

/// Encabezado y pie en todas las páginas, con tres huecos por zona
/// (izquierda/centro/derecha). Plantillas: `{n}` número de página, `{total}`
/// total, `{fecha}` fecha de hoy. Numerar páginas = pie centro con `{n}`.
#[tauri::command(async)]
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
    page_indices: Option<Vec<u16>>,
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
    mutacion(work_path, move |work_path| on_pdfium_thread(move || {
        let pdfium = pdfium()?;
        let mut doc = pdfium
            .load_pdf_from_file(&work_path, None)
            .map_err(|e| e.to_string())?;
        let font = doc.fonts_mut().helvetica();
        let size = font_size.clamp(6.0, 24.0);
        let total = doc.pages().len();
        let fecha = chrono::Local::now().format("%d/%m/%Y").to_string();
        const MARGEN_X: f32 = 36.0;
        // el número de página y el total siguen siendo los del documento,
        // aunque solo se escriban unas cuantas
        for i in paginas_pedidas(total, &page_indices) {
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
    }))
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
        let pt = crate::busqueda::get_page_text(work.clone(), 0).expect("texto");
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
        let png_b64 = crate::render_page_b64(work, 0, 300, None).expect("render");
        assert!(!png_b64.is_empty());
    }

    #[test]
    fn quitar_marca_de_agua_y_pies() {
        let pdf = std::env::temp_dir().join("paginas2-quitar-test.pdf");
        crea_pdf(&["Contenido uno", "Contenido dos"], &pdf);
        let work = pdf.to_string_lossy().to_string();
        add_watermark(
            work.clone(),
            "BORRADOR".into(),
            60.0,
            [200, 30, 30, 90],
            true,
            None,
            None,
            None,
            None,
            None,
        )
        .expect("marca");
        add_header_footer(
            work.clone(),
            None,
            None,
            None,
            None,
            Some("{n} / {total}".into()),
            None,
            10.0,
            None,
        )
        .expect("pie");
        // dry run cuenta sin tocar
        let previa = remove_marginal_text(work.clone(), "watermark".into(), true)
            .expect("dry run");
        assert_eq!(previa.textos, 2);
        let t = textos(&work);
        assert!(t[0].contains("BORRADOR"));
        // quitar de verdad
        let informe = remove_marginal_text(work.clone(), "watermark".into(), false)
            .expect("quitar marca");
        assert_eq!(informe.textos, 2);
        let informe = remove_marginal_text(work.clone(), "footer".into(), false)
            .expect("quitar pies");
        assert_eq!(informe.textos, 2);
        let t = textos(&work);
        assert!(!t[0].contains("BORRADOR"), "{:?}", t[0]);
        assert!(!t[0].contains("1 / 2"));
        // el contenido normal sobrevive
        assert!(t[0].contains("Contenido uno"));
        assert!(t[1].contains("Contenido dos"));
    }

    #[test]
    fn marca_de_agua_horizontal_en_esquina_se_quita() {
        // sin rotación la marca se reconoce por translucidez; en una esquina
        // no debe llevarse por delante encabezados/pies ni contenido
        let pdf = std::env::temp_dir().join("paginas2-marca-esquina-test.pdf");
        crea_pdf(&["Contenido uno"], &pdf);
        let work = pdf.to_string_lossy().to_string();
        add_watermark(
            work.clone(),
            "CONFIDENCIAL".into(),
            30.0,
            [200, 30, 30, 255],
            false,
            Some("se".into()),
            None,
            None,
            None,
            None,
        )
        .expect("marca");
        add_header_footer(
            work.clone(),
            None,
            Some("Encabezado".into()),
            None,
            None,
            Some("{n}".into()),
            None,
            10.0,
            None,
        )
        .expect("pie");
        assert!(textos(&work)[0].contains("CONFIDENCIAL"));
        // la marca está en el tercio inferior derecho de la página
        let bounds = on_pdfium_thread({
            let work = work.clone();
            move || {
                let pdfium = pdfium().expect("pdfium");
                let doc = pdfium.load_pdf_from_file(&work, None).expect("abrir");
                let page = doc.pages().get(0).expect("página");
                let (pw, ph) = (page.width().value, page.height().value);
                for i in 0..page.objects().len() {
                    let Ok(obj) = page.objects().get(i) else { continue };
                    if obj.as_text_object().is_none() {
                        continue;
                    }
                    let Ok(c) = obj.fill_color() else { continue };
                    if c.alpha() < 250 {
                        let b = obj.bounds().expect("bounds");
                        return Some((b.left().value / pw, b.bottom().value / ph));
                    }
                }
                None
            }
        })
        .expect("marca translúcida no encontrada");
        assert!(bounds.0 > 0.3, "muy a la izquierda: {}", bounds.0);
        assert!(bounds.1 < 0.35, "muy arriba: {}", bounds.1);
        let informe = remove_marginal_text(work.clone(), "watermark".into(), false)
            .expect("quitar marca");
        assert_eq!(informe.textos, 1);
        let t = textos(&work);
        assert!(!t[0].contains("CONFIDENCIAL"), "{:?}", t[0]);
        assert!(t[0].contains("Contenido uno"));
        assert!(t[0].contains("Encabezado"));
        assert!(t[0].contains('1'));
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
            None,
            None,
            None,
            None,
            None,
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
            None,
        )
        .expect("pie");
        let t = textos(&work);
        assert!(t[0].contains("BORRADOR"), "{:?}", t[0]);
        assert!(t[1].contains("BORRADOR"));
        assert!(t[0].contains("1 / 2"), "{:?}", t[0]);
        assert!(t[1].contains("2 / 2"));
        assert!(t[0].contains("Informe"));
    }

    fn pasos(work: &str) -> u16 {
        crate::historial::history_state(work.to_string()).expect("historial").undo
    }

    /// «Reemplazar páginas» de Acrobat: se eligen las del destino y las del
    /// origen y el resto se conserva. Un solo ⌘Z lo devuelve.
    #[test]
    fn reemplazar_paginas_conserva_el_resto() {
        let pdf = std::env::temp_dir().join("paginas2-reemplazar-test.pdf");
        let otro = std::env::temp_dir().join("paginas2-reemplazar-otro.pdf");
        crea_pdf(&["Uno", "Dos", "Tres", "Cuatro", "Cinco", "Seis"], &pdf);
        crea_pdf(&["Alfa", "Beta"], &otro);
        let work = pdf.to_string_lossy().into_owned();
        let antes = pasos(&work);

        let total = replace_pages(work.clone(), vec![1, 2], otro.to_string_lossy().into_owned(), None)
            .expect("reemplazar");

        assert_eq!(total, 6, "dos por dos: el total no cambia");
        let t = textos(&work);
        assert!(t[0].contains("Uno"), "página 1: {:?}", t[0]);
        assert!(t[1].contains("Alfa"), "página 2: {:?}", t[1]);
        assert!(t[2].contains("Beta"), "página 3: {:?}", t[2]);
        assert!(t[3].contains("Cuatro"), "página 4: {:?}", t[3]);
        assert!(t[5].contains("Seis"), "página 6: {:?}", t[5]);
        assert_eq!(pasos(&work), antes + 1, "reemplazar es UN paso");
        crate::historial::undo(work.clone()).expect("deshacer");
        let t = textos(&work);
        assert!(t[1].contains("Dos") && t[2].contains("Tres"), "⌘Z lo devuelve");

        // y se puede elegir qué páginas del origen entran
        replace_pages(
            work.clone(),
            vec![0],
            otro.to_string_lossy().into_owned(),
            Some(vec![1]),
        )
        .expect("reemplazar con selección del origen");
        let t = textos(&work);
        assert_eq!(t.len(), 6);
        assert!(t[0].contains("Beta"), "solo la segunda del origen: {:?}", t[0]);

        std::fs::remove_file(&pdf).ok();
        std::fs::remove_file(&otro).ok();
    }

    /// «Dividir»: cada N páginas y por marcadores de primer nivel, con
    /// carpeta de salida y sin tocar el documento abierto.
    #[test]
    fn dividir_cada_n_paginas_y_por_marcadores() {
        let pdf = std::env::temp_dir().join("paginas2-dividir-test.pdf");
        crea_pdf(&["1", "2", "3", "4", "5", "6", "7", "8", "9", "10"], &pdf);
        let work = pdf.to_string_lossy().into_owned();
        let dir = std::env::temp_dir().join("vitela-dividir-test");
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).expect("carpeta");
        let antes = pasos(&work);

        let partes = split_pdf(
            work.clone(),
            dir.to_string_lossy().into_owned(),
            "cada".into(),
            Some(3),
        )
        .expect("dividir cada 3");
        assert_eq!(partes.len(), 4, "10 páginas de 3 en 3 son 3+3+3+1");
        let cuentas: Vec<usize> = partes.iter().map(|p| textos(p).len()).collect();
        assert_eq!(cuentas, vec![3, 3, 3, 1]);
        assert_eq!(pasos(&work), antes, "dividir escribe fuera: no muta nada");
        assert_eq!(textos(&work).len(), 10, "el documento se queda entero");

        // por marcadores de primer nivel
        crate::documento::set_outline(
            work.clone(),
            vec![
                crate::documento::OutlineNode {
                    title: "Capítulo 1".into(),
                    page_index: Some(0),
                    children: vec![],
                },
                crate::documento::OutlineNode {
                    title: "Capítulo 2".into(),
                    page_index: Some(4),
                    children: vec![],
                },
            ],
        )
        .expect("marcadores");
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).expect("carpeta");
        let partes = split_pdf(
            work.clone(),
            dir.to_string_lossy().into_owned(),
            "marcadores".into(),
            None,
        )
        .expect("dividir por marcadores");
        let cuentas: Vec<usize> = partes.iter().map(|p| textos(p).len()).collect();
        assert_eq!(cuentas, vec![4, 6], "el corte cae en el marcador");

        // un modo que no existe se dice, no se adivina
        assert!(split_pdf(
            work.clone(),
            dir.to_string_lossy().into_owned(),
            "loquesea".into(),
            None
        )
        .is_err());

        std::fs::remove_dir_all(&dir).ok();
        std::fs::remove_file(&pdf).ok();
    }

    /// «Combinar ficheros»: varios PDF en el orden de la lista y un solo
    /// paso de deshacer.
    #[test]
    fn combinar_varios_ficheros_es_un_solo_paso() {
        let pdf = std::env::temp_dir().join("paginas2-combinar-test.pdf");
        let a = std::env::temp_dir().join("paginas2-combinar-a.pdf");
        let b = std::env::temp_dir().join("paginas2-combinar-b.pdf");
        crea_pdf(&["Base"], &pdf);
        crea_pdf(&["A1", "A2"], &a);
        crea_pdf(&["B1"], &b);
        let work = pdf.to_string_lossy().into_owned();
        let antes = pasos(&work);

        let total = merge_many(
            work.clone(),
            vec![a.to_string_lossy().into_owned(), b.to_string_lossy().into_owned()],
            None,
        )
        .expect("combinar");
        assert_eq!(total, 4);
        let t = textos(&work);
        assert!(t[0].contains("Base"));
        assert!(t[1].contains("A1") && t[2].contains("A2"), "orden de la lista");
        assert!(t[3].contains("B1"));
        assert_eq!(pasos(&work), antes + 1, "combinar es UN paso");
        crate::historial::undo(work.clone()).expect("deshacer");
        assert_eq!(textos(&work).len(), 1, "⌘Z devuelve el documento");

        // y se puede insertar en un punto concreto, conservando el orden
        merge_many(
            work.clone(),
            vec![a.to_string_lossy().into_owned(), b.to_string_lossy().into_owned()],
            Some(0),
        )
        .expect("combinar al principio");
        let t = textos(&work);
        assert!(
            t[0].contains("A1") && t[1].contains("A2") && t[2].contains("B1") && t[3].contains("Base"),
            "el orden de la lista se conserva al insertar: {t:?}"
        );

        for p in [&pdf, &a, &b] {
            std::fs::remove_file(p).ok();
        }
    }

    /// Poner una marca de agua o un pie solo en unas páginas es lo primero
    /// que pide cualquiera («en todas menos la portada»), y hasta ahora era
    /// todo o nada. Y sigue siendo un solo paso de deshacer.
    #[test]
    fn la_marca_de_agua_y_el_pie_solo_en_las_paginas_que_se_piden() {
        let pdf = std::env::temp_dir().join("paginas2-marca-rango-test.pdf");
        crea_pdf(&["Uno", "Dos", "Tres", "Cuatro"], &pdf);
        let work = pdf.to_string_lossy().to_string();
        let pasos = |w: &str| {
            crate::historial::history_state(w.to_string())
                .expect("historial")
                .undo
        };
        let antes = pasos(&work);
        add_watermark(
            work.clone(),
            "BORRADOR".into(),
            60.0,
            [200, 30, 30, 255],
            true,
            None,
            Some(vec![0, 2]),
            None,
            None,
            None,
        )
        .expect("marca en dos páginas");
        assert_eq!(pasos(&work), antes + 1, "el lote entero es UN paso");
        let t = textos(&work);
        assert!(t[0].contains("BORRADOR"), "{:?}", t[0]);
        assert!(!t[1].contains("BORRADOR"), "la 2 tenía que quedar limpia");
        assert!(t[2].contains("BORRADOR"), "{:?}", t[2]);
        assert!(!t[3].contains("BORRADOR"), "la 4 tenía que quedar limpia");

        // el pie solo en la última, y el número sigue siendo el del
        // documento (4 de 4), no el del rango
        add_header_footer(
            work.clone(),
            None,
            None,
            None,
            None,
            Some("{n} / {total}".into()),
            None,
            10.0,
            Some(vec![3]),
        )
        .expect("pie");
        let t = textos(&work);
        assert!(t[3].contains("4 / 4"), "{:?}", t[3]);
        assert!(!t[0].contains("1 / 4"), "la portada no lleva pie");
        std::fs::remove_file(&pdf).ok();
    }

    /// Marca de agua con imagen (el logo de la empresa), que es la otra
    /// mitad del diálogo de Acrobat: se ve, es translúcida (su alfa lleva
    /// la opacidad, así que PDFium le escribe el /SMask) y solo cae donde
    /// se ha pedido.
    #[test]
    fn la_marca_de_agua_puede_ser_una_imagen_translucida() {
        let pdf = std::env::temp_dir().join("paginas2-marca-imagen-test.pdf");
        crea_pdf(&["Uno", "Dos"], &pdf);
        let work = pdf.to_string_lossy().to_string();
        // un cuadrado azul opaco como «logo»
        let img = image::RgbaImage::from_pixel(120, 120, image::Rgba([20, 40, 200, 255]));
        let mut buf = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(img)
            .write_to(&mut buf, image::ImageFormat::Png)
            .expect("png");
        use base64::Engine;
        let b64 = base64::engine::general_purpose::STANDARD.encode(buf.into_inner());

        let tinta_antes = tinta_en_el_centro(&work, 0);
        add_watermark(
            work.clone(),
            String::new(),
            0.0,
            [0, 0, 0, 255],
            false,
            None,
            Some(vec![0]),
            Some(b64),
            Some(0.3),
            Some(45.0),
        )
        .expect("marca de imagen");
        assert!(
            tinta_en_el_centro(&work, 0) > tinta_antes,
            "la marca de imagen no ha pintado nada"
        );
        assert_eq!(
            tinta_en_el_centro(&work, 1),
            0,
            "la página 2 no llevaba marca"
        );
        // el objeto nuevo es una imagen con alfa: /SMask escrito
        let bytes = std::fs::read(&pdf).expect("leer");
        assert!(
            bytes.windows(6).any(|w| w == b"/SMask"),
            "la imagen translúcida tiene que llevar su /SMask"
        );
        std::fs::remove_file(&pdf).ok();
    }

    /// Píxeles con tinta en el centro de la página (una banda ancha), para
    /// juzgar si algo se ha pintado ahí.
    fn tinta_en_el_centro(work: &str, pagina: u16) -> u32 {
        let png = crate::render_page_png(work.to_string(), pagina, 400, true).expect("render");
        let img = image::load_from_memory(&png).expect("PNG").to_rgba8();
        let (w, h) = (img.width(), img.height());
        let mut n = 0;
        for y in h / 3..h * 2 / 3 {
            for x in w / 3..w * 2 / 3 {
                let p = img.get_pixel(x, y).0;
                if p[0] < 245 || p[1] < 245 || p[2] < 245 {
                    n += 1;
                }
            }
        }
        n
    }

}

/// Reemplaza las páginas de `page_indices` por las de otro documento,
/// conservando el resto: es «Reemplazar páginas» de Acrobat. Sin
/// `other_indices` entran todas las del origen.
///
/// Se hace en una sola mutación: se insertan las nuevas donde empezaban las
/// viejas y después se borran las viejas, de mayor a menor.
#[tauri::command(async)]
pub fn replace_pages(
    work_path: String,
    page_indices: Vec<u16>,
    other_path: String,
    other_indices: Option<Vec<u16>>,
) -> Result<u16, String> {
    if page_indices.is_empty() {
        return Err("No hay páginas que reemplazar".into());
    }
    mutacion(work_path, move |work_path| on_pdfium_thread(move || {
        let pdfium = pdfium()?;
        let mut doc = pdfium
            .load_pdf_from_file(&work_path, None)
            .map_err(crate::mensaje_llano)?;
        // AC-046: importar de una copia sin las ventanas de las notas
        // (el par /Popup ↔ /Parent es un ciclo y mata a FPDF_ImportPages)
        let fuente = crate::anotaciones::fuente_importable(&other_path);
        let other = pdfium
            .load_pdf_from_file(fuente.ruta(), None)
            .map_err(crate::mensaje_llano)?;
        let total = doc.pages().len();
        let mut viejas: Vec<u16> = page_indices.clone();
        viejas.sort_unstable();
        viejas.dedup();
        if let Some(fuera) = viejas.iter().find(|i| **i >= total) {
            return Err(format!("La página {} ya no está en el documento", fuera + 1));
        }
        let rango = match &other_indices {
            Some(indices) if !indices.is_empty() => {
                if let Some(fuera) = indices.iter().find(|i| **i >= other.pages().len()) {
                    return Err(format!("El otro documento no tiene la página {}", fuera + 1));
                }
                indices
                    .iter()
                    .map(|i| (i + 1).to_string())
                    .collect::<Vec<_>>()
                    .join(",")
            }
            _ => format!("1-{}", other.pages().len()),
        };
        let destino = viejas[0];
        doc.pages_mut()
            .copy_pages_from_document(&other, &rango, destino)
            .map_err(crate::mensaje_llano)?;
        drop(other);
        // las viejas se han desplazado tantas posiciones como páginas nuevas
        let metidas = doc.pages().len() - total;
        for i in viejas.iter().rev() {
            doc.pages()
                .get(i + metidas)
                .map_err(crate::mensaje_llano)?
                .delete()
                .map_err(crate::mensaje_llano)?;
        }
        let nuevo = doc.pages().len();
        if nuevo == 0 {
            return Err("Un documento no puede quedarse sin páginas".into());
        }
        save_and_close(doc, &work_path)?;
        crate::anotaciones::repon_popups_en(&work_path)?;
        Ok(nuevo)
    }))
}

/// Divide el documento en varios ficheros dentro de `dest_dir` y devuelve
/// las rutas escritas. Dos modos, los de Acrobat que se pueden cumplir con
/// lo que ya sabe el core: `"cada"` (cada N páginas) y `"marcadores"` (un
/// fichero por marcador de primer nivel).
///
/// No muta el documento abierto: escribe fuera, así que no deja paso de
/// deshacer y, si un fichero falla, el original no se ha tocado.
#[tauri::command(async)]
pub fn split_pdf(
    work_path: String,
    dest_dir: String,
    modo: String,
    cada: Option<u16>,
) -> Result<Vec<String>, String> {
    on_pdfium_thread(move || {
        let total = crate::with_doc(&work_path, |doc| Ok(doc.pages().len()))?;
        if total == 0 {
            return Err("El documento no tiene páginas".into());
        }
        let cortes: Vec<u16> = match modo.as_str() {
            "cada" => {
                let n = cada.unwrap_or(1).max(1);
                (0..total).step_by(n as usize).collect()
            }
            "marcadores" => {
                let raices = crate::documento::get_outline(work_path.clone())?;
                let mut inicios: Vec<u16> = raices.iter().filter_map(|n| n.page_index).collect();
                inicios.sort_unstable();
                inicios.dedup();
                if inicios.is_empty() {
                    return Err(
                        "El documento no tiene marcadores de primer nivel por los que dividir"
                            .into(),
                    );
                }
                // lo que va antes del primer marcador es el primer trozo
                if inicios[0] != 0 {
                    inicios.insert(0, 0);
                }
                inicios
            }
            _ => return Err("Modo de división desconocido".into()),
        };
        // AC-046: dividir de una copia sin las ventanas de las notas
        // (el par /Popup ↔ /Parent es un ciclo y mata a FPDF_ImportPages)
        let fuente = crate::anotaciones::fuente_importable(&work_path);
        let origen = pdfium()?
            .load_pdf_from_file(fuente.ruta(), None)
            .map_err(crate::mensaje_llano)?;
        let mut escritos: Vec<String> = Vec::new();
        for (n, inicio) in cortes.iter().enumerate() {
            let fin = cortes.get(n + 1).copied().unwrap_or(total);
            if fin <= *inicio {
                continue;
            }
            let destino =
                std::path::Path::new(&dest_dir).join(format!("parte-{}.pdf", escritos.len() + 1));
            let rango = format!("{}-{}", inicio + 1, fin);
            let mut nuevo = pdfium()?.create_new_pdf().map_err(crate::mensaje_llano)?;
            nuevo
                .pages_mut()
                .copy_pages_from_document(&origen, &rango, 0)
                .map_err(crate::mensaje_llano)?;
            nuevo.save_to_file(&destino).map_err(|e| {
                crate::mensaje_llano(format!(
                    "No se ha podido escribir {}: {e}",
                    destino.display()
                ))
            })?;
            let escrito = destino.to_string_lossy().into_owned();
            crate::anotaciones::repon_popups_en(&escrito)?;
            escritos.push(escrito);
        }
        Ok(escritos)
    })
}

/// Une varios PDF al documento abierto, en el orden de la lista y a partir
/// de `at` (al final si no llega). Es «Combinar ficheros» de Acrobat: todo
/// el lote en UNA mutación, así que un solo ⌘Z lo deshace.
#[tauri::command(async)]
pub fn merge_many(work_path: String, others: Vec<String>, at: Option<u16>) -> Result<u16, String> {
    if others.is_empty() {
        return Err("No hay ningún fichero que unir".into());
    }
    mutacion(work_path, move |work_path| on_pdfium_thread(move || {
        let pdfium = pdfium()?;
        let mut doc = pdfium
            .load_pdf_from_file(&work_path, None)
            .map_err(crate::mensaje_llano)?;
        let mut destino = at.unwrap_or(u16::MAX).min(doc.pages().len());
        for otro in &others {
            // AC-046: importar de una copia sin las ventanas de las notas
            // (el par /Popup ↔ /Parent es un ciclo y mata a FPDF_ImportPages)
            let fuente = crate::anotaciones::fuente_importable(otro);
            let other = pdfium
                .load_pdf_from_file(fuente.ruta(), None)
                .map_err(|e| crate::mensaje_llano(format!("No se ha podido abrir {otro}: {e}")))?;
            let paginas = other.pages().len();
            let rango = format!("1-{paginas}");
            doc.pages_mut()
                .copy_pages_from_document(&other, &rango, destino)
                .map_err(crate::mensaje_llano)?;
            drop(other);
            // el siguiente va detrás, para conservar el orden de la lista
            destino += paginas;
        }
        let nuevo = doc.pages().len();
        save_and_close(doc, &work_path)?;
        crate::anotaciones::repon_popups_en(&work_path)?;
        Ok(nuevo)
    }))
}


#[derive(serde::Serialize, Debug)]
pub struct MarginalReport {
    pub textos: u32,
}

/// Elimina el texto "marginal" añadido por la app: marca de agua (objetos de
/// texto con matriz rotada) o encabezados/pies (objetos de texto contenidos
/// en las bandas superior/inferior de 40 pt). Con `dry_run` solo cuenta.
#[tauri::command(async)]
pub fn remove_marginal_text(
    work_path: String,
    zona: String,
    dry_run: bool,
) -> Result<MarginalReport, String> {
    // sin instantánea en el ensayo (dry_run): no se escribe nada
    let cuerpo = move |work_path: String| on_pdfium_thread(move || {
        let pdfium = pdfium()?;
        let doc = pdfium
            .load_pdf_from_file(&work_path, None)
            .map_err(crate::mensaje_llano)?;
        let mut total = 0u32;
        for p in 0..doc.pages().len() {
            let mut page = doc.pages().get(p).map_err(crate::mensaje_llano)?;
            let page_h = page.height().value;
            let mut caen: Vec<usize> = Vec::new();
            {
                let objects = page.objects();
                for i in 0..objects.len() {
                    let Ok(obj) = objects.get(i) else { continue };
                    if obj.as_text_object().is_none() {
                        continue;
                    }
                    let rotado = obj
                        .matrix()
                        .map(|m| m.b().abs() > 0.01 || m.c().abs() > 0.01)
                        .unwrap_or(false);
                    // las marcas de agua sin rotar se reconocen por su
                    // translucidez (add_watermark limita el alpha a 240)
                    let translucido = obj
                        .fill_color()
                        .map(|c| c.alpha() < 250)
                        .unwrap_or(false);
                    let Ok(b) = obj.bounds() else { continue };
                    let en_zona = match zona.as_str() {
                        "watermark" => rotado || translucido,
                        // banda superior: el objeto entero por encima de h-40
                        "header" => !rotado && b.bottom().value > page_h - 40.0,
                        // banda inferior: el objeto entero por debajo de 40
                        "footer" => !rotado && b.top().value < 40.0,
                        _ => return Err(format!("Zona desconocida: {zona}")),
                    };
                    if en_zona {
                        caen.push(i);
                    }
                }
            }
            total += caen.len() as u32;
            if !dry_run && !caen.is_empty() {
                for &i in caen.iter().rev() {
                    let removed = page
                        .objects_mut()
                        .remove_object_at_index(i)
                        .map_err(crate::mensaje_llano)?;
                    // regla del proyecto: su Drop llama a FPDFPageObj_Destroy
                    // y PDFium casca — fuga puntual asumida
                    std::mem::forget(removed);
                }
                page.regenerate_content().map_err(crate::mensaje_llano)?;
            }
        }
        if dry_run {
            drop(doc);
            crate::invalidate_doc_cache();
        } else {
            save_and_close(doc, &work_path)?;
        }
        Ok(MarginalReport { textos: total })
    });
    if dry_run {
        cuerpo(work_path).map_err(crate::mensaje_llano)
    } else {
        mutacion(work_path, cuerpo)
    }
}
