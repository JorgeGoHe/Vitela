//! Edición real de texto: bloques del content stream, fuentes y texto nuevo.

use crate::{on_pdfium_thread, pdfium, save_and_close, with_doc};
use crate::historial::mutacion;
use pdfium_render::prelude::*;
use serde::Serialize;

#[derive(Serialize, Clone)]
pub struct TextBlock {
    pub object_index: u32,
    pub text: String,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub font_size: f32,
    /// Familia **normalizada** (Helvetica, Times, Courier…): las fuentes
    /// internas de PDFium cambian de nombre entre builds. El estilo va
    /// aparte, en `negrita` y `cursiva`, porque `normaliza_familia` se lo
    /// llevaba por delante («Arial-Bold» salía «Helvetica») y al exportar a
    /// Word el párrafo perdía el énfasis.
    pub font_family: String,
    pub negrita: bool,
    pub cursiva: bool,
    /// Color del relleno del texto, RGBA. Sin él la UI no puede pintar del
    /// color real el botón «A» («el color que ya tenga») ni conservarlo al
    /// corregir un párrafo de varias líneas.
    pub color: [u8; 4],
}

/// Directorios de fuentes TTF del sistema, por plataforma.
pub fn directorios_de_fuentes() -> Vec<std::path::PathBuf> {
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
    if !n.contains("helvetica") && !n.contains("arial") && !n.contains("chrom sans") && !n.is_empty() {
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

/// Nombre de familia tal como lo enseña la UI. Las fuentes internas de
/// PDFium cambian de nombre entre builds (la Helvetica builtin era «Arial»
/// y desde ~chromium/8000 es «Chrom Sans OTF»): se devuelven las estándar,
/// que además son las que `fuente_por_nombre` sabe volver a cargar.
pub(crate) fn normaliza_familia(familia: &str) -> String {
    let f = familia.trim();
    let n = f.to_lowercase();
    // por subcadena, no por prefijo exacto: el nombre real de un PDF de
    // fuera viene con el estilo pegado («TimesNewRomanPS-BoldItalicMT»),
    // y ese estilo lo lee `estilo_del_nombre`, no esta función
    if n.contains("chrom sans") || n.contains("arial") || n.contains("helvetica") {
        return "Helvetica".into();
    }
    if n.contains("chrom serif") || n.contains("times") {
        return "Times".into();
    }
    if n.contains("chrom mono") || n.contains("courier") {
        return "Courier".into();
    }
    f.to_string()
}

/// Familia de fuente más usada por los objetos de texto de una página.
pub(crate) fn familia_dominante(doc: &PdfDocument<'static>, page_index: u16) -> Option<String> {
    let page = doc.pages().get(page_index).ok()?;
    let objects = page.objects();
    let mut cuentas: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for i in 0..objects.len() {
        if let Ok(obj) = objects.get(i) {
            if let Some(t) = obj.as_text_object() {
                let familia = normaliza_familia(&t.font().family());
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

/// Desplazamiento horizontal que hay que aplicar a un texto de ancho `w`
/// para que quede alineado como se pide respecto del punto de anclaje:
/// «izq» (por defecto) no mueve nada, «centro» lo centra y «der» lo pega a
/// la derecha. En un PDF no hay operador de alineación: se coloca el
/// origen, que es lo que hace Acrobat al alinear un párrafo.
pub(crate) fn desplazamiento_por_alineacion(align: Option<&str>, w: f32) -> f32 {
    match align.unwrap_or("izq") {
        "centro" | "center" => -w / 2.0,
        "der" | "derecha" | "right" => -w,
        _ => 0.0,
    }
}

/// Ancho de un objeto de texto recién creado (aún sin página). Si PDFium no
/// lo sabe dar, se estima por caracteres, que para alinear basta.
fn ancho_del_objeto(obj: &PdfPageTextObject, texto: &str, size: f32) -> f32 {
    match obj.bounds() {
        Ok(b) if b.right().value > b.left().value => b.right().value - b.left().value,
        _ => texto.chars().count() as f32 * size * 0.5,
    }
}

/// Lista los objetos de texto de una página (bloques editables), con bounds
/// en coords de UI.
#[tauri::command(async)]
pub fn get_text_blocks(path: String, page_index: u16) -> Result<Vec<TextBlock>, String> {
    on_pdfium_thread(move || with_doc(&path, |doc| Ok(bloques_de(doc, page_index))))
}

/// El cuerpo de [`get_text_blocks`], sobre un documento ya abierto: lo usa
/// también la exportación a Word, que trabaja sobre una copia propia de
/// solo lectura. Una página que no existe no es un error: no tiene bloques.
pub(crate) fn bloques_de(doc: &PdfDocument<'static>, page_index: u16) -> Vec<TextBlock> {
    let Ok(page) = doc.pages().get(page_index) else {
        return Vec::new();
    };
    // espacio propio de la página: las cajas de los objetos no
    // llevan la rotación, y `page.height()` sí (ver `Geo`)
    let geo = crate::Geo::de_pagina(&page).propia();
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
        // `bounds()` de un objeto de página son quadpoints; los
        // giros del PDF son múltiplos de 90°, así que su caja
        // envolvente es el rect
        let caja = geo.pdf_rect_a_ui(&PdfRect::new(b.bottom(), b.left(), b.top(), b.right()));
        let fuente = t.font();
        let cruda = fuente.family();
        let c = obj.fill_color().unwrap_or(PdfColor::new(0, 0, 0, 255));
        let estilo = estilo_de(&fuente, &cruda);
        out.push(TextBlock {
            object_index: i as u32,
            text,
            x: caja.x,
            y: caja.y,
            w: caja.w,
            h: caja.h,
            font_size: t.unscaled_font_size().value,
            font_family: normaliza_familia(&cruda),
            negrita: estilo.0,
            cursiva: estilo.1,
            color: [c.red(), c.green(), c.blue(), c.alpha()],
        });
    }
    out
}

/// El estilo que declara el **nombre** de la fuente: «Arial-BoldMT»,
/// «Helvetica-Oblique», «TimesNewRomanPS-BoldItalicMT». Devuelve
/// `(negrita, cursiva)`.
///
/// El nombre manda porque **el peso que devuelve PDFium no es de fiar**: lo
/// avisa su propia documentación y se comprueba a ojo con las fuentes
/// internas de este build (chromium/8009), donde `times_bold()` sale como
/// «Times New Roman» con peso 0 y sin la bandera de negrita del descriptor.
/// En un PDF de fuera, que lleva la fuente embebida con su nombre real, el
/// nombre acierta.
pub(crate) fn estilo_del_nombre(nombre: &str) -> (bool, bool) {
    let n = nombre.to_lowercase();
    let negrita = n.contains("bold") || n.contains("negrita") || n.contains("black")
        || n.contains("heavy") || n.contains("semibold");
    let cursiva = n.contains("italic") || n.contains("oblique") || n.contains("cursiva");
    (negrita, cursiva)
}

/// El estilo de una fuente concreta: el nombre y, si calla, lo que digan el
/// peso y la bandera del descriptor.
fn estilo_de(fuente: &PdfFont, nombre: &str) -> (bool, bool) {
    let (negrita, cursiva) = estilo_del_nombre(nombre);
    let pesada = matches!(
        fuente.weight(),
        Ok(PdfFontWeight::Weight600
            | PdfFontWeight::Weight700Bold
            | PdfFontWeight::Weight800
            | PdfFontWeight::Weight900)
    ) || matches!(fuente.weight(), Ok(PdfFontWeight::Custom(p)) if p >= 600);
    (negrita || pesada, cursiva || fuente.is_italic())
}

/// Edición real de texto: reescribe el objeto de texto del content stream.
/// Mantiene la fuente del objeto (si la fuente embebida no tiene los glifos
/// del texto nuevo, esos caracteres no se verán). Si el texto nuevo tiene
/// varias líneas, la primera reemplaza al objeto original y las demás se
/// insertan como objetos nuevos con la misma fuente, colocados debajo.
#[tauri::command(async)]
pub fn edit_text_block(
    work_path: String,
    page_index: u16,
    object_index: u32,
    new_text: String,
    color: Option<[u8; 4]>,
    align: Option<String>,
) -> Result<(), String> {
    mutacion(work_path, move |work_path| on_pdfium_thread(move || {
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
            if let Some([r, g, b, a]) = color {
                t.set_fill_color(PdfColor::new(r, g, b, a))
                    .map_err(|e| e.to_string())?;
            }
            // alinear es recolocar el origen: el bloque conserva su centro
            // o su borde derecho, según se pida, en vez de crecer siempre
            // hacia la derecha
            if align.is_some() {
                let ancho_viejo = bounds.right().value - bounds.left().value;
                let nuevos = obj.bounds().map_err(|e| e.to_string())?;
                let ancho_nuevo = nuevos.right().value - nuevos.left().value;
                let dx = desplazamiento_por_alineacion(align.as_deref(), ancho_nuevo)
                    - desplazamiento_por_alineacion(align.as_deref(), ancho_viejo);
                if dx.abs() > 0.01 {
                    obj.translate(PdfPoints::new(dx), PdfPoints::ZERO)
                        .map_err(|e| e.to_string())?;
                }
            }
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
                if let Some([r, g, b, a]) = color {
                    nuevo
                        .set_fill_color(PdfColor::new(r, g, b, a))
                        .map_err(|e| e.to_string())?;
                }
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
    }))
}

/// Una coincidencia que hay que reescribir: el bloque donde está (el
/// `object_index` de `get_text_blocks`, que trae también `search_pdf` con
/// `context`), el texto que se busca y el que lo sustituye.
#[derive(serde::Deserialize, Clone)]
pub struct Reemplazo {
    pub page_index: u16,
    pub block_index: u32,
    pub from: String,
    pub to: String,
}

/// Lo que se ha podido hacer, para decirlo tal cual: «9 de 12 reemplazadas;
/// 3 están en una fuente que no se puede editar».
#[derive(serde::Serialize, Debug, Default)]
pub struct InformeReemplazo {
    /// Coincidencias reescritas.
    pub hechas: u16,
    /// Coincidencias que no se han podido tocar: su bloque ya no dice lo
    /// que decía, o está en una fuente que no se deja reescribir.
    pub saltadas: u16,
}

/// «Reemplazar todo»: reescribe de una vez todas las coincidencias que se
/// le pasen, **en una sola mutación**, así que un ⌘Z las devuelve todas
/// juntas. Devuelve el recuento honesto: cuántas ha cambiado y cuántas no.
///
/// Una coincidencia cuyo bloque ya no dice lo que decía —porque el
/// documento ha cambiado entre la búsqueda y el reemplazo, o porque el
/// texto está en una fuente que no se puede reescribir— se salta sin romper
/// el lote: es mejor cambiar 9 de 12 y decirlo que no cambiar ninguna.
#[tauri::command(async)]
pub fn replace_text(
    work_path: String,
    matches: Vec<Reemplazo>,
) -> Result<InformeReemplazo, String> {
    if matches.is_empty() {
        return Err("No hay ninguna coincidencia que reemplazar".into());
    }
    mutacion(work_path, move |work_path| on_pdfium_thread(move || {
        let pdfium = pdfium()?;
        let doc = pdfium
            .load_pdf_from_file(&work_path, None)
            .map_err(crate::mensaje_llano)?;
        // varias coincidencias pueden caer en el mismo bloque: se agrupan
        // para tocar cada objeto una sola vez y reemplazar tantas
        // ocurrencias como coincidencias se hayan pedido
        let mut por_bloque: std::collections::BTreeMap<(u16, u32, String, String), usize> =
            Default::default();
        for m in matches {
            *por_bloque
                .entry((m.page_index, m.block_index, m.from, m.to))
                .or_default() += 1;
        }
        let mut informe = InformeReemplazo::default();
        for ((page_index, block_index, from, to), veces) in por_bloque {
            let pedidas = veces as u16;
            let salta = |informe: &mut InformeReemplazo| informe.saltadas += pedidas;
            if from.is_empty() {
                salta(&mut informe);
                continue;
            }
            let Ok(mut page) = doc.pages().get(page_index) else {
                salta(&mut informe);
                continue;
            };
            let Ok(mut obj) = page.objects_mut().get(block_index as usize) else {
                salta(&mut informe);
                continue;
            };
            let Some(t) = obj.as_text_object_mut() else {
                salta(&mut informe);
                continue;
            };
            let viejo = t.text();
            let cuantas = viejo.matches(&from).count().min(veces);
            if cuantas == 0 {
                // el bloque ya no dice lo que decía: se salta
                salta(&mut informe);
                continue;
            }
            let nuevo = viejo.replacen(&from, &to, cuantas);
            if t.set_text(&nuevo).is_err() {
                // fuente que no se deja reescribir
                salta(&mut informe);
                continue;
            }
            drop(obj);
            page.regenerate_content().map_err(crate::mensaje_llano)?;
            informe.hechas += cuantas as u16;
            informe.saltadas += pedidas - cuantas as u16;
        }
        if informe.hechas == 0 {
            return Err(
                "Ninguna de esas coincidencias sigue donde estaba: vuelve a buscar".into(),
            );
        }
        save_and_close(doc, &work_path)?;
        Ok(informe)
    }))
}

/// Añade un bloque de texto nuevo en el punto dado (coords de UI, el punto
/// es la esquina superior izquierda de la primera línea). Cada línea del
/// texto se inserta como un objeto propio. La fuente puede elegirse por
/// nombre; sin nombre (o "auto") se detecta la familia dominante de la
/// página y se aproxima.
#[tauri::command(async)]
#[allow(clippy::too_many_arguments)]
pub fn add_text_block(
    work_path: String,
    page_index: u16,
    x: f32,
    y: f32,
    text: String,
    font_size: f32,
    font: Option<String>,
    color: Option<[u8; 4]>,
    align: Option<String>,
) -> Result<(), String> {
    if text.trim().is_empty() {
        return Err("El texto está vacío".into());
    }
    let font_size = font_size.clamp(6.0, 96.0);
    mutacion(work_path, move |work_path| on_pdfium_thread(move || {
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
        // el punto llega en el espacio PROPIO de la página; los ejes de la
        // vista dicen hacia dónde se lee, para que en una página girada el
        // texto salga derecho y no tumbado (como hace add_stamp)
        let vista = crate::Geo::de_pagina(&page);
        let rot = vista.rot;
        let (derecha, abajo) = vista.ejes();
        let ancla = vista.propia().ui_a_pdf(x, y);
        let line_h = font_size * 1.2;
        for (i, linea) in text.lines().enumerate() {
            if linea.trim().is_empty() {
                continue;
            }
            let mut obj = PdfPageTextObject::new(&doc, linea, font, PdfPoints::new(font_size))
                .map_err(|e| e.to_string())?;
            if let Some([r, g, b, a]) = color {
                obj.set_fill_color(PdfColor::new(r, g, b, a))
                    .map_err(|e| e.to_string())?;
            }
            // alinear: el punto que marcó el usuario es el borde izquierdo,
            // el centro o el borde derecho de la línea, según se pida
            let dx = desplazamiento_por_alineacion(
                align.as_deref(),
                ancho_del_objeto(&obj, linea, font_size),
            );
            if rot != 0 {
                obj.rotate_counter_clockwise_degrees(rot as f32)
                    .map_err(|e| e.to_string())?;
            }
            // el clic marca la parte superior de la primera línea; el objeto
            // se coloca por su baseline aproximada, bajando en el sentido en
            // el que baja la vista
            let bajada = font_size + line_h * i as f32;
            obj.translate(
                PdfPoints::new(ancla.0 + abajo.0 * bajada + derecha.0 * dx),
                PdfPoints::new(ancla.1 + abajo.1 * bajada + derecha.1 * dx),
            )
            .map_err(|e| e.to_string())?;
            page.objects_mut()
                .add_text_object(obj)
                .map_err(|e| e.to_string())?;
        }
        page.regenerate_content().map_err(|e| e.to_string())?;
        drop(page);
        save_and_close(doc, &work_path)?;
        Ok(())
    }))
}

/// Borra un bloque de texto del content stream.
#[tauri::command(async)]
pub fn delete_text_block(work_path: String, page_index: u16, object_index: u32) -> Result<(), String> {
    mutacion(work_path, |work_path| on_pdfium_thread(move || {
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
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[allow(unused_imports)]
    use crate::tests::{crea_pdf, textos_de};
    #[allow(unused_imports)]
    use crate::{get_page_sizes, open_pdf, render_page_b64, render_page_png};

    /// **G2.** La familia va normalizada y el estilo aparte: hasta el ciclo
    /// 5 `normaliza_familia` convertía «Arial-BoldMT» en «Helvetica» y el
    /// párrafo perdía la negrita al exportar a Word. El nombre es la fuente
    /// de verdad porque el peso que devuelve PDFium no lo es (con las
    /// fuentes internas de chromium/8009, `times_bold()` sale como «Times
    /// New Roman» con peso 0).
    #[test]
    fn el_nombre_de_la_fuente_dice_el_estilo_y_la_familia_no_se_lo_come() {
        assert_eq!(estilo_del_nombre("Arial-BoldMT"), (true, false));
        assert_eq!(estilo_del_nombre("Helvetica-Oblique"), (false, true));
        assert_eq!(
            estilo_del_nombre("TimesNewRomanPS-BoldItalicMT"),
            (true, true)
        );
        assert_eq!(estilo_del_nombre("Georgia"), (false, false));
        // y la familia sigue normalizándose, que es lo que la UI enseña
        assert_eq!(normaliza_familia("Arial-BoldMT"), "Helvetica");
        assert_eq!(normaliza_familia("TimesNewRomanPS-BoldItalicMT"), "Times");
    }

    #[test]
    fn normaliza_nombres_de_fuentes_internas() {
        assert_eq!(normaliza_familia("Arial"), "Helvetica");
        assert_eq!(normaliza_familia("Chrom Sans OTF"), "Helvetica");
        assert_eq!(normaliza_familia("Helvetica"), "Helvetica");
        assert_eq!(normaliza_familia("Chrom Serif OTF"), "Times");
        assert_eq!(normaliza_familia("Times New Roman"), "Times");
        assert_eq!(normaliza_familia("Chrom Mono OTF"), "Courier");
        assert_eq!(normaliza_familia("Georgia"), "Georgia");
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
            None,
            None,
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

        render_page_b64(work.clone(), 0, 200, None).expect("render tras editar");
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
            None,
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
        assert!(add_text_block(work.clone(), 0, 0.0, 0.0, "  ".into(), 12.0, None, None, None).is_err());

        std::fs::remove_file(&tmp).ok();
    }

    #[test]
    fn fuente_elegida_y_detectada() {
        let tmp = std::env::temp_dir().join("editor_pdf_test_fuentes.pdf");
        crea_pdf(&["Texto base"], &tmp); // crea_pdf usa Helvetica
        let work = tmp.to_string_lossy().into_owned();

        // fuente automática primero (solo hay Helvetica en la página, sin
        // empates): debe detectar la dominante
        add_text_block(work.clone(), 0, 60.0, 400.0, "Detectada".into(), 12.0, None, None, None)
            .expect("añadir automática");
        let blocks = get_text_blocks(work.clone(), 0).expect("listar");
        let auto = blocks
            .iter()
            .find(|b| b.text.contains("Detectada"))
            .expect("bloque automático");
        // la builtin de PDFium se llama «Arial» o «Chrom Sans OTF» según el
        // build: normaliza_familia la devuelve siempre como Helvetica
        assert_eq!(auto.font_family, "Helvetica", "familia detectada");

        // fuente elegida a mano
        add_text_block(
            work.clone(),
            0,
            60.0,
            200.0,
            "Con serifa".into(),
            14.0,
            Some("Times Bold".into()),
            None,
            None,
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
            None,
            None,
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

    /// Acrobat escribe el texto derecho tal como se ve la página y donde se
    /// pulsa, también si está girada. Vitela lo colocaba con
    /// `page.height()` (ya rotada) y sin girar el objeto: el texto salía
    /// tumbado y a 246 pt del clic.
    #[test]
    fn el_texto_nuevo_sale_derecho_y_donde_se_pulsa_en_una_pagina_girada() {
        for veces in 1..4u8 {
            let grados = veces as u32 * 90;
            let pdf = std::env::temp_dir().join(format!("texto-girada-{veces}-test.pdf"));
            crea_pdf(&["Fondo"], &pdf);
            let work = pdf.to_string_lossy().into_owned();
            for _ in 0..veces {
                crate::paginas::rotate_page(work.clone(), 0).expect("girar");
            }
            let s = &crate::get_page_sizes(work.clone()).expect("tamaños")[0];
            // clic en el centro de la página VISTA, convertido como hace la UI
            let (vx, vy) = (s.width / 2.0, s.height / 2.0);
            let (px, py) = match s.rotation {
                90 => (vy, s.width - vx),
                180 => (s.width - vx, s.height - vy),
                270 => (s.height - vy, vx),
                _ => (vx, vy),
            };
            add_text_block(work.clone(), 0, px, py, "NUEVO".into(), 24.0, None, None, None)
                .expect("añadir texto");

            let bloques = get_text_blocks(work.clone(), 0).expect("bloques");
            let nuevo = bloques
                .iter()
                .find(|b| b.text.contains("NUEVO"))
                .expect("el bloque nuevo");
            // la caja vuelve en el espacio propio: se pasa a la vista como
            // hace `rectAVista` en la UI y tiene que caer donde se pulsó
            let (bx, by, bw, bh) = match s.rotation {
                90 => (s.width - (nuevo.y + nuevo.h), nuevo.x, nuevo.h, nuevo.w),
                180 => (
                    s.width - (nuevo.x + nuevo.w),
                    s.height - (nuevo.y + nuevo.h),
                    nuevo.w,
                    nuevo.h,
                ),
                270 => (nuevo.y, s.height - (nuevo.x + nuevo.w), nuevo.h, nuevo.w),
                _ => (nuevo.x, nuevo.y, nuevo.w, nuevo.h),
            };
            assert!(
                (bx - vx).abs() < 10.0 && (by - vy).abs() < 10.0,
                "con /Rotate {grados} el texto se ve en ({bx:.1},{by:.1}) y se pulsó en ({vx:.1},{vy:.1})"
            );
            // y se lee derecho: en la vista es más ancho que alto
            assert!(
                bw > bh * 1.5,
                "con /Rotate {grados} el texto sale tumbado: {bw:.1}x{bh:.1} en la vista"
            );
            std::fs::remove_file(&pdf).ok();
        }
    }

    /// Lo mismo con una imagen: en una página girada tiene que salir
    /// derecha y con su esquina superior izquierda donde se pulsó.
    #[test]
    fn la_imagen_nueva_sale_derecha_en_una_pagina_girada() {
        // a 90° y a 270°: el 270 era la deuda de R2, la ruta de escritura
        // más delicada y la única sin juez
        for veces in [1u8, 3] {
            let pdf = std::env::temp_dir().join(format!("texto-imagen-girada-{veces}.pdf"));
            let png = std::env::temp_dir().join(format!("texto-imagen-girada-{veces}.png"));
            crea_pdf(&["Fondo"], &pdf);
            image::RgbaImage::from_pixel(120, 40, image::Rgba([200, 30, 30, 255]))
                .save(&png)
                .expect("crear png");
            let work = pdf.to_string_lossy().into_owned();
            for _ in 0..veces {
                crate::paginas::rotate_page(work.clone(), 0).expect("girar");
            }
            let s = &crate::get_page_sizes(work.clone()).expect("tamaños")[0];
            assert_eq!(s.rotation, veces as u16 * 90);
            // lo que hace la UI: pasar el punto de la vista al espacio
            // propio de la página antes de mandarlo
            let a_pagina = |x: f32, y: f32| match s.rotation {
                90 => (y, s.width - x),
                180 => (s.width - x, s.height - y),
                270 => (s.height - y, x),
                _ => (x, y),
            };
            let (vx, vy) = (100.0f32, 150.0f32);
            let (px, py) = a_pagina(vx, vy);

            crate::imagenes::add_image(
                work.clone(),
                0,
                png.to_string_lossy().into_owned(),
                px,
                py,
            )
            .expect("insertar imagen");

            let img = &crate::imagenes::get_images(work.clone(), 0).expect("imágenes")[0];
            // 120x40 px a 72 dpi son 120x40 pt en la VISTA: en el espacio
            // propio de una página girada eso es 40 de ancho por 120 de alto
            assert!(
                (img.w - 40.0).abs() < 2.0 && (img.h - 120.0).abs() < 2.0,
                "a {}° la imagen sale tumbada: {:.1}x{:.1}",
                s.rotation,
                img.w,
                img.h
            );
            // el centro, que no depende de por qué esquina se ancle
            let centro = a_pagina(vx + 60.0, vy + 20.0);
            assert!(
                ((img.x + img.w / 2.0) - centro.0).abs() < 2.0
                    && ((img.y + img.h / 2.0) - centro.1).abs() < 2.0,
                "a {}° el centro queda en ({:.1},{:.1}) y tenía que ser ({:.1},{:.1})",
                s.rotation,
                img.x + img.w / 2.0,
                img.y + img.h / 2.0,
                centro.0,
                centro.1
            );
            std::fs::remove_file(&pdf).ok();
            std::fs::remove_file(&png).ok();
        }
    }

    /// Buscar y reemplazar: el lote entero es UNA mutación (un ⌘Z lo
    /// devuelve), y una coincidencia cuyo bloque ha cambiado entre la
    /// búsqueda y el reemplazo se salta sin romper el resto.
    #[test]
    fn reemplazar_todo_es_un_solo_paso_y_se_salta_lo_que_ya_no_esta() {
        let pdf = std::env::temp_dir().join("texto-reemplazar-test.pdf");
        crate::tests::crea_pdf(&["Vitela edita Vitela", "Vitela firma"], &pdf);
        let work = pdf.to_string_lossy().into_owned();
        let pasos = |w: &str| {
            crate::historial::history_state(w.to_string())
                .expect("historial")
                .undo
        };

        // la búsqueda con contexto trae el bloque donde cae cada una
        let coincidencias =
            crate::busqueda::search_pdf(work.clone(), "Vitela".into(), None, None, Some(true))
                .expect("buscar");
        assert_eq!(coincidencias.len(), 3, "tres veces en dos páginas");
        assert!(
            coincidencias.iter().all(|m| m.block_index.is_some()),
            "sin bloque no se puede reemplazar"
        );
        assert!(
            coincidencias[0].after.contains("edita"),
            "el contexto de la lista: {:?}",
            coincidencias[0].after
        );

        let lote: Vec<Reemplazo> = coincidencias
            .iter()
            .map(|m| Reemplazo {
                page_index: m.page_index,
                block_index: m.block_index.unwrap(),
                from: "Vitela".into(),
                to: "Pergamino".into(),
            })
            .collect();
        let antes = pasos(&work);
        let informe = replace_text(work.clone(), lote).expect("reemplazar");
        assert_eq!(informe.hechas, 3);
        assert_eq!(informe.saltadas, 0);
        assert_eq!(pasos(&work), antes + 1, "reemplazar todo es UN paso");

        let texto_de = |p: u16| -> String {
            crate::busqueda::get_page_text(work.clone(), p)
                .expect("texto")
                .chars
                .iter()
                .map(|c| c.ch.as_str())
                .collect()
        };
        assert!(!texto_de(0).contains("Vitela"), "{}", texto_de(0));
        assert_eq!(texto_de(0).matches("Pergamino").count(), 2);
        assert!(texto_de(1).contains("Pergamino"));
        assert!(
            crate::busqueda::search_pdf(work.clone(), "Vitela".into(), None, None, None)
                .expect("buscar")
                .is_empty(),
            "ya no queda ninguna"
        );

        // ⌘Z devuelve las tres de una vez
        crate::historial::undo(work.clone()).expect("deshacer");
        assert_eq!(
            crate::busqueda::search_pdf(work.clone(), "Vitela".into(), None, None, None)
                .expect("buscar")
                .len(),
            3
        );

        // una coincidencia que ya no dice lo que decía se salta, y las
        // demás se hacen igual
        let lote = vec![
            Reemplazo {
                page_index: 0,
                block_index: 0,
                from: "Vitela".into(),
                to: "Pergamino".into(),
            },
            Reemplazo {
                page_index: 1,
                block_index: 0,
                from: "Lo que ya no está".into(),
                to: "Nada".into(),
            },
        ];
        let informe = replace_text(work.clone(), lote).expect("reemplazar");
        assert_eq!(informe.hechas, 1, "una hecha");
        assert_eq!(informe.saltadas, 1, "y la otra contada como saltada");
        assert!(texto_de(1).contains("Vitela"), "la página 2 no se ha tocado");
        std::fs::remove_file(&pdf).ok();
    }
    /// Color y alineación del texto: la barra de propiedades de Acrobat.
    /// La alineación no es un operador del PDF, es dónde se coloca el
    /// origen del objeto; el color se ve en el render.
    #[test]
    fn el_texto_nuevo_puede_ir_en_color_y_alineado() {
        let tmp = std::env::temp_dir().join("texto-color-align-test.pdf");
        crea_pdf(&["Base"], &tmp);
        let work = tmp.to_string_lossy().into_owned();
        let ancla = 300.0f32;
        for (align, etiqueta) in [
            (Some("izq".to_string()), "Izquierda"),
            (Some("centro".to_string()), "Centrada"),
            (Some("der".to_string()), "Derecha"),
        ] {
            add_text_block(
                work.clone(),
                0,
                ancla,
                100.0 + 40.0 * (etiqueta.len() as f32 % 3.0),
                etiqueta.into(),
                14.0,
                None,
                Some([200, 20, 20, 255]),
                align,
            )
            .expect("añadir texto");
        }
        let bloques = get_text_blocks(work.clone(), 0).expect("listar");
        let de = |t: &str| {
            bloques
                .iter()
                .find(|b| b.text.contains(t))
                .unwrap_or_else(|| panic!("falta el bloque {t}"))
        };
        let izq = de("Izquierda");
        let centro = de("Centrada");
        let der = de("Derecha");
        assert!(
            (izq.x - ancla).abs() < 2.0,
            "«izq» empieza en el punto: {}",
            izq.x
        );
        assert!(
            ((centro.x + centro.w / 2.0) - ancla).abs() < 3.0,
            "«centro» se centra en el punto: {} + {}",
            centro.x,
            centro.w
        );
        assert!(
            ((der.x + der.w) - ancla).abs() < 3.0,
            "«der» acaba en el punto: {} + {}",
            der.x,
            der.w
        );

        // el color se ve: hay rojo en el render donde antes había papel
        let png = crate::render_page_png(work.clone(), 0, 600, true).expect("render");
        let img = image::load_from_memory(&png).expect("PNG").to_rgba8();
        let rojos = img
            .pixels()
            .filter(|p| p.0[0] > 150 && p.0[1] < 110 && p.0[2] < 110)
            .count();
        assert!(rojos > 50, "el texto rojo apenas pinta ({rojos} píxeles)");

        // y corregir un bloque puede cambiarle el color sin tocar el texto
        let idx = izq.object_index;
        edit_text_block(
            work.clone(),
            0,
            idx,
            "Izquierda".into(),
            Some([20, 20, 200, 255]),
            None,
        )
        .expect("recolorear");
        let png = crate::render_page_png(work.clone(), 0, 600, true).expect("render");
        let img = image::load_from_memory(&png).expect("PNG").to_rgba8();
        let azules = img
            .pixels()
            .filter(|p| p.0[2] > 150 && p.0[0] < 110 && p.0[1] < 110)
            .count();
        assert!(azules > 50, "el texto no se ha puesto azul ({azules})");
        std::fs::remove_file(&tmp).ok();
    }

    /// Corregir un bloque centrado lo deja centrado: el texto nuevo crece
    /// hacia los dos lados, no solo hacia la derecha.
    #[test]
    fn corregir_un_bloque_centrado_lo_deja_centrado() {
        let tmp = std::env::temp_dir().join("texto-align-edit-test.pdf");
        crea_pdf(&["Base"], &tmp);
        let work = tmp.to_string_lossy().into_owned();
        add_text_block(
            work.clone(),
            0,
            300.0,
            200.0,
            "Centrado".into(),
            14.0,
            None,
            None,
            Some("centro".into()),
        )
        .expect("añadir");
        let b = get_text_blocks(work.clone(), 0)
            .expect("listar")
            .into_iter()
            .find(|b| b.text.contains("Centrado"))
            .expect("bloque");
        let centro_antes = b.x + b.w / 2.0;
        edit_text_block(
            work.clone(),
            0,
            b.object_index,
            "Un texto bastante más largo".into(),
            None,
            Some("centro".into()),
        )
        .expect("corregir");
        let b = get_text_blocks(work.clone(), 0)
            .expect("listar")
            .into_iter()
            .find(|b| b.text.contains("bastante"))
            .expect("bloque corregido");
        assert!(
            ((b.x + b.w / 2.0) - centro_antes).abs() < 3.0,
            "el bloque centrado se ha desplazado: {} -> {}",
            centro_antes,
            b.x + b.w / 2.0
        );
        std::fs::remove_file(&tmp).ok();
    }
}
