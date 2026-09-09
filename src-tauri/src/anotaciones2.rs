//! Anotaciones nuevas: subrayado/tachado (markup con quadpoints, como el
//! resaltado), formas geométricas (Ink con su path dentro, como los trazos:
//! es la única vía que renderiza sin /AP y se borra como anotación) y sellos
//! (Stamp con borde + texto dentro).

use crate::anotaciones::{remata_annot, remata_annot_en, EstiloMarca};
use crate::historial::mutacion;
use crate::{on_pdfium_thread, pdfium, save_and_close, Geo, Rect};
use pdfium_render::prelude::*;

fn color_de(c: [u8; 4]) -> PdfColor {
    PdfColor::new(c[0], c[1], c[2], c[3])
}

/// Marca de texto sobre los rects dados (coords de UI): resaltado, subrayado
/// o tachado. Igual que `add_highlight` pero con subtipo y color a elegir.
#[tauri::command(async)]
pub fn add_markup(
    work_path: String,
    page_index: u16,
    rects: Vec<Rect>,
    kind: String,
    color: Option<[u8; 4]>,
    author: Option<String>,
) -> Result<(), String> {
    if rects.is_empty() {
        return Err("No hay nada que marcar".into());
    }
    mutacion(work_path, |work_path| on_pdfium_thread(move || {
        let pdfium = pdfium()?;
        let doc = pdfium
            .load_pdf_from_file(&work_path, None)
            .map_err(|e| e.to_string())?;
        let mut page = doc.pages().get(page_index).map_err(|e| e.to_string())?;
        let geo = Geo::de_pagina(&page).propia();
        let left = rects.iter().map(|r| r.x).fold(f32::MAX, f32::min);
        let top = rects.iter().map(|r| r.y).fold(f32::MAX, f32::min);
        let right = rects.iter().map(|r| r.x + r.w).fold(f32::MIN, f32::max);
        let bottom = rects.iter().map(|r| r.y + r.h).fold(f32::MIN, f32::max);
        let envelope = geo.ui_rect_a_pdf(&Rect {
            x: left,
            y: top,
            w: right - left,
            h: bottom - top,
        });
        // los tres subtipos comparten API pero son tipos distintos sin trait
        // común para los quadpoints: macro local en vez de duplicar
        macro_rules! configurar {
            ($annot:expr, $default:expr) => {{
                let mut annot = $annot.map_err(|e| e.to_string())?;
                // flag Print: sin él, aplanar (FLAT_PRINT) la descarta
                annot.set_is_printed(true).map_err(|e| e.to_string())?;
                annot
                    .set_stroke_color(color_de(color.unwrap_or($default)))
                    .map_err(|e| e.to_string())?;
                annot.set_bounds(envelope).map_err(|e| e.to_string())?;
                let points = annot.attachment_points_mut();
                for r in &rects {
                    let pr = geo.ui_rect_a_pdf(r);
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
        let estilo = match kind.as_str() {
            "highlight" => EstiloMarca::Resaltado,
            "underline" => EstiloMarca::Subrayado,
            "strikeout" => EstiloMarca::Tachado,
            otro => return Err(format!("Tipo de marca desconocido: {otro}")),
        };
        {
            let annotations = page.annotations_mut();
            match estilo {
                EstiloMarca::Resaltado => {
                    configurar!(annotations.create_highlight_annotation(), [255, 220, 0, 140])
                }
                EstiloMarca::Subrayado => {
                    configurar!(annotations.create_underline_annotation(), [46, 160, 67, 255])
                }
                EstiloMarca::Tachado => {
                    configurar!(annotations.create_strikeout_annotation(), [226, 61, 61, 255])
                }
            }
        }
        drop(page);
        save_and_close(doc, &work_path)?;
        // segundo pase: PDFium genera la apariencia en memoria pero no la
        // escribe, así que la marca no existiría fuera de Vitela
        remata_annot(&work_path, page_index, Some(estilo), author)
    }))
}

/// Forma geométrica entre dos puntos (coords de UI): rectángulo, elipse,
/// línea o flecha. Va como anotación Ink con el path dentro para que
/// renderice en cualquier visor y se pueda borrar individualmente.
// la firma es el contrato con la UI: un argumento por propiedad de la forma
#[allow(clippy::too_many_arguments)]
#[tauri::command(async)]
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
    author: Option<String>,
) -> Result<(), String> {
    mutacion(work_path, |work_path| on_pdfium_thread(move || {
        let pdfium = pdfium()?;
        let doc = pdfium
            .load_pdf_from_file(&work_path, None)
            .map_err(|e| e.to_string())?;
        let mut page = doc.pages().get(page_index).map_err(|e| e.to_string())?;
        let geo = Geo::de_pagina(&page).propia();
        let stroke_color = color_de(stroke);
        let width = PdfPoints::new(stroke_width.max(0.5));
        let fill_color = fill.map(color_de);

        // coords PDF (origen abajo-izquierda y sin rotar)
        let (px1, py1) = geo.ui_a_pdf(x1, y1);
        let (px2, py2) = geo.ui_a_pdf(x2, y2);
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
        annot.set_is_printed(true).map_err(|e| e.to_string())?;
        // /C antes de añadir objetos (con /AP PDFium ya no deja fijarlo);
        // es lo que lee get_annotations para pintar los overlays
        annot
            .set_stroke_color(stroke_color)
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
        drop(page);
        save_and_close(doc, &work_path)?;
        remata_annot(&work_path, page_index, None, author)
    }))
}

/// Sello de texto (APROBADO, BORRADOR…): anotación Stamp con un borde y el
/// texto dentro, centrado en el punto dado (coords de UI).
// la firma es el contrato con la UI: un argumento por propiedad del sello
#[allow(clippy::too_many_arguments)]
#[tauri::command(async)]
pub fn add_stamp(
    work_path: String,
    page_index: u16,
    text: String,
    color: [u8; 4],
    x: f32,
    y: f32,
    font_size: f32,
    author: Option<String>,
) -> Result<(), String> {
    let text = text.trim().to_string();
    if text.is_empty() {
        return Err("El sello está vacío".into());
    }
    mutacion(work_path, |work_path| on_pdfium_thread(move || {
        let pdfium = pdfium()?;
        let mut doc = pdfium
            .load_pdf_from_file(&work_path, None)
            .map_err(|e| e.to_string())?;
        let font = doc.fonts_mut().helvetica_bold();
        let mut page = doc.pages().get(page_index).map_err(|e| e.to_string())?;
        let vista = Geo::de_pagina(&page);
        let rot = vista.rot;
        // el punto llega en el espacio propio de la página (sin rotar)
        let geo = vista.propia();
        let size = font_size.clamp(8.0, 96.0);
        // Helvetica Bold en mayúsculas ronda 0.66 em de media por carácter
        let text_w = text.chars().count() as f32 * size * 0.66;
        let pad = size * 0.45;
        let w = text_w + pad * 2.0;
        let h = size + pad * 2.0;
        // si la página se ve girada, el sello va cruzado en la página para
        // leerse derecho en pantalla, que es lo que hace Acrobat
        let (cw, ch) = if rot == 90 || rot == 270 { (h, w) } else { (w, h) };
        let caja = geo.ui_rect_a_pdf(&Rect {
            x: x - cw / 2.0,
            y: y - ch / 2.0,
            w: cw,
            h: ch,
        });
        let c = color_de(color);

        let mut annot = page
            .annotations_mut()
            .create_stamp_annotation()
            .map_err(|e| e.to_string())?;
        annot.set_is_printed(true).map_err(|e| e.to_string())?;
        annot.set_stroke_color(c).map_err(|e| e.to_string())?;
        annot
            .set_bounds(PdfRect::new(
                PdfPoints::new(caja.bottom().value - 2.0),
                PdfPoints::new(caja.left().value - 2.0),
                PdfPoints::new(caja.top().value + 2.0),
                PdfPoints::new(caja.right().value + 2.0),
            ))
            .map_err(|e| e.to_string())?;
        let border = PdfPagePathObject::new_rect(
            &doc,
            caja,
            Some(c),
            Some(PdfPoints::new((size * 0.09).max(1.2))),
            None,
        )
        .map_err(|e| e.to_string())?;
        let mut texto = PdfPageTextObject::new(&doc, &text, font, PdfPoints::new(size))
            .map_err(|e| e.to_string())?;
        texto.set_fill_color(c).map_err(|e| e.to_string())?;
        // el texto se gira al revés que la página y arranca en la esquina
        // que, en pantalla, es la de abajo a la izquierda de la línea base
        let (izq, aba, der, arr) = (
            caja.left().value,
            caja.bottom().value,
            caja.right().value,
            caja.top().value,
        );
        let base = size * 0.14;
        let (tx, ty) = match rot {
            90 => (der - pad - base, aba + pad),
            180 => (der - pad, arr - pad - base),
            270 => (izq + pad + base, arr - pad),
            _ => (izq + pad, aba + pad + base),
        };
        let rad = (rot as f32).to_radians();
        let (sen, cos) = (rad.sin(), rad.cos());
        texto
            .transform(cos, sen, -sen, cos, tx, ty)
            .map_err(|e| e.to_string())?;
        annot
            .objects_mut()
            .add_path_object(border)
            .map_err(|e| e.to_string())?;
        annot
            .objects_mut()
            .add_text_object(texto)
            .map_err(|e| e.to_string())?;
        drop(page);
        save_and_close(doc, &work_path)?;
        remata_annot(&work_path, page_index, None, author)
    }))
}

/// Texto para un stream de contenido en WinAnsi (≈ latin-1, así que los
/// acentos del español entran), escapando `\`, `(` y `)`.
fn winansi(texto: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(texto.len());
    for c in texto.chars() {
        match c {
            '\\' | '(' | ')' => {
                out.push(b'\\');
                out.push(c as u8);
            }
            c if (c as u32) < 0x20 => out.push(b' '),
            c if (c as u32) <= 0xFF => out.push(c as u8),
            _ => out.push(b'?'),
        }
    }
    out
}

/// Apariencia (`/AP /N`) de un cuadro de texto: el borde opcional y las
/// líneas del texto en Helvetica, dibujadas en local (`/BBox 0 0 w h`).
/// La comparten la creación y la reescritura del texto: sin regenerarla,
/// corregir el cuadro cambiaría el `/Contents` y no lo que se ve.
pub(crate) fn apariencia_freetext(
    doc: &mut lopdf::Document,
    w: f32,
    h: f32,
    texto: &str,
    size: f32,
    color: [f32; 3],
    border: bool,
) -> lopdf::ObjectId {
    use lopdf::{Dictionary, Object, Stream};
    let [r, g, b] = color;
    let mut ops = Vec::new();
    if border {
        ops.extend_from_slice(
            format!(
                "q {r:.4} {g:.4} {b:.4} RG 1 w 0.5 0.5 {:.2} {:.2} re S Q\n",
                w - 1.0,
                h - 1.0
            )
            .as_bytes(),
        );
    }
    let pad = (size * 0.35).max(2.0);
    let interlineado = size * 1.2;
    ops.extend_from_slice(
        format!(
            "BT /Helv {size:.2} Tf {interlineado:.2} TL {r:.4} {g:.4} {b:.4} rg {pad:.2} {:.2} Td\n",
            h - pad - size * 0.85
        )
        .as_bytes(),
    );
    for (n, linea) in texto.split('\n').enumerate() {
        if n > 0 {
            ops.extend_from_slice(b"T* ");
        }
        ops.push(b'(');
        ops.extend_from_slice(&winansi(linea));
        ops.extend_from_slice(b") Tj\n");
    }
    ops.extend_from_slice(b"ET\n");

    let helv = crate::seguridad::fuente_helvetica(doc);
    let mut fuentes = Dictionary::new();
    fuentes.set("Helv", Object::Reference(helv));
    let mut recursos = Dictionary::new();
    recursos.set("Font", Object::Dictionary(fuentes));
    let mut forma = Dictionary::new();
    forma.set("Type", Object::Name(b"XObject".to_vec()));
    forma.set("Subtype", Object::Name(b"Form".to_vec()));
    forma.set("FormType", 1i64);
    forma.set(
        "BBox",
        Object::Array(vec![0.into(), 0.into(), w.into(), h.into()]),
    );
    forma.set("Resources", Object::Dictionary(recursos));
    doc.add_object(Stream::new(forma, ops))
}

/// Tamaño de fuente y color de un `/DA` (`/Helv 12 Tf 1 0 0 rg`).
pub(crate) fn lee_da(da: &str) -> (f32, [f32; 3]) {
    let piezas: Vec<&str> = da.split_whitespace().collect();
    let mut size = 12.0;
    let mut color = [0.0, 0.0, 0.0];
    for (i, p) in piezas.iter().enumerate() {
        match *p {
            "Tf" if i >= 1 => {
                if let Ok(v) = piezas[i - 1].parse::<f32>() {
                    if v > 0.0 {
                        size = v;
                    }
                }
            }
            "rg" if i >= 3 => {
                let v: Vec<f32> = piezas[i - 3..i]
                    .iter()
                    .filter_map(|n| n.parse::<f32>().ok())
                    .collect();
                if v.len() == 3 {
                    color = [v[0], v[1], v[2]];
                }
            }
            "g" if i >= 1 => {
                if let Ok(v) = piezas[i - 1].parse::<f32>() {
                    color = [v, v, v];
                }
            }
            _ => {}
        }
    }
    (size, color)
}

/// Cuadro de texto: el comentario que Acrobat llama así, escrito ENCIMA del
/// documento sin tocar su contenido (a diferencia de «Añadir texto», que
/// reescribe el content stream). Se arrastra un rectángulo, se escribe
/// dentro y queda una caja con borde opcional, sin relleno, que se mueve,
/// se redimensiona, se borra y sale en la lista de comentarios.
///
/// Se construye entero con lopdf: `/Subtype /FreeText`, `/DA`, `/Contents`,
/// `/C` y un `/AP` propio —PDFium no escribe la apariencia de las
/// anotaciones de marcado, y sin `/AP` la caja no existe fuera de Vitela.
// la firma es el contrato con la UI: un argumento por propiedad del cuadro
#[allow(clippy::too_many_arguments)]
#[tauri::command(async)]
pub fn add_free_text(
    work_path: String,
    page_index: u16,
    rect: Rect,
    text: String,
    font_size: f32,
    color: [u8; 4],
    border: bool,
    author: Option<String>,
) -> Result<(), String> {
    if text.trim().is_empty() {
        return Err("El cuadro de texto está vacío".into());
    }
    if rect.w < 8.0 || rect.h < 8.0 {
        return Err("El cuadro de texto es demasiado pequeño".into());
    }
    let size = font_size.clamp(6.0, 96.0);
    let autor = crate::anotaciones::autor_o_sistema(author);
    let fecha = crate::anotaciones::fecha_pdf_ahora();
    crate::cirugia(&work_path, move |doc| {
        use lopdf::{Dictionary, Object};
        let page_id = *doc
            .get_pages()
            .get(&(page_index as u32 + 1))
            .ok_or("Página fuera de rango")?;
        let geo = crate::formularios2::geo_pagina(doc, page_id)?;
        let caja = geo.ui_rect_a_pdf(&rect);
        // la caja se dibuja en local (BBox 0 0 w h) y el visor la coloca
        let (w, h) = (
            caja.right().value - caja.left().value,
            caja.top().value - caja.bottom().value,
        );
        let (r, g, b) = (
            color[0] as f32 / 255.0,
            color[1] as f32 / 255.0,
            color[2] as f32 / 255.0,
        );

        let ap_id = apariencia_freetext(doc, w, h, &text, size, [r, g, b], border);

        let mut annot = Dictionary::new();
        annot.set("Type", Object::Name(b"Annot".to_vec()));
        annot.set("Subtype", Object::Name(b"FreeText".to_vec()));
        annot.set(
            "Rect",
            Object::Array(vec![
                caja.left().value.into(),
                caja.bottom().value.into(),
                caja.right().value.into(),
                caja.top().value.into(),
            ]),
        );
        annot.set(
            "DA",
            Object::string_literal(format!("/Helv {size:.2} Tf {r:.4} {g:.4} {b:.4} rg")),
        );
        annot.set("Contents", crate::documento::cadena_pdf(&text));
        annot.set(
            "C",
            Object::Array(vec![r.into(), g.into(), b.into()]),
        );
        annot.set("F", 4i64); // Print
        annot.set("T", crate::documento::cadena_pdf(&autor));
        annot.set("CreationDate", Object::string_literal(fecha.clone()));
        annot.set("M", Object::string_literal(fecha));
        let mut bs = Dictionary::new();
        bs.set("W", Object::Integer(if border { 1 } else { 0 }));
        bs.set("S", Object::Name(b"S".to_vec()));
        annot.set("BS", Object::Dictionary(bs));
        let mut ap = Dictionary::new();
        ap.set("N", Object::Reference(ap_id));
        annot.set("AP", Object::Dictionary(ap));
        let annot_id = doc.add_object(annot);
        crate::formularios2::anade_a_annots(doc, page_id, annot_id)
    })
}

/// Mueve y/o reescala una anotación, que es lo que hace Acrobat al
/// arrastrarla o tirar de sus manijas. Cuatro tipos, con tres caminos:
///
/// - **Stamp** e **Ink** llevan su dibujo DENTRO de la anotación: hay que
///   transformar cada objeto para que ocupe el rect nuevo, y después los
///   bounds.
/// - **FreeText** se mueve y se redimensiona, y su `/AP` se vuelve a
///   dibujar con el tamaño nuevo (si no, el borde y las líneas seguirían
///   siendo los de la caja vieja).
/// - **Text** solo se mueve: el icono del post-it tiene tamaño fijo en
///   Acrobat, así que el rect nuevo solo aporta la esquina.
///
/// Para el resto de tipos no hay más que borrado.
#[tauri::command(async)]
pub fn transform_annotation(
    work_path: String,
    page_index: u16,
    annot_index: u16,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
) -> Result<(), String> {
    if w <= 1.0 || h <= 1.0 {
        return Err("Tamaño demasiado pequeño".into());
    }
    mutacion(work_path, |work_path| on_pdfium_thread(move || {
        let pdfium = pdfium()?;
        let doc = pdfium
            .load_pdf_from_file(&work_path, None)
            .map_err(|e| e.to_string())?;
        let mut page = doc.pages().get(page_index).map_err(|e| e.to_string())?;
        let geo = Geo::de_pagina(&page).propia();
        let pedido = geo.ui_rect_a_pdf(&Rect { x, y, w, h });
        let es_freetext;
        {
            let mut annot = page
                .annotations_mut()
                .get(annot_index as usize)
                .map_err(|e| e.to_string())?;
            let tipo = annot.annotation_type();
            es_freetext = tipo == PdfPageAnnotationType::FreeText;
            // el icono de la nota no se estira: se lleva su caja entera a la
            // esquina nueva, como el post-it de Acrobat
            let solo_mover = tipo == PdfPageAnnotationType::Text;
            let viejo = annot.bounds().map_err(|e| e.to_string())?;
            let (vw, vh) = (
                viejo.right().value - viejo.left().value,
                viejo.top().value - viejo.bottom().value,
            );
            if vw <= 0.0 || vh <= 0.0 {
                return Err("La anotación no tiene tamaño".into());
            }
            let nuevo = if solo_mover {
                PdfRect::new(
                    PdfPoints::new(pedido.top().value - vh),
                    pedido.left(),
                    pedido.top(),
                    PdfPoints::new(pedido.left().value + vw),
                )
            } else {
                pedido
            };
            // las escalas se calculan en el espacio del PDF: con la página
            // rotada, el ancho de la UI puede ser el alto del PDF
            let sx = (nuevo.right().value - nuevo.left().value) / vw;
            let sy = (nuevo.top().value - nuevo.bottom().value) / vh;
            // matriz compuesta: llevar el rect viejo al origen, escalar y
            // colocarlo en el rect nuevo
            let e = nuevo.left().value - viejo.left().value * sx;
            let f = nuevo.bottom().value - viejo.bottom().value * sy;
            // Text y FreeText no tienen objetos dentro que transformar: su
            // apariencia se dibuja (o se redibuja) desde el /Rect
            let interno = annot.as_stamp_annotation_mut().is_some()
                || annot.as_ink_annotation_mut().is_some();
            if !interno && !solo_mover && !es_freetext {
                return Err("Esta anotación no se puede transformar".into());
            }
            if interno {
                let objects = match annot.as_stamp_annotation_mut().is_some() {
                    true => annot.as_stamp_annotation_mut().unwrap().objects_mut(),
                    false => annot.as_ink_annotation_mut().unwrap().objects_mut(),
                };
                for i in 0..objects.len() {
                    let mut obj = objects.get(i).map_err(|e| e.to_string())?;
                    obj.transform(sx, 0.0, 0.0, sy, e, f)
                        .map_err(|e| e.to_string())?;
                }
            }
            annot.set_bounds(nuevo).map_err(|e| e.to_string())?;
        }
        drop(page);
        save_and_close(doc, &work_path)?;
        if es_freetext {
            // la apariencia del cuadro se dibuja en local (/BBox 0 0 w h):
            // con el tamaño nuevo hay que rehacerla entera
            crate::cirugia_en_hilo(&work_path, |doc| {
                let id = crate::anotaciones::annot_id(doc, page_index, annot_index as usize)?;
                crate::anotaciones::regenera_freetext(doc, id)
            })?;
        }
        // mover un comentario actualiza su fecha de modificación (Acrobat);
        // PDFium escribe una suya en UTC al guardar, así que la reescribimos
        remata_annot_en(&work_path, page_index, Some(annot_index as usize), None, None)
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::crea_pdf;
    use base64::Engine;

    fn render_rgba(path: &str) -> image::RgbaImage {
        let png_b64 = crate::render_page_b64(path.to_string(), 0, 600).expect("render");
        let png = base64::engine::general_purpose::STANDARD
            .decode(png_b64)
            .expect("base64");
        image::load_from_memory(&png).expect("PNG").to_rgba8()
    }

    /// Centro del rect que devuelve `get_annotations` para la anotación `i`.
    fn centro(work: &str, i: usize) -> (f32, f32) {
        let a = &crate::anotaciones::get_annotations(work.to_string(), 0).expect("listar")[i];
        (a.x + a.w / 2.0, a.y + a.h / 2.0)
    }

    fn gira(work: &str, veces: u8) {
        for _ in 0..veces {
            crate::paginas::rotate_page(work.to_string(), 0).expect("girar");
        }
    }

    /// ¿Hay tinta en el render alrededor de ese punto de la página VISTA?
    /// Es el único juez: la UI dibuja sus overlays sobre el render.
    fn hay_tinta(work: &str, x: f32, y: f32) -> bool {
        let sizes = crate::get_page_sizes(work.to_string()).expect("tamaños");
        let escala = 600.0 / sizes[0].width;
        let img = render_rgba(work);
        let (px, py) = ((x * escala) as i64, (y * escala) as i64);
        for dx in -4i64..=4 {
            for dy in -4i64..=4 {
                let (cx, cy) = (px + dx, py + dy);
                if cx < 0 || cy < 0 || cx >= img.width() as i64 || cy >= img.height() as i64 {
                    continue;
                }
                let p = img.get_pixel(cx as u32, cy as u32).0;
                if p[0] < 250 || p[1] < 250 || p[2] < 250 {
                    return true;
                }
            }
        }
        false
    }

    /// Lo que hace la UI antes de mandar: pasa un punto de la página vista
    /// al espacio propio de la página, con la `rotation` de
    /// `get_page_sizes`. Los comandos que escriben esperan ese espacio.
    fn vista_a_pagina(work: &str, x: f32, y: f32) -> (f32, f32) {
        let s = &crate::get_page_sizes(work.to_string()).expect("tamaños")[0];
        let (vw, vh) = (s.width, s.height);
        match s.rotation {
            90 => (y, vw - x),
            180 => (vw - x, vh - y),
            270 => (vh - y, x),
            _ => (x, y),
        }
    }

    /// En una página rotada, lo que se pone donde se pulsa tiene que caer
    /// donde se pulsó: `page.height()` de PDFium devuelve la altura YA
    /// rotada mientras que el `/Rect` de la anotación no lo está, así que
    /// voltear la `y` con ella descolocaba sellos, trazos, formas y
    /// resaltados 246 pt en una A4 girada.
    ///
    /// Reparto: la UI convierte el punto de vista al espacio propio de la
    /// página antes de mandar (`vista_a_pagina`), y `get_annotations`
    /// devuelve el rect ya en el espacio de la página vista.
    #[test]
    fn las_anotaciones_caen_donde_se_pulsa_en_una_pagina_rotada() {
        for veces in 0..4u8 {
            let grados = veces as u32 * 90;
            let pdf = std::env::temp_dir().join(format!("anotaciones2-rotada-{veces}-test.pdf"));
            crea_pdf(&["Página"], &pdf);
            let work = pdf.to_string_lossy().to_string();
            gira(&work, veces);

            let (sx, sy) = vista_a_pagina(&work, 100.0, 200.0);
            add_stamp(work.clone(), 0, "X".into(), [192, 57, 43, 255], sx, sy, 22.0, None)
                .expect("sello");
            assert!(
                hay_tinta(&work, 100.0, 200.0),
                "con /Rotate {grados} el sello no se pinta donde se pulsó"
            );
            let (cx, cy) = centro(&work, 0);
            assert!(
                (cx - 100.0).abs() < 3.0 && (cy - 200.0).abs() < 3.0,
                "con /Rotate {grados}: el sello se lee en ({cx:.1},{cy:.1})"
            );

            // trazo: la UI convierte punto a punto
            let a = vista_a_pagina(&work, 300.0, 400.0);
            let b = vista_a_pagina(&work, 340.0, 400.0);
            crate::anotaciones::add_stroke(
                work.clone(),
                0,
                vec![[a.0, a.1], [b.0, b.1]],
                Some([0, 0, 255, 255]),
                Some(3.0),
                None,
            )
            .expect("trazo");
            assert!(
                hay_tinta(&work, 320.0, 400.0),
                "con /Rotate {grados} el trazo no pasa por donde se dibujó"
            );

            // marca de texto: el /AP se pinta sobre los quads
            let (rx, ry) = vista_a_pagina(&work, 60.0, 500.0);
            let (rx2, ry2) = vista_a_pagina(&work, 180.0, 516.0);
            crate::anotaciones::add_highlight(
                work.clone(),
                0,
                vec![Rect {
                    x: rx.min(rx2),
                    y: ry.min(ry2),
                    w: (rx2 - rx).abs(),
                    h: (ry2 - ry).abs(),
                }],
                None,
            )
            .expect("resaltar");
            assert!(
                hay_tinta(&work, 120.0, 508.0),
                "con /Rotate {grados} el resaltado no cae sobre lo resaltado"
            );
            let a = &crate::anotaciones::get_annotations(work.clone(), 0).expect("listar")[2];
            assert!(
                (a.rects[0].x - 60.0).abs() < 1.0
                    && (a.rects[0].y - 500.0).abs() < 1.0
                    && (a.rects[0].w - 120.0).abs() < 1.0
                    && (a.rects[0].h - 16.0).abs() < 1.0,
                "con /Rotate {grados}: quad leído en {:?}",
                a.rects[0]
            );
            std::fs::remove_file(&pdf).ok();
        }
    }

    /// Rotar la página mueve las anotaciones con ella, y donde
    /// `get_annotations` dice que están es donde el render las pinta.
    #[test]
    fn rotar_la_pagina_lleva_las_anotaciones_a_su_sitio_en_el_render() {
        let pdf = std::env::temp_dir().join("anotaciones2-rotada-render-test.pdf");
        crea_pdf(&["Página"], &pdf);
        let work = pdf.to_string_lossy().to_string();
        add_stamp(work.clone(), 0, "X".into(), [192, 57, 43, 255], 100.0, 200.0, 22.0, None)
            .expect("sello");
        gira(&work, 1);

        // A4: (100,200) sin rotar → (841,89 − 200, 100) al girar 90° CW
        let (cx, cy) = centro(&work, 0);
        assert!(
            (cx - 641.9).abs() < 3.0 && (cy - 100.0).abs() < 3.0,
            "tras girar 90° el sello se lee en ({cx:.1},{cy:.1})"
        );
        assert!(hay_tinta(&work, cx, cy), "el render no pinta nada ahí");

        let sizes = crate::get_page_sizes(work.clone()).expect("tamaños");
        assert_eq!(sizes[0].rotation, 90, "get_page_sizes debe dar la rotación");
        assert!(
            (sizes[0].width - 841.89).abs() < 1.0 && (sizes[0].height - 595.28).abs() < 1.0,
            "el tamaño que devuelve get_page_sizes ya viene rotado: {}×{}",
            sizes[0].width,
            sizes[0].height
        );
        std::fs::remove_file(&pdf).ok();
    }

    #[test]
    fn cuadro_de_texto_con_apariencia_y_texto() {
        let pdf = std::env::temp_dir().join("anotaciones2-freetext-test.pdf");
        crea_pdf(&["Página"], &pdf);
        let work = pdf.to_string_lossy().to_string();
        let caja = Rect { x: 120.0, y: 300.0, w: 220.0, h: 60.0 };
        add_free_text(
            work.clone(),
            0,
            caja.clone(),
            "Primera línea\nsegunda con acentós".into(),
            12.0,
            [200, 0, 0, 255],
            true,
            Some("Jorge".into()),
        )
        .expect("cuadro de texto");

        // sale en la lista de comentarios, con su texto
        let a = &crate::anotaciones::get_annotations(work.clone(), 0).expect("listar")[0];
        assert_eq!(a.kind, "FreeText");
        assert_eq!(a.contents, "Primera línea\nsegunda con acentós");
        assert_eq!(a.author, "Jorge");
        assert!((a.x - 120.0).abs() < 1.0 && (a.y - 300.0).abs() < 1.0);
        assert_eq!(a.color, Some([200, 0, 0, 255]));

        // el fichero lleva /AP y /DA: sin ellos la caja no existe fuera
        let doc = lopdf::Document::load(&work).expect("cargar");
        let page_id = *doc.get_pages().get(&1).expect("página 1");
        let rid = doc
            .get_object(page_id)
            .and_then(|o| o.as_dict())
            .and_then(|d| d.get(b"Annots"))
            .and_then(|o| o.as_array())
            .expect("Annots")[0]
            .as_reference()
            .expect("referencia");
        let annot = doc.get_object(rid).and_then(|o| o.as_dict()).expect("annot");
        assert_eq!(
            annot.get(b"Subtype").and_then(|o| o.as_name()).unwrap_or_default(),
            b"FreeText"
        );
        assert!(annot.get(b"DA").is_ok(), "el cuadro necesita /DA");
        let ap = annot
            .get(b"AP")
            .and_then(|o| o.as_dict())
            .and_then(|d| d.get(b"N"))
            .and_then(|o| o.as_reference())
            .expect("/AP /N");
        assert!(
            !doc.get_object(ap).and_then(|o| o.as_stream()).expect("stream").content.is_empty(),
            "la apariencia va vacía"
        );

        // y hay tinta dentro del rect
        assert!(
            hay_tinta(&work, caja.x + 4.0, caja.y + 4.0),
            "el borde del cuadro no se ve en el render"
        );
        assert!(
            hay_tinta(&work, caja.x + 12.0, caja.y + 14.0),
            "el texto del cuadro no se ve en el render"
        );

        // corregir el texto reescribe la apariencia: si no, cambiaría el
        // dato y no lo que se ve
        crate::anotaciones::set_annotation_contents(work.clone(), 0, 0, "Corto".into(), None)
            .expect("corregir");
        let doc = lopdf::Document::load(&work).expect("recargar");
        let page_id = *doc.get_pages().get(&1).expect("página 1");
        let rid = doc
            .get_object(page_id)
            .and_then(|o| o.as_dict())
            .and_then(|d| d.get(b"Annots"))
            .and_then(|o| o.as_array())
            .expect("Annots")[0]
            .as_reference()
            .expect("referencia");
        let ap = doc
            .get_object(rid)
            .and_then(|o| o.as_dict())
            .and_then(|d| d.get(b"AP"))
            .and_then(|o| o.as_dict())
            .and_then(|d| d.get(b"N"))
            .and_then(|o| o.as_reference())
            .expect("/AP /N");
        let contenido = String::from_utf8_lossy(
            &doc.get_object(ap).and_then(|o| o.as_stream()).expect("stream").content,
        )
        .into_owned();
        assert!(contenido.contains("(Corto)"), "la apariencia no se rehízo: {contenido}");
        assert!(
            !contenido.contains("Primera"),
            "la apariencia conserva el texto viejo: {contenido}"
        );
        std::fs::remove_file(&pdf).ok();
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
        add_markup(work.clone(), 0, r.clone(), "underline".into(), None, None).expect("subrayar");
        add_markup(work.clone(), 0, r, "strikeout".into(), None, None).expect("tachar");
        let annots = crate::anotaciones::get_annotations(work, 0).expect("listar");
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
            None,
        )
        .expect("tachar");
        // renderizar con el caché del documento (como hace la UI)
        crate::render_page_b64(work.clone(), 0, 400).expect("render");
        let annots = crate::anotaciones::get_annotations(work, 0).expect("listar");
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
            None,
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
            None,
        )
        .expect("flecha");
        let annots = crate::anotaciones::get_annotations(work.clone(), 0).expect("listar");
        assert_eq!(annots.len(), 2);
        let img = render_rgba(&work);
        let escala = 600.0 / 595.28;
        let p = img.get_pixel((160.0 * escala) as u32, (340.0 * escala) as u32);
        assert!(p[0] > 150 && p[1] < 100, "esperaba rojo dentro del rect, hay {p:?}");
        // las formas llevan /AP desde el principio: listar sus colores tras
        // el render es justo el caso que hacía SIGSEGV en Linux
        let annots = crate::anotaciones::get_annotations(work, 0).expect("listar tras render");
        assert_eq!(annots[0].color, Some([200, 0, 0, 255]));
        assert_eq!(annots[1].color, Some([0, 0, 200, 255]));
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
            None,
        )
        .expect("sello");
        let annots = crate::anotaciones::get_annotations(work.clone(), 0).expect("listar");
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
        let tras = crate::anotaciones::get_annotations(work, 0).expect("listar tras render");
        assert_eq!(tras[0].color, Some([200, 30, 30, 255]), "color del sello tras render");
        assert!(rojos > 200, "el sello apenas pinta ({rojos} píxeles rojos)");
    }

    fn rojos_en(img: &image::RgbaImage, x0: u32, y0: u32, x1: u32, y1: u32) -> u32 {
        let escala = 600.0 / 595.28;
        let mut n = 0;
        for yy in y0..y1 {
            for xx in x0..x1 {
                let p = img.get_pixel((xx as f32 * escala) as u32, (yy as f32 * escala) as u32);
                if p[0] > 150 && p[1] < 110 && p[2] < 110 {
                    n += 1;
                }
            }
        }
        n
    }

    #[test]
    fn sello_movido_y_escalado() {
        let pdf = std::env::temp_dir().join("anotaciones2-transform-test.pdf");
        crea_pdf(&["Página"], &pdf);
        let work = pdf.to_string_lossy().to_string();
        add_stamp(
            work.clone(),
            0,
            "APROBADO".into(),
            [200, 30, 30, 255],
            200.0,
            600.0,
            22.0,
            None,
        )
        .expect("sello");
        let a = &crate::anotaciones::get_annotations(work.clone(), 0).expect("listar")[0];
        // moverlo arriba a la izquierda y doblar el tamaño
        let (nx, ny, nw, nh) = (60.0, 100.0, a.w * 2.0, a.h * 2.0);
        transform_annotation(work.clone(), 0, 0, nx, ny, nw, nh).expect("transformar");
        let b = &crate::anotaciones::get_annotations(work.clone(), 0).expect("listar")[0];
        assert!((b.x - nx).abs() < 1.0 && (b.y - ny).abs() < 1.0, "bounds {b:?}");
        assert!((b.w - nw).abs() < 1.0 && (b.h - nh).abs() < 1.0, "bounds {b:?}");
        let img = render_rgba(&work);
        // pinta en la zona nueva…
        let en_nuevo = rojos_en(&img, 60, 100, (nx + nw) as u32, (ny + nh) as u32);
        assert!(en_nuevo > 400, "el sello transformado apenas pinta ({en_nuevo})");
        // …y ya no en la vieja (el sello original rondaba y=590..615, x=130..270)
        let en_viejo = rojos_en(&img, 130, 570, 270, 630);
        assert_eq!(en_viejo, 0, "quedan restos del sello en la posición vieja");
    }

    /// Arrastrar una nota o un cuadro de texto es lo que ofrece la UI (el
    /// icono de la nota es su propia zona de arrastre y el cuadro lleva los
    /// ocho tiradores), y en Acrobat las dos cosas se mueven. El backend
    /// solo sabía de Stamp e Ink y devolvía «Esta anotación no se puede
    /// transformar»: un error rojo en la cara del usuario.
    #[test]
    fn mover_una_nota_y_un_cuadro_de_texto() {
        let tmp = std::env::temp_dir().join("anotaciones2-transform-texto-test.pdf");
        crea_pdf(&["Hola"], &tmp);
        let work = tmp.to_string_lossy().into_owned();
        crate::anotaciones::add_note(
            work.clone(),
            0,
            100.0,
            100.0,
            "Nota".into(),
            Some("Jorge".into()),
        )
        .expect("nota");
        add_free_text(
            work.clone(),
            0,
            Rect { x: 60.0, y: 300.0, w: 200.0, h: 60.0 },
            "Cuadro".into(),
            12.0,
            [0, 0, 0, 255],
            true,
            Some("Jorge".into()),
        )
        .expect("cuadro de texto");

        let antes = crate::anotaciones::get_annotations(work.clone(), 0).expect("listar");
        let (i_nota, i_cuadro) = (antes[0].index, antes[1].index);
        assert_eq!(antes[0].kind, "Text");
        assert_eq!(antes[1].kind, "FreeText");

        // la nota se mueve (el icono conserva su tamaño, como en Acrobat)
        transform_annotation(work.clone(), 0, i_nota, 300.0, 400.0, 22.0, 22.0)
            .expect("mover la nota");
        // el cuadro se mueve Y se redimensiona
        transform_annotation(work.clone(), 0, i_cuadro, 100.0, 500.0, 260.0, 90.0)
            .expect("mover el cuadro de texto");

        let despues = crate::anotaciones::get_annotations(work.clone(), 0).expect("listar");
        let nota = &despues[0];
        assert!(
            (nota.x - 300.0).abs() < 2.0 && (nota.y - 400.0).abs() < 2.0,
            "la nota quedó en ({:.1},{:.1})",
            nota.x,
            nota.y
        );
        assert!(
            (nota.w - 22.0).abs() < 2.0 && (nota.h - 22.0).abs() < 2.0,
            "el icono de la nota no conserva su tamaño: {:.1}x{:.1}",
            nota.w,
            nota.h
        );
        let cuadro = &despues[1];
        assert!(
            (cuadro.x - 100.0).abs() < 2.0
                && (cuadro.y - 500.0).abs() < 2.0
                && (cuadro.w - 260.0).abs() < 2.0
                && (cuadro.h - 90.0).abs() < 2.0,
            "el cuadro quedó en ({:.1},{:.1}) {:.1}x{:.1}",
            cuadro.x,
            cuadro.y,
            cuadro.w,
            cuadro.h
        );
        assert_eq!(cuadro.author, "Jorge", "mover no debe tocar el autor");

        // y su apariencia se rehace con el tamaño nuevo: sin esto el /AP
        // seguiría dibujando el borde de la caja vieja
        let (bw, bh) = bbox_del_ap(&work, i_cuadro as usize);
        assert!(
            (bw - 260.0).abs() < 2.0 && (bh - 90.0).abs() < 2.0,
            "el /AP del cuadro sigue midiendo {bw:.1}x{bh:.1}"
        );
        std::fs::remove_file(&tmp).ok();
    }

    /// Tamaño del `/BBox` del `/AP /N` de una anotación de la primera página.
    fn bbox_del_ap(work: &str, i: usize) -> (f32, f32) {
        let hacer = || -> Result<(f32, f32), String> {
            let mut doc = lopdf::Document::load(work).map_err(|e| e.to_string())?;
            let doc = &mut doc;
            let id = crate::anotaciones::annot_id(doc, 0, i)?;
            let annot = doc
                .get_object(id)
                .and_then(|o| o.as_dict())
                .map_err(|e| e.to_string())?;
            let ap = annot
                .get(b"AP")
                .and_then(|o| o.as_dict())
                .map_err(|e| e.to_string())?
                .get(b"N")
                .map_err(|e| e.to_string())?
                .as_reference()
                .map_err(|e| e.to_string())?;
            let caja: Vec<f32> = doc
                .get_object(ap)
                .and_then(|o| o.as_stream())
                .map_err(|e| e.to_string())?
                .dict
                .get(b"BBox")
                .and_then(|o| o.as_array())
                .map_err(|e| e.to_string())?
                .iter()
                .filter_map(|o| o.as_float().ok().or_else(|| o.as_i64().ok().map(|n| n as f32)))
                .collect();
            Ok((caja[2] - caja[0], caja[3] - caja[1]))
        };
        hacer().expect("leer el /AP")
    }

}
