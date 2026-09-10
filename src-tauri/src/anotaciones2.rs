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
/// o tachado: el subtipo y el color se eligen (es el único camino; el
/// `add_highlight` del ciclo 1 lo sustituyó y se retiró en el 5).
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

/// Texto para un stream de contenido en WinAnsiEncoding, escapando `\`,
/// `(` y `)`. No es latin-1: el tramo 0x80–0x9F, que en latin-1 son
/// controles, en WinAnsi lleva justo los signos que un texto en español usa
/// a diario —la raya «—», el guion «–», los puntos suspensivos «…», las
/// comillas tipográficas y el «€»—, y salían como interrogantes.
pub(crate) fn winansi(texto: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(texto.len());
    for c in texto.chars() {
        match c {
            '\\' | '(' | ')' => {
                out.push(b'\\');
                out.push(c as u8);
            }
            c if (c as u32) < 0x20 => out.push(b' '),
            // ASCII imprimible y el latin-1 alto coinciden con WinAnsi
            c if (0x20..0x7F).contains(&(c as u32)) => out.push(c as u8),
            c if (0xA0..=0xFF).contains(&(c as u32)) => out.push(c as u8),
            c => out.push(winansi_alto(c).unwrap_or(b'?')),
        }
    }
    out
}

/// El tramo 0x80–0x9F de WinAnsiEncoding, que no es Unicode.
fn winansi_alto(c: char) -> Option<u8> {
    Some(match c {
        '\u{20AC}' => 0x80,
        '\u{201A}' => 0x82,
        '\u{0192}' => 0x83,
        '\u{201E}' => 0x84,
        '\u{2026}' => 0x85,
        '\u{2020}' => 0x86,
        '\u{2021}' => 0x87,
        '\u{02C6}' => 0x88,
        '\u{2030}' => 0x89,
        '\u{0160}' => 0x8A,
        '\u{2039}' => 0x8B,
        '\u{0152}' => 0x8C,
        '\u{017D}' => 0x8E,
        '\u{2018}' => 0x91,
        '\u{2019}' => 0x92,
        '\u{201C}' => 0x93,
        '\u{201D}' => 0x94,
        '\u{2022}' => 0x95,
        '\u{2013}' => 0x96,
        '\u{2014}' => 0x97,
        '\u{02DC}' => 0x98,
        '\u{2122}' => 0x99,
        '\u{0161}' => 0x9A,
        '\u{203A}' => 0x9B,
        '\u{0153}' => 0x9C,
        '\u{017E}' => 0x9E,
        '\u{0178}' => 0x9F,
        _ => return None,
    })
}

/// Anchos de Helvetica (los del AFM, en milésimas de em) para el tramo
/// imprimible de ASCII. Fuera de él: 667 para las mayúsculas acentuadas y
/// 556 para el resto, que es lo que miden casi todas en esta fuente.
const ANCHOS_HELVETICA: [u16; 95] = [
    278, 278, 355, 556, 556, 889, 667, 191, 333, 333, 389, 584, 278, 333, 278, 278, // ' ' … '/'
    556, 556, 556, 556, 556, 556, 556, 556, 556, 556, 278, 278, 584, 584, 584, 556, // '0' … '?'
    1015, 667, 667, 722, 722, 667, 611, 778, 722, 278, 500, 667, 556, 833, 722, 778, // '@' … 'O'
    667, 778, 722, 667, 611, 722, 667, 944, 667, 667, 611, 278, 278, 278, 469, 556, // 'P' … '_'
    191, 556, 556, 500, 556, 556, 278, 556, 556, 222, 222, 500, 222, 833, 556, 556, // '`' … 'o'
    556, 556, 333, 500, 278, 556, 500, 722, 500, 500, 500, 334, 260, 334, 584, // 'p' … '~'
];

/// Ancho de un texto en Helvetica al tamaño dado, en puntos. Es la medida
/// con la que el backend parte las líneas del cuadro de texto: la
/// apariencia se dibuja en Helvetica, así que medir en Helvetica es medir
/// lo que se va a ver.
pub(crate) fn ancho_helvetica(texto: &str, size: f32) -> f32 {
    let milesimas: u32 = texto
        .chars()
        .map(|c| {
            let i = c as u32;
            if (32..127).contains(&i) {
                ANCHOS_HELVETICA[(i - 32) as usize] as u32
            } else if c.is_uppercase() {
                667
            } else {
                556
            }
        })
        .sum();
    milesimas as f32 * size / 1000.0
}

/// Parte el texto para que quepa en `ancho` puntos, como hace Acrobat con
/// el cuadro de texto: se respetan los saltos escritos, se parte por
/// espacios y, si una palabra sola no cabe, por letras. Sin esto, corregir
/// un cuadro ya creado o estrecharlo dejaba la frase saliéndose por el
/// borde derecho, porque el `/AP` solo partía por `\n`.
pub(crate) fn parte_lineas(texto: &str, size: f32, ancho: f32) -> Vec<String> {
    let ancho = ancho.max(size * 0.5);
    let mut salida = Vec::new();
    for parrafo in texto.split('\n') {
        if parrafo.is_empty() {
            salida.push(String::new());
            continue;
        }
        let mut linea = String::new();
        for palabra in parrafo.split(' ') {
            let candidata = if linea.is_empty() {
                palabra.to_string()
            } else {
                format!("{linea} {palabra}")
            };
            if ancho_helvetica(&candidata, size) <= ancho {
                linea = candidata;
                continue;
            }
            if !linea.is_empty() {
                salida.push(std::mem::take(&mut linea));
            }
            // una palabra que no cabe entera se parte por letras
            let mut trozo = String::new();
            for c in palabra.chars() {
                let mas = format!("{trozo}{c}");
                if !trozo.is_empty() && ancho_helvetica(&mas, size) > ancho {
                    salida.push(std::mem::take(&mut trozo));
                    trozo.push(c);
                } else {
                    trozo = mas;
                }
            }
            linea = trozo;
        }
        salida.push(linea);
    }
    salida
}

/// Apariencia (`/AP /N`) de un cuadro de texto: el borde opcional y las
/// líneas del texto en Helvetica, **partidas al ancho de la caja**,
/// dibujadas en local (`/BBox 0 0 w h`).
/// La comparten la creación y la reescritura del texto: sin regenerarla,
/// corregir el cuadro cambiaría el `/Contents` y no lo que se ve. Y como el
/// reparto de líneas se hace aquí, redimensionar el cuadro (que rehace la
/// apariencia) refluye el texto, igual que al tirar de un tirador en
/// Acrobat.
pub(crate) fn apariencia_freetext(
    doc: &mut lopdf::Document,
    w: f32,
    h: f32,
    texto: &str,
    size: f32,
    color: [f32; 3],
    border: bool,
) -> lopdf::ObjectId {
    apariencia_freetext_con_llamada(doc, w, h, texto, size, color, border, [0.0; 4], &[])
}

/// La apariencia de un `/FreeText`, con o sin llamada. `rd` son los cuatro
/// márgenes del `/RD` (izquierda, abajo, derecha, arriba): lo que separa la
/// **caja de texto** del `/Rect` de la anotación, que en una llamada tiene
/// que abarcar también la línea y su punta. `linea` son los puntos del
/// `/CL` ya en coordenadas locales del `/BBox`, empezando por la punta.
#[allow(clippy::too_many_arguments)]
pub(crate) fn apariencia_freetext_con_llamada(
    doc: &mut lopdf::Document,
    w: f32,
    h: f32,
    texto: &str,
    size: f32,
    color: [f32; 3],
    border: bool,
    rd: [f32; 4],
    linea: &[(f32, f32)],
) -> lopdf::ObjectId {
    use lopdf::{Dictionary, Object, Stream};
    let [r, g, b] = color;
    // la caja de texto dentro del /Rect
    let (cx, cy) = (rd[0], rd[1]);
    let (cw, ch) = ((w - rd[0] - rd[2]).max(1.0), (h - rd[1] - rd[3]).max(1.0));
    let mut ops = Vec::new();
    // la línea de la llamada va DEBAJO de la caja, para que el borde la
    // tape en el punto de anclaje y no se vea el remate
    if linea.len() >= 2 {
        ops.extend_from_slice(
            format!("q {r:.4} {g:.4} {b:.4} RG 1 w {:.2} {:.2} m ", linea[0].0, linea[0].1)
                .as_bytes(),
        );
        for p in &linea[1..] {
            ops.extend_from_slice(format!("{:.2} {:.2} l ", p.0, p.1).as_bytes());
        }
        ops.extend_from_slice(b"S Q\n");
        // la punta de flecha, en el primer punto y mirando hacia el segundo
        // (`/LE /OpenArrow` es lo que declara la anotación; el dibujo lo
        // ponemos nosotros porque PDFium no escribe apariencias)
        let (px, py) = linea[0];
        let (qx, qy) = linea[1];
        let (dx, dy) = (qx - px, qy - py);
        let largo = (dx * dx + dy * dy).sqrt().max(0.001);
        let (ux, uy) = (dx / largo, dy / largo);
        let punta = 8.0f32.min(largo * 0.6);
        let ala = 0.42f32; // ±24°, la de Acrobat
        let (c, s2) = (ala.cos(), ala.sin());
        let a1 = (px + punta * (ux * c - uy * s2), py + punta * (ux * s2 + uy * c));
        let a2 = (px + punta * (ux * c + uy * s2), py + punta * (-ux * s2 + uy * c));
        ops.extend_from_slice(
            format!(
                "q {r:.4} {g:.4} {b:.4} RG 1 w {:.2} {:.2} m {px:.2} {py:.2} l {:.2} {:.2} l S Q\n",
                a1.0, a1.1, a2.0, a2.1
            )
            .as_bytes(),
        );
    }
    if border {
        ops.extend_from_slice(
            format!(
                "q 1 1 1 rg {:.2} {:.2} {:.2} {:.2} re f Q\n\
                 q {r:.4} {g:.4} {b:.4} RG 1 w {:.2} {:.2} {:.2} {:.2} re S Q\n",
                cx, cy, cw, ch,
                cx + 0.5, cy + 0.5, cw - 1.0, ch - 1.0
            )
            .as_bytes(),
        );
    }
    let pad = (size * 0.35).max(2.0);
    let interlineado = size * 1.2;
    ops.extend_from_slice(
        format!(
            "BT /Helv {size:.2} Tf {interlineado:.2} TL {r:.4} {g:.4} {b:.4} rg {:.2} {:.2} Td\n",
            cx + pad,
            cy + ch - pad - size * 0.85
        )
        .as_bytes(),
    );
    for (n, linea) in parte_lineas(texto, size, cw - pad * 2.0).iter().enumerate() {
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

/// La **llamada** de Acrobat: un `/FreeText` con `/IT /FreeTextCallout`,
/// una línea `/CL` que sale de la caja de texto y acaba en punta de flecha
/// (`/LE /OpenArrow`) sobre lo que se quiere señalar. El gesto es el de
/// Acrobat: clic donde señala, arrastrar hasta donde va el texto.
///
/// `rect` es la **caja del texto**, `punta` el punto al que apunta y `codo`
/// el punto intermedio opcional, los tres en el espacio propio de la página
/// (la UI convierte antes de mandar). El `/Rect` de la anotación abarca
/// todo —es lo que exige el spec, y lo que hace que arrastrarla se lleve la
/// línea entera— y el `/RD` dice dónde queda la caja dentro de él.
///
/// **El codo** (R52) es lo que Acrobat dibuja por defecto: la línea sale de
/// la caja en horizontal, dobla y llega a la punta. Sin `codo` sale la
/// recta de siempre, que es lo que hace Acrobat cuando se arrastra en
/// diagonal.
#[allow(clippy::too_many_arguments)]
#[tauri::command(async)]
pub fn add_callout(
    work_path: String,
    page_index: u16,
    rect: Rect,
    punta: [f32; 2],
    text: String,
    color: [u8; 4],
    author: Option<String>,
    codo: Option<[f32; 2]>,
) -> Result<(), String> {
    if text.trim().is_empty() {
        return Err("La llamada está vacía".into());
    }
    if rect.w < 8.0 || rect.h < 8.0 {
        return Err("El cuadro de la llamada es demasiado pequeño".into());
    }
    let size = 11.0f32;
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
        let (bx0, by0, bx1, by1) = (
            caja.left().value,
            caja.bottom().value,
            caja.right().value,
            caja.top().value,
        );
        let (px, py) = geo.ui_a_pdf(punta[0], punta[1]);
        let codo = codo.map(|c| geo.ui_a_pdf(c[0], c[1]));
        // el ancla: el centro del lado de la caja que mira a **por donde
        // llega la línea** (el codo si lo hay, la punta si no), que es de
        // donde sale en Acrobat
        let hacia = codo.unwrap_or((px, py));
        let ancla = ancla_de_la_caja((bx0, by0, bx1, by1), hacia);
        // el /Rect abarca la caja Y toda la línea, con hueco para la flecha
        const AIRE: f32 = 10.0;
        let (kx, ky) = codo.unwrap_or((px, py));
        let x0 = bx0.min(px - AIRE).min(kx - AIRE);
        let y0 = by0.min(py - AIRE).min(ky - AIRE);
        let x1 = bx1.max(px + AIRE).max(kx + AIRE);
        let y1 = by1.max(py + AIRE).max(ky + AIRE);
        let rd = [bx0 - x0, by0 - y0, x1 - bx1, y1 - by1];
        let (w, h) = (x1 - x0, y1 - y0);
        let (r, g, b) = (
            color[0] as f32 / 255.0,
            color[1] as f32 / 255.0,
            color[2] as f32 / 255.0,
        );
        // el primer punto es la punta: ahí va el remate que declara /LE
        let mut linea = vec![(px - x0, py - y0)];
        if let Some((cx, cy)) = codo {
            linea.push((cx - x0, cy - y0));
        }
        linea.push((ancla.0 - x0, ancla.1 - y0));
        let ap_id = apariencia_freetext_con_llamada(
            doc, w, h, &text, size, [r, g, b], true, rd, &linea,
        );

        let mut annot = Dictionary::new();
        annot.set("Type", Object::Name(b"Annot".to_vec()));
        annot.set("Subtype", Object::Name(b"FreeText".to_vec()));
        annot.set("IT", Object::Name(b"FreeTextCallout".to_vec()));
        annot.set(
            "Rect",
            Object::Array(vec![x0.into(), y0.into(), x1.into(), y1.into()]),
        );
        annot.set(
            "RD",
            Object::Array(vec![rd[0].into(), rd[1].into(), rd[2].into(), rd[3].into()]),
        );
        // el /CL empieza en la punta: es ahí donde va el remate que declara
        // /LE
        let mut cl: Vec<Object> = vec![px.into(), py.into()];
        if let Some((cx, cy)) = codo {
            cl.push(cx.into());
            cl.push(cy.into());
        }
        cl.push(ancla.0.into());
        cl.push(ancla.1.into());
        annot.set("CL", Object::Array(cl));
        annot.set("LE", Object::Name(b"OpenArrow".to_vec()));
        annot.set(
            "DA",
            Object::string_literal(format!("/Helv {size:.2} Tf {r:.4} {g:.4} {b:.4} rg")),
        );
        annot.set("Contents", crate::documento::cadena_pdf(&text));
        annot.set("C", Object::Array(vec![r.into(), g.into(), b.into()]));
        annot.set("F", 4i64); // Print
        annot.set("T", crate::documento::cadena_pdf(&autor));
        annot.set("CreationDate", Object::string_literal(fecha.clone()));
        annot.set("M", Object::string_literal(fecha));
        let mut bs = Dictionary::new();
        bs.set("W", Object::Integer(1));
        bs.set("S", Object::Name(b"S".to_vec()));
        annot.set("BS", Object::Dictionary(bs));
        let mut ap = Dictionary::new();
        ap.set("N", Object::Reference(ap_id));
        annot.set("AP", Object::Dictionary(ap));
        let annot_id = doc.add_object(annot);
        crate::formularios2::anade_a_annots(doc, page_id, annot_id)
    })
}

/// Deja puesta una medida como **comentario**: una anotación `/Line` (dos
/// puntos), `/PolyLine` (perímetro) o `/Polygon` (área) con la cifra en su
/// `/Contents` y su `/AP` dibujado a mano.
///
/// Es lo que hace Acrobat, y la diferencia se nota: la medida sale en el
/// panel de comentarios, se borra con Supr como cualquier otro y **no
/// ensucia el texto del documento**. Vitela la escribía como contenido de
/// página (`add_shape` más `add_text_block`): no aparecía en Comentarios,
/// no se podía quitar como comentario, `get_text_blocks` devolvía «150,44
/// m» como texto del documento y hacían falta **dos** ⌘Z aunque la banda
/// prometiera uno. Aquí es una sola mutación.
///
/// `points` va en el espacio propio de la página, como el resto de los
/// comandos que escriben. No se escribe un diccionario `/Measure`: la
/// escala la fija el usuario por documento en la interfaz, y un `/Measure`
/// sin escala de verdad diría que el PDF trae una que no trae.
#[tauri::command(async)]
pub fn add_measure(
    work_path: String,
    page_index: u16,
    points: Vec<[f32; 2]>,
    text: String,
    color: [u8; 4],
    closed: Option<bool>,
    author: Option<String>,
) -> Result<(), String> {
    if points.len() < 2 {
        return Err("Una medida necesita al menos dos puntos".into());
    }
    let text = text.trim().to_string();
    if text.is_empty() {
        return Err("La medida está vacía".into());
    }
    let cerrada = closed.unwrap_or(false) && points.len() > 2;
    let autor = crate::anotaciones::autor_o_sistema(author);
    let fecha = crate::anotaciones::fecha_pdf_ahora();
    crate::cirugia(&work_path, move |doc| {
        use lopdf::{Dictionary, Object};
        let page_id = *doc
            .get_pages()
            .get(&(page_index as u32 + 1))
            .ok_or("Página fuera de rango")?;
        let geo = crate::formularios2::geo_pagina(doc, page_id)?;
        let pdf: Vec<(f32, f32)> = points.iter().map(|p| geo.ui_a_pdf(p[0], p[1])).collect();

        const SIZE: f32 = 10.0;
        // el /Rect abarca el dibujo, las flechas y la etiqueta
        let aire = SIZE * 1.6;
        let x0 = pdf.iter().map(|p| p.0).fold(f32::MAX, f32::min) - aire;
        let y0 = pdf.iter().map(|p| p.1).fold(f32::MAX, f32::min) - aire;
        let x1 = pdf.iter().map(|p| p.0).fold(f32::MIN, f32::max) + aire;
        let y1 = pdf.iter().map(|p| p.1).fold(f32::MIN, f32::max) + aire;
        let (r, g, b) = (
            color[0] as f32 / 255.0,
            color[1] as f32 / 255.0,
            color[2] as f32 / 255.0,
        );

        let ap_id = apariencia_medida(doc, [x0, y0, x1, y1], &pdf, cerrada, &text, [r, g, b]);

        let (subtipo, it) = match (pdf.len(), cerrada) {
            (2, _) => (&b"Line"[..], &b"LineDimension"[..]),
            (_, true) => (&b"Polygon"[..], &b"PolygonDimension"[..]),
            _ => (&b"PolyLine"[..], &b"PolyLineDimension"[..]),
        };
        let mut annot = Dictionary::new();
        annot.set("Type", Object::Name(b"Annot".to_vec()));
        annot.set("Subtype", Object::Name(subtipo.to_vec()));
        // `/IT` es lo que distingue una medida de una raya cualquiera, y es
        // lo que mira Acrobat al abrirla
        annot.set("IT", Object::Name(it.to_vec()));
        annot.set(
            "Rect",
            Object::Array(vec![x0.into(), y0.into(), x1.into(), y1.into()]),
        );
        if pdf.len() == 2 {
            annot.set(
                "L",
                Object::Array(vec![
                    pdf[0].0.into(),
                    pdf[0].1.into(),
                    pdf[1].0.into(),
                    pdf[1].1.into(),
                ]),
            );
        } else {
            annot.set(
                "Vertices",
                Object::Array(pdf.iter().flat_map(|p| [p.0.into(), p.1.into()]).collect()),
            );
        }
        if !cerrada {
            // los remates de los extremos: es lo que dibuja la cota
            annot.set(
                "LE",
                Object::Array(vec![
                    Object::Name(b"OpenArrow".to_vec()),
                    Object::Name(b"OpenArrow".to_vec()),
                ]),
            );
        }
        // la cifra va en el /Contents: es lo que enseña el panel de
        // comentarios y lo que sale en el post-it de cualquier visor
        annot.set("Contents", crate::documento::cadena_pdf(&text));
        annot.set("C", Object::Array(vec![r.into(), g.into(), b.into()]));
        annot.set("F", 4i64); // Print
        annot.set("T", crate::documento::cadena_pdf(&autor));
        annot.set("CreationDate", Object::string_literal(fecha.clone()));
        annot.set("M", Object::string_literal(fecha));
        let mut bs = Dictionary::new();
        bs.set("W", Object::Integer(1));
        bs.set("S", Object::Name(b"S".to_vec()));
        annot.set("BS", Object::Dictionary(bs));
        let mut ap = Dictionary::new();
        ap.set("N", Object::Reference(ap_id));
        annot.set("AP", Object::Dictionary(ap));
        let annot_id = doc.add_object(annot);
        crate::formularios2::anade_a_annots(doc, page_id, annot_id)
    })
}

/// El `/AP` de una medida, dibujado **en coordenadas de página** (`/BBox`
/// igual al `/Rect`, como la apariencia de las marcas de texto): la
/// polilínea, sus dos puntas de flecha y la cifra sobre un recuadro blanco
/// para que se lea encima del documento.
fn apariencia_medida(
    doc: &mut lopdf::Document,
    caja: [f32; 4],
    pdf: &[(f32, f32)],
    cerrada: bool,
    texto: &str,
    color: [f32; 3],
) -> lopdf::ObjectId {
    use lopdf::{Dictionary, Object, Stream};
    let [r, g, b] = color;
    const SIZE: f32 = 10.0;
    let mut ops = Vec::new();
    ops.extend_from_slice(
        format!(
            "q {r:.4} {g:.4} {b:.4} RG 1.5 w {:.2} {:.2} m ",
            pdf[0].0, pdf[0].1
        )
        .as_bytes(),
    );
    for p in &pdf[1..] {
        ops.extend_from_slice(format!("{:.2} {:.2} l ", p.0, p.1).as_bytes());
    }
    if cerrada {
        ops.extend_from_slice(b"h ");
    }
    ops.extend_from_slice(b"S Q\n");
    if !cerrada {
        ops.extend_from_slice(&flecha(pdf[0], pdf[1], color));
        let n = pdf.len();
        ops.extend_from_slice(&flecha(pdf[n - 1], pdf[n - 2], color));
    }

    // la cifra, centrada en el punto medio del dibujo y sobre un recuadro
    // blanco: sin él, encima de un texto no se lee
    let cx = pdf.iter().map(|p| p.0).sum::<f32>() / pdf.len() as f32;
    let cy = pdf.iter().map(|p| p.1).sum::<f32>() / pdf.len() as f32;
    let ancho = ancho_helvetica(texto, SIZE);
    let pad = 2.0;
    let (bx, by) = (cx - ancho / 2.0 - pad, cy - SIZE * 0.5 - pad);
    ops.extend_from_slice(
        format!(
            "q 1 1 1 rg {bx:.2} {by:.2} {:.2} {:.2} re f Q\n",
            ancho + pad * 2.0,
            SIZE + pad * 2.0
        )
        .as_bytes(),
    );
    ops.extend_from_slice(
        format!(
            "BT /Helv {SIZE:.2} Tf {r:.4} {g:.4} {b:.4} rg {:.2} {:.2} Td ",
            cx - ancho / 2.0,
            cy - SIZE * 0.3
        )
        .as_bytes(),
    );
    ops.push(b'(');
    ops.extend_from_slice(&winansi(texto));
    ops.extend_from_slice(b") Tj ET\n");

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
        Object::Array(vec![
            caja[0].into(),
            caja[1].into(),
            caja[2].into(),
            caja[3].into(),
        ]),
    );
    forma.set("Resources", Object::Dictionary(recursos));
    let mut stream = Stream::new(forma, ops);
    let _ = stream.compress();
    doc.add_object(stream)
}

/// La punta de flecha en `desde`, mirando hacia `hacia`. PDFium no escribe
/// apariencias, así que el remate que declara `/LE` lo dibujamos nosotros.
fn flecha(desde: (f32, f32), hacia: (f32, f32), color: [f32; 3]) -> Vec<u8> {
    let [r, g, b] = color;
    let (px, py) = desde;
    let (dx, dy) = (hacia.0 - px, hacia.1 - py);
    let largo = (dx * dx + dy * dy).sqrt().max(0.001);
    let (ux, uy) = (dx / largo, dy / largo);
    let punta = 8.0f32.min(largo * 0.4);
    let ala = 0.42f32; // ±24°, la de Acrobat
    let (c, s) = (ala.cos(), ala.sin());
    let a1 = (px + punta * (ux * c - uy * s), py + punta * (ux * s + uy * c));
    let a2 = (px + punta * (ux * c + uy * s), py + punta * (-ux * s + uy * c));
    format!(
        "q {r:.4} {g:.4} {b:.4} RG 1.5 w {:.2} {:.2} m {px:.2} {py:.2} l {:.2} {:.2} l S Q\n",
        a1.0, a1.1, a2.0, a2.1
    )
    .into_bytes()
}

/// Le aplica a la línea de una llamada (`/CL`) y a los márgenes de su caja
/// (`/RD`) la misma transformación que se le ha aplicado al `/Rect`: sin
/// esto, arrastrar el cuadro dejaba la punta donde estaba.
fn mueve_la_llamada(
    doc: &mut lopdf::Document,
    id: lopdf::ObjectId,
    (sx, sy, e, f): (f32, f32, f32, f32),
) -> Result<(), String> {
    use lopdf::Object;
    let annot = doc
        .get_object(id)
        .and_then(|o| o.as_dict())
        .map_err(|err| err.to_string())?
        .clone();
    let numero = |o: &Object| match o {
        Object::Integer(i) => Some(*i as f32),
        Object::Real(r) => Some(*r),
        _ => None,
    };
    let cl: Vec<f32> = match annot.get(b"CL").and_then(|o| o.as_array()) {
        Ok(a) => a.iter().filter_map(numero).collect(),
        Err(_) => return Ok(()),
    };
    let movida: Vec<Object> = cl
        .chunks(2)
        .filter(|c| c.len() == 2)
        .flat_map(|c| [(c[0] * sx + e).into(), (c[1] * sy + f).into()])
        .collect();
    let rd: Vec<f32> = annot
        .get(b"RD")
        .and_then(|o| o.as_array())
        .map(|a| a.iter().filter_map(numero).collect())
        .unwrap_or_default();
    let d = doc
        .get_object_mut(id)
        .and_then(|o| o.as_dict_mut())
        .map_err(|err| err.to_string())?;
    d.set("CL", Object::Array(movida));
    if rd.len() == 4 {
        d.set(
            "RD",
            Object::Array(vec![
                (rd[0] * sx).into(),
                (rd[1] * sy).into(),
                (rd[2] * sx).into(),
                (rd[3] * sy).into(),
            ]),
        );
    }
    Ok(())
}

/// Lo que se ha llevado un pase de goma, para que la UI pueda contarlo sin
/// tener que mirar el documento otra vez.
#[derive(serde::Serialize, Debug, Default, PartialEq)]
pub struct BorradoTinta {
    /// Trazos a los que la goma ha quitado algo de verdad.
    pub tocados: u16,
    /// De esos, los que se han quedado sin nada y se han ido del `/Annots`.
    pub borrados: u16,
}

/// La goma de borrar, **de una pasada**: busca ella los trazos de la página
/// que tocan el rectángulo y los borra todos dentro de **una sola**
/// mutación. `rect` va en el espacio propio de la página.
///
/// Es lo que hace Acrobat, donde un pase de goma es un paso de deshacer.
/// El camino viejo —una llamada por trazo, con la UI recorriendo las
/// anotaciones y decidiendo cuáles tocaba— se retiró en el ciclo 8: era
/// geometría de PDF fuera de su sitio, y un arrastre sobre tres trazos
/// gastaba tres ⌘Z aunque la banda prometiera uno.
///
/// No es un error que la goma no encuentre nada: se contesta `{ tocados: 0,
/// borrados: 0 }` y la mutación se retira sin dejar paso.
#[tauri::command(async)]
pub fn erase_ink_area(
    work_path: String,
    page_index: u16,
    rect: Rect,
) -> Result<BorradoTinta, String> {
    if rect.w <= 0.0 || rect.h <= 0.0 {
        return Err("El área de borrado no tiene tamaño".into());
    }
    crate::historial::mutacion(work_path, move |work_path| {
        on_pdfium_thread(move || {
            let mut hecho = BorradoTinta::default();
            crate::cirugia_en_hilo(&work_path, |doc| {
                let page_id = *doc
                    .get_pages()
                    .get(&(page_index as u32 + 1))
                    .ok_or("Página fuera de rango")?;
                let geo = crate::formularios2::geo_pagina(doc, page_id)?;
                let g = geo.ui_rect_a_pdf(&rect);
                let goma = (
                    g.left().value,
                    g.bottom().value,
                    g.right().value,
                    g.top().value,
                );
                // los trazos cuya caja toca la goma, con su sitio en el
                // /Annots: los ids no se mueven al borrar, los índices sí
                let candidatos = trazos_en(doc, page_index, goma);
                let mut fuera: Vec<usize> = Vec::new();
                for (indice, id) in candidatos {
                    let (queda, cambiado) = borra_del_trazo(doc, id, goma)?;
                    if !cambiado {
                        continue;
                    }
                    hecho.tocados += 1;
                    if !queda {
                        hecho.borrados += 1;
                        fuera.push(indice);
                    }
                }
                // de mayor a menor: quitar el primero correría los demás
                fuera.sort_unstable_by(|a, b| b.cmp(a));
                for indice in fuera {
                    crate::anotaciones::quita_annot(doc, page_index, indice)?;
                }
                Ok(())
            })?;
            if hecho.tocados == 0 {
                // la goma ha pasado por donde no había trazo: sin cambios,
                // no hay paso de deshacer que ofrecer
                crate::historial::retira_paso(&work_path);
            }
            Ok(hecho)
        })
    })
}

/// Los `Ink` de la página cuya caja toca la goma, como `(índice en /Annots,
/// id del objeto)`. La caja es una criba barata: el recorte de verdad lo
/// hace [`borra_del_trazo`] tramo a tramo.
fn trazos_en(
    doc: &lopdf::Document,
    page_index: u16,
    goma: (f32, f32, f32, f32),
) -> Vec<(usize, lopdf::ObjectId)> {
    use lopdf::Object;
    let Some(annots) = crate::anotaciones::lista_annots(doc, page_index) else {
        return Vec::new();
    };
    annots
        .iter()
        .enumerate()
        .filter_map(|(i, o)| {
            let Object::Reference(id) = o else { return None };
            let d = doc.get_object(*id).ok()?.as_dict().ok()?;
            if d.get(b"Subtype").ok()?.as_name().ok()? != b"Ink" {
                return None;
            }
            let r: Vec<f32> = d
                .get(b"Rect")
                .ok()?
                .as_array()
                .ok()?
                .iter()
                .filter_map(|n| match n {
                    Object::Integer(v) => Some(*v as f32),
                    Object::Real(v) => Some(*v),
                    _ => None,
                })
                .collect();
            if r.len() != 4 {
                return None;
            }
            let (l, b) = (r[0].min(r[2]), r[1].min(r[3]));
            let (der, t) = (r[0].max(r[2]), r[1].max(r[3]));
            let toca = der >= goma.0 && l <= goma.2 && t >= goma.1 && b <= goma.3;
            toca.then_some((i, *id))
        })
        .collect()
}

/// Quita del `/AP` de un Ink los tramos que tocan el rectángulo y rehace su
/// `/Rect` y su `/BBox`. Devuelve si ha quedado algo y si de verdad ha
/// quitado alguno: la goma puede pasar por encima de la caja de un trazo
/// sin tocar ni un tramo, y entonces no hay por qué reescribir nada.
fn borra_del_trazo(
    doc: &mut lopdf::Document,
    id: lopdf::ObjectId,
    goma: (f32, f32, f32, f32),
) -> Result<(bool, bool), String> {
    use lopdf::Object;
    let annot = doc
        .get_object(id)
        .and_then(|o| o.as_dict())
        .map_err(|e| e.to_string())?
        .clone();
    if annot.get(b"Subtype").and_then(|o| o.as_name()).unwrap_or_default() != b"Ink" {
        return Err("Ese comentario no es un trazo".into());
    }
    let ap_id = annot
        .get(b"AP")
        .and_then(|o| o.as_dict())
        .map_err(|_| "El trazo no tiene dibujo que borrar".to_string())?
        .get(b"N")
        .and_then(|o| o.as_reference())
        .map_err(|e| e.to_string())?;
    let stream = doc
        .get_object(ap_id)
        .and_then(|o| o.as_stream())
        .map_err(|e| e.to_string())?;
    let datos = stream
        .decompressed_content()
        .unwrap_or_else(|_| stream.content.clone());
    let (cabecera, trazos) = parte_el_camino(&datos);
    // cada tramo que toca la goma se va; los de los lados sobreviven, que
    // es lo que distingue una goma de borrar el comentario
    let quedan: Vec<Vec<(f32, f32)>> = trazos
        .iter()
        .flat_map(|t| trocea_fuera(t, goma))
        .filter(|t| t.len() >= 2)
        .collect();
    if quedan.is_empty() {
        return Ok((false, true));
    }
    let puntos = |t: &[Vec<(f32, f32)>]| t.iter().map(Vec::len).sum::<usize>();
    if quedan.len() == trazos.len() && puntos(&quedan) == puntos(&trazos) {
        // la goma ha pasado por la caja pero no por el dibujo
        return Ok((true, false));
    }
    let mut ops = cabecera;
    for trazo in &quedan {
        ops.extend_from_slice(format!("{:.2} {:.2} m ", trazo[0].0, trazo[0].1).as_bytes());
        for p in &trazo[1..] {
            ops.extend_from_slice(format!("{:.2} {:.2} l ", p.0, p.1).as_bytes());
        }
    }
    ops.extend_from_slice(b"S Q\n");

    const MARGEN: f32 = 3.0;
    let xs: Vec<f32> = quedan.iter().flatten().map(|p| p.0).collect();
    let ys: Vec<f32> = quedan.iter().flatten().map(|p| p.1).collect();
    let caja = [
        xs.iter().copied().fold(f32::MAX, f32::min) - MARGEN,
        ys.iter().copied().fold(f32::MAX, f32::min) - MARGEN,
        xs.iter().copied().fold(f32::MIN, f32::max) + MARGEN,
        ys.iter().copied().fold(f32::MIN, f32::max) + MARGEN,
    ];
    let caja_obj = || {
        Object::Array(vec![
            caja[0].into(),
            caja[1].into(),
            caja[2].into(),
            caja[3].into(),
        ])
    };
    {
        let st = doc
            .get_object_mut(ap_id)
            .and_then(|o| o.as_stream_mut())
            .map_err(|e| e.to_string())?;
        st.dict.set("BBox", caja_obj());
        st.set_plain_content(ops);
        let _ = st.compress();
    }
    doc.get_object_mut(id)
        .and_then(|o| o.as_dict_mut())
        .map_err(|e| e.to_string())?
        .set("Rect", caja_obj());
    Ok((true, true))
}

/// Parte el content stream de un trazo en (todo lo de antes del camino, los
/// caminos). Solo entiende `m` y `l`, que es lo único que escribe el dibujo
/// de Vitela; lo demás se conserva tal cual delante.
pub(crate) fn parte_el_camino(datos: &[u8]) -> (Vec<u8>, Vec<Vec<(f32, f32)>>) {
    let texto = String::from_utf8_lossy(datos);
    let piezas: Vec<&str> = texto.split_whitespace().collect();
    let mut cabecera = String::new();
    let mut trazos: Vec<Vec<(f32, f32)>> = Vec::new();
    let mut i = 0;
    let mut en_camino = false;
    while i < piezas.len() {
        let p = piezas[i];
        let punto = |i: usize| -> Option<(f32, f32)> {
            Some((piezas.get(i - 2)?.parse().ok()?, piezas.get(i - 1)?.parse().ok()?))
        };
        match p {
            "m" if i >= 2 => {
                if let Some(pt) = punto(i) {
                    trazos.push(vec![pt]);
                    en_camino = true;
                }
            }
            "l" if i >= 2 => {
                if let (Some(pt), Some(t)) = (punto(i), trazos.last_mut()) {
                    t.push(pt);
                }
            }
            // lo que va después del camino (S, Q) se rehace al escribir
            "S" | "s" | "f" | "F" | "B" | "n" | "Q" if en_camino => {}
            // los números se copian con su operador, no sueltos
            _ if !en_camino && p.parse::<f32>().is_err() => {
                let mut j = i;
                let mut nums = Vec::new();
                while j > 0 && piezas[j - 1].parse::<f32>().is_ok() {
                    j -= 1;
                    nums.push(piezas[j]);
                }
                nums.reverse();
                for n in nums {
                    cabecera.push_str(n);
                    cabecera.push(' ');
                }
                cabecera.push_str(p);
                cabecera.push(' ');
            }
            _ => {}
        }
        i += 1;
    }
    (cabecera.into_bytes(), trazos)
}

/// Trocea una polilínea dejando fuera los tramos que tocan el rectángulo.
fn trocea_fuera(
    trazo: &[(f32, f32)],
    goma: (f32, f32, f32, f32),
) -> Vec<Vec<(f32, f32)>> {
    let mut fuera: Vec<Vec<(f32, f32)>> = Vec::new();
    let mut actual: Vec<(f32, f32)> = Vec::new();
    for par in trazo.windows(2) {
        let (a, b) = (par[0], par[1]);
        if toca(a, b, goma) {
            if actual.len() >= 2 {
                fuera.push(std::mem::take(&mut actual));
            } else {
                actual.clear();
            }
            continue;
        }
        if actual.is_empty() {
            actual.push(a);
        }
        actual.push(b);
    }
    if actual.len() >= 2 {
        fuera.push(actual);
    }
    fuera
}

/// ¿El tramo `a`→`b` toca el rectángulo? Recorte de Liang-Barsky: es la
/// prueba exacta, y con ella la goma borra lo que el usuario ha tachado
/// aunque el trazo cruce la zona de lado a lado sin ningún punto dentro.
fn toca(a: (f32, f32), b: (f32, f32), (x0, y0, x1, y1): (f32, f32, f32, f32)) -> bool {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let mut t0 = 0.0f32;
    let mut t1 = 1.0f32;
    for (p, q) in [
        (-dx, a.0 - x0),
        (dx, x1 - a.0),
        (-dy, a.1 - y0),
        (dy, y1 - a.1),
    ] {
        if p.abs() < f32::EPSILON {
            if q < 0.0 {
                return false;
            }
            continue;
        }
        let r = q / p;
        if p < 0.0 {
            if r > t1 {
                return false;
            }
            t0 = t0.max(r);
        } else {
            if r < t0 {
                return false;
            }
            t1 = t1.min(r);
        }
    }
    t0 <= t1
}

/// De qué punto de la caja sale la línea de la llamada: el centro del lado
/// que mira a la punta. Con la punta a un lado sale del lado; con la punta
/// arriba o abajo, del borde de arriba o de abajo.
pub(crate) fn ancla_de_la_caja(caja: (f32, f32, f32, f32), punta: (f32, f32)) -> (f32, f32) {
    let (x0, y0, x1, y1) = caja;
    let (cx, cy) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
    let (dx, dy) = (punta.0 - cx, punta.1 - cy);
    // ¿manda la horizontal o la vertical? se compara con la forma de la
    // caja, que si no una caja muy ancha siempre saldría por el lado
    if dx.abs() * (y1 - y0).max(1.0) >= dy.abs() * (x1 - x0).max(1.0) {
        (if dx >= 0.0 { x1 } else { x0 }, cy)
    } else {
        (cx, if dy >= 0.0 { y1 } else { y0 })
    }
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
/// - **Square** son las marcas de redacción: se mueven y se redimensionan
///   como cualquier otro comentario (el contrato lo prometía y no era
///   verdad) y su `/AP` —el borde rojo, dibujado en local— se rehace.
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
        let es_square;
        // la transformación que se le aplica al rect, para aplicársela
        // también a la línea de una llamada
        let movimiento;
        {
            let mut annot = page
                .annotations_mut()
                .get(annot_index as usize)
                .map_err(|e| e.to_string())?;
            let tipo = annot.annotation_type();
            es_freetext = tipo == PdfPageAnnotationType::FreeText;
            es_square = tipo == PdfPageAnnotationType::Square;
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
            if !interno && !solo_mover && !es_freetext && !es_square {
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
            movimiento = (sx, sy, e, f);
        }
        drop(page);
        save_and_close(doc, &work_path)?;
        if es_freetext {
            // la apariencia del cuadro se dibuja en local (/BBox 0 0 w h):
            // con el tamaño nuevo hay que rehacerla entera. Y si es una
            // llamada, la línea y su punta se mueven con la caja: al `/CL`
            // y al `/RD` se les aplica la misma transformación que al rect,
            // que es lo que hace que arrastrar el cuadro arrastre la punta
            crate::cirugia_en_hilo(&work_path, |doc| {
                let id = crate::anotaciones::annot_id(doc, page_index, annot_index as usize)?;
                mueve_la_llamada(doc, id, movimiento)?;
                crate::anotaciones::regenera_freetext(doc, id)
            })?;
        }
        if es_square {
            // lo mismo con el borde rojo de la marca de redacción
            crate::cirugia_en_hilo(&work_path, |doc| {
                let id = crate::anotaciones::annot_id(doc, page_index, annot_index as usize)?;
                crate::seguridad2::regenera_marca(doc, id)
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

    /// Las líneas del `/AP` de la anotación `i` de la página 0: cada `Tj`
    /// del Form XObject es una línea dibujada.
    fn lineas_del_ap(work: &str, i: usize) -> Vec<String> {
        let mut doc = lopdf::Document::load(work).expect("cargar");
        let id = crate::anotaciones::annot_id(&mut doc, 0, i).expect("annot");
        let annot = doc.get_object(id).and_then(|o| o.as_dict()).expect("dict");
        let ap = annot
            .get(b"AP")
            .and_then(|o| o.as_dict())
            .expect("la anotación no tiene /AP");
        let ap_id = ap.get(b"N").and_then(|o| o.as_reference()).expect("/AP /N");
        let stream = doc
            .get_object(ap_id)
            .and_then(|o| o.as_stream())
            .expect("stream");
        let bytes = stream.decompressed_content().unwrap_or_else(|_| stream.content.clone());
        let contenido = String::from_utf8_lossy(&bytes).into_owned();
        contenido
            .split(") Tj")
            .filter(|t| t.contains('('))
            .map(|t| t.rsplit('(').next().unwrap_or("").to_string())
            .collect()
    }

    /// El contenido crudo del `/AP` de una anotación.
    fn ap_crudo(work: &str, i: usize) -> String {
        let mut doc = lopdf::Document::load(work).expect("cargar");
        let id = crate::anotaciones::annot_id(&mut doc, 0, i).expect("annot");
        let annot = doc.get_object(id).and_then(|o| o.as_dict()).expect("dict");
        let ap_id = annot
            .get(b"AP")
            .and_then(|o| o.as_dict())
            .expect("sin /AP")
            .get(b"N")
            .and_then(|o| o.as_reference())
            .expect("/AP /N");
        let st = doc.get_object(ap_id).and_then(|o| o.as_stream()).expect("stream");
        String::from_utf8_lossy(&st.decompressed_content().unwrap_or_else(|_| st.content.clone()))
            .into_owned()
    }

    /// El valor de una clave de la anotación `i`, como lista de números.
    fn numeros_de(work: &str, i: usize, clave: &[u8]) -> Vec<f32> {
        let mut doc = lopdf::Document::load(work).expect("cargar");
        let id = crate::anotaciones::annot_id(&mut doc, 0, i).expect("annot");
        doc.get_object(id)
            .and_then(|o| o.as_dict())
            .expect("dict")
            .get(clave)
            .and_then(|o| o.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|o| match o {
                        lopdf::Object::Integer(n) => Some(*n as f32),
                        lopdf::Object::Real(r) => Some(*r),
                        _ => None,
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// **H3.** La llamada: un `/FreeText` con `/IT /FreeTextCallout`, su
    /// línea `/CL` y la punta de flecha. Al arrastrarla, la línea y la
    /// punta se van con la caja: si el `/Rect` se mueve y el `/CL` se
    /// queda, la flecha apunta a otro sitio.
    #[test]
    fn una_llamada_apunta_a_su_sitio_y_la_punta_se_mueve_con_la_caja() {
        let tmp = std::env::temp_dir().join("anot2-llamada.pdf");
        crea_pdf(&["Plano"], &tmp);
        let work = tmp.to_string_lossy().into_owned();
        // la punta abajo a la izquierda de la caja de texto
        add_callout(
            work.clone(),
            0,
            Rect { x: 250.0, y: 120.0, w: 160.0, h: 50.0 },
            [90.0, 300.0],
            "Esta cota está mal".into(),
            [226, 61, 61, 255],
            Some("Ana".into()),
            None,
        )
        .expect("crear la llamada");

        let anots = crate::anotaciones::get_annotations(work.clone(), 0).expect("listar");
        assert_eq!(anots.len(), 1);
        assert_eq!(anots[0].kind, "FreeText");
        assert_eq!(anots[0].contents, "Esta cota está mal");
        assert_eq!(anots[0].author, "Ana");

        let cl = numeros_de(&work, 0, b"CL");
        assert_eq!(cl.len(), 4, "la línea va de la punta a la caja: {cl:?}");
        let rect = numeros_de(&work, 0, b"Rect");
        let rd = numeros_de(&work, 0, b"RD");
        assert_eq!(rd.len(), 4, "el /RD dice dónde queda la caja dentro del /Rect");
        // el /Rect abarca la punta
        assert!(
            cl[0] >= rect[0] && cl[0] <= rect[2] && cl[1] >= rect[1] && cl[1] <= rect[3],
            "la punta {:?} tiene que caber en el /Rect {rect:?}",
            (cl[0], cl[1])
        );
        // y el dibujo lleva la flecha
        let ap = ap_crudo(&work, 0);
        assert!(ap.contains(" m ") && ap.contains(" l "), "la línea: {ap}");
        assert!(ap.contains("Esta cota"), "y el texto: {ap}");

        // arrastrarla: la línea y la punta se mueven con la caja
        let antes_rect = rect.clone();
        let antes_cl = cl.clone();
        let vista = &crate::get_page_sizes(work.clone()).expect("tamaños")[0];
        let _ = vista;
        let ui = &crate::anotaciones::get_annotations(work.clone(), 0).expect("listar")[0];
        let (x, y, w, h) = (ui.x, ui.y, ui.w, ui.h);
        transform_annotation(work.clone(), 0, 0, x + 40.0, y + 25.0, w, h).expect("mover");
        let rect2 = numeros_de(&work, 0, b"Rect");
        let cl2 = numeros_de(&work, 0, b"CL");
        let dx = rect2[0] - antes_rect[0];
        let dy = rect2[1] - antes_rect[1];
        assert!(dx.abs() > 1.0 || dy.abs() > 1.0, "algo se ha movido");
        assert!(
            (cl2[0] - antes_cl[0] - dx).abs() < 0.6 && (cl2[1] - antes_cl[1] - dy).abs() < 0.6,
            "el /CL tiene que moverse lo mismo que el /Rect: {antes_cl:?} → {cl2:?} con ({dx},{dy})"
        );
        // y la apariencia se rehace con la línea, no se queda la de antes
        let ap = ap_crudo(&work, 0);
        assert!(ap.contains(" m ") && ap.contains("Esta cota"), "{ap}");

        // el texto vacío se dice
        assert!(add_callout(
            work.clone(),
            0,
            Rect { x: 250.0, y: 120.0, w: 160.0, h: 50.0 },
            [90.0, 300.0],
            "  ".into(),
            [0, 0, 0, 255],
            None,
            None,
        )
        .is_err());
        std::fs::remove_file(&tmp).ok();
    }

    /// **R52.** El codo: en Acrobat la línea de una llamada sale de la caja,
    /// **dobla** y llega a la punta. Vitela solo sabía trazar la recta, así
    /// que una llamada que tenía que rodear una cota pasaba por encima de
    /// ella.
    #[test]
    fn la_llamada_con_codo_escribe_tres_puntos_y_los_arrastra_juntos() {
        let tmp = std::env::temp_dir().join("anot2-llamada-codo.pdf");
        crea_pdf(&["Plano"], &tmp);
        let work = tmp.to_string_lossy().into_owned();
        add_callout(
            work.clone(),
            0,
            Rect { x: 250.0, y: 120.0, w: 160.0, h: 50.0 },
            [90.0, 300.0],
            "Esta cota está mal".into(),
            [226, 61, 61, 255],
            None,
            Some([160.0, 200.0]),
        )
        .expect("crear la llamada con codo");

        let cl = numeros_de(&work, 0, b"CL");
        assert_eq!(cl.len(), 6, "punta, codo y ancla: {cl:?}");
        let rect = numeros_de(&work, 0, b"Rect");
        for (x, y) in [(cl[0], cl[1]), (cl[2], cl[3])] {
            assert!(
                x >= rect[0] && x <= rect[2] && y >= rect[1] && y <= rect[3],
                "el /Rect tiene que abarcar también el codo: {rect:?} vs {:?}",
                (x, y)
            );
        }
        // el dibujo lleva los dos tramos
        let ap = ap_crudo(&work, 0);
        assert!(ap.matches(" l ").count() >= 2, "dos tramos de línea: {ap}");

        // y arrastrar la caja se lleva la línea entera, codo incluido
        let antes = cl.clone();
        let ui = &crate::anotaciones::get_annotations(work.clone(), 0).expect("listar")[0];
        transform_annotation(work.clone(), 0, 0, ui.x + 40.0, ui.y + 25.0, ui.w, ui.h)
            .expect("mover");
        let rect2 = numeros_de(&work, 0, b"Rect");
        let cl2 = numeros_de(&work, 0, b"CL");
        assert_eq!(cl2.len(), 6, "el codo no se pierde al mover: {cl2:?}");
        let (dx, dy) = (rect2[0] - rect[0], rect2[1] - rect[1]);
        for i in 0..3 {
            assert!(
                (cl2[i * 2] - antes[i * 2] - dx).abs() < 0.6
                    && (cl2[i * 2 + 1] - antes[i * 2 + 1] - dy).abs() < 0.6,
                "el punto {i} tiene que moverse con el /Rect: {antes:?} → {cl2:?}"
            );
        }
        std::fs::remove_file(&tmp).ok();
    }

    /// El ancla sale por el lado que mira a la punta.
    #[test]
    fn la_linea_de_la_llamada_sale_por_el_lado_que_mira_a_la_punta() {
        let caja = (100.0, 100.0, 200.0, 140.0);
        assert_eq!(ancla_de_la_caja(caja, (300.0, 120.0)), (200.0, 120.0), "a la derecha");
        assert_eq!(ancla_de_la_caja(caja, (20.0, 120.0)), (100.0, 120.0), "a la izquierda");
        assert_eq!(ancla_de_la_caja(caja, (150.0, 400.0)), (150.0, 140.0), "arriba");
        assert_eq!(ancla_de_la_caja(caja, (150.0, 10.0)), (150.0, 100.0), "abajo");
    }

    /// **H3.** La goma de borrar quita del trazo lo que se tacha y deja el
    /// resto: hasta ahora la única forma de arreglar un garabato era
    /// borrarlo entero.
    #[test]
    fn la_goma_borra_medio_trazo_y_deja_la_otra_mitad() {
        let tmp = std::env::temp_dir().join("anot2-goma.pdf");
        crea_pdf(&["Dibujo"], &tmp);
        let work = tmp.to_string_lossy().into_owned();
        // una línea horizontal de doce tramos, de x=100 a x=340
        let puntos: Vec<[f32; 2]> = (0..13).map(|i| [100.0 + i as f32 * 20.0, 300.0]).collect();
        crate::anotaciones::add_stroke(work.clone(), 0, puntos, None, None, None)
            .expect("dibujar");
        let ancho_antes = crate::anotaciones::get_annotations(work.clone(), 0).expect("listar")[0].w;

        // la goma en el trozo del medio
        let hecho = erase_ink_area(
            work.clone(),
            0,
            Rect { x: 180.0, y: 280.0, w: 80.0, h: 40.0 },
        )
        .expect("borrar el medio");
        assert_eq!(
            hecho,
            BorradoTinta { tocados: 1, borrados: 0 },
            "queda trazo a los dos lados"
        );

        let ap = ap_crudo(&work, 0);
        let subcaminos = ap.matches(" m ").count();
        assert_eq!(subcaminos, 2, "dos trozos, uno a cada lado:\n{ap}");
        assert!(ap.contains("2 w"), "el grosor y el color se conservan:\n{ap}");
        assert!(
            !ap.contains("220.00 300.00"),
            "el punto de en medio ya no está:\n{ap}"
        );
        // la caja se encoge un poco, pero el comentario sigue ahí
        let anots = crate::anotaciones::get_annotations(work.clone(), 0).expect("listar");
        assert_eq!(anots.len(), 1);
        assert!(anots[0].w <= ancho_antes + 0.1);
        crate::render_page_png(work.clone(), 0, 200, true).expect("render con el trazo a medias");

        // ⌘Z devuelve el trazo entero
        crate::historial::undo(work.clone()).expect("deshacer");
        let ap = ap_crudo(&work, 0);
        assert_eq!(ap.matches(" m ").count(), 2, "PDFium escribe un `m` de más al crear");

        // y borrarlo todo se lleva el comentario, como en Acrobat
        let hecho = erase_ink_area(
            work.clone(),
            0,
            Rect { x: 50.0, y: 250.0, w: 400.0, h: 100.0 },
        )
        .expect("borrar entero");
        assert_eq!(hecho, BorradoTinta { tocados: 1, borrados: 1 });
        assert!(crate::anotaciones::get_annotations(work.clone(), 0)
            .expect("listar")
            .is_empty());
        std::fs::remove_file(&tmp).ok();
    }

    /// **R43b (AC-069 y «Distinto» 3).** La medida que se deja puesta es un
    /// **comentario**, no contenido de la página: sale en el panel de
    /// comentarios, se borra con Supr y no ensucia el texto del documento.
    /// Y es **una** mutación, que es lo que la banda promete: antes eran
    /// `add_shape` más `add_text_block`, así que el primer ⌘Z quitaba la
    /// cifra y dejaba la raya.
    #[test]
    fn la_medida_puesta_es_un_comentario_y_un_solo_paso_de_deshacer() {
        let tmp = std::env::temp_dir().join("anot2-medida.pdf");
        crea_pdf(&["Plano"], &tmp);
        let work = tmp.to_string_lossy().into_owned();
        let textos_antes = crate::texto::get_text_blocks(work.clone(), 0).expect("bloques").len();
        let pasos = crate::historial::history_state(work.clone()).expect("historial").undo;

        add_measure(
            work.clone(),
            0,
            vec![[100.0, 300.0], [340.0, 300.0]],
            "150,44 m".into(),
            [200, 40, 40, 255],
            None,
            Some("Jorge".into()),
        )
        .expect("dejar la medida puesta");

        assert_eq!(
            crate::historial::history_state(work.clone()).expect("historial").undo,
            pasos + 1,
            "un gesto, un paso de deshacer"
        );

        // es un comentario, con la cifra dentro
        let anots = crate::anotaciones::get_annotations(work.clone(), 0).expect("listar");
        assert_eq!(anots.len(), 1, "sale en el panel de comentarios: {anots:?}");
        assert_eq!(anots[0].kind, "Line");
        assert_eq!(anots[0].contents, "150,44 m");
        assert_eq!(anots[0].author, "Jorge");

        // y NO es texto del documento
        let textos = crate::texto::get_text_blocks(work.clone(), 0).expect("bloques");
        assert_eq!(textos.len(), textos_antes, "la cifra no entra en el texto: {textos:?}");
        assert!(
            !textos.iter().any(|b| b.text.contains("150,44")),
            "la medida no ensucia el content stream"
        );

        // se ve en el render, que es lo que PDFium no hace solo
        crate::render_page_png(work.clone(), 0, 200, true).expect("render con la medida");

        // se borra como cualquier comentario
        crate::anotaciones::remove_annotation(work.clone(), 0, 0).expect("borrar con Supr");
        assert!(crate::anotaciones::get_annotations(work.clone(), 0)
            .expect("listar")
            .is_empty());

        // el área de un polígono es un /Polygon cerrado, sin flechas
        add_measure(
            work.clone(),
            0,
            vec![[100.0, 300.0], [300.0, 300.0], [300.0, 420.0], [100.0, 420.0]],
            "2,4 m²".into(),
            [40, 120, 200, 255],
            Some(true),
            None,
        )
        .expect("dejar puesta el área");
        let anots = crate::anotaciones::get_annotations(work.clone(), 0).expect("listar");
        assert_eq!(anots[0].kind, "Polygon");
        assert_eq!(anots[0].contents, "2,4 m²");
        // y el perímetro, una polilínea abierta
        add_measure(
            work.clone(),
            0,
            vec![[100.0, 500.0], [200.0, 520.0], [300.0, 500.0]],
            "310 cm".into(),
            [40, 120, 200, 255],
            None,
            None,
        )
        .expect("dejar puesto el perímetro");
        let anots = crate::anotaciones::get_annotations(work.clone(), 0).expect("listar");
        // ojo: `get_annotations` da el nombre con el que PDFium llama al
        // subtipo, que aquí es «Polyline» y no el «PolyLine» del spec
        assert!(
            anots.iter().any(|a| a.kind == "Polyline"),
            "el perímetro es una polilínea: {anots:?}"
        );

        // dos puntos como mínimo, y algo que decir
        assert!(add_measure(work.clone(), 0, vec![[1.0, 1.0]], "x".into(), [0, 0, 0, 255], None, None)
            .is_err());
        assert!(add_measure(
            work.clone(),
            0,
            vec![[1.0, 1.0], [2.0, 2.0]],
            "  ".into(),
            [0, 0, 0, 255],
            None,
            None
        )
        .is_err());
        std::fs::remove_file(&tmp).ok();
    }

    /// **R34b.** En Acrobat un pase de goma es **un** paso de deshacer.
    /// Aquí la UI llamaba una vez por trazo y cada llamada
    /// traía su propia `mutacion`: un arrastre sobre tres trazos gastaba
    /// tres ⌘Z mientras la banda prometía uno. `erase_ink_area` busca él
    /// los trazos que tocan la zona y hace el lote entero de una vez.
    #[test]
    fn la_goma_de_una_pasada_es_un_solo_paso_de_deshacer() {
        let tmp = std::env::temp_dir().join("anot2-goma-area.pdf");
        crea_pdf(&["Dibujo"], &tmp);
        let work = tmp.to_string_lossy().into_owned();
        // dos rayas horizontales que cruzan la zona de la goma y una
        // tercera lejos, que no se puede tocar
        for y in [280.0f32, 320.0] {
            let puntos: Vec<[f32; 2]> = (0..13).map(|i| [100.0 + i as f32 * 20.0, y]).collect();
            crate::anotaciones::add_stroke(work.clone(), 0, puntos, None, None, None)
                .expect("dibujar");
        }
        let lejos: Vec<[f32; 2]> = (0..5).map(|i| [100.0 + i as f32 * 20.0, 600.0]).collect();
        crate::anotaciones::add_stroke(work.clone(), 0, lejos, None, None, None).expect("dibujar");
        let pasos = crate::historial::history_state(work.clone()).expect("historial").undo;

        let hecho = erase_ink_area(
            work.clone(),
            0,
            Rect { x: 180.0, y: 260.0, w: 80.0, h: 100.0 },
        )
        .expect("una sola pasada de goma");
        assert_eq!(
            hecho,
            BorradoTinta { tocados: 2, borrados: 0 },
            "las dos rayas que cruzan la zona, y solo esas"
        );
        let ahora = crate::historial::history_state(work.clone()).expect("historial").undo;
        assert_eq!(ahora, pasos + 1, "un pase de goma, un paso de deshacer");

        // los tres comentarios siguen ahí y los dos borrados están partidos
        let anots = crate::anotaciones::get_annotations(work.clone(), 0).expect("listar");
        assert_eq!(anots.len(), 3);

        // y un ⌘Z devuelve las dos rayas enteras, no media
        crate::historial::undo(work.clone()).expect("deshacer");
        for i in 0..2 {
            let ap = ap_crudo(&work, i);
            assert_eq!(
                ap.matches(" m ").count(),
                2,
                "la raya {i} vuelve entera (PDFium escribe un `m` de más al crear):\n{ap}"
            );
        }

        // la goma que se lleva los trazos enteros los quita del /Annots, y
        // los índices no se pisan al borrar dos de golpe
        let hecho = erase_ink_area(
            work.clone(),
            0,
            Rect { x: 50.0, y: 250.0, w: 400.0, h: 120.0 },
        )
        .expect("borrarlas del todo");
        assert_eq!(hecho, BorradoTinta { tocados: 2, borrados: 2 });
        let anots = crate::anotaciones::get_annotations(work.clone(), 0).expect("listar");
        assert_eq!(anots.len(), 1, "queda la raya de arriba");

        // pasar la goma por donde no hay nada no es un error ni deja paso
        let pasos = crate::historial::history_state(work.clone()).expect("historial").undo;
        let hecho = erase_ink_area(
            work.clone(),
            0,
            Rect { x: 20.0, y: 20.0, w: 30.0, h: 30.0 },
        )
        .expect("la goma en el vacío no es un error");
        assert_eq!(hecho, BorradoTinta::default());
        assert_eq!(
            crate::historial::history_state(work.clone()).expect("historial").undo,
            pasos,
            "sin cambios no se ofrece un ⌘Z que no hace nada"
        );
        std::fs::remove_file(&tmp).ok();
    }

    /// El cuadro de texto se ajusta al ancho SIEMPRE, no solo al crearlo:
    /// en Acrobat la frase refluye al corregir el texto y al tirar de un
    /// tirador. Aquí el `/AP` solo partía por `\n`, así que una frase larga
    /// escrita después se salía por el borde derecho.
    #[test]
    fn el_cuadro_de_texto_reajusta_las_lineas_al_ancho() {
        let tmp = std::env::temp_dir().join("anotaciones2-reflujo-test.pdf");
        crea_pdf(&["Hola"], &tmp);
        let work = tmp.to_string_lossy().into_owned();
        let frase = "Esta es una frase larga de una sola linea que no cabe de ninguna manera en la caja";
        add_free_text(
            work.clone(),
            0,
            Rect { x: 40.0, y: 100.0, w: 220.0, h: 80.0 },
            frase.into(),
            12.0,
            [0, 0, 0, 255],
            true,
            None,
        )
        .expect("cuadro de texto");
        let al_crear = lineas_del_ap(&work, 0);
        assert!(al_crear.len() > 1, "al crear ya se parte: {al_crear:?}");
        for l in &al_crear {
            assert!(
                ancho_helvetica(l, 12.0) <= 220.0,
                "la línea {l:?} se sale de la caja"
            );
        }

        // corregir el texto vuelve a partirlo (antes iba crudo al /AP)
        crate::anotaciones::set_annotation_contents(
            work.clone(),
            0,
            0,
            format!("{frase} y encima le añadimos todavía un poco más de texto"),
            None,
        )
        .expect("corregir");
        let al_corregir = lineas_del_ap(&work, 0);
        assert!(
            al_corregir.len() > al_crear.len(),
            "corregir no ha reajustado: {al_corregir:?}"
        );

        // estrechar la caja parte más; ensancharla junta
        transform_annotation(work.clone(), 0, 0, 40.0, 100.0, 120.0, 80.0).expect("estrechar");
        let estrecho = lineas_del_ap(&work, 0);
        assert!(
            estrecho.len() > al_corregir.len(),
            "estrechar no ha reajustado: {estrecho:?}"
        );
        for l in &estrecho {
            assert!(
                ancho_helvetica(l, 12.0) <= 120.0,
                "la línea {l:?} se sale de la caja estrecha"
            );
        }
        transform_annotation(work.clone(), 0, 0, 40.0, 100.0, 400.0, 80.0).expect("ensanchar");
        let ancho = lineas_del_ap(&work, 0);
        assert!(
            ancho.len() < estrecho.len(),
            "ensanchar no ha reajustado: {ancho:?}"
        );
        std::fs::remove_file(&tmp).ok();
    }

    /// Los saltos que escribe el usuario se respetan, y una palabra que no
    /// cabe entera se parte por letras en vez de desbordar.
    #[test]
    fn el_reparto_de_lineas_respeta_los_saltos_y_parte_las_palabras_largas() {
        let lineas = parte_lineas("Uno\nDos", 12.0, 300.0);
        assert_eq!(lineas, vec!["Uno".to_string(), "Dos".to_string()]);
        let largas = parte_lineas("supercalifragilisticoespialidoso", 12.0, 60.0);
        assert!(largas.len() > 2, "{largas:?}");
        for l in &largas {
            assert!(ancho_helvetica(l, 12.0) <= 60.0, "{l:?} desborda");
        }
    }

    fn render_rgba(path: &str) -> image::RgbaImage {
        let png_b64 = crate::render_page_b64(path.to_string(), 0, 600, None).expect("render");
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
            add_markup(
                work.clone(),
                0,
                vec![Rect {
                    x: rx.min(rx2),
                    y: ry.min(ry2),
                    w: (rx2 - rx).abs(),
                    h: (ry2 - ry).abs(),
                }],
                "highlight".into(),
                None,
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
        crate::render_page_b64(work.clone(), 0, 400, None).expect("render");
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


    /// El `/AP` de un cuadro de texto y el de la firma se escriben en
    /// WinAnsiEncoding, que NO es latin-1: el tramo 0x80–0x9F lleva la raya,
    /// el guion, los puntos suspensivos, las comillas tipográficas y el
    /// euro, justo lo que un texto en español usa a diario y lo que salía
    /// como interrogantes.
    #[test]
    fn el_winansi_no_convierte_en_interrogantes_lo_que_lleva_el_espanol() {
        assert_eq!(winansi("año ñ ü"), b"a\xf1o \xf1 \xfc".to_vec());
        assert_eq!(
            winansi("— – … “x” ‘y’ € •"),
            b"\x97 \x96 \x85 \x93x\x94 \x91y\x92 \x80 \x95".to_vec()
        );
        // lo que de verdad no cabe en WinAnsi sigue siendo un interrogante
        assert_eq!(winansi("漢"), b"?".to_vec());
        // y los paréntesis y la barra siguen escapados
        assert_eq!(winansi("(a\\b)"), b"\\(a\\\\b\\)".to_vec());
    }

}
