//! Extracción de texto con cajas de glifos y búsqueda en el documento.

use crate::{on_pdfium_thread, with_doc, Rect};
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
}

/// Extrae los caracteres de una página con sus cajas de glifos, en puntos PDF
/// y con origen arriba-izquierda (PDFium usa origen abajo-izquierda).
pub fn extract_chars(page: &PdfPage) -> Result<PageText, String> {
    let text = page.text().map_err(|e| e.to_string())?;
    let page_h = page.height().value;
    let mut chars = Vec::new();
    for c in text.chars().iter() {
        let ch = c.unicode_char().unwrap_or('\u{fffd}');
        let b = match c.loose_bounds() {
            Ok(b) => b,
            Err(_) => continue,
        };
        chars.push(CharBox {
            ch: ch.to_string(),
            x: b.left().value,
            y: page_h - b.top().value,
            w: b.right().value - b.left().value,
            h: b.top().value - b.bottom().value,
        });
    }
    Ok(PageText {
        width: page.width().value,
        height: page_h,
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
/// coincidencias dentro de una palabra.
#[tauri::command(async)]
pub fn search_pdf(
    path: String,
    query: String,
    match_case: bool,
    whole_word: bool,
) -> Result<Vec<SearchMatch>, String> {
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
                            results.push(SearchMatch {
                                page_index: page_index as u16,
                                rects,
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
            search_pdf(tmp.to_string_lossy().into_owned(), "mundo".into(), false, false).expect("buscar");
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
        let m = search_pdf(path.clone(), "ana".into(), false, false).expect("buscar ana");
        assert_eq!(m.len(), 2, "'ana' en 'banana banana'");

        // sin distinguir mayúsculas
        let m = search_pdf(path.clone(), "hola".into(), false, false).expect("buscar hola");
        assert_eq!(m.len(), 4, "'hola' aparece 4 veces");

        // rachas de espacios en el documento cuentan como un espacio
        let m = search_pdf(path.clone(), "hola mundo".into(), false, false).expect("buscar frase");
        assert_eq!(m.len(), 1, "'Hola  Mundo' con doble espacio");

        std::fs::remove_file(&tmp).ok();
    }

    #[test]
    fn coincidir_mayusculas_y_palabra_completa() {
        let tmp = std::env::temp_dir().join("editor_pdf_test_busqueda_opciones.pdf");
        crea_pdf(&["Casa casaca CASA — año año, añoso"], &tmp);
        let path = tmp.to_string_lossy().into_owned();
        let cuenta = |q: &str, mc: bool, ww: bool| {
            search_pdf(path.clone(), q.into(), mc, ww)
                .expect("buscar")
                .len()
        };

        // por defecto (las dos apagadas, como Acrobat)
        assert_eq!(cuenta("casa", false, false), 3, "Casa, casaca, CASA");
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
}
