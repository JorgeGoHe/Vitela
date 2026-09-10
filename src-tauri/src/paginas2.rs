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

/// Crea un PDF nuevo con una imagen por página. Es «Crear PDF desde
/// archivo» de Acrobat para el caso que se usa de verdad: unas fotos o unos
/// escaneos sueltos que hay que mandar como un solo documento.
///
/// `tamano` es `"a4"`, `"carta"` o `"imagen"`. Con los dos primeros la
/// imagen se ajusta a la página dejando 36 pt de margen (media pulgada,
/// como Acrobat) y se centra, sin deformarla nunca; con `"imagen"` la
/// página mide lo que mide la imagen a 72 dpi y no hay margen.
///
/// Devuelve **cuántas páginas** ha hecho y **cuáles se han quedado fuera y
/// por qué**: una imagen que no se deje leer se salta en vez de tirar el
/// lote entero —quien acaba de elegir veinte escaneos no quiere empezar de
/// cero por uno—, pero un resultado parcial en silencio es peor todavía.
/// Con la lista, la UI puede decir «19 de 20 páginas · *foto-7.heic* no se
/// ha podido leer» y dejar marcada esa fila, que es la regla que el
/// proyecto ya cumple en `replace_text` y en `export_docx`.
///
/// Si no se puede leer **ninguna** sigue siendo un error, con los nombres y
/// los motivos dentro: no hay documento que enseñar.
///
/// Escribe un fichero nuevo: no toca ningún documento abierto y no deja
/// paso de deshacer.
#[tauri::command(async)]
pub fn pdf_from_images(
    image_paths: Vec<String>,
    dest_path: String,
    tamano: String,
) -> Result<InformeImagenes, String> {
    if image_paths.is_empty() {
        return Err("No hay ninguna imagen con la que hacer el PDF".into());
    }
    on_pdfium_thread(move || {
        let pdfium = pdfium()?;
        let mut doc = pdfium.create_new_pdf().map_err(crate::mensaje_llano)?;
        const MARGEN: f32 = 36.0;
        let mut informe = InformeImagenes::default();
        for ruta in &image_paths {
            // una foto que no se deja leer no se lleva por delante el lote:
            // se salta, se apunta con su motivo y el recuento lo dice
            // («19 de 20»), que es lo que pide quien acaba de elegir veinte
            // escaneos
            let img = match image::open(ruta) {
                Ok(img) => img,
                Err(e) => {
                    informe.salta(ruta, motivo_de_imagen(&e));
                    continue;
                }
            };
            let (iw, ih) = (img.width() as f32, img.height() as f32);
            if iw < 1.0 || ih < 1.0 {
                informe.salta(ruta, "La imagen no tiene tamaño".into());
                continue;
            }
            let papel = match tamano.as_str() {
                "carta" => PdfPagePaperSize::from_points(PdfPoints::new(612.0), PdfPoints::new(792.0)),
                "imagen" => PdfPagePaperSize::from_points(PdfPoints::new(iw), PdfPoints::new(ih)),
                _ => PdfPagePaperSize::a4(),
            };
            let mut page = doc
                .pages_mut()
                .create_page_at_end(papel)
                .map_err(crate::mensaje_llano)?;
            let (pw, ph) = (page.width().value, page.height().value);
            let margen = if tamano == "imagen" { 0.0 } else { MARGEN };
            // se ajusta al hueco sin deformarla: la escala es la misma en
            // los dos ejes, la que quepa
            let escala = ((pw - margen * 2.0) / iw).min((ph - margen * 2.0) / ih);
            let (w, h) = (iw * escala, ih * escala);
            let mut obj = PdfPageImageObject::new_with_size(
                &doc,
                &img,
                PdfPoints::new(w),
                PdfPoints::new(h),
            )
            .map_err(crate::mensaje_llano)?;
            // centrada en la página, que es donde se espera una foto
            obj.translate(
                PdfPoints::new((pw - w) / 2.0),
                PdfPoints::new((ph - h) / 2.0),
            )
            .map_err(|e| e.to_string())?;
            page.objects_mut()
                .add_image_object(obj)
                .map_err(crate::mensaje_llano)?;
            page.regenerate_content().map_err(crate::mensaje_llano)?;
        }
        let total = doc.pages().len();
        if total == 0 {
            let cuales = (0..informe.saltadas.len())
                .map(|i| format!("{} ({})", informe.nombre(i), informe.motivos[i]))
                .collect::<Vec<_>>()
                .join(", ");
            return Err(format!("No se ha podido leer ninguna de las imágenes: {cuales}"));
        }
        doc.save_to_file(&dest_path).map_err(|e| {
            crate::mensaje_llano(format!("No se ha podido escribir {dest_path}: {e}"))
        })?;
        informe.paginas = total;
        Ok(informe)
    })
}

/// Lo que ha salido de «Crear PDF desde imágenes»: las páginas escritas y
/// las imágenes que se han quedado fuera, con su motivo.
///
/// `saltadas` son **las rutas tal como llegaron**, para que la UI pueda
/// compararlas con su lista y marcar esas filas del diálogo, que es donde
/// el usuario las eligió. `motivos` va en paralelo, una frase en llano por
/// ruta y en el mismo orden: sin ella, «no se ha podido leer» es lo único
/// que se puede decir de una foto corrupta, de una que ya no está y de un
/// formato que Vitela no entiende.
#[derive(serde::Serialize, Debug, Default)]
pub struct InformeImagenes {
    pub paginas: u16,
    pub saltadas: Vec<String>,
    pub motivos: Vec<String>,
}

impl InformeImagenes {
    fn salta(&mut self, ruta: &str, motivo: String) {
        self.saltadas.push(ruta.to_string());
        self.motivos.push(motivo);
    }

    /// El nombre suelto de la saltada `i`, que es lo que se enseña.
    fn nombre(&self, i: usize) -> String {
        let ruta = &self.saltadas[i];
        std::path::Path::new(ruta)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| ruta.clone())
    }
}

/// Por qué no se ha podido leer una imagen, en llano y en español. El
/// `Display` del crate `image` va en inglés y en su jerga («The image
/// format could not be determined»), y esto lo lee el usuario en la banda.
fn motivo_de_imagen(e: &image::ImageError) -> String {
    use image::ImageError;
    match e {
        ImageError::IoError(io) if io.kind() == std::io::ErrorKind::NotFound => {
            "El fichero ya no está donde estaba".into()
        }
        ImageError::IoError(_) => "No se ha podido leer el fichero".into(),
        ImageError::Unsupported(_) => "Vitela no entiende ese formato de imagen".into(),
        ImageError::Decoding(_) => "El fichero está dañado o no es una imagen".into(),
        ImageError::Limits(_) => "La imagen es demasiado grande".into(),
        _ => "No se ha podido leer como imagen".into(),
    }
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

/// Inserta en la posición dada las páginas de otro PDF: todas, o **solo
/// las que se pidan** (`page_indices`, base 0, en el orden en que
/// lleguen). Devuelve el total resultante (generaliza `merge_pdf`, que
/// solo añade al final).
///
/// El rango del origen es R57: el diálogo preguntaba antes o después de
/// qué página y entraba el documento entero, y quien inserta un anexo de
/// tres páginas de un PDF de cuarenta tenía que insertarlo todo y borrar.
#[tauri::command(async)]
pub fn insert_pdf_at(
    work_path: String,
    other_path: String,
    index: u16,
    page_indices: Option<Vec<u16>>,
) -> Result<u16, String> {
    mutacion(work_path, move |work_path| on_pdfium_thread(move || {
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
        // el rango que entiende PDFium va en base 1 y por comas; sin lista,
        // el documento entero
        let rango = match &page_indices {
            Some(v) => {
                let dentro: Vec<String> = v
                    .iter()
                    .filter(|i| **i < other.pages().len())
                    .map(|i| (i + 1).to_string())
                    .collect();
                if dentro.is_empty() {
                    return Err(
                        "Ninguna de esas páginas está en el documento que se inserta".into(),
                    );
                }
                dentro.join(",")
            }
            None => format!("1-{}", other.pages().len()),
        };
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
    detras: Option<bool>,
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
    let detras = detras.unwrap_or(false);
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
            let habia = page.objects().len();
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
            if detras {
                manda_al_fondo(&mut page, habia)?;
            }
            page.regenerate_content().map_err(|e| e.to_string())?;
        }
        save_and_close(doc, &work_path)?;
        Ok(())
    }))
}

/// Deja el **último** objeto de la página el primero, que es el que se
/// pinta debajo de todo lo demás: `habia` es cuántos objetos tenía la
/// página antes de añadirlo.
///
/// pdfium-render 0.8 no expone insertar por índice, así que se hace como en
/// `reorder_image`: se pasan por detrás los `habia` objetos de antes, en su
/// mismo orden. **Nunca se suelta un objeto sacado** —su `Drop` llama a
/// `FPDFPageObj_Destroy` y PDFium casca—: `add_object` se lo lleva.
fn manda_al_fondo(page: &mut PdfPage, habia: usize) -> Result<(), String> {
    for _ in 0..habia {
        let otro = page
            .objects_mut()
            .remove_object_at_index(0)
            .map_err(crate::mensaje_llano)?;
        page.objects_mut().add_object(otro).map_err(crate::mensaje_llano)?;
    }
    Ok(())
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

/// **Numeración Bates** (Acrobat: «Más ▸ Numeración Bates»): un sello
/// correlativo por página, con prefijo, sufijo y un número de dígitos fijo
/// —`ABC-000001-2026`—. Es lo que pide un juzgado o una auditoría para
/// poder citar «la 000123» y que todo el mundo mire lo mismo.
///
/// Los defectos son los de Acrobat: **seis dígitos** (`digitos` se acota
/// entre 1 y 15, que es el tope del diálogo), empezar en 1 y abajo a la
/// derecha. `position` usa los mismos códigos que la marca de agua
/// (`"nw"`, `"n"`, `"ne"`, `"w"`, `"c"`, `"e"`, `"sw"`, `"s"`, `"se"`).
///
/// Con `page_indices`, solo esas páginas —pero **el correlativo sigue
/// contando por el orden en que se numeran**, no por el número de página:
/// numerar las páginas 3, 7 y 8 escribe 000001, 000002 y 000003. Un número
/// Bates es un contador de folios, no la página en la que cae.
///
/// Devuelve cuántas páginas ha numerado, y es una sola mutación.
#[tauri::command(async)]
#[allow(clippy::too_many_arguments)]
pub fn add_bates(
    work_path: String,
    prefijo: String,
    sufijo: String,
    digitos: u8,
    empieza_en: u32,
    position: Option<String>,
    font_size: Option<f32>,
    page_indices: Option<Vec<u16>>,
) -> Result<u16, String> {
    let digitos = digitos.clamp(1, 15) as usize;
    let empieza_en = empieza_en.max(1);
    let pos = position.unwrap_or_else(|| "se".into());
    let size = font_size.unwrap_or(9.0).clamp(6.0, 24.0);
    mutacion(work_path, move |work_path| on_pdfium_thread(move || {
        let pdfium = pdfium()?;
        let mut doc = pdfium
            .load_pdf_from_file(&work_path, None)
            .map_err(crate::mensaje_llano)?;
        let font = doc.fonts_mut().helvetica();
        const MARGEN_X: f32 = 36.0;
        const MARGEN_Y: f32 = 20.0;
        let total = doc.pages().len();
        let paginas = paginas_pedidas(total, &page_indices);
        if paginas.is_empty() {
            crate::historial::retira_paso(&work_path);
            return Ok(0);
        }
        let cuantas = paginas.len() as u16;
        for (n, i) in paginas.into_iter().enumerate() {
            let numero = empieza_en as u64 + n as u64;
            let texto = format!("{prefijo}{numero:0>ancho$}{sufijo}", ancho = digitos);
            let mut page = doc.pages().get(i).map_err(crate::mensaje_llano)?;
            let (page_w, page_h) = (page.width().value, page.height().value);
            let ancho = ancho_estimado(&texto, size);
            // el sello se coloca por su esquina, con el margen de Acrobat;
            // `centro_en_celda` da el centro de la celda que toque
            let (cx, cy) = centro_en_celda(&pos, page_w, page_h, ancho / 2.0, size / 2.0);
            let x = cx - ancho / 2.0;
            let x = x.clamp(MARGEN_X, (page_w - MARGEN_X - ancho).max(MARGEN_X));
            let y = (cy - size / 2.0).clamp(MARGEN_Y, (page_h - MARGEN_Y - size).max(MARGEN_Y));
            let mut obj = PdfPageTextObject::new(&doc, &texto, font, PdfPoints::new(size))
                .map_err(crate::mensaje_llano)?;
            obj.set_fill_color(PdfColor::new(60, 60, 60, 255))
                .map_err(crate::mensaje_llano)?;
            obj.translate(PdfPoints::new(x), PdfPoints::new(y))
                .map_err(crate::mensaje_llano)?;
            page.objects_mut()
                .add_text_object(obj)
                .map_err(crate::mensaje_llano)?;
            page.regenerate_content().map_err(crate::mensaje_llano)?;
        }
        save_and_close(doc, &work_path)?;
        Ok(cuantas)
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

    /// **R57 — el rango del documento que entra.** «Insertar PDF aquí…»
    /// preguntaba antes o después de qué página y metía el documento
    /// entero: quien inserta un anexo de tres páginas de un PDF de cuarenta
    /// tenía que meterlo todo y borrar treinta y siete.
    #[test]
    fn insertar_un_pdf_puede_traerse_solo_unas_paginas() {
        let destino = std::env::temp_dir().join("paginas2-insertar-rango.pdf");
        let origen = std::env::temp_dir().join("paginas2-insertar-rango-origen.pdf");
        crea_pdf(&["Contrato A", "Contrato B"], &destino);
        crea_pdf(&["Anexo uno", "Anexo dos", "Anexo tres", "Anexo cuatro"], &origen);
        let work = destino.to_string_lossy().to_string();
        let otro = origen.to_string_lossy().to_string();

        // solo la segunda y la cuarta del origen, detrás de la primera
        let total = insert_pdf_at(work.clone(), otro.clone(), 1, Some(vec![1, 3]))
            .expect("insertar el rango");
        assert_eq!(total, 4);
        let t = textos(&work);
        assert!(t[0].contains("Contrato A"));
        assert!(t[1].contains("Anexo dos"), "la 2 tendría que ser el anexo dos: {t:?}");
        assert!(t[2].contains("Anexo cuatro"), "y la 3 el cuatro: {t:?}");
        assert!(t[3].contains("Contrato B"));

        // sin lista sigue entrando el documento entero, como hasta ahora
        crate::historial::undo(work.clone()).expect("deshacer");
        assert_eq!(
            insert_pdf_at(work.clone(), otro.clone(), 2, None).expect("insertar entero"),
            6
        );

        // y un rango que no toca ninguna página se dice, no se traga
        crate::historial::undo(work.clone()).expect("deshacer");
        assert!(insert_pdf_at(work.clone(), otro, 0, Some(vec![9]))
            .unwrap_err()
            .contains("Ninguna de esas páginas"));
        std::fs::remove_file(&destino).ok();
        std::fs::remove_file(&origen).ok();
    }

    /// **El fondo, entero** (orden 1.4 del ciclo 9). Acrobat abre su
    /// diálogo de fondo con **un color sólido** seleccionado, y junto a
    /// «Añadir» tiene «Quitar»: lo que se pone se quita. En Vitela el
    /// fondo era la marca de agua con la casilla «detrás», sin color
    /// sólido, y un fondo de imagen no se podía quitar —`remove_marginal_text`
    /// solo borra objetos de texto—: en cuanto ⌘Z dejaba de alcanzar, se
    /// quedaba puesto para siempre.
    #[test]
    fn el_fondo_se_pone_de_color_o_de_imagen_y_se_quita_entero() {
        let pdf = std::env::temp_dir().join("paginas2-fondo-propio.pdf");
        crea_pdf(&["Contenido uno", "Contenido dos"], &pdf);
        let work = pdf.to_string_lossy().to_string();

        // ni color ni imagen no es un fondo
        assert!(add_background(work.clone(), None, None, None, None)
            .unwrap_err()
            .contains("color o una imagen"));

        // color sólido, a sangre, en las dos páginas
        let puestas = add_background(
            work.clone(),
            Some([250, 240, 200, 255]),
            None,
            None,
            None,
        )
        .expect("fondo de color");
        assert_eq!(puestas, 2);
        let con_fondo = render_rgba(&work);
        let esquina = con_fondo.get_pixel(4, 4).0;
        assert!(
            esquina[0] > 230 && esquina[2] < 230,
            "la esquina tendría que llevar el crema del fondo, hay {esquina:?}"
        );
        // y el contenido sigue entero, debajo no se ha comido nada
        assert!(
            textos(&work)[0].contains("Contenido uno"),
            "el fondo se ha llevado el texto de la página"
        );

        // el ensayo previo cuenta sin tocar
        assert_eq!(remove_background(work.clone(), true).expect("ensayo").objetos, 2);
        assert_eq!(remove_background(work.clone(), false).expect("quitar").objetos, 2);
        assert_eq!(remove_background(work.clone(), true).expect("ensayo").objetos, 0);
        let sin_fondo = render_rgba(&work);
        let esquina = sin_fondo.get_pixel(4, 4).0;
        assert!(
            esquina[0] > 240 && esquina[1] > 240 && esquina[2] > 240,
            "el fondo no se ha ido: {esquina:?}"
        );
        assert!(textos(&work)[0].contains("Contenido uno"));

        // y ahora una imagen, que es el caso que no se podía deshacer
        let mut img = image::RgbaImage::new(64, 64);
        for p in img.pixels_mut() {
            *p = image::Rgba([40, 90, 200, 255]);
        }
        let mut buf = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(img)
            .write_to(&mut buf, image::ImageFormat::Png)
            .expect("png");
        let b64 = {
            use base64::Engine;
            base64::engine::general_purpose::STANDARD.encode(buf.into_inner())
        };
        let pesa_limpio = std::fs::metadata(&work).expect("peso").len();
        assert_eq!(
            add_background(work.clone(), None, Some(b64), None, Some(vec![0]))
                .expect("fondo de imagen"),
            1
        );
        let con_imagen = render_rgba(&work);
        let centro = con_imagen.get_pixel(
            con_imagen.width() / 2,
            con_imagen.height() / 2,
        ).0;
        assert!(
            centro[2] > 150 && centro[0] < 120,
            "el fondo de imagen no se ve: {centro:?}"
        );
        assert_eq!(remove_background(work.clone(), false).expect("quitar").objetos, 1);
        let centro = render_rgba(&work)
            .get_pixel(con_imagen.width() / 2, con_imagen.height() / 2)
            .0;
        assert!(
            centro[0] > 200 && centro[2] > 200,
            "el fondo de imagen se ha quedado puesto: {centro:?}"
        );
        // y los bytes de la imagen se han ido con él
        let pesa_tras = std::fs::metadata(&work).expect("peso").len();
        assert!(
            pesa_tras < pesa_limpio + 2_000,
            "los bytes del fondo siguen dentro: {pesa_limpio} → {pesa_tras}"
        );
        assert!(textos(&work)[0].contains("Contenido uno"));
        std::fs::remove_file(&pdf).ok();
    }

    /// El render de la copia de trabajo, para mirar píxeles.
    fn render_rgba(work: &str) -> image::RgbaImage {
        let png = crate::render_page_png(work.to_string(), 0, 400, true).expect("render");
        image::load_from_memory(&png).expect("leer render").to_rgba8()
    }

    /// **Numeración Bates.** Un juzgado cita «la 000123» y todo el mundo
    /// tiene que mirar el mismo folio: por eso el número lleva dígitos
    /// fijos, prefijo y sufijo, y por eso el correlativo **cuenta folios
    /// numerados, no páginas del documento**. Numerar solo tres páginas de
    /// cinco escribe 1, 2 y 3, no 3, 7 y 8.
    #[test]
    fn el_numero_bates_es_correlativo_y_lleva_sus_digitos() {
        let pdf = std::env::temp_dir().join("paginas2-bates.pdf");
        crea_pdf(&["Uno", "Dos", "Tres", "Cuatro"], &pdf);
        let work = pdf.to_string_lossy().to_string();
        let antes = pasos(&work);

        assert_eq!(
            add_bates(work.clone(), "ABC-".into(), "-2026".into(), 6, 1, None, None, None)
                .expect("numerar"),
            4,
            "dice cuántas páginas ha numerado"
        );
        assert_eq!(pasos(&work), antes + 1, "el documento entero es UN paso");
        let t = textos(&work);
        assert!(t[0].contains("ABC-000001-2026"), "{:?}", t[0]);
        assert!(t[3].contains("ABC-000004-2026"), "{:?}", t[3]);
        crate::historial::undo(work.clone()).expect("deshacer");
        assert!(!textos(&work)[0].contains("ABC-"), "un ⌘Z lo quita entero");

        // un rango: el correlativo cuenta los folios que se numeran
        assert_eq!(
            add_bates(
                work.clone(),
                String::new(),
                String::new(),
                4,
                100,
                None,
                None,
                Some(vec![1, 3])
            )
            .expect("numerar dos"),
            2
        );
        let t = textos(&work);
        assert!(!t[0].contains("0100"), "la primera no se numera: {:?}", t[0]);
        assert!(t[1].contains("0100"), "{:?}", t[1]);
        assert!(t[3].contains("0101"), "el siguiente folio, no la página: {:?}", t[3]);

        // los dígitos no recortan un número que no cabe: perder una cifra
        // sería citar mal el folio
        add_bates(
            work.clone(),
            String::new(),
            String::new(),
            2,
            12345,
            None,
            None,
            Some(vec![0]),
        )
        .expect("numerar corto");
        assert!(textos(&work)[0].contains("12345"));

        // y arriba a la izquierda, que es la otra esquina del diálogo
        add_bates(
            work.clone(),
            "N.º ".into(),
            String::new(),
            3,
            1,
            Some("nw".into()),
            Some(12.0),
            Some(vec![2]),
        )
        .expect("numerar arriba");
        assert!(textos(&work)[2].contains("N.º 001"));
        std::fs::remove_file(&pdf).ok();
    }

    /// **El fondo del documento**, que en Acrobat es «Editar PDF ▸ Fondo»
    /// y aquí es la marca de agua **debajo** del contenido. La diferencia
    /// con la marca de agua de siempre es solo dónde se pinta, y en un PDF
    /// eso es el orden de los objetos de la página: todo lo que se añade va
    /// al final de la lista —o sea, encima—, que es por lo que hasta ahora
    /// no se podía hacer.
    #[test]
    fn la_marca_de_agua_puede_ir_debajo_del_contenido() {
        let pdf = std::env::temp_dir().join("paginas2-fondo.pdf");
        crea_pdf(&["Contenido uno", "Contenido dos"], &pdf);
        let work = pdf.to_string_lossy().to_string();
        let objetos = |p: u16| {
            crate::on_pdfium_thread({
                let work = work.clone();
                move || {
                    crate::with_doc(&work, |doc| {
                        let page = doc.pages().get(p).map_err(|e| e.to_string())?;
                        Ok((0..page.objects().len())
                            .filter_map(|i| page.objects().get(i).ok())
                            .map(|o| format!("{:?}", o.object_type()))
                            .collect::<Vec<_>>())
                    })
                }
            })
            .expect("objetos")
        };
        let antes = objetos(0).len();

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
            Some(true),
        )
        .expect("marca detrás");
        let ahora = objetos(0);
        assert_eq!(ahora.len(), antes + 1, "un objeto más: {ahora:?}");
        assert_eq!(ahora[0], "Text", "y va el primero, debajo de todo: {ahora:?}");
        // el texto del documento se sigue leyendo, que es lo que distingue
        // un fondo de una marca encima
        assert!(textos(&work)[0].contains("Contenido uno"));
        assert!(textos(&work)[0].contains("BORRADOR"));
        crate::render_page_png(work.clone(), 0, 200, true).expect("render con el fondo");

        // y sin la casilla, la marca sigue yendo encima
        crate::historial::undo(work.clone()).expect("deshacer");
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
            None,
        )
        .expect("marca encima");
        let ahora = objetos(0);
        assert_eq!(ahora[ahora.len() - 1], "Text", "la última: {ahora:?}");
        std::fs::remove_file(&pdf).ok();
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
                    top: None,
                    zoom: None,
                    children: vec![],
                },
                crate::documento::OutlineNode {
                    title: "Capítulo 2".into(),
                    page_index: Some(4),
                    top: None,
                    zoom: None,
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
            None,
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
            crate::invalidate_doc_cache(&work_path);
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

/// **El fondo del documento** (Acrobat: «Editar PDF ▸ Fondo»), que es otra
/// cosa que la marca de agua aunque las dos vayan detrás del contenido: el
/// caso por defecto de Acrobat es **un color sólido** a sangre en la
/// página, y hasta el ciclo 9 en Vitela no se podía hacer sin fabricarse
/// antes un PNG.
///
/// **Lo que se pone se puede quitar.** El fondo se escribe con lopdf como
/// un Form XObject marcado con la clave privada `/Vitela /Fondo`, invocado
/// desde un flujo de contenido **propio** que va el primero de la página
/// (y que lleva la misma marca): así [`remove_background`] sabe qué es
/// suyo y lo quita entero, en vez de adivinar por posición como
/// `remove_marginal_text`. Un fondo de imagen puesto y no quitable era un
/// callejón sin salida en cuanto ⌘Z dejaba de alcanzar.
mod fondo {
    use lopdf::{Dictionary, Document as LoDoc, Object, ObjectId, Stream};

    /// El nombre con el que el fondo entra en los recursos de la página.
    /// Es único a propósito: `remove_background` lo busca tal cual en los
    /// flujos de contenido para quitar su `Do`.
    pub(super) const NOMBRE: &[u8] = b"VitelaFondo";
    /// La clave privada que marca lo que ha puesto Vitela.
    pub(super) const MARCA: &[u8] = b"Vitela";
    pub(super) const VALOR: &[u8] = b"Fondo";

    /// ¿Este objeto es un fondo puesto por Vitela?
    fn es_nuestro(doc: &LoDoc, o: &Object) -> bool {
        let dict = match o {
            Object::Reference(id) => doc
                .get_object(*id)
                .ok()
                .and_then(|obj| match obj {
                    Object::Stream(s) => Some(&s.dict),
                    Object::Dictionary(d) => Some(d),
                    _ => None,
                })
                .cloned(),
            Object::Stream(s) => Some(s.dict.clone()),
            Object::Dictionary(d) => Some(d.clone()),
            _ => None,
        };
        dict.map(|d| d.get(MARCA).and_then(|o| o.as_name()).unwrap_or_default() == VALOR)
            .unwrap_or(false)
    }

    /// Los flujos de contenido de una página, como lista de objetos.
    fn contenidos(doc: &LoDoc, page_id: ObjectId) -> Vec<Object> {
        let Ok(page) = doc.get_object(page_id).and_then(|o| o.as_dict()) else {
            return Vec::new();
        };
        match page.get(b"Contents") {
            Ok(Object::Array(a)) => a.clone(),
            Ok(Object::Reference(id)) => match doc.get_object(*id) {
                Ok(Object::Array(a)) => a.clone(),
                _ => vec![Object::Reference(*id)],
            },
            Ok(o) => vec![o.clone()],
            Err(_) => Vec::new(),
        }
    }

    /// El diccionario de recursos de la página, resuelto si va por
    /// referencia. Devuelve también su id cuando lo tiene, para escribirlo
    /// donde vive de verdad.
    fn recursos(doc: &LoDoc, page_id: ObjectId) -> (Dictionary, Option<ObjectId>) {
        let Ok(page) = doc.get_object(page_id).and_then(|o| o.as_dict()) else {
            return (Dictionary::new(), None);
        };
        match page.get(b"Resources") {
            Ok(Object::Reference(id)) => (
                doc.get_object(*id)
                    .and_then(|o| o.as_dict())
                    .cloned()
                    .unwrap_or_default(),
                Some(*id),
            ),
            Ok(Object::Dictionary(d)) => (d.clone(), None),
            _ => (Dictionary::new(), None),
        }
    }

    fn escribe_recursos(doc: &mut LoDoc, page_id: ObjectId, res: Dictionary, res_id: Option<ObjectId>) {
        match res_id {
            Some(id) => {
                if let Ok(o) = doc.get_object_mut(id) {
                    *o = Object::Dictionary(res);
                }
            }
            None => {
                if let Ok(page) = doc.get_object_mut(page_id).and_then(|o| o.as_dict_mut()) {
                    page.set("Resources", Object::Dictionary(res));
                }
            }
        }
    }

    /// ¿Esta página lleva un fondo puesto por Vitela? Es la pregunta del
    /// ensayo previo (`dry_run`), que no puede tocar el documento.
    pub(super) fn hay_en(doc: &LoDoc, page_id: ObjectId) -> bool {
        if contenidos(doc, page_id).iter().any(|o| es_nuestro(doc, o)) {
            return true;
        }
        let (res, _) = recursos(doc, page_id);
        match res.get(b"XObject") {
            Ok(Object::Reference(id)) => doc
                .get_object(*id)
                .and_then(|o| o.as_dict())
                .map(|d| d.has(NOMBRE))
                .unwrap_or(false),
            Ok(Object::Dictionary(d)) => d.has(NOMBRE),
            _ => false,
        }
    }

    /// Quita de una página el fondo que hubiera puesto Vitela: su flujo de
    /// contenido, su entrada en los recursos y, por si un pase de PDFium
    /// hubiera refundido los flujos, la invocación `/VitelaFondo Do` que
    /// quede suelta en los demás. Devuelve si había alguno.
    pub(super) fn quita_de(doc: &mut LoDoc, page_id: ObjectId) -> bool {
        let mut habia = false;
        // 1) los flujos de contenido nuestros
        let flujos = contenidos(doc, page_id);
        let quedan: Vec<Object> = flujos
            .iter()
            .filter(|o| {
                let nuestro = es_nuestro(doc, o);
                habia |= nuestro;
                !nuestro
            })
            .cloned()
            .collect();
        if habia {
            if let Ok(page) = doc.get_object_mut(page_id).and_then(|o| o.as_dict_mut()) {
                page.set("Contents", Object::Array(quedan.clone()));
            }
        }
        // 2) la entrada de los recursos
        let (mut res, res_id) = recursos(doc, page_id);
        if let Ok(Object::Dictionary(xobj)) = res.get(b"XObject").cloned().map(|o| match o {
            Object::Reference(id) => doc
                .get_object(id)
                .cloned()
                .unwrap_or(Object::Dictionary(Dictionary::new())),
            otro => otro,
        }) {
            let mut xobj = xobj;
            if xobj.remove(NOMBRE).is_some() {
                habia = true;
                res.set("XObject", Object::Dictionary(xobj));
                escribe_recursos(doc, page_id, res, res_id);
            }
        }
        // 3) el `Do` que pudiera quedar suelto en otro flujo
        for o in &quedan {
            let Object::Reference(id) = o else { continue };
            let Ok(Object::Stream(s)) = doc.get_object(*id) else {
                continue;
            };
            let Ok(contenido) = s.decompressed_content() else {
                continue;
            };
            if let Some(limpio) = sin_invocacion(&contenido) {
                habia = true;
                if let Ok(Object::Stream(s)) = doc.get_object_mut(*id) {
                    s.set_plain_content(limpio);
                    let _ = s.compress();
                }
            }
        }
        habia
    }

    /// Borra del flujo las apariciones de `/VitelaFondo Do`, dejando
    /// espacios en su sitio (no se recorta el flujo: cualquier otro
    /// desplazamiento rompería lo que venga detrás). `None` si no había.
    fn sin_invocacion(contenido: &[u8]) -> Option<Vec<u8>> {
        let aguja: Vec<u8> = [b"/".as_ref(), NOMBRE].concat();
        let mut out = contenido.to_vec();
        let mut i = 0;
        let mut tocado = false;
        while i + aguja.len() <= out.len() {
            if &out[i..i + aguja.len()] != aguja.as_slice() {
                i += 1;
                continue;
            }
            let mut j = i + aguja.len();
            while j < out.len() && out[j].is_ascii_whitespace() {
                j += 1;
            }
            if out[j..].starts_with(b"Do") {
                for b in &mut out[i..j + 2] {
                    *b = b' ';
                }
                tocado = true;
                i = j + 2;
            } else {
                i += 1;
            }
        }
        tocado.then_some(out)
    }

    /// Mete el fondo en una página: el Form XObject en los recursos y su
    /// flujo de contenido **el primero**, que es lo que hace que se pinte
    /// debajo de todo lo demás sin tocar el contenido que ya había.
    pub(super) fn pon_en(
        doc: &mut LoDoc,
        page_id: ObjectId,
        caja: [f32; 4],
        color: Option<[u8; 4]>,
        imagen: Option<&image::DynamicImage>,
        opacidad: f32,
    ) -> Result<(), String> {
        let (x0, y0, x1, y1) = (caja[0], caja[1], caja[2], caja[3]);
        let (w, h) = (x1 - x0, y1 - y0);
        if w <= 1.0 || h <= 1.0 {
            return Err("La página no tiene tamaño".into());
        }
        let mut recursos_forma = Dictionary::new();
        let mut estados = Dictionary::new();
        let mut gs = Dictionary::new();
        gs.set("Type", Object::Name(b"ExtGState".to_vec()));
        gs.set("ca", Object::Real(opacidad));
        gs.set("CA", Object::Real(opacidad));
        estados.set("VitelaGs", Object::Dictionary(gs));
        recursos_forma.set("ExtGState", Object::Dictionary(estados));

        let mut dibujo = String::from("/VitelaGs gs\n");
        if let Some(c) = color {
            dibujo.push_str(&format!(
                "{:.4} {:.4} {:.4} rg {x0:.2} {y0:.2} {w:.2} {h:.2} re f\n",
                c[0] as f32 / 255.0,
                c[1] as f32 / 255.0,
                c[2] as f32 / 255.0,
            ));
        }
        if let Some(img) = imagen {
            let id = incrusta_imagen(doc, img);
            let mut xobj = Dictionary::new();
            xobj.set("VitelaImg", Object::Reference(id));
            recursos_forma.set("XObject", Object::Dictionary(xobj));
            // la imagen entra entera y sin deformarse, centrada, que es lo
            // que hace el «ajustar a la página» de Acrobat
            let (iw, ih) = (img.width() as f32, img.height() as f32);
            let escala = (w / iw).min(h / ih);
            let (dw, dh) = (iw * escala, ih * escala);
            let (tx, ty) = (x0 + (w - dw) / 2.0, y0 + (h - dh) / 2.0);
            dibujo.push_str(&format!(
                "q {dw:.2} 0 0 {dh:.2} {tx:.2} {ty:.2} cm /VitelaImg Do Q\n"
            ));
        }

        let mut forma = Dictionary::new();
        forma.set("Type", Object::Name(b"XObject".to_vec()));
        forma.set("Subtype", Object::Name(b"Form".to_vec()));
        forma.set(
            "BBox",
            Object::Array(vec![x0.into(), y0.into(), x1.into(), y1.into()]),
        );
        forma.set("Resources", Object::Dictionary(recursos_forma));
        forma.set(MARCA, Object::Name(VALOR.to_vec()));
        let mut stream = Stream::new(forma, dibujo.into_bytes());
        let _ = stream.compress();
        let forma_id = doc.add_object(Object::Stream(stream));

        // los recursos de la página apuntan al fondo
        let (mut res, res_id) = recursos(doc, page_id);
        let mut xobj = match res.get(b"XObject") {
            Ok(Object::Reference(id)) => doc
                .get_object(*id)
                .and_then(|o| o.as_dict())
                .cloned()
                .unwrap_or_default(),
            Ok(Object::Dictionary(d)) => d.clone(),
            _ => Dictionary::new(),
        };
        xobj.set(NOMBRE, Object::Reference(forma_id));
        let xobj_id = match res.get(b"XObject") {
            Ok(Object::Reference(id)) => Some(*id),
            _ => None,
        };
        match xobj_id {
            Some(id) => {
                if let Ok(o) = doc.get_object_mut(id) {
                    *o = Object::Dictionary(xobj);
                }
            }
            None => res.set("XObject", Object::Dictionary(xobj)),
        }
        escribe_recursos(doc, page_id, res, res_id);

        // y el flujo propio, el primero de la página: **detrás** de todo
        let mut dict = Dictionary::new();
        dict.set(MARCA, Object::Name(VALOR.to_vec()));
        let mut nuestro = Stream::new(dict, b"q /VitelaFondo Do Q\n".to_vec());
        let _ = nuestro.compress();
        let nuestro_id = doc.add_object(Object::Stream(nuestro));
        let mut flujos = contenidos(doc, page_id);
        flujos.insert(0, Object::Reference(nuestro_id));
        if let Ok(page) = doc.get_object_mut(page_id).and_then(|o| o.as_dict_mut()) {
            page.set("Contents", Object::Array(flujos));
        }
        Ok(())
    }

    /// La imagen como XObject: RGB en Flate y, si tiene transparencia, su
    /// `/SMask` en gris. Sin JPEG a propósito: recomprimir con pérdida el
    /// fondo que ha elegido el usuario no es cosa nuestra.
    fn incrusta_imagen(doc: &mut LoDoc, img: &image::DynamicImage) -> ObjectId {
        let rgba = img.to_rgba8();
        let (w, h) = (rgba.width(), rgba.height());
        let mut rgb = Vec::with_capacity((w * h * 3) as usize);
        let mut alfa = Vec::with_capacity((w * h) as usize);
        let mut hay_alfa = false;
        for p in rgba.pixels() {
            rgb.extend_from_slice(&p.0[..3]);
            alfa.push(p.0[3]);
            hay_alfa |= p.0[3] < 255;
        }
        let mask_id = hay_alfa.then(|| {
            let mut d = Dictionary::new();
            d.set("Type", Object::Name(b"XObject".to_vec()));
            d.set("Subtype", Object::Name(b"Image".to_vec()));
            d.set("Width", Object::Integer(w as i64));
            d.set("Height", Object::Integer(h as i64));
            d.set("ColorSpace", Object::Name(b"DeviceGray".to_vec()));
            d.set("BitsPerComponent", Object::Integer(8));
            let mut s = Stream::new(d, alfa);
            let _ = s.compress();
            doc.add_object(Object::Stream(s))
        });
        let mut d = Dictionary::new();
        d.set("Type", Object::Name(b"XObject".to_vec()));
        d.set("Subtype", Object::Name(b"Image".to_vec()));
        d.set("Width", Object::Integer(w as i64));
        d.set("Height", Object::Integer(h as i64));
        d.set("ColorSpace", Object::Name(b"DeviceRGB".to_vec()));
        d.set("BitsPerComponent", Object::Integer(8));
        if let Some(id) = mask_id {
            d.set("SMask", Object::Reference(id));
        }
        let mut s = Stream::new(d, rgb);
        let _ = s.compress();
        doc.add_object(Object::Stream(s))
    }
}

/// Pone el fondo del documento: un **color sólido** a sangre en la página
/// (el caso por defecto de Acrobat) o una **imagen** ajustada a la página
/// sin deformarla, en todas las páginas o en las que se pidan.
///
/// Devuelve cuántas páginas lo llevan. Poner un fondo donde ya había otro
/// **sustituye** el anterior, que es lo que hace el «Actualizar» del
/// diálogo de Acrobat.
#[tauri::command(async)]
pub fn add_background(
    work_path: String,
    color: Option<[u8; 4]>,
    image_png: Option<String>,
    opacity: Option<f32>,
    page_indices: Option<Vec<u16>>,
) -> Result<u16, String> {
    let imagen = match image_png.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some(b64) => {
            use base64::Engine;
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(b64.split(',').next_back().unwrap_or_default())
                .map_err(|_| "La imagen del fondo no se ha podido leer")?;
            Some(
                image::load_from_memory(&bytes)
                    .map_err(|e| format!("La imagen del fondo no vale: {e}"))?,
            )
        }
        None => None,
    };
    if color.is_none() && imagen.is_none() {
        return Err("El fondo necesita un color o una imagen".into());
    }
    let opacidad = opacity.unwrap_or(1.0).clamp(0.05, 1.0);
    // el recuento sale de dentro de la cirugía, que se ejecuta en el hilo
    // de PDFium y no puede prestarse una variable de aquí
    let puestas = std::sync::Arc::new(std::sync::atomic::AtomicU16::new(0));
    let contador = puestas.clone();
    crate::cirugia(&work_path, move |doc| {
        let paginas: Vec<lopdf::ObjectId> = doc.get_pages().into_values().collect();
        let total = paginas.len() as u16;
        for i in paginas_pedidas(total, &page_indices) {
            let page_id = paginas[i as usize];
            let caja = crate::formularios2::caja_de_pagina(doc, page_id)?;
            // sustituir, no apilar: dos fondos no son un fondo
            fondo::quita_de(doc, page_id);
            fondo::pon_en(doc, page_id, caja, color, imagen.as_ref(), opacidad)?;
            contador.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
        Ok(())
    })?;
    Ok(puestas.load(std::sync::atomic::Ordering::Relaxed))
}

/// Quita el fondo que puso Vitela, sea color o imagen, de todas las
/// páginas. Con `dry_run` solo cuenta, para que el diálogo pueda decir qué
/// ha encontrado antes de tocar nada.
///
/// **No adivina**: quita lo que lleva la marca `/Vitela /Fondo`. Un fondo
/// que venía dentro del PDF de fuera no se toca, porque no hay forma
/// honesta de distinguirlo del contenido del documento.
#[tauri::command(async)]
pub fn remove_background(work_path: String, dry_run: bool) -> Result<InformeFondo, String> {
    // el fondo de un documento puede ser de dos clases: el que Vitela
    // marca (`objetos`) y el que se puso como marca de agua detrás del
    // contenido, que es un objeto de texto y se reconoce por dónde y cómo
    // está (`textos`, el mismo criterio que `remove_marginal_text`)
    let textos = remove_marginal_text(work_path.clone(), "watermark".into(), dry_run)?.textos;
    if dry_run {
        let objetos = on_pdfium_thread(move || {
            crate::with_lopdf(&work_path, |doc| {
                Ok(doc
                    .get_pages()
                    .into_values()
                    .filter(|id| fondo::hay_en(doc, *id))
                    .count() as u16)
            })
        })
        .map_err(crate::mensaje_llano)?;
        return Ok(InformeFondo { objetos, textos });
    }
    let quitados = std::sync::Arc::new(std::sync::atomic::AtomicU16::new(0));
    let contador = quitados.clone();
    crate::cirugia(&work_path, move |doc| {
        let paginas: Vec<lopdf::ObjectId> = doc.get_pages().into_values().collect();
        for page_id in paginas {
            if fondo::quita_de(doc, page_id) {
                contador.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            }
        }
        // los bytes de la imagen se van con él: dejarlos dentro sería el
        // mismo defecto que tenía borrar un adjunto
        doc.prune_objects();
        Ok(())
    })?;
    Ok(InformeFondo {
        objetos: quitados.load(std::sync::atomic::Ordering::Relaxed),
        textos,
    })
}

/// Lo que se ha quitado al quitar el fondo, por clases: los objetos que
/// Vitela había marcado y el texto puesto detrás del contenido.
#[derive(serde::Serialize, Debug)]
pub struct InformeFondo {
    pub objetos: u16,
    pub textos: u32,
}
