use base64::Engine;
use pdfium_render::prelude::*;
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::io::Cursor;
use std::sync::{mpsc, OnceLock};

type Job = Box<dyn FnOnce() + Send>;

static PDFIUM_TX: OnceLock<mpsc::Sender<Job>> = OnceLock::new();

/// Directorio `lib/` dentro de los resources del bundle (solo en producción;
/// lo fija el setup de Tauri antes de usar PDFium).
static RESOURCE_LIB_DIR: OnceLock<std::path::PathBuf> = OnceLock::new();

/// Ejecuta `f` en el hilo dedicado de PDFium y devuelve su resultado.
///
/// PDFium se cuelga (deadlock, sin error) si se inicializa una segunda
/// instancia mientras otra sigue viva en el proceso, y sus tipos no son
/// `Send`. Un único hilo propietario garantiza por construcción una sola
/// instancia y serializa todo el acceso, venga de donde venga la llamada
/// (comandos de Tauri o tests).
pub(crate) fn on_pdfium_thread<R: Send + 'static>(f: impl FnOnce() -> R + Send + 'static) -> R {
    let tx = PDFIUM_TX.get_or_init(|| {
        let (tx, rx) = mpsc::channel::<Job>();
        std::thread::Builder::new()
            .name("pdfium".into())
            .spawn(move || {
                for job in rx {
                    job();
                }
            })
            .expect("no se pudo crear el hilo de PDFium");
        tx
    });
    let (rtx, rrx) = mpsc::channel();
    tx.send(Box::new(move || {
        let _ = rtx.send(f());
    }))
    .expect("el hilo de PDFium ha muerto");
    rrx.recv().expect("el hilo de PDFium ha muerto")
}

thread_local! {
    static PDFIUM: RefCell<Option<&'static Pdfium>> = const { RefCell::new(None) };
    static DOC_CACHE: RefCell<Option<(String, PdfDocument<'static>)>> = const { RefCell::new(None) };
}

/// Instancia única de PDFium, creada una sola vez y viva todo el proceso.
/// Solo debe llamarse desde el hilo de PDFium (dentro de `on_pdfium_thread`).
/// Orden de búsqueda: resources del bundle (producción) → src-tauri/lib/
/// (dev y tests, donde el cwd es src-tauri) → librería del sistema.
pub(crate) fn pdfium() -> Result<&'static Pdfium, String> {
    PDFIUM.with(|cell| {
        let mut slot = cell.borrow_mut();
        if let Some(p) = *slot {
            return Ok(p);
        }
        let from_resources = RESOURCE_LIB_DIR.get().and_then(|dir| {
            Pdfium::bind_to_library(Pdfium::pdfium_platform_library_name_at_path(&format!(
                "{}/",
                dir.display()
            )))
            .ok()
        });
        let bindings = match from_resources {
            Some(b) => b,
            None => Pdfium::bind_to_library(Pdfium::pdfium_platform_library_name_at_path("./lib/"))
                .or_else(|_| Pdfium::bind_to_system_library())
                .map_err(|e| format!("No se pudo cargar libpdfium: {e}"))?,
        };
        let leaked: &'static Pdfium = Box::leak(Box::new(Pdfium::new(bindings)));
        *slot = Some(leaked);
        Ok(leaked)
    })
}

/// Ejecuta `f` con el documento cacheado para `path`, recargándolo del disco
/// solo si el caché apunta a otro fichero. Solo debe llamarse desde el hilo
/// de PDFium.
pub(crate) fn with_doc<R>(
    path: &str,
    f: impl FnOnce(&PdfDocument<'static>) -> Result<R, String>,
) -> Result<R, String> {
    DOC_CACHE.with(|cell| {
        let mut cache = cell.borrow_mut();
        let stale = !matches!(cache.as_ref(), Some((p, _)) if p == path);
        if stale {
            let doc = pdfium()?
                .load_pdf_from_file(path, None)
                .map_err(|e| e.to_string())?;
            *cache = Some((path.to_string(), doc));
        }
        f(&cache.as_ref().unwrap().1)
    })
}

/// Descarta el documento cacheado. Llamar tras cualquier mutación en disco.
pub(crate) fn invalidate_doc_cache() {
    DOC_CACHE.with(|cell| *cell.borrow_mut() = None);
}

#[derive(Serialize)]
struct DocumentInfo {
    page_count: u16,
    work_path: String,
}

/// Ruta única en temp para la copia de trabajo del documento.
fn work_copy_path(original: &str) -> std::path::PathBuf {
    let name = std::path::Path::new(original)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("documento");
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!("editor-pdf-{name}-{nanos}.pdf"))
}

/// Guarda el documento sobre `path` y lo cierra. PDFium lee el fichero de
/// forma perezosa mientras el documento está abierto, y en Windows no se
/// puede renombrar encima de un fichero abierto, así que el orden importa:
/// escribir a un temporal, cerrar el documento (y el caché, que puede tener
/// otro handle del mismo fichero) y solo entonces renombrar.
pub(crate) fn save_and_close(doc: PdfDocument<'static>, path: &str) -> Result<(), String> {
    let tmp = format!("{path}.tmp");
    doc.save_to_file(&tmp).map_err(|e| e.to_string())?;
    drop(doc);
    invalidate_doc_cache();
    std::fs::rename(&tmp, path).map_err(|e| e.to_string())
}

/// Abre un PDF creando una copia de trabajo en temp. Todas las mutaciones
/// operan sobre la copia; el original solo se toca al guardar.
#[tauri::command]
fn open_pdf(path: String) -> Result<DocumentInfo, String> {
    let work = work_copy_path(&path);
    std::fs::copy(&path, &work)
        .map_err(|e| format!("No se pudo crear la copia de trabajo: {e}"))?;
    let work_path = work.to_string_lossy().into_owned();
    on_pdfium_thread(move || {
        let page_count = with_doc(&work_path, |doc| Ok(doc.pages().len()))?;
        Ok(DocumentInfo {
            page_count,
            work_path,
        })
    })
}

/// Renderiza una página a PNG (base64) con el ancho pedido en píxeles.
#[tauri::command]
fn render_page(path: String, page_index: u16, width: i32) -> Result<String, String> {
    on_pdfium_thread(move || {
        with_doc(&path, |doc| {
            let page = doc.pages().get(page_index).map_err(|e| e.to_string())?;
            let bitmap = page
                .render_with_config(
                    &PdfRenderConfig::new()
                        .set_target_width(width)
                        .render_form_data(true)
                        .render_annotations(true),
                )
                .map_err(|e| e.to_string())?;
            let mut png = Vec::new();
            bitmap
                .as_image()
                .write_to(&mut Cursor::new(&mut png), image::ImageFormat::Png)
                .map_err(|e| e.to_string())?;
            Ok(base64::engine::general_purpose::STANDARD.encode(png))
        })
    })
}

#[derive(Serialize, Clone)]
struct CharBox {
    ch: String,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
}

#[derive(Serialize)]
struct PageText {
    width: f32,
    height: f32,
    chars: Vec<CharBox>,
}

#[derive(Serialize, Deserialize, Clone)]
pub(crate) struct Rect {
    pub(crate) x: f32,
    pub(crate) y: f32,
    pub(crate) w: f32,
    pub(crate) h: f32,
}

#[derive(Serialize)]
struct SearchMatch {
    page_index: u16,
    rects: Vec<Rect>,
}

/// Extrae los caracteres de una página con sus cajas de glifos, en puntos PDF
/// y con origen arriba-izquierda (PDFium usa origen abajo-izquierda).
fn extract_chars(page: &PdfPage) -> Result<PageText, String> {
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

#[tauri::command]
fn get_page_text(path: String, page_index: u16) -> Result<PageText, String> {
    on_pdfium_thread(move || {
        with_doc(&path, |doc| {
            let page = doc.pages().get(page_index).map_err(|e| e.to_string())?;
            extract_chars(&page)
        })
    })
}

/// Normaliza para búsqueda sin distinción de mayúsculas; todo espacio en
/// blanco (incl. \r\n que PDFium intercala) se trata como espacio simple.
fn normalize(c: char) -> char {
    let c = c.to_lowercase().next().unwrap_or(c);
    if c.is_whitespace() {
        ' '
    } else {
        c
    }
}

/// Une cajas de caracteres consecutivos en rectángulos por línea.
fn merge_line_rects(boxes: &[CharBox]) -> Vec<Rect> {
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
fn normaliza_colapsando(chars: impl Iterator<Item = char>) -> Vec<(char, usize)> {
    let mut out: Vec<(char, usize)> = Vec::new();
    for (i, c) in chars.enumerate() {
        let n = normalize(c);
        if n == ' ' && matches!(out.last(), Some((' ', _))) {
            continue;
        }
        out.push((n, i));
    }
    out
}

/// Busca `query` en todas las páginas (sin distinguir mayúsculas, sin
/// solapamientos, y tratando cualquier racha de espacios/saltos de línea como
/// un espacio) y devuelve los rectángulos de cada coincidencia.
#[tauri::command]
fn search_pdf(path: String, query: String) -> Result<Vec<SearchMatch>, String> {
    let needle: Vec<char> = normaliza_colapsando(query.trim().chars())
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
                );
                if hay.len() < needle.len() {
                    continue;
                }
                let mut start = 0;
                while start + needle.len() <= hay.len() {
                    if hay[start..start + needle.len()]
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

/// Borra una página y devuelve el nuevo número de páginas.
#[tauri::command]
fn delete_page(work_path: String, page_index: u16) -> Result<u16, String> {
    on_pdfium_thread(move || {
        let pdfium = pdfium()?;
        let doc = pdfium
            .load_pdf_from_file(&work_path, None)
            .map_err(|e| e.to_string())?;
        let page = doc.pages().get(page_index).map_err(|e| e.to_string())?;
        page.delete().map_err(|e| e.to_string())?;
        let count = doc.pages().len();
        save_and_close(doc, &work_path)?;
        Ok(count)
    })
}

/// Rota una página 90° en sentido horario (acumulativo).
#[tauri::command]
fn rotate_page(work_path: String, page_index: u16) -> Result<(), String> {
    on_pdfium_thread(move || {
        let pdfium = pdfium()?;
        let doc = pdfium
            .load_pdf_from_file(&work_path, None)
            .map_err(|e| e.to_string())?;
        let mut page = doc.pages().get(page_index).map_err(|e| e.to_string())?;
        let next = match page.rotation().unwrap_or(PdfPageRenderRotation::None) {
            PdfPageRenderRotation::None => PdfPageRenderRotation::Degrees90,
            PdfPageRenderRotation::Degrees90 => PdfPageRenderRotation::Degrees180,
            PdfPageRenderRotation::Degrees180 => PdfPageRenderRotation::Degrees270,
            PdfPageRenderRotation::Degrees270 => PdfPageRenderRotation::None,
        };
        page.set_rotation(next);
        drop(page);
        save_and_close(doc, &work_path)?;
        Ok(())
    })
}

/// Mueve una página a otra posición reconstruyendo el documento en el nuevo
/// orden (FPDF_ImportPages respeta el orden del rango dado).
#[tauri::command]
fn move_page(work_path: String, from_index: u16, to_index: u16) -> Result<(), String> {
    if from_index == to_index {
        return Ok(());
    }
    on_pdfium_thread(move || {
        let pdfium = pdfium()?;
        let doc = pdfium
            .load_pdf_from_file(&work_path, None)
            .map_err(|e| e.to_string())?;
        let count = doc.pages().len();
        if from_index >= count || to_index >= count {
            return Err("Índice de página fuera de rango".into());
        }
        let mut order: Vec<u16> = (0..count).collect();
        let moved = order.remove(from_index as usize);
        order.insert(to_index as usize, moved);
        let range = order
            .iter()
            .map(|i| (i + 1).to_string())
            .collect::<Vec<_>>()
            .join(",");
        let mut new_doc = pdfium.create_new_pdf().map_err(|e| e.to_string())?;
        new_doc
            .pages_mut()
            .copy_pages_from_document(&doc, &range, 0)
            .map_err(|e| e.to_string())?;
        drop(doc);
        save_and_close(new_doc, &work_path)?;
        Ok(())
    })
}

/// Añade todas las páginas de otro PDF al final y devuelve el nuevo total.
#[tauri::command]
fn merge_pdf(work_path: String, other_path: String) -> Result<u16, String> {
    on_pdfium_thread(move || {
        let pdfium = pdfium()?;
        let mut doc = pdfium
            .load_pdf_from_file(&work_path, None)
            .map_err(|e| e.to_string())?;
        let other = pdfium
            .load_pdf_from_file(&other_path, None)
            .map_err(|e| e.to_string())?;
        doc.pages_mut().append(&other).map_err(|e| e.to_string())?;
        let count = doc.pages().len();
        drop(other);
        save_and_close(doc, &work_path)?;
        Ok(count)
    })
}

/// Extrae las páginas indicadas (índices base 0) a un PDF nuevo.
#[tauri::command]
fn extract_pages(
    work_path: String,
    page_indices: Vec<u16>,
    dest_path: String,
) -> Result<(), String> {
    if page_indices.is_empty() {
        return Err("No hay páginas que extraer".into());
    }
    let range = page_indices
        .iter()
        .map(|i| (i + 1).to_string())
        .collect::<Vec<_>>()
        .join(",");
    on_pdfium_thread(move || {
        with_doc(&work_path, |doc| {
            let mut new_doc = pdfium()?.create_new_pdf().map_err(|e| e.to_string())?;
            new_doc
                .pages_mut()
                .copy_pages_from_document(doc, &range, 0)
                .map_err(|e| e.to_string())?;
            new_doc.save_to_file(&dest_path).map_err(|e| e.to_string())
        })
    })
}

/// Convierte un rect en coords de UI (origen arriba-izquierda) a PdfRect
/// (origen abajo-izquierda).
pub(crate) fn ui_rect_to_pdf(r: &Rect, page_h: f32) -> PdfRect {
    PdfRect::new(
        PdfPoints::new(page_h - r.y - r.h),
        PdfPoints::new(r.x),
        PdfPoints::new(page_h - r.y),
        PdfPoints::new(r.x + r.w),
    )
}

/// Crea una anotación de resaltado amarillo sobre los rects dados
/// (coords de UI en puntos PDF).
#[tauri::command]
fn add_highlight(work_path: String, page_index: u16, rects: Vec<Rect>) -> Result<(), String> {
    if rects.is_empty() {
        return Err("No hay nada que resaltar".into());
    }
    on_pdfium_thread(move || {
        let pdfium = pdfium()?;
        let doc = pdfium
            .load_pdf_from_file(&work_path, None)
            .map_err(|e| e.to_string())?;
        let mut page = doc.pages().get(page_index).map_err(|e| e.to_string())?;
        let page_h = page.height().value;
        let mut annot = page
            .annotations_mut()
            .create_highlight_annotation()
            .map_err(|e| e.to_string())?;
        annot
            .set_stroke_color(PdfColor::new(255, 220, 0, 140))
            .map_err(|e| e.to_string())?;
        let left = rects.iter().map(|r| r.x).fold(f32::MAX, f32::min);
        let top = rects.iter().map(|r| r.y).fold(f32::MAX, f32::min);
        let right = rects.iter().map(|r| r.x + r.w).fold(f32::MIN, f32::max);
        let bottom = rects.iter().map(|r| r.y + r.h).fold(f32::MIN, f32::max);
        let envelope = Rect {
            x: left,
            y: top,
            w: right - left,
            h: bottom - top,
        };
        annot
            .set_bounds(ui_rect_to_pdf(&envelope, page_h))
            .map_err(|e| e.to_string())?;
        for r in &rects {
            let pr = ui_rect_to_pdf(r, page_h);
            // Orden del spec (UL, UR, LL, LR): otros visores generan la
            // apariencia a partir de los quads y el orden importa.
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
            annot
                .attachment_points_mut()
                .create_attachment_point_at_end(quad)
                .map_err(|e| e.to_string())?;
        }
        drop(annot);
        drop(page);
        save_and_close(doc, &work_path)?;
        Ok(())
    })
}

/// Añade un trazo a mano alzada como anotación Ink con su apariencia
/// (un path dentro de la anotación), de modo que se puede borrar
/// individualmente. Los puntos vienen en coords de UI (puntos PDF,
/// origen arriba-izquierda).
#[tauri::command]
fn add_stroke(work_path: String, page_index: u16, points: Vec<[f32; 2]>) -> Result<(), String> {
    if points.len() < 2 {
        return Err("Trazo demasiado corto".into());
    }
    on_pdfium_thread(move || {
        let pdfium = pdfium()?;
        let doc = pdfium
            .load_pdf_from_file(&work_path, None)
            .map_err(|e| e.to_string())?;
        let mut page = doc.pages().get(page_index).map_err(|e| e.to_string())?;
        let page_h = page.height().value;
        let mut annot = page
            .annotations_mut()
            .create_ink_annotation()
            .map_err(|e| e.to_string())?;
        const MARGIN: f32 = 3.0;
        let min_x = points.iter().map(|p| p[0]).fold(f32::MAX, f32::min) - MARGIN;
        let max_x = points.iter().map(|p| p[0]).fold(f32::MIN, f32::max) + MARGIN;
        let min_y = points.iter().map(|p| p[1]).fold(f32::MAX, f32::min) - MARGIN;
        let max_y = points.iter().map(|p| p[1]).fold(f32::MIN, f32::max) + MARGIN;
        annot
            .set_bounds(PdfRect::new(
                PdfPoints::new(page_h - max_y),
                PdfPoints::new(min_x),
                PdfPoints::new(page_h - min_y),
                PdfPoints::new(max_x),
            ))
            .map_err(|e| e.to_string())?;
        let mut path = PdfPagePathObject::new(
            &doc,
            PdfPoints::new(points[0][0]),
            PdfPoints::new(page_h - points[0][1]),
            Some(PdfColor::new(226, 61, 61, 255)),
            Some(PdfPoints::new(2.0)),
            None,
        )
        .map_err(|e| e.to_string())?;
        for p in &points[1..] {
            path.line_to(PdfPoints::new(p[0]), PdfPoints::new(page_h - p[1]))
                .map_err(|e| e.to_string())?;
        }
        annot
            .objects_mut()
            .add_path_object(path)
            .map_err(|e| e.to_string())?;
        drop(annot);
        drop(page);
        save_and_close(doc, &work_path)?;
        Ok(())
    })
}

/// Crea una nota (anotación de texto) en el punto dado (coords de UI).
#[tauri::command]
fn add_note(
    work_path: String,
    page_index: u16,
    x: f32,
    y: f32,
    text: String,
) -> Result<(), String> {
    if text.trim().is_empty() {
        return Err("La nota está vacía".into());
    }
    on_pdfium_thread(move || {
        let pdfium = pdfium()?;
        let doc = pdfium
            .load_pdf_from_file(&work_path, None)
            .map_err(|e| e.to_string())?;
        let mut page = doc.pages().get(page_index).map_err(|e| e.to_string())?;
        let page_h = page.height().value;
        let mut annot = page
            .annotations_mut()
            .create_text_annotation(&text)
            .map_err(|e| e.to_string())?;
        const ICON: f32 = 22.0;
        annot
            .set_bounds(ui_rect_to_pdf(
                &Rect {
                    x,
                    y,
                    w: ICON,
                    h: ICON,
                },
                page_h,
            ))
            .map_err(|e| e.to_string())?;
        drop(annot);
        drop(page);
        save_and_close(doc, &work_path)?;
        Ok(())
    })
}

#[derive(Serialize)]
struct AnnotationInfo {
    index: u16,
    kind: String,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    contents: String,
    /// Para resaltados: un rect por línea (los quads), en coords de UI.
    rects: Vec<Rect>,
}

/// Lista las anotaciones de una página (bounds en coords de UI). La UI las
/// usa para pintar los iconos de nota, los rects de los resaltados (PDFium no
/// genera apariencia automática para Text ni Highlight) y para borrar con clic.
#[tauri::command]
fn get_annotations(path: String, page_index: u16) -> Result<Vec<AnnotationInfo>, String> {
    on_pdfium_thread(move || {
        with_doc(&path, |doc| {
            let page = doc.pages().get(page_index).map_err(|e| e.to_string())?;
            let page_h = page.height().value;
            let annotations = page.annotations();
            let mut out = Vec::new();
            for i in 0..annotations.len() {
                let Ok(mut a) = annotations.get(i) else {
                    continue;
                };
                let Ok(b) = a.bounds() else { continue };
                let mut rects = Vec::new();
                {
                    // resaltado, subrayado y tachado guardan sus líneas como
                    // quadpoints; tipos distintos sin trait común
                    macro_rules! lee_quads {
                        ($m:expr) => {
                            if let Some(m) = $m {
                                let points = m.attachment_points_mut();
                                for j in 0..points.len() {
                                    if let Ok(q) = points.get(j) {
                                        rects.push(Rect {
                                            x: q.left().value,
                                            y: page_h - q.top().value,
                                            w: q.right().value - q.left().value,
                                            h: q.top().value - q.bottom().value,
                                        });
                                    }
                                }
                            }
                        };
                    }
                    lee_quads!(a.as_highlight_annotation_mut());
                    lee_quads!(a.as_underline_annotation_mut());
                    lee_quads!(a.as_strikeout_annotation_mut());
                }
                out.push(AnnotationInfo {
                    index: i as u16,
                    kind: format!("{:?}", a.annotation_type()),
                    x: b.left().value,
                    y: page_h - b.top().value,
                    w: b.right().value - b.left().value,
                    h: b.top().value - b.bottom().value,
                    contents: a.contents().unwrap_or_default(),
                    rects,
                });
            }
            Ok(out)
        })
    })
}

/// Elimina la anotación con el índice dado.
#[tauri::command]
fn remove_annotation(work_path: String, page_index: u16, annot_index: u16) -> Result<(), String> {
    on_pdfium_thread(move || {
        let pdfium = pdfium()?;
        let doc = pdfium
            .load_pdf_from_file(&work_path, None)
            .map_err(|e| e.to_string())?;
        let mut page = doc.pages().get(page_index).map_err(|e| e.to_string())?;
        let annotations = page.annotations_mut();
        let annot = annotations
            .get(annot_index as usize)
            .map_err(|e| e.to_string())?;
        annotations
            .delete_annotation(annot)
            .map_err(|e| e.to_string())?;
        drop(page);
        save_and_close(doc, &work_path)?;
        Ok(())
    })
}

#[derive(Serialize)]
struct FormFieldInfo {
    annot_index: u16,
    name: String,
    kind: String,
    value: String,
    checked: bool,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
}

/// Lista los campos de formulario (widgets) de una página, con bounds en
/// coords de UI.
#[tauri::command]
fn get_form_fields(path: String, page_index: u16) -> Result<Vec<FormFieldInfo>, String> {
    on_pdfium_thread(move || {
        with_doc(&path, |doc| {
            let page = doc.pages().get(page_index).map_err(|e| e.to_string())?;
            let page_h = page.height().value;
            let annotations = page.annotations();
            let mut out = Vec::new();
            for i in 0..annotations.len() {
                let Ok(a) = annotations.get(i) else { continue };
                let Some(widget) = a.as_widget_annotation() else {
                    continue;
                };
                let Some(field) = widget.form_field() else {
                    continue;
                };
                let Ok(b) = a.bounds() else { continue };
                let kind = format!("{:?}", field.field_type());
                let (value, checked) = match field.field_type() {
                    PdfFormFieldType::Checkbox => (
                        String::new(),
                        field
                            .as_checkbox_field()
                            .and_then(|c| c.is_checked().ok())
                            .unwrap_or(false),
                    ),
                    PdfFormFieldType::RadioButton => (
                        String::new(),
                        field
                            .as_radio_button_field()
                            .and_then(|r| r.is_checked().ok())
                            .unwrap_or(false),
                    ),
                    PdfFormFieldType::Text => (
                        field
                            .as_text_field()
                            .and_then(|t| t.value())
                            .unwrap_or_default(),
                        false,
                    ),
                    _ => (String::new(), false),
                };
                out.push(FormFieldInfo {
                    annot_index: i as u16,
                    name: field.name().unwrap_or_default(),
                    kind,
                    value,
                    checked,
                    x: b.left().value,
                    y: page_h - b.top().value,
                    w: b.right().value - b.left().value,
                    h: b.top().value - b.bottom().value,
                });
            }
            Ok(out)
        })
    })
}

/// Escribe el valor de un campo de texto de formulario.
#[tauri::command]
fn set_form_text(
    work_path: String,
    page_index: u16,
    annot_index: u16,
    value: String,
) -> Result<(), String> {
    on_pdfium_thread(move || {
        let pdfium = pdfium()?;
        let doc = pdfium
            .load_pdf_from_file(&work_path, None)
            .map_err(|e| e.to_string())?;
        let page = doc.pages().get(page_index).map_err(|e| e.to_string())?;
        let mut annot = page
            .annotations()
            .get(annot_index as usize)
            .map_err(|e| e.to_string())?;
        annot
            .as_widget_annotation_mut()
            .and_then(|w| w.form_field_mut())
            .and_then(|f| f.as_text_field_mut())
            .ok_or("No es un campo de texto")?
            .set_value(&value)
            .map_err(|e| e.to_string())?;
        drop(annot);
        drop(page);
        save_and_close(doc, &work_path)?;
        Ok(())
    })
}

/// Marca o desmarca una casilla (o selecciona un radio button).
#[tauri::command]
fn set_form_checked(
    work_path: String,
    page_index: u16,
    annot_index: u16,
    checked: bool,
) -> Result<(), String> {
    on_pdfium_thread(move || {
        let pdfium = pdfium()?;
        let doc = pdfium
            .load_pdf_from_file(&work_path, None)
            .map_err(|e| e.to_string())?;
        let page = doc.pages().get(page_index).map_err(|e| e.to_string())?;
        let mut annot = page
            .annotations()
            .get(annot_index as usize)
            .map_err(|e| e.to_string())?;
        let field = annot
            .as_widget_annotation_mut()
            .and_then(|w| w.form_field_mut())
            .ok_or("No es un campo de formulario")?;
        if let Some(cb) = field.as_checkbox_field_mut() {
            cb.set_checked(checked).map_err(|e| e.to_string())?;
        } else if let Some(rb) = field.as_radio_button_field_mut() {
            if checked {
                rb.set_checked().map_err(|e| e.to_string())?;
            }
        } else {
            return Err("No es una casilla".into());
        }
        drop(annot);
        drop(page);
        save_and_close(doc, &work_path)?;
        Ok(())
    })
}

#[derive(Serialize)]
struct TextBlock {
    object_index: u32,
    text: String,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    font_size: f32,
    font_family: String,
}

/// Directorios de fuentes TTF del sistema, por plataforma.
fn directorios_de_fuentes() -> Vec<std::path::PathBuf> {
    let mut dirs = Vec::new();
    #[cfg(target_os = "macos")]
    {
        dirs.push("/System/Library/Fonts/Supplemental".into());
        dirs.push("/Library/Fonts".into());
    }
    #[cfg(target_os = "windows")]
    {
        let windir = std::env::var("WINDIR").unwrap_or_else(|_| "C:\\Windows".into());
        dirs.push(std::path::Path::new(&windir).join("Fonts"));
    }
    #[cfg(target_os = "linux")]
    {
        dirs.push("/usr/share/fonts/truetype".into());
    }
    dirs
}

/// Resuelve un nombre de familia a un token de fuente utilizable: estándar
/// aproximada por subcadenas, TTF real del sistema si existe (directorios
/// por plataforma), o Helvetica como último recurso.
/// (No se puede reutilizar una fuente embebida del PDF para objetos nuevos:
/// el handle de FPDFTextObj_GetFont queda ligado a su página.)
pub(crate) fn fuente_por_nombre(doc: &mut PdfDocument<'static>, nombre: &str) -> PdfFontToken {
    let n = nombre.to_lowercase();
    let bold = n.contains("bold") || n.contains("negrita");
    let italic = n.contains("italic") || n.contains("oblique") || n.contains("cursiva");
    if n.contains("times") {
        let fonts = doc.fonts_mut();
        return match (bold, italic) {
            (true, true) => fonts.times_bold_italic(),
            (true, false) => fonts.times_bold(),
            (false, true) => fonts.times_italic(),
            (false, false) => fonts.times_roman(),
        };
    }
    if n.contains("courier") || n.contains("mono") {
        let fonts = doc.fonts_mut();
        return match (bold, italic) {
            (true, true) => fonts.courier_bold_oblique(),
            (true, false) => fonts.courier_bold(),
            (false, true) => fonts.courier_oblique(),
            (false, false) => fonts.courier(),
        };
    }
    // Arial es métricamente equivalente a Helvetica (y la Helvetica builtin
    // de PDFium se identifica como "Arial"): usar la estándar, que además
    // extrae bien los acentos (los TTF cargados con FPDFText_LoadFont no
    // llevan ToUnicode y la extracción pierde los no-ASCII).
    if !n.contains("helvetica") && !n.contains("arial") && !n.is_empty() {
        // best effort: TTF del sistema con ese nombre (Georgia, Verdana…)
        let base = nombre
            .split(['-', ','])
            .next()
            .unwrap_or(nombre)
            .trim()
            .to_string();
        for nombre_fichero in [
            format!("{base}.ttf"),
            format!("{}.ttf", base.replace(' ', "")),
        ] {
            for dir in directorios_de_fuentes() {
                let path = dir.join(&nombre_fichero);
                if path.exists() {
                    if let Ok(token) = doc.fonts_mut().load_true_type_from_file(&path, false) {
                        return token;
                    }
                }
            }
        }
    }
    let fonts = doc.fonts_mut();
    match (bold, italic) {
        (true, true) => fonts.helvetica_bold_oblique(),
        (true, false) => fonts.helvetica_bold(),
        (false, true) => fonts.helvetica_oblique(),
        (false, false) => fonts.helvetica(),
    }
}

/// Familia de fuente más usada por los objetos de texto de una página.
pub(crate) fn familia_dominante(doc: &PdfDocument<'static>, page_index: u16) -> Option<String> {
    let page = doc.pages().get(page_index).ok()?;
    let objects = page.objects();
    let mut cuentas: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for i in 0..objects.len() {
        if let Ok(obj) = objects.get(i) {
            if let Some(t) = obj.as_text_object() {
                let familia = t.font().family();
                if !familia.is_empty() {
                    *cuentas.entry(familia).or_insert(0) += 1;
                }
            }
        }
    }
    cuentas
        .into_iter()
        .max_by_key(|(_, c)| *c)
        .map(|(familia, _)| familia)
}

/// Lista los objetos de texto de una página (bloques editables), con bounds
/// en coords de UI.
#[tauri::command]
fn get_text_blocks(path: String, page_index: u16) -> Result<Vec<TextBlock>, String> {
    on_pdfium_thread(move || {
        with_doc(&path, |doc| {
            let page = doc.pages().get(page_index).map_err(|e| e.to_string())?;
            let page_h = page.height().value;
            let objects = page.objects();
            let mut out = Vec::new();
            for i in 0..objects.len() {
                let Ok(obj) = objects.get(i) else { continue };
                let Some(t) = obj.as_text_object() else {
                    continue;
                };
                let text = t.text();
                if text.trim().is_empty() {
                    continue;
                }
                let Ok(b) = obj.bounds() else { continue };
                out.push(TextBlock {
                    object_index: i as u32,
                    text,
                    x: b.left().value,
                    y: page_h - b.top().value,
                    w: b.right().value - b.left().value,
                    h: b.top().value - b.bottom().value,
                    font_size: t.unscaled_font_size().value,
                    font_family: t.font().family(),
                });
            }
            Ok(out)
        })
    })
}

/// Edición real de texto: reescribe el objeto de texto del content stream.
/// Mantiene la fuente del objeto (si la fuente embebida no tiene los glifos
/// del texto nuevo, esos caracteres no se verán). Si el texto nuevo tiene
/// varias líneas, la primera reemplaza al objeto original y las demás se
/// insertan como objetos nuevos con la misma fuente, colocados debajo.
#[tauri::command]
fn edit_text_block(
    work_path: String,
    page_index: u16,
    object_index: u32,
    new_text: String,
) -> Result<(), String> {
    on_pdfium_thread(move || {
        let pdfium = pdfium()?;
        let mut doc = pdfium
            .load_pdf_from_file(&work_path, None)
            .map_err(|e| e.to_string())?;
        let mut lineas = new_text.lines();
        let primera = lineas.next().unwrap_or("").to_string();
        let resto: Vec<String> = lineas.map(|l| l.to_string()).collect();

        // 1) reescribir la primera línea y leer familia/tamaño/posición
        let (familia, font_size, base_x, base_y) = {
            let mut page = doc.pages().get(page_index).map_err(|e| e.to_string())?;
            let mut obj = page
                .objects_mut()
                .get(object_index as usize)
                .map_err(|e| e.to_string())?;
            let bounds = obj.bounds().map_err(|e| e.to_string())?;
            let t = obj.as_text_object_mut().ok_or("No es un bloque de texto")?;
            let info = (
                t.font().family().to_lowercase(),
                t.unscaled_font_size(),
                bounds.left(),
                bounds.bottom(),
            );
            t.set_text(&primera).map_err(|e| e.to_string())?;
            drop(obj);
            page.regenerate_content().map_err(|e| e.to_string())?;
            info
        };

        // Fuente para las líneas nuevas: se aproxima la del bloque original.
        // Reutilizar el handle de FPDFTextObj_GetFont sería más fiel, pero
        // queda ligado a la página ya cerrada y PDFium no perdona los handles
        // colgantes (SIGSEGV).
        let font_token = if !resto.is_empty() {
            Some(fuente_por_nombre(&mut doc, &familia))
        } else {
            None
        };

        // líneas adicionales: objetos nuevos, colocados debajo
        if let Some(token) = font_token {
            let mut page = doc.pages().get(page_index).map_err(|e| e.to_string())?;
            let line_h = font_size.value * 1.2;
            for (i, linea) in resto.iter().enumerate() {
                if linea.trim().is_empty() {
                    continue;
                }
                let mut nuevo = PdfPageTextObject::new(&doc, linea, token, font_size)
                    .map_err(|e| e.to_string())?;
                nuevo
                    .translate(
                        base_x,
                        PdfPoints::new(base_y.value - line_h * (i as f32 + 1.0)),
                    )
                    .map_err(|e| e.to_string())?;
                page.objects_mut()
                    .add_text_object(nuevo)
                    .map_err(|e| e.to_string())?;
            }
            page.regenerate_content().map_err(|e| e.to_string())?;
            drop(page);
        }
        save_and_close(doc, &work_path)?;
        Ok(())
    })
}

/// Añade un bloque de texto nuevo en el punto dado (coords de UI, el punto
/// es la esquina superior izquierda de la primera línea). Cada línea del
/// texto se inserta como un objeto propio. La fuente puede elegirse por
/// nombre; sin nombre (o "auto") se detecta la familia dominante de la
/// página y se aproxima.
#[tauri::command]
fn add_text_block(
    work_path: String,
    page_index: u16,
    x: f32,
    y: f32,
    text: String,
    font_size: f32,
    font: Option<String>,
) -> Result<(), String> {
    if text.trim().is_empty() {
        return Err("El texto está vacío".into());
    }
    let font_size = font_size.clamp(6.0, 96.0);
    on_pdfium_thread(move || {
        let pdfium = pdfium()?;
        let mut doc = pdfium
            .load_pdf_from_file(&work_path, None)
            .map_err(|e| e.to_string())?;
        let familia = match font.as_deref() {
            Some(nombre) if !nombre.is_empty() && nombre != "auto" => nombre.to_string(),
            _ => familia_dominante(&doc, page_index).unwrap_or_else(|| "helvetica".into()),
        };
        let font = fuente_por_nombre(&mut doc, &familia);
        let mut page = doc.pages().get(page_index).map_err(|e| e.to_string())?;
        let page_h = page.height().value;
        let line_h = font_size * 1.2;
        for (i, linea) in text.lines().enumerate() {
            if linea.trim().is_empty() {
                continue;
            }
            let mut obj = PdfPageTextObject::new(&doc, linea, font, PdfPoints::new(font_size))
                .map_err(|e| e.to_string())?;
            // el clic marca la parte superior de la primera línea; el objeto
            // se coloca por su baseline aproximada
            let baseline = page_h - y - font_size - line_h * i as f32;
            obj.translate(PdfPoints::new(x), PdfPoints::new(baseline))
                .map_err(|e| e.to_string())?;
            page.objects_mut()
                .add_text_object(obj)
                .map_err(|e| e.to_string())?;
        }
        page.regenerate_content().map_err(|e| e.to_string())?;
        drop(page);
        save_and_close(doc, &work_path)?;
        Ok(())
    })
}

#[derive(Serialize)]
struct ImageInfo {
    object_index: u32,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
}

/// Lista los objetos de imagen de una página (bounds en coords de UI).
#[tauri::command]
fn get_images(path: String, page_index: u16) -> Result<Vec<ImageInfo>, String> {
    on_pdfium_thread(move || {
        with_doc(&path, |doc| {
            let page = doc.pages().get(page_index).map_err(|e| e.to_string())?;
            let page_h = page.height().value;
            let objects = page.objects();
            let mut out = Vec::new();
            for i in 0..objects.len() {
                let Ok(obj) = objects.get(i) else { continue };
                if obj.as_image_object().is_none() {
                    continue;
                }
                let Ok(b) = obj.bounds() else { continue };
                out.push(ImageInfo {
                    object_index: i as u32,
                    x: b.left().value,
                    y: page_h - b.top().value,
                    w: b.right().value - b.left().value,
                    h: b.top().value - b.bottom().value,
                });
            }
            Ok(out)
        })
    })
}

/// Inserta una imagen (png/jpg/webp…) con su tamaño natural a 72 dpi,
/// limitado a caber en la página. El punto dado (coords de UI) es la esquina
/// superior izquierda.
#[tauri::command]
fn add_image(
    work_path: String,
    page_index: u16,
    image_path: String,
    x: f32,
    y: f32,
) -> Result<(), String> {
    on_pdfium_thread(move || {
        let img =
            image::open(&image_path).map_err(|e| format!("No se pudo leer la imagen: {e}"))?;
        let pdfium = pdfium()?;
        let doc = pdfium
            .load_pdf_from_file(&work_path, None)
            .map_err(|e| e.to_string())?;
        let mut page = doc.pages().get(page_index).map_err(|e| e.to_string())?;
        let page_w = page.width().value;
        let page_h = page.height().value;
        let mut w = img.width() as f32;
        let mut h = img.height() as f32;
        let max_w = page_w * 0.6;
        if w > max_w {
            let f = max_w / w;
            w *= f;
            h *= f;
        }
        if h > page_h * 0.8 {
            let f = page_h * 0.8 / h;
            w *= f;
            h *= f;
        }
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

/// Mueve y/o redimensiona una imagen a los bounds dados (coords de UI).
/// Válido para imágenes sin rotación.
#[tauri::command]
fn transform_image(
    work_path: String,
    page_index: u16,
    object_index: u32,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
) -> Result<(), String> {
    if w <= 1.0 || h <= 1.0 {
        return Err("Tamaño de imagen inválido".into());
    }
    on_pdfium_thread(move || {
        let pdfium = pdfium()?;
        let doc = pdfium
            .load_pdf_from_file(&work_path, None)
            .map_err(|e| e.to_string())?;
        let mut page = doc.pages().get(page_index).map_err(|e| e.to_string())?;
        let page_h = page.height().value;
        let mut obj = page
            .objects_mut()
            .get(object_index as usize)
            .map_err(|e| e.to_string())?;
        if obj.as_image_object().is_none() {
            return Err("No es una imagen".into());
        }
        let b = obj.bounds().map_err(|e| e.to_string())?;
        let old_w = b.right().value - b.left().value;
        let old_h = b.top().value - b.bottom().value;
        if old_w > 0.0 && old_h > 0.0 {
            obj.scale(w / old_w, h / old_h).map_err(|e| e.to_string())?;
        }
        let b2 = obj.bounds().map_err(|e| e.to_string())?;
        let dx = x - b2.left().value;
        let dy = (page_h - y - h) - b2.bottom().value;
        obj.translate(PdfPoints::new(dx), PdfPoints::new(dy))
            .map_err(|e| e.to_string())?;
        drop(obj);
        page.regenerate_content().map_err(|e| e.to_string())?;
        drop(page);
        save_and_close(doc, &work_path)?;
        Ok(())
    })
}

/// Reemplaza el contenido de una imagen manteniendo posición y tamaño.
#[tauri::command]
fn replace_image(
    work_path: String,
    page_index: u16,
    object_index: u32,
    image_path: String,
) -> Result<(), String> {
    on_pdfium_thread(move || {
        let img =
            image::open(&image_path).map_err(|e| format!("No se pudo leer la imagen: {e}"))?;
        let pdfium = pdfium()?;
        let doc = pdfium
            .load_pdf_from_file(&work_path, None)
            .map_err(|e| e.to_string())?;
        let mut page = doc.pages().get(page_index).map_err(|e| e.to_string())?;
        let (left, bottom, w, h) = {
            let obj = page
                .objects()
                .get(object_index as usize)
                .map_err(|e| e.to_string())?;
            if obj.as_image_object().is_none() {
                return Err("No es una imagen".into());
            }
            let b = obj.bounds().map_err(|e| e.to_string())?;
            (
                b.left().value,
                b.bottom().value,
                b.right().value - b.left().value,
                b.top().value - b.bottom().value,
            )
        };
        let removed = page
            .objects_mut()
            .remove_object_at_index(object_index as usize)
            .map_err(|e| e.to_string())?;
        // ver nota en delete_text_block: soltar el objeto extraído casca
        std::mem::forget(removed);
        let mut obj =
            PdfPageImageObject::new_with_size(&doc, &img, PdfPoints::new(w), PdfPoints::new(h))
                .map_err(|e| e.to_string())?;
        obj.translate(PdfPoints::new(left), PdfPoints::new(bottom))
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

/// Elimina una imagen de la página.
#[tauri::command]
fn delete_image(work_path: String, page_index: u16, object_index: u32) -> Result<(), String> {
    on_pdfium_thread(move || {
        let pdfium = pdfium()?;
        let doc = pdfium
            .load_pdf_from_file(&work_path, None)
            .map_err(|e| e.to_string())?;
        let mut page = doc.pages().get(page_index).map_err(|e| e.to_string())?;
        {
            let obj = page
                .objects()
                .get(object_index as usize)
                .map_err(|e| e.to_string())?;
            if obj.as_image_object().is_none() {
                return Err("No es una imagen".into());
            }
        }
        let removed = page
            .objects_mut()
            .remove_object_at_index(object_index as usize)
            .map_err(|e| e.to_string())?;
        // ver nota en delete_text_block: soltar el objeto extraído casca
        std::mem::forget(removed);
        page.regenerate_content().map_err(|e| e.to_string())?;
        drop(page);
        save_and_close(doc, &work_path)?;
        Ok(())
    })
}

/// Borra un bloque de texto del content stream.
#[tauri::command]
fn delete_text_block(work_path: String, page_index: u16, object_index: u32) -> Result<(), String> {
    on_pdfium_thread(move || {
        let pdfium = pdfium()?;
        let doc = pdfium
            .load_pdf_from_file(&work_path, None)
            .map_err(|e| e.to_string())?;
        let mut page = doc.pages().get(page_index).map_err(|e| e.to_string())?;
        let removed = page
            .objects_mut()
            .remove_object_at_index(object_index as usize)
            .map_err(|e| e.to_string())?;
        // Su Drop llamaría a FPDFPageObj_Destroy y PDFium casca (SIGSEGV) con
        // objetos de documentos reabiertos; fuga puntual asumida.
        std::mem::forget(removed);
        page.regenerate_content().map_err(|e| e.to_string())?;
        drop(page);
        save_and_close(doc, &work_path)?;
        Ok(())
    })
}

mod anotaciones2;
mod documento;
mod firma;
mod firmas_visuales;
mod imagenes;
mod paginas2;

/// Firma digitalmente la copia de trabajo y escribe el PDF firmado en
/// `dest_path`. Certificado y clave privada en PEM (RSA sin cifrar).
#[tauri::command]
fn sign_pdf(
    work_path: String,
    dest_path: String,
    cert_pem_path: String,
    key_pem_path: String,
    reason: Option<String>,
) -> Result<(), String> {
    let cert_pem = std::fs::read_to_string(&cert_pem_path)
        .map_err(|e| format!("No se pudo leer el certificado: {e}"))?;
    let key_pem = std::fs::read_to_string(&key_pem_path)
        .map_err(|e| format!("No se pudo leer la clave: {e}"))?;
    let cred = firma::credenciales_pem(&cert_pem, &key_pem)?;
    firma::sign(&work_path, &dest_path, &cred, reason)
}

/// Igual que `sign_pdf` pero con un contenedor PKCS#12 (.p12/.pfx).
#[tauri::command]
fn sign_pdf_p12(
    work_path: String,
    dest_path: String,
    p12_path: String,
    password: String,
    reason: Option<String>,
) -> Result<(), String> {
    let bytes = std::fs::read(&p12_path).map_err(|e| format!("No se pudo leer el .p12: {e}"))?;
    let cred = firma::credenciales_p12(&bytes, &password)?;
    firma::sign(&work_path, &dest_path, &cred, reason)
}

/// Vuelca la copia de trabajo en el destino (guardar / guardar como).
#[tauri::command]
fn save_pdf(work_path: String, dest_path: String) -> Result<(), String> {
    std::fs::copy(&work_path, &dest_path)
        .map(|_| ())
        .map_err(|e| format!("No se pudo guardar: {e}"))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Crea un PDF de prueba con una página de texto por cada entrada.
    pub(crate) fn crea_pdf(textos: &[&str], dest: &std::path::Path) {
        let textos: Vec<String> = textos.iter().map(|t| t.to_string()).collect();
        let dest = dest.to_path_buf();
        on_pdfium_thread(move || {
            let pdfium = pdfium().expect("no cargó libpdfium");
            let mut doc = pdfium.create_new_pdf().expect("crear documento");
            let font = doc.fonts_mut().helvetica();
            for texto in &textos {
                let mut page = doc
                    .pages_mut()
                    .create_page_at_end(PdfPagePaperSize::a4())
                    .expect("crear página");
                let mut obj = PdfPageTextObject::new(&doc, texto, font, PdfPoints::new(14.0))
                    .expect("crear objeto de texto");
                // posición realista (no en la esquina 0,0)
                obj.translate(PdfPoints::new(50.0), PdfPoints::new(700.0))
                    .expect("posicionar texto");
                page.objects_mut()
                    .add_text_object(obj)
                    .expect("añadir texto");
            }
            doc.save_to_file(&dest).expect("guardar PDF de prueba");
        })
    }

    /// Texto plano de cada página de un PDF, para verificar orden y contenido.
    fn textos_de(path: &std::path::Path) -> Vec<String> {
        let path = path.to_path_buf();
        on_pdfium_thread(move || {
            let pdfium = pdfium().expect("no cargó libpdfium");
            let doc = pdfium.load_pdf_from_file(&path, None).expect("abrir PDF");
            doc.pages()
                .iter()
                .map(|p| p.text().map(|t| t.all()).unwrap_or_default())
                .collect()
        })
    }

    #[test]
    fn renderiza_pagina() {
        let tmp = std::env::temp_dir().join("editor_pdf_test_render.pdf");
        crea_pdf(&["Hola"], &tmp);
        let png_b64 = render_page(tmp.to_string_lossy().into_owned(), 0, 200).expect("render");
        std::fs::remove_file(&tmp).ok();
        let png = base64::engine::general_purpose::STANDARD
            .decode(&png_b64)
            .expect("base64 válido");
        assert!(png.len() > 100, "PNG sospechosamente pequeño");
        assert_eq!(&png[1..4], b"PNG");
    }

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
            search_pdf(tmp.to_string_lossy().into_owned(), "mundo".into()).expect("buscar");
        std::fs::remove_file(&tmp).ok();
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].page_index, 0);
        assert!(!matches[0].rects.is_empty());
    }

    #[test]
    fn gestion_de_paginas() {
        let dir = std::env::temp_dir();
        let doc_a = dir.join("editor_pdf_test_paginas_a.pdf");
        let doc_b = dir.join("editor_pdf_test_paginas_b.pdf");
        let extraido = dir.join("editor_pdf_test_paginas_extra.pdf");
        crea_pdf(&["Uno", "Dos", "Tres"], &doc_a);
        crea_pdf(&["Cuatro"], &doc_b);
        let work = doc_a.to_string_lossy().into_owned();

        // mover: [Uno, Dos, Tres] -> [Dos, Uno, Tres]
        move_page(work.clone(), 0, 1).expect("mover página");
        let t = textos_de(&doc_a);
        assert!(t[0].contains("Dos") && t[1].contains("Uno"), "orden: {t:?}");

        // borrar la primera: -> [Uno, Tres]
        let count = delete_page(work.clone(), 0).expect("borrar página");
        assert_eq!(count, 2);

        // unir doc_b: -> [Uno, Tres, Cuatro]
        let count =
            merge_pdf(work.clone(), doc_b.to_string_lossy().into_owned()).expect("unir PDFs");
        assert_eq!(count, 3);
        let t = textos_de(&doc_a);
        assert!(t[2].contains("Cuatro"), "tras unir: {t:?}");

        // rotar la primera página 90°
        rotate_page(work.clone(), 0).expect("rotar página");

        // extraer la última a un PDF nuevo
        extract_pages(
            work.clone(),
            vec![2],
            extraido.to_string_lossy().into_owned(),
        )
        .expect("extraer página");
        let t = textos_de(&extraido);
        assert_eq!(t.len(), 1);
        assert!(t[0].contains("Cuatro"), "extraído: {t:?}");

        for f in [&doc_a, &doc_b, &extraido] {
            std::fs::remove_file(f).ok();
        }
    }

    #[test]
    fn cuenta_coincidencias() {
        let tmp = std::env::temp_dir().join("editor_pdf_test_contador.pdf");
        crea_pdf(&["banana banana — Hola hola HOLA — Hola  Mundo"], &tmp);
        let path = tmp.to_string_lossy().into_owned();

        // no solapadas: una por "banana", no dos dentro de la misma palabra
        let m = search_pdf(path.clone(), "ana".into()).expect("buscar ana");
        assert_eq!(m.len(), 2, "'ana' en 'banana banana'");

        // sin distinguir mayúsculas
        let m = search_pdf(path.clone(), "hola".into()).expect("buscar hola");
        assert_eq!(m.len(), 4, "'hola' aparece 4 veces");

        // rachas de espacios en el documento cuentan como un espacio
        let m = search_pdf(path.clone(), "hola mundo".into()).expect("buscar frase");
        assert_eq!(m.len(), 1, "'Hola  Mundo' con doble espacio");

        std::fs::remove_file(&tmp).ok();
    }

    #[test]
    fn trazo_es_visible() {
        let tmp = std::env::temp_dir().join("editor_pdf_test_trazo_vis.pdf");
        crea_pdf(&["Hola"], &tmp);
        let work = tmp.to_string_lossy().into_owned();
        let decode = |b64: String| {
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(b64)
                .unwrap();
            image::load_from_memory(&bytes).unwrap().to_rgba8()
        };
        let antes = decode(render_page(work.clone(), 0, 200).unwrap());
        // trazo horizontal que pasa por (150, 120) pt
        add_stroke(work.clone(), 0, vec![[50.0, 120.0], [250.0, 120.0]]).expect("trazo");
        let despues = decode(render_page(work.clone(), 0, 200).unwrap());
        let px = (150.0f32 * 200.0 / 595.0) as u32;
        let py = (120.0f32 * 200.0 / 595.0) as u32;
        let mut cambiado = false;
        for dy in 0..3 {
            if antes.get_pixel(px, py + dy) != despues.get_pixel(px, py + dy) {
                cambiado = true;
            }
        }
        assert!(cambiado, "el trazo no cambió ningún píxel");
        std::fs::remove_file(&tmp).ok();
    }

    #[test]
    fn resaltado_devuelve_sus_rects() {
        // PDFium no genera apariencia para Highlight: la UI lo pinta con los
        // rects que devuelve get_annotations. Verificamos ese contrato.
        let tmp = std::env::temp_dir().join("editor_pdf_test_resaltado_rects.pdf");
        crea_pdf(&["Hola"], &tmp);
        let work = tmp.to_string_lossy().into_owned();
        add_highlight(
            work.clone(),
            0,
            vec![
                Rect {
                    x: 50.0,
                    y: 100.0,
                    w: 200.0,
                    h: 14.0,
                },
                Rect {
                    x: 50.0,
                    y: 118.0,
                    w: 120.0,
                    h: 14.0,
                },
            ],
        )
        .expect("resaltar");
        let annots = get_annotations(work.clone(), 0).expect("listar");
        let hl = annots
            .iter()
            .find(|a| a.kind == "Highlight")
            .expect("hay un resaltado");
        assert_eq!(hl.rects.len(), 2, "un rect por línea");
        assert!((hl.rects[0].x - 50.0).abs() < 0.5, "x: {}", hl.rects[0].x);
        assert!((hl.rects[0].y - 100.0).abs() < 0.5, "y: {}", hl.rects[0].y);
        assert!((hl.rects[0].w - 200.0).abs() < 0.5, "w: {}", hl.rects[0].w);
        std::fs::remove_file(&tmp).ok();
    }

    #[test]
    fn anotaciones() {
        let tmp = std::env::temp_dir().join("editor_pdf_test_anotaciones.pdf");
        crea_pdf(&["Hola Mundo"], &tmp);
        let work = tmp.to_string_lossy().into_owned();

        // resaltado + nota
        add_highlight(
            work.clone(),
            0,
            vec![Rect {
                x: 50.0,
                y: 700.0,
                w: 100.0,
                h: 14.0,
            }],
        )
        .expect("resaltar");
        add_note(work.clone(), 0, 200.0, 100.0, "Una nota".into()).expect("añadir nota");
        let annots = get_annotations(work.clone(), 0).expect("listar anotaciones");
        assert_eq!(annots.len(), 2, "anotaciones: {:?}", annots.len());
        let nota = annots.iter().find(|a| a.kind == "Text").expect("nota");
        assert_eq!(nota.contents, "Una nota");

        // trazo como anotación Ink
        add_stroke(
            work.clone(),
            0,
            vec![[10.0, 10.0], [50.0, 40.0], [90.0, 10.0]],
        )
        .expect("añadir trazo");
        let annots = get_annotations(work.clone(), 0).expect("listar con trazo");
        assert_eq!(annots.len(), 3);
        let trazo = annots.iter().find(|a| a.kind == "Ink").expect("trazo");

        // borrar la nota y el trazo individualmente
        remove_annotation(work.clone(), 0, nota.index).expect("borrar nota");
        let annots = get_annotations(work.clone(), 0).expect("listar tras borrar");
        assert_eq!(annots.len(), 2);
        let trazo_idx = annots
            .iter()
            .find(|a| a.kind == "Ink")
            .map(|a| a.index)
            .unwrap_or(trazo.index);
        remove_annotation(work.clone(), 0, trazo_idx).expect("borrar trazo");
        assert_eq!(get_annotations(work.clone(), 0).expect("listar").len(), 1);

        // el render con anotaciones no debe fallar
        render_page(work.clone(), 0, 200).expect("render con anotaciones");

        std::fs::remove_file(&tmp).ok();
    }

    /// Construye un PDF mínimo con AcroForm: un campo de texto y una casilla.
    /// PDFium no puede crear campos de formulario, así que se escribe a mano.
    fn crea_pdf_formulario(dest: &std::path::Path) {
        let ap_si = "q 0 0 1 RG 2 2 m 18 18 l S 2 18 m 18 2 l S Q";
        let ap_no = "q 0.5 w 0 0 20 20 re S Q";
        let objs: Vec<(u32, String)> = vec![
            (
                1,
                "<</Type/Catalog/Pages 2 0 R/AcroForm<</Fields[4 0 R 5 0 R]\
                 /DA(/Helv 0 Tf 0 g)/DR<</Font<</Helv 6 0 R>>>>>>>>"
                    .into(),
            ),
            (2, "<</Type/Pages/Kids[3 0 R]/Count 1>>".into()),
            (
                3,
                "<</Type/Page/Parent 2 0 R/MediaBox[0 0 595 842]\
                 /Annots[4 0 R 5 0 R]/Resources<</Font<</Helv 6 0 R>>>>>>"
                    .into(),
            ),
            (
                4,
                "<</Type/Annot/Subtype/Widget/FT/Tx/T(nombre)\
                 /Rect[50 700 250 720]/F 4/DA(/Helv 12 Tf 0 g)>>"
                    .into(),
            ),
            (
                5,
                "<</Type/Annot/Subtype/Widget/FT/Btn/T(acepto)\
                 /Rect[50 650 70 670]/F 4/V/Off/AS/Off\
                 /AP<</N<</Yes 7 0 R/Off 8 0 R>>>>>>"
                    .into(),
            ),
            (6, "<</Type/Font/Subtype/Type1/BaseFont/Helvetica>>".into()),
            (
                7,
                format!(
                    "<</BBox[0 0 20 20]/Length {}>>\nstream\n{}\nendstream",
                    ap_si.len(),
                    ap_si
                ),
            ),
            (
                8,
                format!(
                    "<</BBox[0 0 20 20]/Length {}>>\nstream\n{}\nendstream",
                    ap_no.len(),
                    ap_no
                ),
            ),
        ];
        let mut out: Vec<u8> = b"%PDF-1.7\n".to_vec();
        let mut offsets = vec![0usize; objs.len() + 1];
        for (num, body) in &objs {
            offsets[*num as usize] = out.len();
            out.extend_from_slice(format!("{num} 0 obj\n{body}\nendobj\n").as_bytes());
        }
        let xref_pos = out.len();
        out.extend_from_slice(format!("xref\n0 {}\n", objs.len() + 1).as_bytes());
        out.extend_from_slice(b"0000000000 65535 f \n");
        for i in 1..=objs.len() {
            out.extend_from_slice(format!("{:010} 00000 n \n", offsets[i]).as_bytes());
        }
        out.extend_from_slice(
            format!(
                "trailer\n<</Size {}/Root 1 0 R>>\nstartxref\n{}\n%%EOF",
                objs.len() + 1,
                xref_pos
            )
            .as_bytes(),
        );
        std::fs::write(dest, out).expect("escribir PDF de formulario");
    }

    #[test]
    fn formularios() {
        let tmp = std::env::temp_dir().join("editor_pdf_test_formulario.pdf");
        crea_pdf_formulario(&tmp);
        let work = tmp.to_string_lossy().into_owned();

        let fields = get_form_fields(work.clone(), 0).expect("listar campos");
        assert_eq!(fields.len(), 2, "campos: {}", fields.len());
        let nombre = fields.iter().find(|f| f.name == "nombre").expect("texto");
        assert_eq!(nombre.kind, "Text");
        assert_eq!(nombre.value, "");
        let acepto = fields.iter().find(|f| f.name == "acepto").expect("casilla");
        assert_eq!(acepto.kind, "Checkbox");
        assert!(!acepto.checked);

        set_form_text(work.clone(), 0, nombre.annot_index, "Jorge".into()).expect("escribir texto");
        set_form_checked(work.clone(), 0, acepto.annot_index, true).expect("marcar");

        let fields = get_form_fields(work.clone(), 0).expect("relistar");
        assert_eq!(
            fields.iter().find(|f| f.name == "nombre").unwrap().value,
            "Jorge"
        );
        assert!(fields.iter().find(|f| f.name == "acepto").unwrap().checked);

        render_page(work.clone(), 0, 200).expect("render con formulario");
        std::fs::remove_file(&tmp).ok();
    }

    #[test]
    fn edicion_de_texto() {
        let tmp = std::env::temp_dir().join("editor_pdf_test_edicion.pdf");
        crea_pdf(&["Texto original"], &tmp);
        let work = tmp.to_string_lossy().into_owned();

        let blocks = get_text_blocks(work.clone(), 0).expect("listar bloques");
        assert_eq!(blocks.len(), 1, "bloques: {}", blocks.len());
        assert!(
            blocks[0].text.contains("Texto original"),
            "texto: {:?}",
            blocks[0].text
        );
        assert!(blocks[0].w > 0.0 && blocks[0].h > 0.0);

        // reescribir el content stream
        edit_text_block(
            work.clone(),
            0,
            blocks[0].object_index,
            "Texto editado".into(),
        )
        .expect("editar bloque");
        let t = textos_de(&tmp);
        assert!(t[0].contains("Texto editado"), "tras editar: {t:?}");
        assert!(!t[0].contains("original"), "no debe quedar el texto viejo");

        // borrar el bloque
        let blocks = get_text_blocks(work.clone(), 0).expect("relistar");
        delete_text_block(work.clone(), 0, blocks[0].object_index).expect("borrar bloque");
        let blocks = get_text_blocks(work.clone(), 0).expect("listar tras borrar");
        assert!(blocks.is_empty(), "quedan {} bloques", blocks.len());

        render_page(work.clone(), 0, 200).expect("render tras editar");
        std::fs::remove_file(&tmp).ok();
    }

    #[test]
    fn anadir_texto_nuevo() {
        let tmp = std::env::temp_dir().join("editor_pdf_test_texto_nuevo.pdf");
        crea_pdf(&["Contenido previo"], &tmp);
        let work = tmp.to_string_lossy().into_owned();

        add_text_block(
            work.clone(),
            0,
            100.0,
            300.0,
            "Añadido a mano\nSegunda línea".into(),
            12.0,
            None,
        )
        .expect("añadir texto");

        let t = textos_de(&tmp).join(" ");
        assert!(t.contains("Contenido previo"), "texto: {t:?}");
        assert!(t.contains("Añadido a mano"), "texto: {t:?}");
        assert!(t.contains("Segunda línea"), "texto: {t:?}");

        // dos bloques nuevos + el previo, y el nuevo cerca del punto pedido
        let blocks = get_text_blocks(work.clone(), 0).expect("listar");
        assert_eq!(blocks.len(), 3, "bloques: {}", blocks.len());
        let nuevo = blocks
            .iter()
            .find(|b| b.text.contains("Añadido"))
            .expect("bloque nuevo");
        assert!(
            (nuevo.x - 100.0).abs() < 3.0 && (nuevo.y - 300.0).abs() < 8.0,
            "posición: ({}, {})",
            nuevo.x,
            nuevo.y
        );

        // el texto vacío debe rechazarse
        assert!(add_text_block(work.clone(), 0, 0.0, 0.0, "  ".into(), 12.0, None).is_err());

        std::fs::remove_file(&tmp).ok();
    }

    #[test]
    fn fuente_elegida_y_detectada() {
        let tmp = std::env::temp_dir().join("editor_pdf_test_fuentes.pdf");
        crea_pdf(&["Texto base"], &tmp); // crea_pdf usa Helvetica
        let work = tmp.to_string_lossy().into_owned();

        // fuente automática primero (solo hay Helvetica en la página, sin
        // empates): debe detectar la dominante
        add_text_block(work.clone(), 0, 60.0, 400.0, "Detectada".into(), 12.0, None)
            .expect("añadir automática");
        let blocks = get_text_blocks(work.clone(), 0).expect("listar");
        let auto = blocks
            .iter()
            .find(|b| b.text.contains("Detectada"))
            .expect("bloque automático");
        assert!(
            auto.font_family.to_lowercase().contains("helvetica")
                || auto.font_family.to_lowercase().contains("arial"),
            "familia detectada: {:?}",
            auto.font_family
        );

        // fuente elegida a mano
        add_text_block(
            work.clone(),
            0,
            60.0,
            200.0,
            "Con serifa".into(),
            14.0,
            Some("Times Bold".into()),
        )
        .expect("añadir con Times");
        let blocks = get_text_blocks(work.clone(), 0).expect("relistar");
        let serif = blocks
            .iter()
            .find(|b| b.text.contains("Con serifa"))
            .expect("bloque nuevo");
        assert!(
            serif.font_family.to_lowercase().contains("times"),
            "familia: {:?}",
            serif.font_family
        );

        std::fs::remove_file(&tmp).ok();
    }

    #[test]
    fn imagenes() {
        let dir = std::env::temp_dir();
        let tmp = dir.join("editor_pdf_test_imagenes.pdf");
        let png = dir.join("editor_pdf_test_imagen.png");
        let png2 = dir.join("editor_pdf_test_imagen2.png");
        crea_pdf(&["Con imagen"], &tmp);
        let work = tmp.to_string_lossy().into_owned();

        // dos PNGs pequeños de colores distintos
        image::RgbaImage::from_pixel(80, 40, image::Rgba([200, 30, 30, 255]))
            .save(&png)
            .expect("crear png");
        image::RgbaImage::from_pixel(40, 40, image::Rgba([30, 30, 200, 255]))
            .save(&png2)
            .expect("crear png2");

        // insertar en (100, 200): tamaño natural 80x40 pt
        add_image(
            work.clone(),
            0,
            png.to_string_lossy().into_owned(),
            100.0,
            200.0,
        )
        .expect("añadir imagen");
        let imgs = get_images(work.clone(), 0).expect("listar imágenes");
        assert_eq!(imgs.len(), 1, "imágenes: {}", imgs.len());
        let im = &imgs[0];
        assert!(
            (im.x - 100.0).abs() < 2.0 && (im.y - 200.0).abs() < 2.0,
            "posición: ({}, {})",
            im.x,
            im.y
        );
        assert!(
            (im.w - 80.0).abs() < 2.0 && (im.h - 40.0).abs() < 2.0,
            "tamaño: {}x{}",
            im.w,
            im.h
        );

        // mover y redimensionar
        transform_image(work.clone(), 0, im.object_index, 50.0, 300.0, 160.0, 80.0)
            .expect("transformar");
        let imgs = get_images(work.clone(), 0).expect("relistar");
        let im = &imgs[0];
        assert!(
            (im.x - 50.0).abs() < 2.0 && (im.y - 300.0).abs() < 2.0,
            "posición tras mover: ({}, {})",
            im.x,
            im.y
        );
        assert!(
            (im.w - 160.0).abs() < 2.0 && (im.h - 80.0).abs() < 2.0,
            "tamaño tras redimensionar: {}x{}",
            im.w,
            im.h
        );

        // reemplazar manteniendo bounds
        replace_image(
            work.clone(),
            0,
            im.object_index,
            png2.to_string_lossy().into_owned(),
        )
        .expect("reemplazar");
        let imgs = get_images(work.clone(), 0).expect("listar tras reemplazo");
        assert_eq!(imgs.len(), 1);
        assert!(
            (imgs[0].w - 160.0).abs() < 2.0 && (imgs[0].h - 80.0).abs() < 2.0,
            "bounds tras reemplazo: {}x{}",
            imgs[0].w,
            imgs[0].h
        );

        // eliminar
        delete_image(work.clone(), 0, imgs[0].object_index).expect("eliminar");
        assert!(get_images(work.clone(), 0)
            .expect("listar final")
            .is_empty());

        render_page(work.clone(), 0, 200).expect("render tras imágenes");
        for f in [&tmp, &png, &png2] {
            std::fs::remove_file(f).ok();
        }
    }

    #[test]
    fn edicion_multilinea() {
        let tmp = std::env::temp_dir().join("editor_pdf_test_multilinea.pdf");
        crea_pdf(&["Una línea"], &tmp);
        let work = tmp.to_string_lossy().into_owned();

        let blocks = get_text_blocks(work.clone(), 0).expect("listar");
        edit_text_block(
            work.clone(),
            0,
            blocks[0].object_index,
            "Primera línea\nSegunda línea\nTercera".into(),
        )
        .expect("editar multilínea");

        let t = textos_de(&tmp).join(" ");
        assert!(t.contains("Primera línea"), "texto: {t:?}");
        assert!(t.contains("Segunda línea"), "texto: {t:?}");
        assert!(t.contains("Tercera"), "texto: {t:?}");

        // deben existir tres bloques, apilados en vertical
        let blocks = get_text_blocks(work.clone(), 0).expect("relistar");
        assert_eq!(blocks.len(), 3, "bloques: {}", blocks.len());
        let primera = blocks.iter().find(|b| b.text.contains("Primera")).unwrap();
        let segunda = blocks.iter().find(|b| b.text.contains("Segunda")).unwrap();
        assert!(
            segunda.y > primera.y,
            "la segunda línea debe quedar debajo ({} > {})",
            segunda.y,
            primera.y
        );

        std::fs::remove_file(&tmp).ok();
    }

    #[test]
    fn firma_digital() {
        use sha2::{Digest, Sha256};

        let dir = std::env::temp_dir();
        let src = dir.join("editor_pdf_test_firma_src.pdf");
        let dest = dir.join("editor_pdf_test_firma_out.pdf");
        crea_pdf(&["Documento importante"], &src);

        let cert_pem = include_str!("../fixtures/test_cert.pem");
        let key_pem = include_str!("../fixtures/test_key.pem");
        let cred = firma::credenciales_pem(cert_pem, key_pem).expect("credenciales PEM");
        firma::sign(
            &src.to_string_lossy(),
            &dest.to_string_lossy(),
            &cred,
            Some("Prueba".into()),
        )
        .expect("firmar");

        let bytes = std::fs::read(&dest).expect("leer PDF firmado");

        // extraer ByteRange [a b c d]
        let txt = String::from_utf8_lossy(&bytes);
        let br_start = txt.find("/ByteRange").expect("ByteRange presente");
        let open = txt[br_start..].find('[').unwrap() + br_start + 1;
        let close = txt[open..].find(']').unwrap() + open;
        let nums: Vec<usize> = txt[open..close]
            .split_whitespace()
            .map(|n| n.parse().expect("número en ByteRange"))
            .collect();
        assert_eq!(nums.len(), 4, "ByteRange: {:?}", nums);
        assert_eq!(nums[0], 0);
        assert_eq!(
            nums[2] + nums[3],
            bytes.len(),
            "el ByteRange debe cubrir hasta el final del fichero"
        );

        // digest sobre los dos rangos
        let mut hasher = Sha256::new();
        hasher.update(&bytes[nums[0]..nums[0] + nums[1]]);
        hasher.update(&bytes[nums[2]..nums[2] + nums[3]]);
        let digest = hasher.finalize();

        // el hueco entre rangos es <hex de la firma>
        let gap = &bytes[nums[0] + nums[1]..nums[2]];
        assert_eq!(gap[0], b'<');
        assert_eq!(gap[gap.len() - 1], b'>');
        let hex: String = String::from_utf8_lossy(&gap[1..gap.len() - 1]).into_owned();
        let der_bytes: Vec<u8> = (0..hex.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).expect("hex válido"))
            .collect();

        // parsear el CMS (el hueco lleva ceros de relleno tras el DER, así
        // que no se puede exigir consumo exacto) y comparar el messageDigest
        let ci: cms::content_info::ContentInfo = {
            use der::Reader;
            let mut reader = der::SliceReader::new(&der_bytes).unwrap();
            reader.decode().expect("ContentInfo DER válido")
        };
        assert_eq!(ci.content_type, const_oid::db::rfc5911::ID_SIGNED_DATA);
        let sd: cms::signed_data::SignedData = ci.content.decode_as().expect("SignedData válido");
        let signer = sd.signer_infos.0.iter().next().expect("un firmante");
        let attrs = signer.signed_attrs.as_ref().expect("atributos firmados");
        let md_attr = attrs
            .iter()
            .find(|a| a.oid == const_oid::db::rfc5911::ID_MESSAGE_DIGEST)
            .expect("atributo messageDigest");
        let md_der = {
            use der::Encode;
            md_attr
                .values
                .iter()
                .next()
                .expect("valor")
                .to_der()
                .unwrap()
        };
        // el valor es un OCTET STRING: 0x04, len, bytes
        assert_eq!(
            &md_der[md_der.len() - 32..],
            digest.as_slice(),
            "el messageDigest firmado debe coincidir con el hash del ByteRange"
        );

        std::fs::remove_file(&src).ok();
        std::fs::remove_file(&dest).ok();
    }

    #[test]
    fn firma_con_p12() {
        let dir = std::env::temp_dir();
        let src = dir.join("editor_pdf_test_p12_src.pdf");
        let dest = dir.join("editor_pdf_test_p12_out.pdf");
        crea_pdf(&["Firmado con p12"], &src);

        let p12 = include_bytes!("../fixtures/test_bundle.p12");
        let cred = firma::credenciales_p12(p12, "test1234").expect("abrir p12");
        firma::sign(&src.to_string_lossy(), &dest.to_string_lossy(), &cred, None)
            .expect("firmar con p12");
        let bytes = std::fs::read(&dest).expect("leer firmado");
        assert!(
            find_in(&bytes, b"/SubFilter/adbe.pkcs7.detached")
                || find_in(&bytes, b"/SubFilter /adbe.pkcs7.detached"),
            "el PDF firmado debe llevar el SubFilter"
        );

        // contraseña incorrecta debe fallar con error, no colgarse ni abrir
        assert!(firma::credenciales_p12(p12, "mala").is_err());

        std::fs::remove_file(&src).ok();
        std::fs::remove_file(&dest).ok();
    }

    fn find_in(haystack: &[u8], needle: &[u8]) -> bool {
        haystack.windows(needle.len()).any(|w| w == needle)
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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            use tauri::Manager;
            if let Ok(dir) = app.path().resource_dir() {
                let _ = RESOURCE_LIB_DIR.set(dir.join("lib"));
            }
            Ok(())
        })
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            open_pdf,
            render_page,
            get_page_text,
            search_pdf,
            delete_page,
            rotate_page,
            move_page,
            merge_pdf,
            extract_pages,
            save_pdf,
            add_highlight,
            add_stroke,
            add_note,
            get_annotations,
            remove_annotation,
            get_form_fields,
            set_form_text,
            set_form_checked,
            get_text_blocks,
            edit_text_block,
            add_text_block,
            delete_text_block,
            get_images,
            add_image,
            transform_image,
            replace_image,
            delete_image,
            sign_pdf,
            sign_pdf_p12,
            firmas_visuales::stamp_signature,
            firmas_visuales::import_signature_file,
            firmas_visuales::save_stored_signature,
            firmas_visuales::list_stored_signatures,
            firmas_visuales::delete_stored_signature,
            imagenes::get_image_data,
            anotaciones2::add_markup,
            anotaciones2::add_shape,
            anotaciones2::add_stamp,
            paginas2::add_blank_page,
            paginas2::duplicate_page,
            paginas2::insert_pdf_at,
            paginas2::crop_page,
            paginas2::add_watermark,
            paginas2::add_header_footer,
            documento::get_outline,
            documento::set_outline,
            documento::get_metadata,
            documento::set_metadata,
            documento::get_links
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
