//! Salida: exportar páginas como imágenes, extraer el texto plano y
//! comprimir el documento recomprimiendo sus imágenes.

use crate::{invalidate_doc_cache, on_pdfium_thread, pdfium, with_doc};
use crate::historial::mutacion;
use pdfium_render::prelude::*;
use serde::Serialize;
use std::io::Cursor;

/// Exporta todas las páginas como PNG o JPEG al directorio dado, a la
/// resolución pedida. Devuelve las rutas escritas.
#[tauri::command(async)]
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
                    .map_err(|e| format!("No se ha podido escribir {}: {e}", dest.display()))?;
                out.push(dest.to_string_lossy().into_owned());
            }
            Ok(out)
        })
    })
}

/// Vuelca el texto de todas las páginas a un fichero de texto plano.
#[tauri::command(async)]
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
            std::fs::write(&dest_path, out).map_err(|e| format!("No se ha podido escribir: {e}"))
        })
    })
}

#[derive(Serialize, Debug)]
pub struct CompressReport {
    pub antes: u64,
    pub despues: u64,
    pub imagenes: u32,
}

/// Comprime el documento recomprimiendo sus imágenes a JPEG con la calidad
/// dada y submuestreando las que superen `max_dpi` respecto a su tamaño en
/// página. Se saltan las imágenes con transparencia (JPEG la perdería), las
/// rotadas (la reinserción solo maneja imágenes sin rotar) y las que no
/// ganarían nada (ni hay que bajarles la resolución ni el JPEG sale menor
/// que su flujo original). Si aun así el resultado no es más pequeño,
/// devuelve Err y deja el fichero intacto.
#[tauri::command(async)]
pub fn compress_pdf(work_path: String, quality: u8, max_dpi: u16) -> Result<CompressReport, String> {
    let quality = quality.clamp(30, 95);
    let max_dpi = max_dpi.clamp(72, 600) as f32;
    let antes = std::fs::metadata(&work_path)
        .map(|m| m.len())
        .unwrap_or(0);
    mutacion(work_path, |work_path| on_pdfium_thread(move || {
        let pdfium = pdfium()?;
        let doc = pdfium
            .load_pdf_from_file(&work_path, None)
            .map_err(|e| e.to_string())?;
        let mut recomprimidas = 0u32;
        // imágenes vistas (aunque se salten): sirve para distinguir «aquí no
        // había imágenes» de «las que había ya estaban bien»
        let mut imagenes_vistas = 0u32;
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
                    imagenes_vistas += 1;
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
                    // tamaño del flujo tal cual está guardado, sin aplicar
                    // filtros: con qué hay que comparar el JPEG nuevo
                    let original = img_obj
                        .get_raw_image_data()
                        .map(|d| d.len())
                        .unwrap_or(0);
                    let rgba = raw.to_rgba8();
                    if rgba.pixels().any(|px| px[3] < 250) {
                        continue; // transparencia: JPEG la perdería
                    }
                    let dpi_efectivo = rgba.width() as f32 / (w_pts / 72.0);
                    let objetivo_px = (w_pts / 72.0 * max_dpi).round().max(16.0) as u32;
                    let submuestrear = dpi_efectivo > max_dpi && objetivo_px < rgba.width();
                    let img = if submuestrear {
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
                    // solo merece la pena si hay que bajar la resolución o si
                    // el JPEG ocupa menos que el flujo original (un PNG de
                    // color plano, por ejemplo, ya está mejor comprimido)
                    if !submuestrear && (original == 0 || jpeg.len() >= original) {
                        continue;
                    }
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
        // dos motivos distintos, los dos con la misma cabeza para no
        // romper el contrato con la UI («No se ha podido reducir…»)
        const SIN_IMAGENES: &str =
            "No se ha podido reducir el tamaño: este documento no tiene imágenes que comprimir";
        const SIN_REDUCIR: &str =
            "No se ha podido reducir el tamaño: las imágenes ya están comprimidas";
        if imagenes_vistas == 0 {
            return Err(SIN_IMAGENES.into());
        }
        if recomprimidas == 0 {
            return Err(SIN_REDUCIR.into());
        }
        // guardar aparte y quedarse con el resultado solo si es más pequeño;
        // si no, la copia de trabajo se queda como estaba (y la mutación
        // fallida retira su instantánea)
        let tmp = format!("{work_path}.comprimido.tmp");
        doc.save_to_file(&tmp).map_err(|e| e.to_string())?;
        drop(doc);
        let despues = std::fs::metadata(&tmp).map(|m| m.len()).unwrap_or(0);
        if despues == 0 || despues >= antes {
            let _ = std::fs::remove_file(&tmp);
            return Err(SIN_REDUCIR.into());
        }
        invalidate_doc_cache();
        std::fs::rename(&tmp, &work_path).map_err(|e| e.to_string())?;
        Ok(CompressReport {
            antes,
            despues,
            imagenes: recomprimidas,
        })
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::crea_pdf;
    use base64::Engine;

    /// Un fichero del `.docx` (que es un zip), como texto.
    fn dentro_del_docx(docx: &std::path::Path, fichero: &str) -> String {
        let f = std::fs::File::open(docx).expect("abrir el .docx");
        let mut zip = zip::ZipArchive::new(f).expect("el .docx tiene que ser un zip válido");
        let mut entrada = zip
            .by_name(fichero)
            .unwrap_or_else(|_| panic!("el .docx no lleva {fichero}"));
        let mut s = String::new();
        std::io::Read::read_to_string(&mut entrada, &mut s).expect("leer");
        s
    }

    /// **G2.** Exportar a Word: los párrafos en orden de lectura, con su
    /// fuente, su tamaño y su color, las imágenes incrustadas y un salto de
    /// página entre páginas. Y el informe honesto de lo que se queda
    /// fuera: un PDF no guarda columnas ni tablas, y no se inventan.
    #[test]
    fn exportar_a_word_lleva_texto_en_orden_color_fuente_e_imagenes() {
        let dir = std::env::temp_dir();
        let pdf = dir.join("exportar-docx-test.pdf");
        let png = dir.join("exportar-docx-test.png");
        let docx = dir.join("exportar-docx-test.docx");
        crea_pdf(&["Primera pagina", "Segunda pagina"], &pdf);
        let work = pdf.to_string_lossy().into_owned();
        image::RgbaImage::from_pixel(60, 30, image::Rgba([20, 120, 220, 255]))
            .save(&png)
            .expect("crear png");

        // un titular rojo en Times arriba del todo y una imagen debajo
        crate::texto::add_text_block(
            work.clone(),
            0,
            50.0,
            60.0,
            "Titular en rojo".into(),
            20.0,
            Some("Times-Bold".into()),
            Some([220, 20, 20, 255]),
            None,
        )
        .expect("titular");
        crate::imagenes::add_image(work.clone(), 0, png.to_string_lossy().into_owned(), 60.0, 300.0)
            .expect("imagen");

        let informe = export_docx(work.clone(), docx.to_string_lossy().into_owned(), None)
            .expect("exportar a docx");
        assert_eq!(informe.parrafos, 3, "dos textos de la página 1 y uno de la 2");
        assert_eq!(informe.imagenes, 1);
        assert!(
            informe.perdido.iter().any(|p| p.contains("columnas")),
            "el informe tiene que decir que la maquetación no sale: {:?}",
            informe.perdido
        );

        let xml = dentro_del_docx(&docx, "word/document.xml");
        let pos = |aguja: &str| xml.find(aguja).unwrap_or_else(|| panic!("falta {aguja} en el XML"));
        // orden de lectura: el titular (y=60) antes que el texto de la
        // página (y≈100) y que la segunda página
        assert!(
            pos("Titular en rojo") < pos("Primera pagina"),
            "el titular va más arriba en la página, así que va antes"
        );
        assert!(pos("Primera pagina") < pos("Segunda pagina"));
        // fuente, tamaño, color y estilo del titular
        assert!(xml.contains("w:ascii=\"Times\""), "la familia del titular");
        assert!(xml.contains("w:val=\"40\""), "20 pt son 40 medios puntos");
        assert!(xml.contains("DC1414"), "el color rojo del titular");
        // el salto de página entre las dos páginas
        assert!(xml.contains("w:type=\"page\""), "falta el salto de página");
        // la imagen, con su relación
        assert!(xml.contains("<w:drawing>"), "falta la imagen");
        let rels = dentro_del_docx(&docx, "word/_rels/document.xml.rels");
        assert!(
            rels.contains("/image") && rels.contains(".png"),
            "la imagen tiene que estar relacionada: {rels}"
        );

        // un rango de páginas exporta solo eso
        let informe = export_docx(work.clone(), docx.to_string_lossy().into_owned(), Some(vec![1]))
            .expect("exportar la segunda");
        assert_eq!(informe.parrafos, 1);
        assert_eq!(informe.imagenes, 0);
        let xml = dentro_del_docx(&docx, "word/document.xml");
        assert!(!xml.contains("Primera pagina"), "solo la página pedida");
        assert!(!xml.contains("w:type=\"page\""), "una sola página, sin salto");

        for f in [&pdf, &png, &docx] {
            std::fs::remove_file(f).ok();
        }
    }

    /// «Reducir tamaño» sobre un documento sin ninguna imagen decía que «las
    /// imágenes ya están comprimidas». No hay imágenes: el motivo es otro.
    #[test]
    fn comprimir_sin_imagenes_lo_dice() {
        let pdf = std::env::temp_dir().join("exportar-comprimir-sin-imagenes-test.pdf");
        crea_pdf(&["Solo texto"], &pdf);
        let work = pdf.to_string_lossy().to_string();
        let e = compress_pdf(work, 70, 150).unwrap_err();
        assert!(
            e.starts_with("No se ha podido reducir"),
            "la cabeza del mensaje es el contrato con la UI: {e}"
        );
        assert!(
            e.contains("no tiene imágenes"),
            "un documento sin imágenes no puede decir que ya están comprimidas: {e}"
        );
        std::fs::remove_file(&pdf).ok();
    }

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
    fn comprimir_no_agranda_un_png_plano() {
        let dir = std::env::temp_dir();
        let pdf = dir.join("exportar-compress-plano-test.pdf");
        crea_pdf(&["Con dibujo plano"], &pdf);
        let work = pdf.to_string_lossy().to_string();
        // PNG 200x120 de color plano: Flate lo deja en unos cientos de bytes,
        // el JPEG equivalente ocupa más; a 72 dpi tampoco hay que reducir
        let mut img = image::RgbaImage::new(200, 120);
        for (_, _, p) in img.enumerate_pixels_mut() {
            *p = image::Rgba([30, 80, 200, 255]);
        }
        let mut buf = Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(img)
            .write_to(&mut buf, image::ImageFormat::Png)
            .unwrap();
        let b64 = base64::engine::general_purpose::STANDARD.encode(buf.into_inner());
        crate::firmas_visuales::stamp_signature(work.clone(), 0, b64, 50.0, 200.0, 200.0, 120.0)
            .expect("insertar imagen");

        let antes = std::fs::read(&pdf).expect("leer antes");
        let e = match compress_pdf(work.clone(), 75, 150) {
            Err(e) => e,
            Ok(r) => panic!("no debería reducir: {} → {}", r.antes, r.despues),
        };
        assert!(
            e.starts_with("No se ha podido reducir"),
            "mensaje inesperado: {e}"
        );
        let despues = std::fs::read(&pdf).expect("leer después");
        assert!(antes == despues, "el fichero cambió al no poder reducir");
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
        let imgs = crate::imagenes::get_images(work, 0).expect("listar");
        assert_eq!(imgs.len(), 1);
        assert!((imgs[0].w - 300.0).abs() < 2.0);
    }
}

/// Lo que ha salido en el `.docx` y lo que se ha quedado por el camino.
/// La UI avisa **antes** de pedir destino; esto es para decir después qué
/// ha pasado de verdad con este documento concreto.
#[derive(Serialize, Debug, Default)]
pub struct DocxReport {
    /// Párrafos escritos (un bloque de texto del PDF, un párrafo).
    pub parrafos: u32,
    /// Imágenes incrustadas.
    pub imagenes: u32,
    /// Lo que el `.docx` NO lleva, en llano y sin excusas, para que la UI
    /// lo cuente sin adornarlo.
    pub perdido: Vec<String>,
}

/// Exporta a Word (`.docx`) lo que Vitela sabe leer del PDF: un párrafo por
/// bloque de texto, ordenados de arriba abajo y de izquierda a derecha, con
/// su familia, tamaño, negrita, cursiva y color; las imágenes en su sitio
/// aproximado; y un salto de página por página.
///
/// **Es una aproximación, y a propósito.** Un PDF no guarda párrafos,
/// columnas ni tablas: guarda trozos de texto colocados en un papel.
/// Reconstruir la maquetación es adivinar, y adivinar mal en un contrato es
/// peor que entregar texto corrido. Así que **no se intenta detectar tablas
/// ni columnas**: el resultado es el texto en orden de lectura, que es lo
/// que sirve para reescribir un documento sencillo en Word.
///
/// Escribe fuera de la copia de trabajo, así que no muta el documento ni
/// deja paso de deshacer.
#[tauri::command(async)]
pub fn export_docx(
    work_path: String,
    dest_path: String,
    page_indices: Option<Vec<u16>>,
) -> Result<DocxReport, String> {
    use docx_rs::{BreakType, Docx, Paragraph, Pic, Run, RunFonts};
    on_pdfium_thread(move || {
        // documento propio de solo lectura: `get_processed_image`
        // transforma el objeto de imagen y sobre el documento cacheado
        // falsearía los bounds del render (lo mismo que `get_image_data`)
        let doc = pdfium()?
            .load_pdf_from_file(&work_path, None)
            .map_err(crate::mensaje_llano)?;
        let total = doc.pages().len();
        let paginas: Vec<u16> = match page_indices {
            Some(v) => v.into_iter().filter(|i| *i < total).collect(),
            None => (0..total).collect(),
        };
        if paginas.is_empty() {
            return Err("No hay ninguna página que exportar".into());
        }

        let mut docx = Docx::new();
        let mut informe = DocxReport::default();
        let (mut ilegibles, mut vectores, mut comentarios, mut campos) = (0u32, false, false, false);

        for (orden, &pi) in paginas.iter().enumerate() {
            let Ok(page) = doc.pages().get(pi) else { continue };
            let geo = crate::Geo::de_pagina(&page).propia();
            // texto e imágenes en una sola lista, ordenada como se lee
            let mut trozos: Vec<(i32, i32, Trozo)> = Vec::new();
            for b in crate::texto::bloques_de(&doc, pi) {
                trozos.push((b.y.round() as i32, b.x.round() as i32, Trozo::Texto(b)));
            }
            let objetos = page.objects();
            for i in 0..objetos.len() {
                let Ok(obj) = objetos.get(i) else { continue };
                if obj.as_path_object().is_some() {
                    vectores = true;
                    continue;
                }
                let Some(img) = obj.as_image_object() else { continue };
                let Ok(b) = obj.bounds() else { continue };
                let caja = geo.pdf_rect_a_ui(&PdfRect::new(b.bottom(), b.left(), b.top(), b.right()));
                match png_de_imagen(img, &doc) {
                    Some((png, ancho_px, alto_px)) => trozos.push((
                        caja.y.round() as i32,
                        caja.x.round() as i32,
                        Trozo::Imagen {
                            png,
                            ancho_px,
                            alto_px,
                            ancho_pt: caja.w,
                            alto_pt: caja.h,
                        },
                    )),
                    None => ilegibles += 1,
                }
            }
            trozos.sort_by_key(|(y, x, _)| (*y, *x));

            for (_, _, trozo) in trozos {
                match trozo {
                    Trozo::Texto(b) => {
                        // Word mide en medios puntos
                        let mut run = Run::new()
                            .add_text(b.text.clone())
                            .size(((b.font_size.round().max(1.0)) as usize) * 2)
                            .fonts(RunFonts::new().ascii(b.font_family.clone()))
                            .color(format!(
                                "{:02X}{:02X}{:02X}",
                                b.color[0], b.color[1], b.color[2]
                            ));
                        if b.negrita {
                            run = run.bold();
                        }
                        if b.cursiva {
                            run = run.italic();
                        }
                        docx = docx.add_paragraph(Paragraph::new().add_run(run));
                        informe.parrafos += 1;
                    }
                    Trozo::Imagen {
                        png,
                        ancho_px,
                        alto_px,
                        ancho_pt,
                        alto_pt,
                    } => {
                        // EMU: 914400 por pulgada, 12700 por punto PDF
                        let pic = Pic::new_with_dimensions(png, ancho_px, alto_px)
                            .size((ancho_pt * 12700.0) as u32, (alto_pt * 12700.0) as u32);
                        docx = docx
                            .add_paragraph(Paragraph::new().add_run(Run::new().add_image(pic)));
                        informe.imagenes += 1;
                    }
                }
            }

            for a in page.annotations().iter() {
                match a.annotation_type() {
                    PdfPageAnnotationType::Widget => campos = true,
                    PdfPageAnnotationType::Popup => {}
                    _ => comentarios = true,
                }
            }

            if orden + 1 < paginas.len() {
                docx = docx.add_paragraph(
                    Paragraph::new().add_run(Run::new().add_break(BreakType::Page)),
                );
            }
        }

        informe.perdido.push(
            "La maquetación: las columnas y las tablas salen como texto corrido".into(),
        );
        if vectores {
            informe
                .perdido
                .push("Los dibujos vectoriales (líneas, recuadros y fondos)".into());
        }
        if comentarios {
            informe.perdido.push("Los comentarios y las anotaciones".into());
        }
        if campos {
            informe
                .perdido
                .push("Los campos de formulario (sale el documento, no el formulario)".into());
        }
        if ilegibles > 0 {
            informe.perdido.push(if ilegibles == 1 {
                "Una imagen que no se ha podido leer".to_string()
            } else {
                format!("{ilegibles} imágenes que no se han podido leer")
            });
        }

        let fichero = std::fs::File::create(&dest_path)
            .map_err(|e| crate::mensaje_llano(format!("No se ha podido escribir {dest_path}: {e}")))?;
        docx.build()
            .pack(fichero)
            .map_err(|e| crate::mensaje_llano(format!("No se ha podido escribir {dest_path}: {e}")))?;
        Ok(informe)
    })
}

/// Un trozo de página que va al `.docx`, con su sitio para ordenarlo.
enum Trozo {
    Texto(crate::texto::TextBlock),
    Imagen {
        png: Vec<u8>,
        ancho_px: u32,
        alto_px: u32,
        ancho_pt: f32,
        alto_pt: f32,
    },
}

/// El bitmap de un objeto de imagen, en PNG y con su tamaño en píxeles.
/// `None` si PDFium no sabe descomprimirlo (JBIG2, JPX raros): esa imagen
/// se cuenta como perdida en vez de tirar la exportación entera.
fn png_de_imagen(
    img: &PdfPageImageObject,
    doc: &PdfDocument<'static>,
) -> Option<(Vec<u8>, u32, u32)> {
    let bitmap = img.get_processed_image(doc).ok()?;
    let (ancho, alto) = (bitmap.width(), bitmap.height());
    let mut buf = Cursor::new(Vec::new());
    bitmap.write_to(&mut buf, image::ImageFormat::Png).ok()?;
    Some((buf.into_inner(), ancho, alto))
}
