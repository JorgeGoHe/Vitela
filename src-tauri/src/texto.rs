//! Edición real de texto: bloques del content stream, fuentes y texto nuevo.

use crate::historial::mutacion;
use crate::{on_pdfium_thread, pdfium, save_and_close, with_doc, with_lopdf};
use pdfium_render::prelude::*;
use serde::Serialize;

#[derive(Serialize, Clone, Debug)]
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
    /// **Corregir este bloque conserva su letra**: su fuente va incrustada
    /// en el documento, o es una de las catorce que cualquier visor tiene.
    ///
    /// Con `false`, reescribirlo cambia el aspecto: la fuente no está
    /// dentro del fichero y hay que sustituirla por la más parecida que
    /// haya (`fuente_por_nombre`). Eso se puede hacer, pero **hay que
    /// decirlo antes**, no después: el usuario corregía una errata y le
    /// cambiaba el tipo de letra del párrafo sin que nadie le avisara.
    pub reescribible: bool,
}

/// Las catorce fuentes estándar del PDF: cualquier visor las tiene, así que
/// un documento no las incrusta y reescribir su texto no cambia nada de lo
/// que se ve.
fn es_estandar(familia: &str) -> bool {
    let f = familia.to_lowercase();
    ["helvetica", "arial", "times", "courier", "symbol", "zapf"]
        .iter()
        .any(|n| f.contains(n))
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
    if !n.contains("helvetica")
        && !n.contains("arial")
        && !n.contains("chrom sans")
        && !n.is_empty()
    {
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
            // el tamaño que se VE: `unscaled_font_size` es el /Tf y se
            // queda corto en cuanto el objeto lleva escala en su matriz,
            // que es justo lo que hace `resize_text_block`
            font_size: t.scaled_font_size().value,
            font_family: normaliza_familia(&cruda),
            negrita: estilo.0,
            cursiva: estilo.1,
            color: [c.red(), c.green(), c.blue(), c.alpha()],
            // ante la duda, que no salte el aviso: acusar en falso a un
            // texto de que va a cambiar de letra es peor que callarse
            reescribible: fuente.is_embedded().unwrap_or(true) || es_estandar(&cruda),
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
    let negrita = n.contains("bold")
        || n.contains("negrita")
        || n.contains("black")
        || n.contains("heavy")
        || n.contains("semibold");
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

/// Lo que se sabe de una corrección, para que la UI pueda repintar la caja
/// y avisar sin tener que volver a preguntar.
#[derive(Serialize, Debug, Default)]
pub struct InformeEdicion {
    /// Cuántas líneas tiene el párrafo después de corregirlo.
    pub lineas: u16,
    /// El párrafo ha crecido tanto que la última línea cae **fuera del
    /// papel**. Escribir fuera en silencio es lo que no puede pasar.
    pub se_sale: bool,
    /// Se ha repartido el texto por el ancho del párrafo (reflujo) o se ha
    /// hecho lo de siempre: la primera línea encima del objeto y las demás
    /// como objetos nuevos debajo.
    pub reflujo: bool,
}

/// Edición real de texto: reescribe el objeto de texto del content stream.
/// Mantiene la fuente del objeto (si la fuente embebida no tiene los glifos
/// del texto nuevo, esos caracteres no se verán).
///
/// **`new_text` es siempre el texto del bloque tocado**, la línea, y nunca
/// el del párrafo entero: es lo que la UI tiene en su cuadro de edición
/// (AC-061). Con `reflow` en `true`, el backend reconoce él el párrafo que
/// cuelga de ese bloque (`parrafo_de`), sustituye **solo esa línea** por el
/// texto nuevo y recoloca el conjunto al ancho de la columna, como en
/// «Editar PDF» de Acrobat. El resto del párrafo no se pierde nunca.
///
/// Sin `reflow` —que es el defecto: `reflow.unwrap_or(false)`— se hace lo
/// de siempre: la primera línea reemplaza al objeto original y las demás
/// se insertan como objetos nuevos colocados debajo. El defecto es el
/// conservador **a propósito**: `reflow` cambia lo que significan los
/// demás argumentos, y una bandera así no puede traer puesto el
/// comportamiento que toca lo que no se le ha pedido.
///
/// **El reflujo no cruza bloques ni páginas**: si el párrafo crece, el
/// contenido siguiente no se mueve. Acrobat tampoco lo hace, y prometerlo
/// sería prometer un procesador de textos.
#[tauri::command(async)]
#[allow(clippy::too_many_arguments)]
pub fn edit_text_block(
    work_path: String,
    page_index: u16,
    object_index: u32,
    new_text: String,
    color: Option<[u8; 4]>,
    align: Option<String>,
    line_height: Option<f32>,
    char_spacing: Option<f32>,
    reflow: Option<bool>,
) -> Result<InformeEdicion, String> {
    let tc = espaciado(char_spacing);
    mutacion(work_path, move |work_path| {
        on_pdfium_thread(move || {
            // el párrafo lo reconoce el backend (`parrafo_de`), que es quien
            // tiene la geometría: la UI solo dice si quiere que se recoloque.
            // Sin pedirlo no se refluye (AC-061): la bandera cambia lo que
            // significan los demás argumentos
            if reflow.unwrap_or(false) {
                return refluye(
                    &work_path,
                    page_index,
                    object_index,
                    &new_text,
                    color,
                    align.as_deref(),
                    line_height,
                    tc,
                );
            }
            edita_sin_reflujo(
                &work_path,
                page_index,
                object_index,
                &new_text,
                color,
                align,
                line_height,
                tc,
            )
        })
    })
}

/// El camino de siempre: la primera línea encima del objeto y las demás
/// como objetos nuevos debajo.
#[allow(clippy::too_many_arguments)]
fn edita_sin_reflujo(
    work_path: &str,
    page_index: u16,
    object_index: u32,
    new_text: &str,
    color: Option<[u8; 4]>,
    align: Option<String>,
    line_height: Option<f32>,
    tc: Option<f32>,
) -> Result<InformeEdicion, String> {
    let work_path = work_path.to_string();
    let new_text = new_text.to_string();
    {
        let pdfium = pdfium()?;
        let mut doc = pdfium
            .load_pdf_from_file(&work_path, None)
            .map_err(|e| e.to_string())?;
        // los bloques a los que hay que escribirles el `Tc`, por su ordinal
        // entre los objetos de texto de la página (ver `escribe_espaciado`)
        let mut ordinales: Vec<usize> = Vec::new();
        let mut lineas = new_text.lines();
        let primera = lineas.next().unwrap_or("").to_string();
        let resto: Vec<String> = lineas.map(|l| l.to_string()).collect();

        // 1) reescribir la primera línea y leer familia/tamaño/posición
        let (familia, font_size, base_x, base_y, color_viejo) = {
            let mut page = doc.pages().get(page_index).map_err(|e| e.to_string())?;
            if tc.is_some() {
                ordinales.push(ordinal_de_texto(&page, object_index as usize));
            }
            let mut obj = page
                .objects_mut()
                .get(object_index as usize)
                .map_err(|e| e.to_string())?;
            let bounds = obj.bounds().map_err(|e| e.to_string())?;
            // el color que ya tiene, para que las líneas 2..n no salgan
            // negras cuando no se pide color: corregir un párrafo rojo de
            // dos líneas dejaba la primera roja y la segunda negra
            let previo = obj
                .fill_color()
                .map(|c| [c.red(), c.green(), c.blue(), c.alpha()])
                .unwrap_or([0, 0, 0, 255]);
            let t = obj.as_text_object_mut().ok_or("No es un bloque de texto")?;
            let info = (
                t.font().family().to_lowercase(),
                t.unscaled_font_size(),
                bounds.left(),
                bounds.bottom(),
                previo,
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
            let line_h = font_size.value * interlineado(line_height);
            let [r, g, b, a] = color.unwrap_or(color_viejo);
            let mut anadidas = 0usize;
            for (i, linea) in resto.iter().enumerate() {
                if linea.trim().is_empty() {
                    continue;
                }
                let mut nuevo = PdfPageTextObject::new(&doc, linea, token, font_size)
                    .map_err(|e| e.to_string())?;
                nuevo
                    .set_fill_color(PdfColor::new(r, g, b, a))
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
                anadidas += 1;
            }
            page.regenerate_content().map_err(|e| e.to_string())?;
            // las líneas nuevas se han añadido al final de la página, así
            // que son los últimos objetos de texto
            if tc.is_some() && anadidas > 0 {
                let total = cuantos_textos(&page);
                ordinales.extend((total - anadidas)..total);
            }
            drop(page);
        }
        save_and_close(doc, &work_path)?;
        // el espaciado entre caracteres, en un segundo pase con lopdf:
        // pdfium-render 0.8 no expone el estado de texto del objeto
        if let Some(tc) = tc {
            crate::cirugia_en_hilo(&work_path, |doc| {
                escribe_espaciado(doc, page_index, &ordinales, tc)
            })?;
        }
        Ok(InformeEdicion {
            lineas: new_text.lines().count().max(1) as u16,
            se_sale: false,
            reflujo: false,
        })
    }
}

/// Una línea del párrafo: dónde está y de qué tamaño, en el espacio del
/// PDF (que es donde hay que colocarla).
struct LineaDelParrafo {
    indice: usize,
    texto: String,
    izq: f32,
    der: f32,
    arriba: f32,
    /// La `y` del **origen del objeto**, que es la línea base del texto:
    /// la caja de los glifos sube y baja con las mayúsculas y los rabos de
    /// las letras, y la línea base no. Es la que hay que respetar para
    /// recolocar un párrafo sin que las líneas bailen.
    base_y: f32,
    size: f32,
}

/// Los datos de todos los objetos de texto de la página, en el orden de la
/// página, con sus cajas **sin voltear** (coordenadas del PDF).
fn lineas_de_la_pagina(page: &PdfPage) -> Vec<LineaDelParrafo> {
    let objetos = page.objects();
    (0..objetos.len())
        .filter_map(|i| {
            let obj = objetos.get(i).ok()?;
            let t = obj.as_text_object()?;
            if t.text().trim().is_empty() {
                return None;
            }
            let b = obj.bounds().ok()?;
            Some(LineaDelParrafo {
                indice: i,
                texto: t.text(),
                izq: b.left().value,
                der: b.right().value,
                arriba: b.top().value,
                base_y: obj.get_vertical_translation().value,
                size: t.scaled_font_size().value,
            })
        })
        .collect()
}

/// Las líneas del párrafo que empieza en `object_index`, **hacia abajo**:
/// las que comparten columna (mismo borde izquierdo, mismo centro o mismo
/// borde derecho, según cómo esté alineado), tienen el mismo cuerpo de
/// letra y van una debajo de otra a una distancia de interlineado.
///
/// En un PDF no hay párrafos: hay trozos de texto colocados en un papel.
/// Esta es la misma heurística con la que cualquier editor los reconoce, y
/// por eso es la que decide hasta dónde llega el reflujo.
fn parrafo_de(page: &PdfPage, object_index: usize) -> Vec<LineaDelParrafo> {
    let mut lineas = lineas_de_la_pagina(page);
    let Some(pos) = lineas.iter().position(|l| l.indice == object_index) else {
        return Vec::new();
    };
    let base = lineas.remove(pos);
    let tol = (base.size * 0.6).max(1.5);
    let centro = |l: &LineaDelParrafo| (l.izq + l.der) / 2.0;
    let (base_izq, base_der, base_centro) = (base.izq, base.der, centro(&base));
    let alineada = |l: &LineaDelParrafo| {
        (l.izq - base_izq).abs() < tol
            || (centro(l) - base_centro).abs() < tol
            || (l.der - base_der).abs() < tol
    };
    let mut grupo = vec![base];
    loop {
        let ultima = grupo.last().unwrap();
        let siguiente = lineas
            .iter()
            .filter(|l| l.arriba < ultima.arriba - 0.5)
            .filter(|l| (l.size - ultima.size).abs() <= ultima.size * 0.25)
            .filter(|l| alineada(l))
            // el hueco entre líneas: más de dos cuerpos y medio ya es otro
            // párrafo, no la línea siguiente
            .filter(|l| ultima.arriba - l.arriba < ultima.size * 2.6)
            .max_by(|a, b| a.arriba.total_cmp(&b.arriba));
        let Some(elegida) = siguiente.map(|l| l.indice) else {
            break;
        };
        let pos = lineas.iter().position(|l| l.indice == elegida).unwrap();
        grupo.push(lineas.remove(pos));
    }
    grupo
}

/// Corrige un párrafo **recolocándolo entero**: `new_text` es el texto de
/// la línea tocada, el resto del párrafo se lee de la página, y el conjunto
/// se reparte al ancho de la columna reescribiendo las líneas que ya había
/// y creando o quitando las que hagan falta.
///
/// Que `new_text` sea la línea y no el párrafo es el contrato de AC-061:
/// hasta el ciclo 6 se entendía como el párrafo entero y las líneas de
/// abajo se borraban sin avisar en cuanto la UI mandaba —correctamente— el
/// texto de un solo bloque.
#[allow(clippy::too_many_arguments)]
fn refluye(
    work_path: &str,
    page_index: u16,
    object_index: u32,
    new_text: &str,
    color: Option<[u8; 4]>,
    align: Option<&str>,
    line_height: Option<f32>,
    tc: Option<f32>,
) -> Result<InformeEdicion, String> {
    let pdfium = pdfium()?;
    let mut doc = pdfium
        .load_pdf_from_file(work_path, None)
        .map_err(crate::mensaje_llano)?;
    let mut ordinales: Vec<usize> = Vec::new();
    let (lineas_finales, se_sale) = {
        let mut page = doc.pages().get(page_index).map_err(crate::mensaje_llano)?;
        let parrafo = parrafo_de(&page, object_index as usize);
        if parrafo.is_empty() {
            return Err("Ese bloque de texto ya no está en la página".into());
        }
        let size = parrafo[0].size;
        // la columna: el ancho es el de la línea más larga, que es lo que
        // el ojo lee como el ancho del párrafo (la última siempre es corta)
        let col_izq = parrafo.iter().map(|l| l.izq).fold(f32::MAX, f32::min);
        let col_der = parrafo.iter().map(|l| l.der).fold(f32::MIN, f32::max);
        // el ancho de la columna es el de la caja **o el que midan con los
        // AFM las líneas que ya hay**, el que sea mayor: la fuente del PDF
        // no es exactamente la que se mide, y sin esta holgura una línea
        // que no ha cambiado se partiría en dos por un error del 5 %
        let medido = parrafo
            .iter()
            .map(|l| crate::anotaciones2::ancho_helvetica(&l.texto, size))
            .fold(0.0f32, f32::max);
        // AC-097: un bloque de una sola línea no tiene hermanos que
        // marquen la columna, así que el ancho del texto viejo no puede
        // ser el ancho del párrafo —casi cualquier añadido se saldría—.
        // Sin hermanos se usa el papel que queda a la derecha, con el
        // mismo margen que hay a la izquierda, que es lo que hace Acrobat
        // cuando el bloque está solo
        let ancho_util = if parrafo.len() == 1 {
            let geo = crate::Geo::de_pagina(&page).propia();
            (geo.ancho() - col_izq - (col_izq - geo.izq()).max(0.0)).max(size * 2.0)
        } else {
            0.0
        };
        let ancho = (col_der - col_izq)
            .max(medido)
            .max(ancho_util)
            .max(size * 2.0);
        // el interlineado: el que se mide entre las líneas que ya hay, que
        // es el del párrafo. Si la UI pide uno, manda el suyo
        let interlineado_medido = match (line_height, parrafo.len() > 1) {
            (Some(lh), _) => size * interlineado(Some(lh)),
            (None, true) => {
                let total: f32 = parrafo.windows(2).map(|p| p[0].base_y - p[1].base_y).sum();
                total / (parrafo.len() - 1) as f32
            }
            (None, false) => size * interlineado(None),
        };
        let familia = {
            let objetos = page.objects();
            objetos
                .get(object_index as usize)
                .ok()
                .and_then(|o| o.as_text_object().map(|t| t.font().family().to_lowercase()))
                .unwrap_or_default()
        };
        let color_viejo = {
            let objetos = page.objects();
            objetos
                .get(object_index as usize)
                .ok()
                .and_then(|o| o.fill_color().ok())
                .map(|c| [c.red(), c.green(), c.blue(), c.alpha()])
                .unwrap_or([0, 0, 0, 255])
        };
        let [r, g, b, a] = color.unwrap_or(color_viejo);

        // el párrafo entero: la línea tocada con su texto nuevo y las de
        // abajo tal como están. Sin esto —con `new_text` entendido como el
        // párrafo entero— corregir una línea borraba las demás (AC-061)
        let texto_parrafo = std::iter::once(new_text.to_string())
            .chain(parrafo.iter().skip(1).map(|l| l.texto.clone()))
            .collect::<Vec<String>>()
            .join(" ");

        // el texto nuevo, repartido al ancho de la columna con los anchos
        // AFM (Helvetica): en una fuente más estrecha las líneas rompen un
        // poco antes, que es el error que no se sale del papel
        let nuevas: Vec<String> = crate::anotaciones2::parte_lineas(&texto_parrafo, size, ancho)
            .into_iter()
            .filter(|l| !l.trim().is_empty())
            .collect();
        if nuevas.is_empty() {
            return Err("El texto está vacío".into());
        }

        // dónde va cada línea: el borde de la columna que manda la
        // alineación, y la altura de la que había o, si es nueva, la
        // siguiente del interlineado
        let coloca = |ancho_linea: f32| match align.unwrap_or("izq") {
            "centro" | "center" => (col_izq + col_der) / 2.0 - ancho_linea / 2.0,
            "der" | "derecha" | "right" => col_der - ancho_linea,
            _ => col_izq,
        };

        // 1) las que se reescriben encima de las que ya había
        let reutilizadas = nuevas.len().min(parrafo.len());
        for (i, texto) in nuevas.iter().take(reutilizadas).enumerate() {
            let objetos = page.objects_mut();
            let mut obj = objetos
                .get(parrafo[i].indice)
                .map_err(crate::mensaje_llano)?;
            {
                let t = obj
                    .as_text_object_mut()
                    .ok_or("Ese bloque ya no es texto")?;
                t.set_text(texto).map_err(crate::mensaje_llano)?;
                t.set_fill_color(PdfColor::new(r, g, b, a))
                    .map_err(crate::mensaje_llano)?;
            }
            let caja = obj.bounds().map_err(crate::mensaje_llano)?;
            let dx = coloca(caja.right().value - caja.left().value) - caja.left().value;
            // la línea vuelve a su sitio del párrafo: la base de la primera
            // manda y las de abajo van a su interlineado, medido entre
            // líneas base y no entre cajas de glifos
            let dy = (parrafo[0].base_y - interlineado_medido * i as f32)
                - obj.get_vertical_translation().value;
            if dx.abs() > 0.01 || dy.abs() > 0.01 {
                obj.translate(PdfPoints::new(dx), PdfPoints::new(dy))
                    .map_err(crate::mensaje_llano)?;
            }
        }
        // el flujo se regenera **antes** de soltar esta vista de la
        // página: `set_text` sin mover nada no la marca, y el
        // `regenerate_content` de la vista de abajo escribía el contenido
        // de antes — corregir una línea sin que cambiara su sitio no se
        // guardaba (AC-097)
        page.regenerate_content().map_err(crate::mensaje_llano)?;
        drop(page);

        // 2) las que sobran del texto nuevo: objetos nuevos debajo, con la
        // fuente estándar aproximada (el handle de la del PDF queda ligado
        // a su página y PDFium casca con handles colgantes)
        let token = if nuevas.len() > parrafo.len() {
            Some(fuente_por_nombre(&mut doc, &familia))
        } else {
            None
        };
        let mut page = doc.pages().get(page_index).map_err(crate::mensaje_llano)?;
        if let Some(token) = token {
            for (k, texto) in nuevas.iter().enumerate().skip(parrafo.len()) {
                let mut obj = PdfPageTextObject::new(&doc, texto, token, PdfPoints::new(size))
                    .map_err(crate::mensaje_llano)?;
                obj.set_fill_color(PdfColor::new(r, g, b, a))
                    .map_err(crate::mensaje_llano)?;
                let ancho_linea = ancho_del_objeto(&obj, texto, size);
                let y = parrafo[0].base_y - interlineado_medido * k as f32;
                obj.translate(PdfPoints::new(coloca(ancho_linea)), PdfPoints::new(y))
                    .map_err(crate::mensaje_llano)?;
                page.objects_mut()
                    .add_text_object(obj)
                    .map_err(crate::mensaje_llano)?;
            }
        }

        // 3) las que sobran del párrafo viejo: fuera, de mayor a menor para
        // no invalidar los índices que quedan
        let mut sobran: Vec<usize> = parrafo
            .iter()
            .skip(nuevas.len())
            .map(|l| l.indice)
            .collect();
        sobran.sort_unstable();
        for indice in sobran.iter().rev() {
            let quitado = page
                .objects_mut()
                .remove_object_at_index(*indice)
                .map_err(crate::mensaje_llano)?;
            // su Drop llama a FPDFPageObj_Destroy y PDFium casca: regla del
            // proyecto, nunca se suelta un objeto sacado
            std::mem::forget(quitado);
        }
        page.regenerate_content().map_err(crate::mensaje_llano)?;

        // el ordinal de cada línea entre los objetos de texto, para el `Tc`
        if tc.is_some() {
            for (i, _) in nuevas.iter().enumerate() {
                let indice = if i < reutilizadas {
                    let viejo = parrafo[i].indice;
                    viejo - sobran.iter().filter(|s| **s < viejo).count()
                } else {
                    // los nuevos se han añadido al final
                    page.objects().len() - (nuevas.len() - i)
                };
                ordinales.push(ordinal_de_texto(&page, indice));
            }
        }

        // ¿se ha salido del papel? La última línea es la que se cae
        let ultima_y = parrafo[0].base_y - interlineado_medido * (nuevas.len() - 1) as f32;
        let se_sale = ultima_y < 0.0;
        drop(page);
        (nuevas.len() as u16, se_sale)
    };
    save_and_close(doc, work_path)?;
    if let Some(tc) = tc {
        crate::cirugia_en_hilo(work_path, |doc| {
            escribe_espaciado(doc, page_index, &ordinales, tc)
        })?;
    }
    Ok(InformeEdicion {
        lineas: lineas_finales,
        se_sale,
        reflujo: true,
    })
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
    mutacion(work_path, move |work_path| {
        on_pdfium_thread(move || {
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
        })
    })
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
    line_height: Option<f32>,
    char_spacing: Option<f32>,
) -> Result<(), String> {
    if text.trim().is_empty() {
        return Err("El texto está vacío".into());
    }
    let font_size = font_size.clamp(6.0, 96.0);
    let tc = espaciado(char_spacing);
    mutacion(work_path, move |work_path| {
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
            // el punto llega en el espacio PROPIO de la página; los ejes de la
            // vista dicen hacia dónde se lee, para que en una página girada el
            // texto salga derecho y no tumbado (como hace add_stamp)
            let vista = crate::Geo::de_pagina(&page);
            let rot = vista.rot;
            let (derecha, abajo) = vista.ejes();
            let ancla = vista.propia().ui_a_pdf(x, y);
            let line_h = font_size * interlineado(line_height);
            let mut anadidas = 0usize;
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
                anadidas += 1;
            }
            page.regenerate_content().map_err(|e| e.to_string())?;
            // los objetos nuevos van al final de la página: son los últimos
            // bloques de texto del content stream
            let ordinales: Vec<usize> = if tc.is_some() && anadidas > 0 {
                let total = cuantos_textos(&page);
                ((total - anadidas)..total).collect()
            } else {
                Vec::new()
            };
            drop(page);
            save_and_close(doc, &work_path)?;
            if let Some(tc) = tc {
                crate::cirugia_en_hilo(&work_path, |doc| {
                    escribe_espaciado(doc, page_index, &ordinales, tc)
                })?;
            }
            Ok(())
        })
    })
}

/// El interlineado, como en un procesador de textos: 1 es el simple, 1,5 el
/// de siempre en un documento. Sin él, el 1,2 de toda la vida (que es lo
/// que hacía Vitela). En un PDF no hay párrafos: las líneas son objetos
/// distintos, así que el interlineado es la distancia a la que se coloca
/// cada uno.
fn interlineado(line_height: Option<f32>) -> f32 {
    line_height.unwrap_or(1.2).clamp(0.6, 4.0)
}

/// El espaciado entre caracteres pedido, ya acotado, o `None` cuando no hay
/// nada que escribir (el operador `Tc` por defecto es 0 y PDFium no lo
/// emite: no escribirlo es exactamente lo mismo que escribir `0 Tc`).
fn espaciado(char_spacing: Option<f32>) -> Option<f32> {
    match char_spacing {
        Some(v) if v.abs() > 0.001 => Some(v.clamp(-20.0, 100.0)),
        _ => None,
    }
}

/// Cuántos objetos de texto hay en la página antes de `hasta` (excluido).
/// Es el ordinal del bloque entre los bloques de texto, que es lo que
/// cuenta en el content stream: PDFium escribe un `BT … ET` por objeto de
/// texto y en el mismo orden que la lista de objetos de la página.
fn ordinal_de_texto(page: &PdfPage, hasta: usize) -> usize {
    let objetos = page.objects();
    (0..hasta.min(objetos.len()))
        .filter(|i| {
            objetos
                .get(*i)
                .ok()
                .is_some_and(|o| o.as_text_object().is_some())
        })
        .count()
}

/// Cuántos objetos de texto tiene la página en total.
fn cuantos_textos(page: &PdfPage) -> usize {
    ordinal_de_texto(page, page.objects().len())
}

/// Un byte «regular» del content stream: ni espacio ni delimitador. Los
/// tokens (`BT`, `ET`, `Tj`, los números) están hechos de estos.
fn regular(b: u8) -> bool {
    !matches!(
        b,
        b'\0'
            | b'\t'
            | b'\n'
            | 0x0c
            | b'\r'
            | b' '
            | b'('
            | b')'
            | b'<'
            | b'>'
            | b'['
            | b']'
            | b'{'
            | b'}'
            | b'/'
            | b'%'
    )
}

/// Recorre un content stream token a token **sobre los bytes**, y no con
/// `Content::decode`: el analizador de contenido de lopdf 0.34 no entiende
/// las imágenes en línea (`BI … ID … EI`) y en un PDF de fuera se
/// atragantaría. Saltando cadenas, hexadecimales y comentarios, lo demás se
/// conserva byte a byte y esto no puede fallar.
///
/// A `f` le llega cada trozo del stream en orden, con `true` cuando es un
/// token de verdad (`BT`, `Tc`, un número) y `false` cuando es relleno que
/// solo hay que copiar (espacios, cadenas, comentarios).
fn recorre_stream(datos: &[u8], mut f: impl FnMut(&[u8], bool)) {
    recorre_stream_con_pos(datos, |r, es_token| f(&datos[r], es_token));
}

/// Lo mismo que [`recorre_stream`], pero diciendo **dónde** está cada
/// trozo: es lo que hace falta para mover un cacho del flujo de sitio
/// (`imagenes::reorder_image`) en vez de solo reescribirlo.
pub(crate) fn recorre_stream_con_pos(
    datos: &[u8],
    mut f: impl FnMut(std::ops::Range<usize>, bool),
) {
    let mut i = 0usize;
    while i < datos.len() {
        match datos[i] {
            // comentario: hasta el final de la línea
            b'%' => {
                let ini = i;
                while i < datos.len() && datos[i] != b'\n' && datos[i] != b'\r' {
                    i += 1;
                }
                f(ini..i, false);
            }
            // cadena literal, con anidamiento y escapes
            b'(' => {
                let ini = i;
                let mut nivel = 0i32;
                while i < datos.len() {
                    match datos[i] {
                        b'\\' => i += 1,
                        b'(' => nivel += 1,
                        b')' => {
                            nivel -= 1;
                            if nivel == 0 {
                                i += 1;
                                break;
                            }
                        }
                        _ => {}
                    }
                    i += 1;
                }
                i = i.min(datos.len());
                f(ini..i, false);
            }
            // `<<` abre diccionario; `<` solo, una cadena hexadecimal
            b'<' => {
                let ini = i;
                if datos.get(i + 1) == Some(&b'<') {
                    i += 2;
                } else {
                    while i < datos.len() && datos[i] != b'>' {
                        i += 1;
                    }
                    i = (i + 1).min(datos.len());
                }
                f(ini..i, false);
            }
            b if regular(b) => {
                let ini = i;
                while i < datos.len() && regular(datos[i]) {
                    i += 1;
                }
                f(ini..i, true);
            }
            _ => {
                f(i..i + 1, false);
                i += 1;
            }
        }
    }
}

/// Escribe `<tc> Tc` justo detrás del `BT` de los bloques cuyo ordinal está
/// en `ordinales`, y `0 Tc` antes de su `ET` para que el espaciado no se
/// escape al resto de la página (`Tc` es estado gráfico, no del bloque).
///
/// Devuelve el contenido nuevo, cuántos `BT` ha visto y si ha cambiado algo.
fn inserta_tc(datos: &[u8], desde: usize, ordinales: &[usize], tc: f32) -> (Vec<u8>, usize, bool) {
    let mut out = Vec::with_capacity(datos.len() + 32);
    let mut n = desde;
    let mut dentro = false;
    let mut cambiado = false;
    recorre_stream(datos, |trozo, es_token| {
        if es_token && trozo == b"BT" {
            let toca = ordinales.contains(&n);
            n += 1;
            out.extend_from_slice(trozo);
            if toca {
                out.extend_from_slice(format!(" {tc} Tc").as_bytes());
                dentro = true;
                cambiado = true;
            }
        } else if es_token && trozo == b"ET" && dentro {
            out.extend_from_slice(b"0 Tc ");
            out.extend_from_slice(trozo);
            dentro = false;
        } else {
            out.extend_from_slice(trozo);
        }
    });
    (out, n, cambiado)
}

/// El espaciado que lleva escrito el bloque cuyo ordinal es `ordinal`: el
/// operando del **primer** `Tc` que aparece dentro de su `BT … ET` (el
/// segundo es el `0 Tc` con el que se cierra). Devuelve también cuántos
/// `BT` ha visto, para poder encadenar varios streams de la misma página.
fn lee_tc(datos: &[u8], desde: usize, ordinal: usize) -> (Option<f32>, usize) {
    let mut n = desde;
    let mut dentro = false;
    let mut ultimo: Option<f32> = None;
    let mut hallado: Option<f32> = None;
    recorre_stream(datos, |trozo, es_token| {
        if !es_token {
            return;
        }
        if trozo == b"BT" {
            dentro = n == ordinal;
            n += 1;
            ultimo = None;
        } else if trozo == b"ET" {
            dentro = false;
        } else if trozo == b"Tc" {
            if dentro && hallado.is_none() {
                hallado = ultimo;
            }
            ultimo = None;
        } else {
            ultimo = std::str::from_utf8(trozo).ok().and_then(|s| s.parse().ok());
        }
    });
    (hallado, n)
}

/// El espaciado entre caracteres que lleva escrito un bloque de la página,
/// o `None` si no lleva ninguno (que es lo mismo que llevar `0 Tc`).
///
/// Es la mitad que le faltaba a [`escribe_espaciado`]: mover o estirar un
/// bloque regenera el content stream con `FPDF_GenerateContent`, que **no
/// vuelve a escribir el `Tc`**, así que hay que leerlo antes y reponerlo
/// después.
pub(crate) fn lee_espaciado(doc: &lopdf::Document, page_index: u16, ordinal: usize) -> Option<f32> {
    let pid = *doc.get_pages().get(&(page_index as u32 + 1))?;
    let mut vistos = 0usize;
    for id in doc.get_page_contents(pid) {
        let Ok(stream) = doc.get_object(id).and_then(lopdf::Object::as_stream) else {
            continue;
        };
        let datos = stream
            .decompressed_content()
            .unwrap_or_else(|_| stream.content.clone());
        let (tc, ahora) = lee_tc(&datos, vistos, ordinal);
        vistos = ahora;
        if let Some(tc) = tc {
            return espaciado(Some(tc));
        }
    }
    None
}

/// Escribe el espaciado entre caracteres de unos bloques de la página.
///
/// pdfium-render 0.8 no expone el estado de texto de un objeto (no hay
/// `set_char_spacing` ni nada equivalente en toda su API), así que el
/// operador se escribe con lopdf en un segundo pase, por el mismo camino
/// con el que `anotaciones::escribe_apariencia_marca` escribe lo que PDFium
/// no escribe. Se hace **stream a stream**, sin refundirlos, para no tocar
/// lo que no hay que tocar.
///
/// Ojo: `FPDF_GenerateContent` **no vuelve a escribir el `Tc`** cuando
/// PDFium regenera el content stream de la página, así que mover o estirar
/// después ese bloque se lleva el espaciado por delante. La UI lo manda en
/// cada corrección, que es cuando el usuario lo elige.
pub(crate) fn escribe_espaciado(
    doc: &mut lopdf::Document,
    page_index: u16,
    ordinales: &[usize],
    tc: f32,
) -> Result<(), String> {
    if ordinales.is_empty() {
        return Ok(());
    }
    let paginas = doc.get_pages();
    let pid = *paginas
        .get(&(page_index as u32 + 1))
        .ok_or("La página no existe")?;
    let mut vistos = 0usize;
    let mut escritos = Vec::new();
    for id in doc.get_page_contents(pid) {
        let Ok(stream) = doc.get_object(id).and_then(lopdf::Object::as_stream) else {
            continue;
        };
        let datos = stream
            .decompressed_content()
            .unwrap_or_else(|_| stream.content.clone());
        let (nuevo, ahora, cambiado) = inserta_tc(&datos, vistos, ordinales, tc);
        vistos = ahora;
        if cambiado {
            escritos.push((id, nuevo));
        }
    }
    for (id, nuevo) in escritos {
        doc.change_content_stream(id, nuevo);
    }
    Ok(())
}

/// Mueve un bloque de texto a un punto de la página, como se arrastra una
/// imagen: el punto es su esquina superior izquierda, **en el espacio
/// propio de la página** (la UI convierte con la `rotation` de
/// `get_page_sizes` antes de mandarlo, como en todos los comandos que
/// escriben).
///
/// Hasta el ciclo 5 el texto se corregía pero no se colocaba: había que
/// borrarlo y volver a escribirlo en otro sitio, perdiendo la fuente.
#[tauri::command(async)]
pub fn move_text_block(
    work_path: String,
    page_index: u16,
    object_index: u32,
    x: f32,
    y: f32,
) -> Result<(), String> {
    mutacion(work_path, move |work_path| {
        on_pdfium_thread(move || {
            let pdfium = pdfium()?;
            let doc = pdfium
                .load_pdf_from_file(&work_path, None)
                .map_err(crate::mensaje_llano)?;
            let mut page = doc.pages().get(page_index).map_err(crate::mensaje_llano)?;
            let geo = crate::Geo::de_pagina(&page).propia();
            // el espaciado que lleva el bloque, ANTES de tocarlo:
            // `FPDF_GenerateContent` regenera el content stream y no vuelve a
            // escribir el `Tc`, así que arrastrar el bloque se lo llevaba por
            // delante y el texto volvía a juntarse solo (R32b)
            let ordinal = ordinal_de_texto(&page, object_index as usize);
            let tc = with_lopdf(&work_path, |d| Ok(lee_espaciado(d, page_index, ordinal)))?;
            let mut obj = page
                .objects_mut()
                .get(object_index as usize)
                .map_err(crate::mensaje_llano)?;
            if obj.as_text_object().is_none() {
                return Err("No es un bloque de texto".into());
            }
            let b = obj.bounds().map_err(|e| e.to_string())?;
            let alto = b.top().value - b.bottom().value;
            // la esquina superior izquierda en coordenadas del papel: la `y` de
            // la UI baja y la del PDF sube
            let (px, py_arriba) = geo.ui_a_pdf(x, y);
            let dx = px - b.left().value;
            let dy = (py_arriba - alto) - b.bottom().value;
            obj.translate(PdfPoints::new(dx), PdfPoints::new(dy))
                .map_err(|e| e.to_string())?;
            drop(obj);
            page.regenerate_content().map_err(|e| e.to_string())?;
            drop(page);
            save_and_close(doc, &work_path)?;
            repon_espaciado(&work_path, page_index, ordinal, tc)
        })
    })
}

/// Estira un bloque de texto a la caja pedida (espacio propio de la
/// página). **No deforma las letras**: escala el tamaño de fuente, que es
/// lo que hace Acrobat al arrastrar un tirador, en proporción al área que
/// se pide —así arrastrar un lado también agranda el bloque, sin que las
/// letras salgan aplastadas—. La esquina superior izquierda se queda donde
/// estaba, que es de donde no se tira.
#[tauri::command(async)]
pub fn resize_text_block(
    work_path: String,
    page_index: u16,
    object_index: u32,
    w: f32,
    h: f32,
) -> Result<(), String> {
    if w < 4.0 || h < 4.0 {
        return Err("El bloque de texto no puede quedarse así de pequeño".into());
    }
    mutacion(work_path, move |work_path| {
        on_pdfium_thread(move || {
            let pdfium = pdfium()?;
            let doc = pdfium
                .load_pdf_from_file(&work_path, None)
                .map_err(crate::mensaje_llano)?;
            let mut page = doc.pages().get(page_index).map_err(crate::mensaje_llano)?;
            // el espaciado del bloque, antes de estirarlo: ver `move_text_block`
            let ordinal = ordinal_de_texto(&page, object_index as usize);
            let tc = with_lopdf(&work_path, |d| Ok(lee_espaciado(d, page_index, ordinal)))?;
            let mut obj = page
                .objects_mut()
                .get(object_index as usize)
                .map_err(crate::mensaje_llano)?;
            if obj.as_text_object().is_none() {
                return Err("No es un bloque de texto".into());
            }
            let b = obj.bounds().map_err(|e| e.to_string())?;
            let (ancho, alto) = (
                b.right().value - b.left().value,
                b.top().value - b.bottom().value,
            );
            if ancho <= 0.0 || alto <= 0.0 {
                return Err("Ese bloque de texto no tiene tamaño".into());
            }
            // escala uniforme por el área pedida: la letra crece igual de ancha
            // que de alta y no se deforma
            let k = ((w / ancho) * (h / alto)).sqrt().clamp(0.1, 20.0);
            let (izq, arriba) = (b.left().value, b.top().value);
            obj.scale(k, k).map_err(|e| e.to_string())?;
            // escalar es respecto del origen del papel: se recoloca por la
            // esquina de la que no se tira
            let b2 = obj.bounds().map_err(|e| e.to_string())?;
            obj.translate(
                PdfPoints::new(izq - b2.left().value),
                PdfPoints::new(arriba - b2.top().value),
            )
            .map_err(|e| e.to_string())?;
            drop(obj);
            page.regenerate_content().map_err(|e| e.to_string())?;
            drop(page);
            save_and_close(doc, &work_path)?;
            repon_espaciado(&work_path, page_index, ordinal, tc)
        })
    })
}

/// Vuelve a escribir el `Tc` que [`lee_espaciado`] había apuntado, en el
/// mismo bloque y dentro de la misma mutación. Sin nada que reponer no
/// toca el fichero: no hay por qué reescribirlo entero para nada.
fn repon_espaciado(
    work_path: &str,
    page_index: u16,
    ordinal: usize,
    tc: Option<f32>,
) -> Result<(), String> {
    let Some(tc) = tc else { return Ok(()) };
    crate::cirugia_en_hilo(work_path, |doc| {
        escribe_espaciado(doc, page_index, &[ordinal], tc)
    })
}

/// Borra un bloque de texto del content stream.
#[tauri::command(async)]
pub fn delete_text_block(
    work_path: String,
    page_index: u16,
    object_index: u32,
) -> Result<(), String> {
    mutacion(work_path, |work_path| {
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
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **Corregir un texto puede cambiarle la letra, y hay que decirlo
    /// antes** (C-12). Si la fuente del bloque no va dentro del documento,
    /// reescribirlo la sustituye por la más parecida que haya: se puede
    /// hacer, pero el usuario corregía una errata y le cambiaba el tipo de
    /// letra del párrafo sin que nadie le avisara. `reescribible` es lo
    /// que la tarjeta de edición necesita para poner esa media frase.
    #[test]
    fn un_bloque_dice_si_corregirlo_conserva_su_letra() {
        let pdf = std::env::temp_dir().join("texto-reescribible.pdf");
        crate::tests::crea_pdf(&["Con Helvetica de las de siempre"], &pdf);
        let work = pdf.to_string_lossy().into_owned();
        let bloques = get_text_blocks(work.clone(), 0).expect("bloques");
        assert!(!bloques.is_empty());
        assert!(
            bloques[0].reescribible,
            "una de las catorce estándar se reescribe sin cambiar nada: {:?}",
            bloques[0].font_family
        );

        // las catorce del spec se reconocen por familia, escriba PDFium el
        // nombre que escriba en cada build
        for f in [
            "Helvetica",
            "Arial-BoldMT",
            "TimesNewRomanPSMT",
            "Courier New",
            "Symbol",
        ] {
            assert!(es_estandar(f), "«{f}» es una de las estándar");
        }
        for f in ["Gill Sans MT", "Whitney-Book", "FGHIJK+Minion Pro"] {
            assert!(!es_estandar(f), "«{f}» no la tiene cualquier visor");
        }

        std::fs::remove_file(&pdf).ok();
    }
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

    /// **R24.** El espaciado entre caracteres es un mando de la fila
    /// contextual que hasta el ciclo 5 no llegaba a ninguna parte: Tauri
    /// descartaba la clave y el comando devolvía `Ok`. Ahora se escribe el
    /// operador `Tc` con lopdf (pdfium-render 0.8 no expone el estado de
    /// texto del objeto) y el texto se separa de verdad: PDFium lo lee al
    /// medir la caja del bloque.
    #[test]
    fn el_espaciado_entre_caracteres_separa_el_texto_y_deja_el_operador_escrito() {
        let tmp = std::env::temp_dir().join("texto-espaciado-tc.pdf");
        crea_pdf(&["Texto original"], &tmp);
        let work = tmp.to_string_lossy().into_owned();
        let antes = get_text_blocks(work.clone(), 0).expect("bloques")[0].clone();

        edit_text_block(
            work.clone(),
            0,
            antes.object_index,
            "Texto original".into(),
            None,
            None,
            None,
            Some(2.0),
            None,
        )
        .expect("corregir con espaciado");

        // el operador está en el content stream, con su valor
        let doc = lopdf::Document::load(&work).expect("releer con lopdf");
        let pid = doc.get_pages()[&1];
        let contenido =
            String::from_utf8_lossy(&doc.get_page_content(pid).expect("content stream"))
                .into_owned();
        assert!(contenido.contains("2 Tc"), "el content stream: {contenido}");
        // y no se escapa al resto de la página
        assert!(
            contenido.contains("0 Tc"),
            "el espaciado se cierra antes del ET"
        );

        // y el texto se ha separado: 2 pt por cada hueco entre caracteres
        let despues = get_text_blocks(work.clone(), 0).expect("bloques")[0].clone();
        let huecos = antes.text.chars().count() as f32;
        let crecimiento = despues.w - antes.w;
        assert!(
            (crecimiento - 2.0 * huecos).abs() < 3.0,
            "con 2 pt de espaciado el bloque debía crecer ~{:.1} pt y ha crecido {crecimiento:.1}",
            2.0 * huecos
        );

        // sin espaciado (o con 0) el operador no se escribe: `Tc` vale 0 por
        // defecto y ensuciar el stream por nada no ayuda a nadie
        edit_text_block(
            work.clone(),
            0,
            antes.object_index,
            "Texto original".into(),
            None,
            None,
            None,
            None,
            None,
        )
        .expect("corregir sin espaciado");
        let doc = lopdf::Document::load(&work).expect("releer");
        let pid = doc.get_pages()[&1];
        let contenido = String::from_utf8_lossy(&doc.get_page_content(pid).unwrap()).into_owned();
        assert!(
            !contenido.contains(" Tc"),
            "sin pedirlo no se escribe: {contenido}"
        );
        let vuelta = get_text_blocks(work.clone(), 0).expect("bloques")[0].clone();
        assert!(
            (vuelta.w - antes.w).abs() < 1.0,
            "el bloque vuelve a su ancho: {:.1} → {:.1}",
            antes.w,
            vuelta.w
        );
        std::fs::remove_file(&tmp).ok();
    }

    /// **R32b.** `FPDF_GenerateContent` no vuelve a escribir el `Tc` al
    /// regenerar el content stream de la página, así que mover o estirar un
    /// bloque le borraba el espaciado que se le acababa de poner: el texto
    /// se volvía a juntar solo, sin que nadie lo hubiera pedido. Los dos
    /// comandos lo leen antes de tocar el bloque y lo reponen al terminar,
    /// dentro de la misma mutación (un solo ⌘Z).
    #[test]
    fn mover_y_estirar_un_bloque_conservan_el_espaciado() {
        let tmp = std::env::temp_dir().join("texto-espaciado-superviviente.pdf");
        crea_pdf(&["Texto original"], &tmp);
        let work = tmp.to_string_lossy().into_owned();
        let antes = get_text_blocks(work.clone(), 0).expect("bloques")[0].clone();

        edit_text_block(
            work.clone(),
            0,
            antes.object_index,
            "Texto original".into(),
            None,
            None,
            None,
            Some(2.0),
            None,
        )
        .expect("corregir con espaciado");
        let separado = get_text_blocks(work.clone(), 0).expect("bloques")[0].clone();
        assert!(
            separado.w > antes.w + 10.0,
            "el espaciado tenía que haber ensanchado el bloque: {:.1} → {:.1}",
            antes.w,
            separado.w
        );

        // moverlo no puede juntar las letras otra vez
        move_text_block(work.clone(), 0, separado.object_index, 100.0, 400.0)
            .expect("mover el bloque");
        let contenido = contenido_de(&work);
        assert!(
            contenido.contains("2 Tc"),
            "el `Tc` tenía que sobrevivir a mover: {contenido}"
        );
        let movido = get_text_blocks(work.clone(), 0).expect("bloques")[0].clone();
        assert!(
            (movido.w - separado.w).abs() < 1.5,
            "mover no cambia el ancho, y sin el `Tc` habría vuelto a {:.1}: {:.1}",
            antes.w,
            movido.w
        );

        // y estirarlo tampoco
        resize_text_block(
            work.clone(),
            0,
            movido.object_index,
            movido.w * 1.5,
            movido.h * 1.5,
        )
        .expect("estirar el bloque");
        let contenido = contenido_de(&work);
        assert!(
            contenido.contains("2 Tc"),
            "el `Tc` tenía que sobrevivir a estirar: {contenido}"
        );
        let estirado = get_text_blocks(work.clone(), 0).expect("bloques")[0].clone();
        assert!(
            estirado.w > movido.w,
            "estirar agranda el bloque: {:.1} → {:.1}",
            movido.w,
            estirado.w
        );
        std::fs::remove_file(&tmp).ok();
    }

    /// El content stream de la primera página, en llano.
    fn contenido_de(work: &str) -> String {
        let doc = lopdf::Document::load(work).expect("releer con lopdf");
        let pid = doc.get_pages()[&1];
        String::from_utf8_lossy(&doc.get_page_content(pid).expect("content stream")).into_owned()
    }

    /// **R24.** El texto nuevo también lo acepta, y en un bloque de varias
    /// líneas lo llevan todas: las líneas 2..n son objetos aparte y sin
    /// esto la primera salía separada y las demás juntas.
    #[test]
    fn el_texto_nuevo_de_varias_lineas_lleva_el_espaciado_en_todas() {
        let tmp = std::env::temp_dir().join("texto-espaciado-nuevo.pdf");
        crea_pdf(&["Contenido previo"], &tmp);
        let work = tmp.to_string_lossy().into_owned();
        add_text_block(
            work.clone(),
            0,
            80.0,
            300.0,
            "Primera linea\nSegunda linea".into(),
            12.0,
            None,
            None,
            None,
            None,
            Some(3.0),
        )
        .expect("añadir con espaciado");

        let doc = lopdf::Document::load(&work).expect("releer");
        let pid = doc.get_pages()[&1];
        let contenido = String::from_utf8_lossy(&doc.get_page_content(pid).unwrap()).into_owned();
        assert_eq!(
            contenido.matches("3 Tc").count(),
            2,
            "una por línea: {contenido}"
        );
        // el bloque que ya estaba no se toca
        let bloques = get_text_blocks(work.clone(), 0).expect("bloques");
        let previo = bloques
            .iter()
            .find(|b| b.text.contains("Contenido"))
            .expect("el bloque previo");
        let ancho_normal = previo.w;
        let nuevas: Vec<_> = bloques
            .iter()
            .filter(|b| b.text.contains("linea"))
            .collect();
        assert_eq!(nuevas.len(), 2, "dos líneas");
        for l in &nuevas {
            assert!(
                l.w > l.text.chars().count() as f32 * 3.0,
                "la línea {:?} mide {:.1} y con 3 pt por hueco tenía que ser más ancha",
                l.text,
                l.w
            );
        }
        assert!(ancho_normal > 0.0);
        std::fs::remove_file(&tmp).ok();
    }

    /// Escribe un párrafo de `n` líneas en la página, una por objeto, con
    /// el interlineado dado, y devuelve el índice del primer objeto.
    fn parrafo_de_prueba(work: &str, lineas: &[&str], size: f32, lh: f32) -> u32 {
        for (i, texto) in lineas.iter().enumerate() {
            add_text_block(
                work.to_string(),
                0,
                60.0,
                200.0 + size * lh * i as f32,
                (*texto).into(),
                size,
                None,
                None,
                None,
                None,
                None,
            )
            .expect("escribir la línea");
        }
        let bloques = get_text_blocks(work.to_string(), 0).expect("bloques");
        bloques
            .iter()
            .find(|b| b.text.contains(lineas[0]))
            .expect("la primera línea")
            .object_index
    }

    /// **AC-097.** Un bloque de una sola línea corta no tiene hermanos que
    /// marquen la columna: el ancho del texto viejo no puede ser el ancho
    /// del párrafo, o casi cualquier añadido se sale. Y una palabra que no
    /// cabe se queda entera, como en cualquier procesador de textos.
    #[test]
    fn un_bloque_de_una_linea_refluye_al_ancho_de_la_pagina_sin_partir_palabras() {
        let tmp = std::env::temp_dir().join("texto-reflujo-una-linea.pdf");
        crea_pdf(&["Suelto"], &tmp);
        let work = tmp.to_string_lossy().into_owned();
        add_text_block(
            work.clone(),
            0,
            60.0,
            200.0,
            "Uno alfa".into(),
            12.0,
            None,
            None,
            None,
            None,
            None,
        )
        .expect("la línea suelta");
        let indice = get_text_blocks(work.clone(), 0)
            .expect("bloques")
            .into_iter()
            .find(|b| b.text.contains("Uno alfa"))
            .expect("la línea")
            .object_index;

        edit_text_block(
            work.clone(),
            0,
            indice,
            "Uno alfa corregído ñ".into(),
            None,
            None,
            None,
            None,
            Some(true),
        )
        .expect("corregir con reflujo");
        let bloques = get_text_blocks(work.clone(), 0).expect("bloques");
        let textos: Vec<&str> = bloques.iter().map(|b| b.text.as_str()).collect();
        assert!(
            textos.contains(&"Uno alfa corregído ñ"),
            "una línea corta cabe entera en el ancho de la página: {textos:?}"
        );

        // y una palabra más ancha que la columna no se parte por la mitad
        let indice = bloques
            .iter()
            .find(|b| b.text.contains("corregído"))
            .expect("la línea corregida")
            .object_index;
        edit_text_block(
            work.clone(),
            0,
            indice,
            "supercalifragilisticoespialidoso".into(),
            None,
            None,
            None,
            None,
            Some(true),
        )
        .expect("corregir con una palabra larga");
        let textos: Vec<String> = get_text_blocks(work.clone(), 0)
            .expect("bloques")
            .into_iter()
            .map(|b| b.text)
            .collect();
        assert!(
            textos
                .iter()
                .any(|t| t == "supercalifragilisticoespialidoso"),
            "la palabra entera en una línea: {textos:?}"
        );
        std::fs::remove_file(&tmp).ok();
    }

    /// **H1 (a).** Cambiar una palabra corta por una larga en la primera
    /// línea de un párrafo de cuatro recoloca **el párrafo entero**: siguen
    /// siendo cuatro líneas, ninguna se pasa del ancho y la última no ha
    /// bajado. Esto es lo que separa «editar un PDF» de «reescribir una
    /// línea y dejar las sobras debajo».
    #[test]
    fn corregir_una_palabra_recoloca_el_parrafo_entero() {
        let tmp = std::env::temp_dir().join("texto-reflujo-cuatro.pdf");
        crea_pdf(&["Contrato"], &tmp);
        let work = tmp.to_string_lossy().into_owned();
        // la última línea es corta: ahí está la holgura que absorbe la
        // palabra nueva sin que el párrafo gane una línea
        let lineas = [
            "El presente contrato se firma entre las partes",
            "que abajo se relacionan y tiene por objeto",
            "regular el uso del local sito en la calle",
            "Mayor numero once.",
        ];
        let indice = parrafo_de_prueba(&work, &lineas, 11.0, 1.35);
        let antes = get_text_blocks(work.clone(), 0).expect("bloques");
        let ancho_antes = antes
            .iter()
            .filter(|b| lineas.iter().any(|l| b.text.contains(&l[..12])))
            .map(|b| b.w)
            .fold(0.0f32, f32::max);
        let ultima_antes = antes
            .iter()
            .find(|b| b.text.contains("Mayor numero"))
            .expect("la última")
            .y;

        // se corrige SOLO la línea tocada, que es lo que la UI tiene en su
        // cuadro de edición: el resto del párrafo lo pone el backend
        let texto = lineas[0].replace("contrato", "contrato de arrendamiento");
        let informe = edit_text_block(
            work.clone(),
            0,
            indice,
            texto,
            None,
            None,
            None,
            None,
            Some(true),
        )
        .expect("corregir con reflujo");
        assert!(
            informe.reflujo,
            "el párrafo tenía cuatro líneas: {informe:?}"
        );
        assert!(!informe.se_sale);

        let despues = get_text_blocks(work.clone(), 0).expect("bloques");
        // el párrafo es lo que va en cuerpo 11; el título del fixture va en 14
        let parrafo: Vec<_> = despues.iter().filter(|b| b.font_size < 12.0).collect();
        assert_eq!(
            parrafo.len(),
            informe.lineas as usize,
            "el informe y el documento dicen lo mismo: {despues:?}"
        );
        assert_eq!(parrafo.len(), 4, "siguen siendo cuatro líneas: {parrafo:?}");
        for b in &parrafo {
            assert!(
                b.w <= ancho_antes + 1.0,
                "«{}» mide {:.1} y la columna medía {ancho_antes:.1}",
                b.text,
                b.w
            );
        }
        let ultima = parrafo.iter().map(|b| b.y).fold(f32::MIN, f32::max);
        assert!(
            (ultima - ultima_antes).abs() < 1.5,
            "la última línea no baja: {ultima_antes:.1} → {ultima:.1}"
        );
        // y el texto entero sigue estando, con la palabra nueva
        let todo = parrafo
            .iter()
            .map(|b| b.text.clone())
            .collect::<Vec<_>>()
            .join(" ");
        assert!(todo.contains("arrendamiento"), "{todo}");
        assert!(
            todo.contains("numero once"),
            "no se pierde el final: {todo}"
        );
        std::fs::remove_file(&tmp).ok();
    }

    /// **R39b (AC-061, crítico).** Corregir una línea de un párrafo con
    /// reflujo **no puede borrar el resto del párrafo**. Hasta el ciclo 6,
    /// `new_text` se entendía como el texto del párrafo entero mientras la
    /// UI mandaba —correctamente— el del bloque tocado: las líneas de abajo
    /// desaparecían del documento sin banda, sin confirmación y sin ningún
    /// signo de que se hubiera borrado nada.
    ///
    /// El contrato es el de ahora: `new_text` es siempre la línea, el
    /// backend reconoce él el párrafo y lo recoloca entero.
    #[test]
    fn corregir_una_linea_con_reflujo_no_se_lleva_el_resto_del_parrafo() {
        let tmp = std::env::temp_dir().join("texto-reflujo-no-borra.pdf");
        crea_pdf(&["Contrato"], &tmp);
        let work = tmp.to_string_lossy().into_owned();
        let lineas = [
            "El presente contrato se firma entre",
            "las partes que abajo se indican y",
            "regula el uso del inmueble situado",
        ];
        let indice = parrafo_de_prueba(&work, &lineas, 11.0, 1.35);

        // se corrige la PRIMERA línea con una palabra más larga, que es lo
        // que la UI manda: el texto de ese bloque y nada más
        let informe = edit_text_block(
            work.clone(),
            0,
            indice,
            "El presente contrato de arrendamiento se firma entre".into(),
            None,
            None,
            None,
            None,
            Some(true),
        )
        .expect("corregir la primera línea");
        assert!(
            informe.reflujo,
            "el backend reconoce el párrafo él solo: {informe:?}"
        );

        let parrafo: Vec<crate::texto::TextBlock> = get_text_blocks(work.clone(), 0)
            .expect("bloques")
            .into_iter()
            .filter(|b| b.font_size < 12.0)
            .collect();
        assert_eq!(
            parrafo.len(),
            informe.lineas as usize,
            "el informe y el documento dicen lo mismo: {parrafo:?}"
        );
        assert!(
            parrafo.len() >= 3,
            "las tres líneas siguen ahí: {parrafo:?}"
        );
        let todo = parrafo
            .iter()
            .map(|b| b.text.clone())
            .collect::<Vec<_>>()
            .join(" ");
        for palabras in ["arrendamiento", "abajo se indican", "inmueble situado"] {
            assert!(
                todo.contains(palabras),
                "falta «{palabras}» del párrafo: {todo}"
            );
        }
        // y las líneas se han recolocado: ninguna se sale de la columna
        let ancho = parrafo.iter().map(|b| b.w).fold(0.0f32, f32::max);
        assert!(
            ancho < 300.0,
            "el párrafo se ha repartido, no alargado: {ancho:.1}"
        );

        // sin pedir reflujo NO se refluye: la bandera cambia lo que
        // significan los demás argumentos y su defecto no puede ser el
        // que toca lo que no se le ha pedido
        crate::historial::undo(work.clone()).expect("deshacer");
        let informe = edit_text_block(
            work.clone(),
            0,
            indice,
            "El presente contrato de arrendamiento se firma entre".into(),
            None,
            None,
            None,
            None,
            None,
        )
        .expect("corregir sin decir nada de reflujo");
        assert!(
            !informe.reflujo,
            "sin `reflow` se hace lo de siempre: {informe:?}"
        );
        std::fs::remove_file(&tmp).ok();
    }

    /// **H1 (b) y (c).** Un párrafo que crece de cuatro a seis líneas tiene
    /// seis objetos, y uno que encoge de cuatro a dos tiene dos: ni líneas
    /// huérfanas debajo ni objetos vacíos.
    #[test]
    fn el_parrafo_crece_y_encoge_sin_dejar_lineas_huerfanas() {
        let tmp = std::env::temp_dir().join("texto-reflujo-crece.pdf");
        crea_pdf(&["Memoria"], &tmp);
        let work = tmp.to_string_lossy().into_owned();
        let lineas = [
            "Uno dos tres cuatro cinco seis",
            "siete ocho nueve diez once",
            "doce trece catorce quince",
            "dieciseis diecisiete dieciocho",
        ];
        let indice = parrafo_de_prueba(&work, &lineas, 11.0, 1.35);
        let cuenta = |work: &str| {
            get_text_blocks(work.to_string(), 0)
                .expect("bloques")
                .into_iter()
                .filter(|b| b.font_size < 12.0)
                .count()
        };
        assert_eq!(cuenta(&work), 4);

        // crece: la primera línea se alarga con el párrafo entero detrás
        let largo = format!("{} {}", lineas[0], lineas.join(" "));
        let informe = edit_text_block(
            work.clone(),
            0,
            indice,
            largo,
            None,
            None,
            None,
            None,
            Some(true),
        )
        .expect("crecer");
        assert!(informe.lineas >= 6, "seis líneas o más: {informe:?}");
        assert_eq!(
            cuenta(&work),
            informe.lineas as usize,
            "un objeto por línea"
        );

        // ⌘Z lo devuelve entero, en un solo paso
        crate::historial::undo(work.clone()).expect("deshacer");
        assert_eq!(cuenta(&work), 4, "el párrafo vuelve como estaba");

        // encoge: en un párrafo cuya primera línea es casi todo el texto,
        // recortarla deja el resto en menos líneas y las que sobran se van
        // sin dejar objetos vacíos detrás
        let tmp2 = std::env::temp_dir().join("texto-reflujo-encoge.pdf");
        crea_pdf(&["Memoria"], &tmp2);
        let work2 = tmp2.to_string_lossy().into_owned();
        let largas = [
            "Uno dos tres cuatro cinco seis siete ocho nueve diez once doce",
            "trece",
            "catorce",
        ];
        let indice2 = parrafo_de_prueba(&work2, &largas, 11.0, 1.35);
        assert_eq!(cuenta(&work2), 3);
        let informe = edit_text_block(
            work2.clone(),
            0,
            indice2,
            "Uno".into(),
            None,
            None,
            None,
            None,
            Some(true),
        )
        .expect("encoger");
        assert!(informe.lineas < 3, "el párrafo encoge: {informe:?}");
        assert_eq!(
            cuenta(&work2),
            informe.lineas as usize,
            "sin objetos huérfanos"
        );
        // y el texto de las líneas de abajo sigue estando entero
        let todo = get_text_blocks(work2.clone(), 0)
            .expect("bloques")
            .into_iter()
            .filter(|b| b.font_size < 12.0)
            .map(|b| b.text)
            .collect::<Vec<_>>()
            .join(" ");
        assert!(todo.contains("catorce"), "no se pierde el final: {todo}");
        std::fs::remove_file(&tmp2).ok();
        std::fs::remove_file(&tmp).ok();
    }

    /// **H1 (d).** El mismo caso en una página girada 90°: el reflujo
    /// trabaja en el espacio propio de la página, donde viven las cajas de
    /// los objetos, así que el `/Rotate` no lo despeina.
    #[test]
    fn el_reflujo_tambien_funciona_en_una_pagina_girada() {
        let tmp = std::env::temp_dir().join("texto-reflujo-girada.pdf");
        crea_pdf(&["Girado"], &tmp);
        let work = tmp.to_string_lossy().into_owned();
        let lineas = [
            "Primera linea del parrafo girado",
            "segunda linea del mismo parrafo",
            "tercera y ultima linea de todas",
        ];
        let indice = parrafo_de_prueba(&work, &lineas, 11.0, 1.35);
        crate::paginas::rotate_page(work.clone(), 0).expect("girar 90°");

        let informe = edit_text_block(
            work.clone(),
            0,
            indice,
            lineas[0].replace("Primera", "La primerísima de todas"),
            None,
            None,
            None,
            None,
            Some(true),
        )
        .expect("corregir en una página girada");
        assert!(informe.reflujo);
        let bloques = get_text_blocks(work.clone(), 0).expect("bloques");
        let parrafo: Vec<_> = bloques.iter().filter(|b| b.font_size < 12.0).collect();
        assert_eq!(parrafo.len(), informe.lineas as usize);
        let todo = parrafo
            .iter()
            .map(|b| b.text.clone())
            .collect::<Vec<_>>()
            .join(" ");
        assert!(todo.contains("primerísima"), "{todo}");
        assert!(todo.contains("ultima linea de todas"), "{todo}");
        crate::render_page_png(work.clone(), 0, 200, true).expect("render");
        std::fs::remove_file(&tmp).ok();
    }

    /// **H1.** Un párrafo que crece hasta salirse del papel se dice, no se
    /// escribe fuera en silencio.
    #[test]
    fn un_parrafo_que_no_cabe_en_la_pagina_lo_dice() {
        let tmp = std::env::temp_dir().join("texto-reflujo-no-cabe.pdf");
        crea_pdf(&["Al fondo"], &tmp);
        let work = tmp.to_string_lossy().into_owned();
        // dos líneas pegadas al borde de abajo de la A4 (842 pt de alto)
        let lineas = ["Casi al final de la hoja", "y esta es la ultima de todas"];
        for (i, texto) in lineas.iter().enumerate() {
            add_text_block(
                work.clone(),
                0,
                60.0,
                810.0 + i as f32 * 15.0,
                (*texto).into(),
                11.0,
                None,
                None,
                None,
                None,
                None,
            )
            .expect("escribir");
        }
        let indice = get_text_blocks(work.clone(), 0)
            .expect("bloques")
            .into_iter()
            .find(|b| b.text.contains("Casi al final"))
            .expect("la primera")
            .object_index;
        let largo = "Casi al final de la hoja ".repeat(12);
        let informe = edit_text_block(
            work.clone(),
            0,
            indice,
            largo,
            None,
            None,
            None,
            None,
            Some(true),
        )
        .expect("corregir");
        assert!(informe.reflujo);
        assert!(
            informe.se_sale,
            "el párrafo se sale del papel y hay que decirlo: {informe:?}"
        );
        std::fs::remove_file(&tmp).ok();
    }

    /// Con `reflow: false` se conserva el camino de siempre, que es el que
    /// usa la UI cuando no quiere que se le mueva nada.
    #[test]
    fn sin_reflujo_se_hace_lo_de_siempre() {
        let tmp = std::env::temp_dir().join("texto-sin-reflujo.pdf");
        crea_pdf(&["Base"], &tmp);
        let work = tmp.to_string_lossy().into_owned();
        let lineas = ["Primera linea corta", "segunda linea corta"];
        let indice = parrafo_de_prueba(&work, &lineas, 11.0, 1.35);
        let informe = edit_text_block(
            work.clone(),
            0,
            indice,
            "Una sola linea muy larga que no se reparte porque no hay reflujo".into(),
            None,
            None,
            None,
            None,
            Some(false),
        )
        .expect("corregir sin reflujo");
        assert!(!informe.reflujo);
        let bloques = get_text_blocks(work.clone(), 0).expect("bloques");
        assert!(
            bloques.iter().any(|b| b.text.contains("no hay reflujo")),
            "la línea se escribe entera, sin partir: {bloques:?}"
        );
        assert!(
            bloques.iter().any(|b| b.text.contains("segunda linea")),
            "y la segunda línea del párrafo se queda donde estaba"
        );
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
            None,
            None,
            None,
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
        assert!(add_text_block(
            work.clone(),
            0,
            0.0,
            0.0,
            "  ".into(),
            12.0,
            None,
            None,
            None,
            None,
            None
        )
        .is_err());

        std::fs::remove_file(&tmp).ok();
    }

    /// Lo que hace la UI antes de mandar: pasa un punto de la página vista
    /// al espacio propio de la página con la `rotation` de
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

    /// **G3.** Un bloque de texto se coloca, no solo se corrige: se arrastra
    /// como una imagen. Y en una página girada tiene que caer donde se
    /// soltó, que es donde siempre se rompe (el `/Rotate` va por un lado y
    /// las cajas de los objetos por otro).
    #[test]
    fn mover_un_bloque_lo_deja_donde_se_suelta_tambien_en_una_pagina_girada() {
        for vueltas in [0u8, 1, 3] {
            let tmp = std::env::temp_dir().join(format!("texto-mover-bloque-{vueltas}.pdf"));
            crea_pdf(&["Parrafo"], &tmp);
            let work = tmp.to_string_lossy().into_owned();
            for _ in 0..vueltas {
                crate::paginas::rotate_page(work.clone(), 0).expect("girar");
            }
            let bloque = &get_text_blocks(work.clone(), 0).expect("bloques")[0];
            let indice = bloque.object_index;

            // la UI suelta el bloque en (120, 300) de la página VISTA
            let (px, py) = vista_a_pagina(&work, 120.0, 300.0);
            move_text_block(work.clone(), 0, indice, px, py).expect("mover");

            let movido = &get_text_blocks(work.clone(), 0).expect("bloques")[0];
            assert!(
                (movido.x - px).abs() < 2.0 && (movido.y - py).abs() < 2.0,
                "con {}° el bloque se ha quedado en ({:.1},{:.1}) y se soltó en ({px:.1},{py:.1})",
                vueltas as u32 * 90,
                movido.x,
                movido.y
            );
            assert_eq!(movido.text, bloque.text, "el texto no cambia al moverlo");
            std::fs::remove_file(&tmp).ok();
        }
    }

    /// **G3.** Estirar un bloque cambia el tamaño de la letra, no la
    /// deforma: el ancho y el alto crecen en la misma proporción, como al
    /// arrastrar un tirador en Acrobat.
    #[test]
    fn estirar_un_bloque_agranda_la_letra_sin_deformarla() {
        let tmp = std::env::temp_dir().join("texto-estirar-bloque.pdf");
        crea_pdf(&["Parrafo que se estira"], &tmp);
        let work = tmp.to_string_lossy().into_owned();
        let antes = get_text_blocks(work.clone(), 0).expect("bloques")[0].clone();
        let proporcion_antes = antes.w / antes.h;

        resize_text_block(
            work.clone(),
            0,
            antes.object_index,
            antes.w * 2.0,
            antes.h * 2.0,
        )
        .expect("estirar");
        let despues = &get_text_blocks(work.clone(), 0).expect("bloques")[0];
        assert!(
            (despues.font_size / antes.font_size - 2.0).abs() < 0.2,
            "el tamaño de la letra pasa de {:.1} a {:.1}",
            antes.font_size,
            despues.font_size
        );
        assert!(
            (despues.w / despues.h - proporcion_antes).abs() < 0.05,
            "las letras no se deforman: {:.2} → {:.2}",
            proporcion_antes,
            despues.w / despues.h
        );
        assert!(
            (despues.x - antes.x).abs() < 1.5 && (despues.y - antes.y).abs() < 1.5,
            "la esquina de la que no se tira se queda donde estaba"
        );
        // y no se puede encoger hasta desaparecer
        assert!(resize_text_block(work.clone(), 0, antes.object_index, 1.0, 1.0).is_err());
        std::fs::remove_file(&tmp).ok();
    }

    /// **G3.** Corregir un párrafo rojo de dos líneas dejaba la primera
    /// roja y la segunda negra: las líneas 2..n son objetos nuevos y salían
    /// con el color por defecto de PDFium. Sin color pedido, heredan el que
    /// tenía el bloque. Y el interlineado se puede elegir.
    #[test]
    fn las_lineas_nuevas_heredan_el_color_y_respetan_el_interlineado() {
        let tmp = std::env::temp_dir().join("texto-color-heredado.pdf");
        crea_pdf(&["Base"], &tmp);
        let work = tmp.to_string_lossy().into_owned();
        add_text_block(
            work.clone(),
            0,
            60.0,
            300.0,
            "Rojo".into(),
            14.0,
            None,
            Some([220, 20, 20, 255]),
            None,
            None,
            None,
        )
        .expect("añadir en rojo");
        let rojo = get_text_blocks(work.clone(), 0)
            .expect("bloques")
            .into_iter()
            .find(|b| b.text.contains("Rojo"))
            .expect("el bloque rojo");
        assert_eq!(rojo.color, [220, 20, 20, 255], "el color se lee del bloque");

        // corregirlo a dos líneas SIN pedir color: las dos siguen rojas
        edit_text_block(
            work.clone(),
            0,
            rojo.object_index,
            "Rojo uno\nRojo dos".into(),
            None,
            None,
            Some(2.0),
            None,
            None,
        )
        .expect("corregir");
        let bloques = get_text_blocks(work.clone(), 0).expect("bloques");
        let uno = bloques
            .iter()
            .find(|b| b.text.contains("Rojo uno"))
            .expect("línea 1");
        let dos = bloques
            .iter()
            .find(|b| b.text.contains("Rojo dos"))
            .expect("línea 2");
        assert_eq!(uno.color, [220, 20, 20, 255]);
        assert_eq!(
            dos.color,
            [220, 20, 20, 255],
            "la segunda línea heredaba el negro de PDFium"
        );
        // interlineado 2: la segunda línea cae a 2 × el tamaño de fuente.
        // La `y` es el borde de arriba de la caja de los glifos y las dos
        // líneas no llevan exactamente la misma fuente (la 2ª es un objeto
        // nuevo con la estándar aproximada), así que se mide con holgura
        let separacion = dos.y - uno.y;
        assert!(
            (separacion - uno.font_size * 2.0).abs() < 4.0,
            "con interlineado 2 la separación es {separacion:.1} y la letra mide {:.1}",
            uno.font_size
        );

        // y con el de siempre (1,2) las líneas quedan más juntas. Ojo: aquí
        // ya hay párrafo, así que la corrección va por el reflujo, y sin
        // pedir interlineado el reflujo respeta el que tenga el párrafo
        // (que es el 2 de arriba): el 1,2 hay que pedirlo
        edit_text_block(
            work.clone(),
            0,
            uno.object_index,
            "Rojo uno\nRojo dos".into(),
            None,
            None,
            Some(1.2),
            None,
            None,
        )
        .expect("corregir con el interlineado de siempre");
        let bloques = get_text_blocks(work.clone(), 0).expect("bloques");
        let a = bloques
            .iter()
            .find(|b| b.text.contains("Rojo uno"))
            .expect("línea 1");
        let b = bloques
            .iter()
            .filter(|b| b.text.contains("Rojo dos"))
            .min_by(|x, y| x.y.total_cmp(&y.y))
            .expect("línea 2");
        assert!(
            b.y - a.y < separacion - 5.0,
            "con 1,2 las líneas van más juntas: {:.1} frente a {separacion:.1}",
            b.y - a.y
        );
        std::fs::remove_file(&tmp).ok();
    }

    #[test]
    fn fuente_elegida_y_detectada() {
        let tmp = std::env::temp_dir().join("editor_pdf_test_fuentes.pdf");
        crea_pdf(&["Texto base"], &tmp); // crea_pdf usa Helvetica
        let work = tmp.to_string_lossy().into_owned();

        // fuente automática primero (solo hay Helvetica en la página, sin
        // empates): debe detectar la dominante
        add_text_block(
            work.clone(),
            0,
            60.0,
            400.0,
            "Detectada".into(),
            12.0,
            None,
            None,
            None,
            None,
            None,
        )
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
            None,
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
            add_text_block(
                work.clone(),
                0,
                px,
                py,
                "NUEVO".into(),
                24.0,
                None,
                None,
                None,
                None,
                None,
            )
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

            crate::imagenes::add_image(work.clone(), 0, png.to_string_lossy().into_owned(), px, py)
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
        assert!(
            texto_de(1).contains("Vitela"),
            "la página 2 no se ha tocado"
        );
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
                None,
                None,
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
            None,
            None,
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
            None,
            None,
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
            None,
            None,
            None,
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
