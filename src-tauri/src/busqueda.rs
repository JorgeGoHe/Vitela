//! Extracción de texto con cajas de glifos y búsqueda en el documento.

use crate::{on_pdfium_thread, with_doc, Geo, Rect};
use pdfium_render::prelude::*;
use serde::Serialize;

#[derive(Serialize, Clone)]
pub struct CharBox {
    pub ch: String,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

#[derive(Serialize)]
pub struct PageText {
    pub width: f32,
    pub height: f32,
    pub chars: Vec<CharBox>,
}

#[derive(Serialize)]
pub struct SearchMatch {
    pub page_index: u16,
    pub rects: Vec<Rect>,
    /// Los 30 caracteres anteriores a la coincidencia, para la lista de
    /// resultados («…el contrato se firma el 3 de…»). Vacío si no se pidió
    /// contexto.
    pub before: String,
    /// Ídem, los 30 siguientes.
    pub after: String,
    /// Bloque de texto (`object_index` de `get_text_blocks`) en el que cae
    /// la coincidencia, que es lo que necesita `replace_text` para
    /// reescribirla. `None` si no se pidió contexto o si la coincidencia no
    /// cae dentro de ningún bloque editable.
    pub block_index: Option<u32>,
}

/// Extrae los caracteres de una página con sus cajas de glifos, en puntos PDF
/// y con origen arriba-izquierda (PDFium usa origen abajo-izquierda).
///
/// El espacio es el PROPIO de la página, sin la rotación aplicada: es donde
/// están escritas de verdad las cajas de los glifos, y la UI convierte al
/// espacio de la vista con la `rotation` de `get_page_sizes`. Por eso el
/// volteo va con `Geo` y no con `page.height()`, que PDFium devuelve YA
/// rotada: en una A4 con /Rotate 90 la diferencia son 246 pt y la selección
/// caía fuera del texto.
pub fn extract_chars(page: &PdfPage) -> Result<PageText, String> {
    let text = page.text().map_err(|e| e.to_string())?;
    let geo = Geo::de_pagina(page).propia();
    let mut chars = Vec::new();
    for c in text.chars().iter() {
        let ch = c.unicode_char().unwrap_or('\u{fffd}');
        let b = match c.loose_bounds() {
            Ok(b) => b,
            Err(_) => continue,
        };
        let caja = geo.pdf_rect_a_ui(&b);
        chars.push(CharBox {
            ch: ch.to_string(),
            x: caja.x,
            y: caja.y,
            w: caja.w,
            h: caja.h,
        });
    }
    Ok(PageText {
        width: geo.ancho(),
        height: geo.alto(),
        chars,
    })
}

#[tauri::command(async)]
pub fn get_page_text(path: String, page_index: u16) -> Result<PageText, String> {
    on_pdfium_thread(move || {
        with_doc(&path, |doc| {
            let page = doc.pages().get(page_index).map_err(|e| e.to_string())?;
            extract_chars(&page)
        })
    })
}

/// Normaliza para búsqueda: todo espacio en blanco (incl. el \r\n que
/// PDFium intercala) se trata como espacio simple y, salvo que se pida
/// distinguir mayúsculas, se pasa a minúscula.
pub fn normalize(c: char, match_case: bool) -> char {
    let c = if match_case {
        c
    } else {
        c.to_lowercase().next().unwrap_or(c)
    };
    if c.is_whitespace() {
        ' '
    } else {
        c
    }
}

/// Un carácter que forma parte de una palabra, para «solo palabras
/// completas». Unicode-aware: los acentos y la ñ cuentan como letra.
fn es_de_palabra(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Une cajas de caracteres consecutivos en rectángulos por línea.
pub fn merge_line_rects(boxes: &[CharBox]) -> Vec<Rect> {
    let mut out: Vec<Rect> = Vec::new();
    for b in boxes {
        if b.w <= 0.0 || b.h <= 0.0 {
            continue;
        }
        if let Some(last) = out.last_mut() {
            let same_line = (b.y - last.y).abs() < last.h.max(b.h) * 0.7;
            if same_line {
                let right = (last.x + last.w).max(b.x + b.w);
                let bottom = (last.y + last.h).max(b.y + b.h);
                last.x = last.x.min(b.x);
                last.y = last.y.min(b.y);
                last.w = right - last.x;
                last.h = bottom - last.y;
                continue;
            }
        }
        out.push(Rect {
            x: b.x,
            y: b.y,
            w: b.w,
            h: b.h,
        });
    }
    out
}

/// Normaliza una secuencia colapsando rachas de espacios en uno solo.
/// Devuelve pares (carácter normalizado, índice original).
pub fn normaliza_colapsando(
    chars: impl Iterator<Item = char>,
    match_case: bool,
) -> Vec<(char, usize)> {
    let mut out: Vec<(char, usize)> = Vec::new();
    for (i, c) in chars.enumerate() {
        let n = normalize(c, match_case);
        if n == ' ' && matches!(out.last(), Some((' ', _))) {
            continue;
        }
        out.push((n, i));
    }
    out
}

/// Busca `query` en todas las páginas, sin solapamientos y tratando
/// cualquier racha de espacios/saltos de línea como un espacio, y devuelve
/// los rectángulos de cada coincidencia.
///
/// `match_case` y `whole_word` son las dos casillas de Acrobat, apagadas
/// por defecto: sin ellas la búsqueda no distingue mayúsculas y acepta
/// coincidencias dentro de una palabra. Se pueden omitir (`null`), que es
/// lo mismo que apagadas.
///
/// Con `context` cada coincidencia trae además la frase de alrededor (30
/// caracteres a cada lado) y el bloque de texto en el que cae: es lo que
/// necesita la lista de resultados y lo que `replace_text` reescribe.
/// Cuesta una pasada más por los objetos de la página, así que solo se hace
/// si se pide.
#[tauri::command(async)]
pub fn search_pdf(
    path: String,
    query: String,
    match_case: Option<bool>,
    whole_word: Option<bool>,
    context: Option<bool>,
) -> Result<Vec<SearchMatch>, String> {
    let match_case = match_case.unwrap_or(false);
    let whole_word = whole_word.unwrap_or(false);
    let context = context.unwrap_or(false);
    let needle: Vec<char> = normaliza_colapsando(query.trim().chars(), match_case)
        .into_iter()
        .map(|(c, _)| c)
        .collect();
    if needle.is_empty() {
        return Ok(Vec::new());
    }
    on_pdfium_thread(move || {
        with_doc(&path, |doc| {
            let mut results = Vec::new();
            for (page_index, page) in doc.pages().iter().enumerate() {
                let page_text = extract_chars(&page)?;
                let bloques = if context { bloques_de_texto(&page) } else { Vec::new() };
                let hay = normaliza_colapsando(
                    page_text
                        .chars
                        .iter()
                        .map(|c| c.ch.chars().next().unwrap_or(' ')),
                    match_case,
                );
                if hay.len() < needle.len() {
                    continue;
                }
                let mut start = 0;
                while start + needle.len() <= hay.len() {
                    let fin = start + needle.len();
                    let en_limite_de_palabra = !whole_word
                        || (start
                            .checked_sub(1)
                            .is_none_or(|i| !es_de_palabra(hay[i].0))
                            && hay.get(fin).is_none_or(|(c, _)| !es_de_palabra(*c)));
                    if en_limite_de_palabra
                        && hay[start..fin]
                            .iter()
                            .map(|(c, _)| *c)
                            .eq(needle.iter().copied())
                    {
                        let from = hay[start].1;
                        let to = hay[start + needle.len() - 1].1;
                        let rects = merge_line_rects(&page_text.chars[from..=to]);
                        if !rects.is_empty() {
                            let (before, after) = if context {
                                alrededor(&page_text.chars, from, to)
                            } else {
                                (String::new(), String::new())
                            };
                            let block_index = if context {
                                bloque_en(&bloques, &rects[0])
                            } else {
                                None
                            };
                            results.push(SearchMatch {
                                page_index: page_index as u16,
                                rects,
                                before,
                                after,
                                block_index,
                            });
                        }
                        start += needle.len();
                    } else {
                        start += 1;
                    }
                }
            }
            Ok(results)
        })
    })
}

/// Cuántos caracteres de contexto se devuelven a cada lado.
const CONTEXTO: usize = 30;

/// La frase de alrededor de una coincidencia, para la lista de resultados.
/// Las rachas de espacios y saltos se colapsan: en una lista, un salto de
/// línea del PDF solo estorba.
fn alrededor(chars: &[CharBox], from: usize, to: usize) -> (String, String) {
    let junta = |trozo: &[CharBox]| {
        let bruto: String = trozo.iter().map(|c| c.ch.as_str()).collect();
        let mut out = String::new();
        for c in bruto.chars() {
            if c.is_whitespace() {
                if !out.ends_with(' ') {
                    out.push(' ');
                }
            } else {
                out.push(c);
            }
        }
        out
    };
    let ini = from.saturating_sub(CONTEXTO);
    let fin = (to + 1 + CONTEXTO).min(chars.len());
    (junta(&chars[ini..from]), junta(&chars[to + 1..fin]))
}

/// Los bloques de texto de la página con su caja, en el espacio propio de
/// la página (el mismo en el que se devuelven los rects de la búsqueda).
fn bloques_de_texto(page: &PdfPage) -> Vec<(u32, Rect)> {
    let geo = Geo::de_pagina(page).propia();
    let objects = page.objects();
    let mut out = Vec::new();
    for i in 0..objects.len() {
        let Ok(obj) = objects.get(i) else { continue };
        let Some(t) = obj.as_text_object() else { continue };
        if t.text().trim().is_empty() {
            continue;
        }
        let Ok(b) = obj.bounds() else { continue };
        let caja = geo.pdf_rect_a_ui(&PdfRect::new(b.bottom(), b.left(), b.top(), b.right()));
        out.push((i as u32, caja));
    }
    out
}

/// El bloque que contiene el centro de esa caja (el más pequeño, si hay
/// varios solapados: es el más ajustado a la coincidencia).
fn bloque_en(bloques: &[(u32, Rect)], r: &Rect) -> Option<u32> {
    let (cx, cy) = (r.x + r.w / 2.0, r.y + r.h / 2.0);
    bloques
        .iter()
        .filter(|(_, b)| {
            cx >= b.x - 1.0 && cx <= b.x + b.w + 1.0 && cy >= b.y - 1.0 && cy <= b.y + b.h + 1.0
        })
        .min_by(|(_, a), (_, b)| {
            (a.w * a.h)
                .partial_cmp(&(b.w * b.h))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|(i, _)| *i)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[allow(unused_imports)]
    use crate::tests::{crea_pdf, textos_de};
    #[allow(unused_imports)]
    use crate::{get_page_sizes, open_pdf, render_page_b64, render_page_png};

    #[test]
    fn extrae_texto_con_cajas() {
        let tmp = std::env::temp_dir().join("editor_pdf_test_texto.pdf");
        crea_pdf(&["Hola Mundo"], &tmp);
        let extracted = get_page_text(tmp.to_string_lossy().into_owned(), 0).expect("extraer");
        std::fs::remove_file(&tmp).ok();
        let joined: String = extracted.chars.iter().map(|c| c.ch.as_str()).collect();
        assert!(joined.contains("Hola"), "texto extraído: {joined:?}");
        assert!(extracted.chars.iter().any(|c| c.w > 0.0 && c.h > 0.0));
        assert!(extracted.width > 0.0 && extracted.height > 0.0);
    }

    #[test]
    fn busca_sin_distinguir_mayusculas() {
        let tmp = std::env::temp_dir().join("editor_pdf_test_busqueda.pdf");
        crea_pdf(&["Hola Mundo"], &tmp);
        let matches =
            search_pdf(tmp.to_string_lossy().into_owned(), "mundo".into(), None, None, None).expect("buscar");
        std::fs::remove_file(&tmp).ok();
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].page_index, 0);
        assert!(!matches[0].rects.is_empty());
    }

    #[test]
    fn cuenta_coincidencias() {
        let tmp = std::env::temp_dir().join("editor_pdf_test_contador.pdf");
        crea_pdf(&["banana banana — Hola hola HOLA — Hola  Mundo"], &tmp);
        let path = tmp.to_string_lossy().into_owned();

        // no solapadas: una por "banana", no dos dentro de la misma palabra
        let m = search_pdf(path.clone(), "ana".into(), None, None, None).expect("buscar ana");
        assert_eq!(m.len(), 2, "'ana' en 'banana banana'");

        // sin distinguir mayúsculas
        let m = search_pdf(path.clone(), "hola".into(), None, None, None).expect("buscar hola");
        assert_eq!(m.len(), 4, "'hola' aparece 4 veces");

        // rachas de espacios en el documento cuentan como un espacio
        let m = search_pdf(path.clone(), "hola mundo".into(), None, None, None).expect("buscar frase");
        assert_eq!(m.len(), 1, "'Hola  Mundo' con doble espacio");

        std::fs::remove_file(&tmp).ok();
    }

    #[test]
    fn coincidir_mayusculas_y_palabra_completa() {
        let tmp = std::env::temp_dir().join("editor_pdf_test_busqueda_opciones.pdf");
        crea_pdf(&["Casa casaca CASA — año año, añoso"], &tmp);
        let path = tmp.to_string_lossy().into_owned();
        let cuenta = |q: &str, mc: bool, ww: bool| {
            search_pdf(path.clone(), q.into(), Some(mc), Some(ww), None)
                .expect("buscar")
                .len()
        };

        // por defecto (las dos apagadas, como Acrobat) y omitiéndolas
        assert_eq!(cuenta("casa", false, false), 3, "Casa, casaca, CASA");
        assert_eq!(
            search_pdf(path.clone(), "casa".into(), None, None, None)
                .expect("buscar sin flags")
                .len(),
            3,
            "omitir las casillas es lo mismo que dejarlas apagadas"
        );
        // solo mayúsculas: descarta Casa y CASA, deja el trozo de «casaca»
        assert_eq!(cuenta("casa", true, false), 1);
        assert_eq!(cuenta("Casa", true, false), 1);
        // solo palabra completa: descarta el trozo dentro de «casaca»
        assert_eq!(cuenta("casa", false, true), 2, "Casa y CASA");
        // las dos a la vez
        assert_eq!(cuenta("CASA", true, true), 1);

        // límites de palabra en Unicode: la ñ y los acentos son letra
        assert_eq!(cuenta("año", false, false), 3, "año, año, añoso");
        assert_eq!(cuenta("año", false, true), 2, "«añoso» no cuenta");
        std::fs::remove_file(&tmp).ok();
    }

    #[test]
    fn merge_line_rects_une_por_linea() {
        let boxes = vec![
            CharBox {
                ch: "a".into(),
                x: 0.0,
                y: 10.0,
                w: 5.0,
                h: 10.0,
            },
            CharBox {
                ch: "b".into(),
                x: 5.0,
                y: 10.0,
                w: 5.0,
                h: 10.0,
            },
            CharBox {
                ch: "c".into(),
                x: 0.0,
                y: 30.0,
                w: 5.0,
                h: 10.0,
            },
        ];
        let rects = merge_line_rects(&boxes);
        assert_eq!(rects.len(), 2);
        assert_eq!(rects[0].w, 10.0);
    }

    /// Girar la página no mueve nada: los objetos siguen escritos donde
    /// estaban y solo cambia cómo se enseñan. Los cuatro comandos que LEEN
    /// contenido (`get_page_text`, `search_pdf`, `get_text_blocks` y
    /// `get_images`) devuelven el espacio PROPIO de la página, así que su
    /// respuesta tiene que ser la MISMA con /Rotate 0, 90, 180 y 270: la UI
    /// convierte al espacio de la vista con la `rotation` de
    /// `get_page_sizes`, igual que hace al revés antes de escribir.
    ///
    /// Se comparaba con `page.height()`, que PDFium devuelve YA rotada
    /// mientras que las cajas de los objetos no lo están: en una A4 girada
    /// el volteo de la `y` salía desplazado 246 pt (AC-014) y la selección,
    /// la búsqueda y los tiradores de las imágenes caían fuera del texto.
    #[test]
    fn el_contenido_se_lee_igual_en_una_pagina_rotada() {
        let tmp = std::env::temp_dir().join("busqueda-rotada-test.pdf");
        crea_pdf(&["Hola Mundo"], &tmp);
        let work = tmp.to_string_lossy().into_owned();
        // una imagen para que get_images tenga algo que decir
        let png = image::RgbaImage::from_pixel(80, 40, image::Rgba([200, 30, 30, 255]));
        let ruta_png = std::env::temp_dir().join("busqueda-rotada-test.png");
        png.save(&ruta_png).expect("crear png");
        crate::imagenes::add_image(
            work.clone(),
            0,
            ruta_png.to_string_lossy().into_owned(),
            120.0,
            400.0,
        )
        .expect("insertar imagen");

        let referencia = lo_que_se_lee(&work);
        assert!(!referencia.is_empty(), "la página de prueba no tiene contenido");

        for grados in [90u16, 180, 270] {
            crate::paginas::rotate_page(work.clone(), 0).expect("girar");
            let ahora = lo_que_se_lee(&work);
            assert_eq!(
                ahora.len(),
                referencia.len(),
                "con /Rotate {grados} cambió el número de cajas leídas"
            );
            for (i, (a, b)) in ahora.iter().zip(referencia.iter()).enumerate() {
                assert!(
                    (a.0 - b.0).abs() < 0.5
                        && (a.1 - b.1).abs() < 0.5
                        && (a.2 - b.2).abs() < 0.5
                        && (a.3 - b.3).abs() < 0.5,
                    "con /Rotate {grados} la caja {i} se lee en {a:?} y sin girar en {b:?}"
                );
            }
        }

        // y el juez de siempre: un sello puesto donde la UI ve el texto cae
        // encima del texto. La UI convierte el rect leído al espacio de la
        // vista, y `get_annotations` ya devuelve ese espacio.
        let s = &crate::get_page_sizes(work.clone()).expect("tamaños")[0];
        assert_eq!(s.rotation, 270, "el test gira la página tres veces");
        let m = &search_pdf(work.clone(), "Mundo".into(), None, None, None).expect("buscar")[0].rects[0];
        let (px, py) = (m.x + m.w / 2.0, m.y + m.h / 2.0);
        // página propia -> vista con /Rotate 270, lo que hace `puntoAVista`
        let (vx, vy) = (py, s.height - px);
        crate::anotaciones2::add_stamp(
            work.clone(),
            0,
            "X".into(),
            [192, 57, 43, 255],
            px,
            py,
            10.0,
            None,
            None,
        )
        .expect("sello");
        let a = &crate::anotaciones::get_annotations(work.clone(), 0).expect("listar")[0];
        let (cx, cy) = (a.x + a.w / 2.0, a.y + a.h / 2.0);
        assert!(
            (cx - vx).abs() < 4.0 && (cy - vy).abs() < 4.0,
            "el sello puesto sobre la coincidencia se ve en ({cx:.1},{cy:.1}) \
             y la coincidencia en ({vx:.1},{vy:.1})"
        );

        std::fs::remove_file(&tmp).ok();
        std::fs::remove_file(&ruta_png).ok();
    }

    /// Las cajas que devuelven los cuatro comandos que leen contenido.
    fn lo_que_se_lee(work: &str) -> Vec<(f32, f32, f32, f32)> {
        let mut out = Vec::new();
        let t = get_page_text(work.to_string(), 0).expect("texto");
        out.extend(t.chars.iter().map(|c| (c.x, c.y, c.w, c.h)));
        for m in search_pdf(work.to_string(), "Mundo".into(), None, None, None).expect("buscar") {
            out.extend(m.rects.iter().map(|r| (r.x, r.y, r.w, r.h)));
        }
        for b in crate::texto::get_text_blocks(work.to_string(), 0).expect("bloques") {
            out.push((b.x, b.y, b.w, b.h));
        }
        for i in crate::imagenes::get_images(work.to_string(), 0).expect("imágenes") {
            out.push((i.x, i.y, i.w, i.h));
        }
        out
    }

}
