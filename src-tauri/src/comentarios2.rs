//! Sacar los comentarios del documento: el **resumen imprimible** (un PDF
//! nuevo con la lista) y el **XFDF** que otro revisor importa sobre su
//! copia.
//!
//! Es lo que cierra el flujo de revisión que abrió el ciclo 5 (hilos y
//! estados): hasta aquí se podía revisar en Vitela y no se podía devolver
//! la revisión a quien la pidió.

use crate::anotaciones::AnotacionDoc;
use pdfium_render::prelude::*;

/// «Crear resumen de comentarios»: **un PDF nuevo** con la lista entera,
/// para imprimirla o mandarla por correo. Se abre al terminar, como «Crear
/// PDF desde imágenes».
///
/// `orden` es `"pagina"` (el de por defecto), `"autor"`, `"fecha"` o
/// `"tipo"`. Las respuestas van **siempre pegadas a su comentario** y
/// sangradas, se ordene por lo que se ordene: un hilo partido por la mitad
/// no se entiende.
///
/// **Lo que no se copia de Acrobat**: allí el resumen puede llevar las
/// páginas a la izquierda y los comentarios enfrentados a la derecha, con
/// una línea que une cada uno con su marca. Aquí es la lista, y se dice: con
/// cuatro ordenaciones, tres de ellas rompen el agrupado por página, y una
/// miniatura por comentario en un documento de doscientos comentarios es un
/// fichero enorme que nadie imprime. Para ver la marca en su sitio está el
/// documento, que es donde vive.
///
/// Escribe fuera del documento: no muta nada ni deja paso de deshacer.
#[tauri::command(async)]
pub fn export_comments_pdf(
    work_path: String,
    dest_path: String,
    orden: String,
    document_name: Option<String>,
) -> Result<u32, String> {
    let comentarios = crate::anotaciones::get_document_annotations(work_path.clone())?;
    if comentarios.is_empty() {
        return Err("El documento no tiene comentarios que resumir".into());
    }
    let nombre = crate::comentarios::nombre_de_documento(document_name.as_deref(), &work_path);
    let filas = ordena(comentarios, &orden);
    let total = filas.len() as u32;

    crate::on_pdfium_thread(move || {
        let pdfium = crate::pdfium()?;
        let mut doc = pdfium.create_new_pdf().map_err(crate::mensaje_llano)?;
        let regular = doc.fonts_mut().helvetica();
        let negrita = doc.fonts_mut().helvetica_bold();

        const MARGEN: f32 = 56.0;
        const CUERPO: f32 = 10.0;
        const SANGRIA: f32 = 20.0;
        let ancho_util = 595.0 - MARGEN * 2.0;

        let mut page = doc
            .pages_mut()
            .create_page_at_end(PdfPagePaperSize::a4())
            .map_err(crate::mensaje_llano)?;
        let mut y = 842.0 - MARGEN;

        // el título, una vez
        escribe(&doc, &mut page, &format!("Comentarios de {nombre}"), negrita, 16.0, MARGEN, y - 16.0)?;
        y -= 16.0 + 10.0;
        escribe(
            &doc,
            &mut page,
            &format!("{total} {}", if total == 1 { "comentario" } else { "comentarios" }),
            regular,
            CUERPO,
            MARGEN,
            y - CUERPO,
        )?;
        y -= CUERPO + 18.0;

        let mut n = 0u32;
        for fila in &filas {
            let respuesta = fila.annot.in_reply_to.is_some();
            let sangria = if respuesta { SANGRIA } else { 0.0 };
            let x = MARGEN + sangria;
            let ancho = ancho_util - sangria;
            if !respuesta {
                n += 1;
            }
            let cabecera = cabecera_de(fila, n, respuesta);
            let cuerpo: Vec<String> = fila
                .annot
                .contents
                .lines()
                .flat_map(|l| crate::anotaciones2::parte_lineas(l, CUERPO, ancho))
                .filter(|l| !l.trim().is_empty())
                .collect();
            let alto = CUERPO * 1.4 * (1 + cuerpo.len()) as f32 + 8.0;
            if y - alto < MARGEN {
                page.regenerate_content().map_err(crate::mensaje_llano)?;
                drop(page);
                page = doc
                    .pages_mut()
                    .create_page_at_end(PdfPagePaperSize::a4())
                    .map_err(crate::mensaje_llano)?;
                y = 842.0 - MARGEN;
            }
            escribe(&doc, &mut page, &cabecera, negrita, CUERPO, x, y - CUERPO)?;
            y -= CUERPO * 1.4;
            for linea in &cuerpo {
                escribe(&doc, &mut page, linea, regular, CUERPO, x, y - CUERPO)?;
                y -= CUERPO * 1.4;
            }
            y -= 8.0;
        }
        page.regenerate_content().map_err(crate::mensaje_llano)?;
        drop(page);
        doc.save_to_file(&dest_path).map_err(|e| {
            crate::mensaje_llano(format!("No se ha podido escribir {dest_path}: {e}"))
        })?;
        Ok(total)
    })
}

/// La línea de cabecera de una fila del resumen, con los mismos datos y en
/// el mismo orden que el resumen en llano (`export_comments`): las
/// respuestas no repiten la página ni el tipo del comentario al que
/// contestan, que en una lista que se lee de arriba abajo es ruido.
fn cabecera_de(fila: &AnotacionDoc, n: u32, respuesta: bool) -> String {
    let mut partes: Vec<String> = Vec::new();
    if respuesta {
        partes.push("En respuesta".into());
    } else {
        partes.push(format!(
            "{n} · Página {} · {}",
            fila.page_index + 1,
            crate::comentarios::tipo_en_llano(&fila.annot.kind)
        ));
    }
    if !fila.annot.author.is_empty() {
        partes.push(fila.annot.author.clone());
    }
    if !fila.annot.modified.is_empty() {
        partes.push(crate::comentarios::fecha_en_espanol(&fila.annot.modified));
    }
    if !fila.annot.state.is_empty() {
        partes.push(crate::comentarios::estado_en_llano(&fila.annot.state).to_string());
    }
    partes.join(" · ")
}

/// Ordena los comentarios como se pida, **sin separar los hilos**: cada
/// respuesta va justo detrás del comentario del que cuelga.
fn ordena(comentarios: Vec<AnotacionDoc>, orden: &str) -> Vec<AnotacionDoc> {
    let (mut padres, respuestas): (Vec<AnotacionDoc>, Vec<AnotacionDoc>) = comentarios
        .into_iter()
        .partition(|c| c.annot.in_reply_to.is_none());
    match orden {
        "autor" => padres.sort_by(|a, b| {
            a.annot
                .author
                .to_lowercase()
                .cmp(&b.annot.author.to_lowercase())
                .then(a.page_index.cmp(&b.page_index))
        }),
        // la fecha va en ISO 8601, que se ordena como texto
        "fecha" => padres.sort_by(|a, b| a.annot.modified.cmp(&b.annot.modified)),
        "tipo" => padres.sort_by(|a, b| {
            crate::comentarios::tipo_en_llano(&a.annot.kind)
                .cmp(crate::comentarios::tipo_en_llano(&b.annot.kind))
                .then(a.page_index.cmp(&b.page_index))
        }),
        // «por página» es el de por defecto, y ya vienen así
        _ => {}
    }
    let mut out = Vec::with_capacity(padres.len() + respuestas.len());
    for padre in padres {
        let suyas: Vec<&AnotacionDoc> = respuestas
            .iter()
            .filter(|r| {
                r.page_index == padre.page_index
                    && r.annot.in_reply_to == Some(padre.annot.index)
            })
            .collect();
        let clones: Vec<AnotacionDoc> = suyas.into_iter().cloned().collect();
        out.push(padre);
        out.extend(clones);
    }
    out
}

/// Un renglón de texto en la página del resumen.
fn escribe(
    doc: &PdfDocument<'static>,
    page: &mut PdfPage<'static>,
    texto: &str,
    fuente: PdfFontToken,
    size: f32,
    x: f32,
    y: f32,
) -> Result<(), String> {
    if texto.trim().is_empty() {
        return Ok(());
    }
    let mut obj = PdfPageTextObject::new(doc, texto, fuente, PdfPoints::new(size))
        .map_err(crate::mensaje_llano)?;
    obj.translate(PdfPoints::new(x), PdfPoints::new(y))
        .map_err(|e| e.to_string())?;
    page.objects_mut()
        .add_text_object(obj)
        .map_err(crate::mensaje_llano)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// XFDF: los comentarios fuera del PDF, para devolverle la revisión a quien
// la pidió.
// ---------------------------------------------------------------------------

/// Los subtipos de anotación que **son comentarios**: los que viajan en el
/// XFDF y los únicos que salen en el panel, en el contador y en los dos
/// resúmenes. Fuera quedan `/Popup` (la ventana de una nota, que no es un
/// comentario), `/Widget` (un campo de formulario), `/Link` y `/Sig`.
const SUBTIPOS: &[&str] = &[
    "Text",
    "Highlight",
    "Underline",
    "StrikeOut",
    "Squiggly",
    "FreeText",
    "Ink",
    "Square",
    "Circle",
    "Line",
    "Polygon",
    "PolyLine",
    "Stamp",
    "Caret",
    "FileAttachment",
];

/// ¿Este subtipo es un comentario? **La única criba**, compartida por el
/// XFDF y por [`crate::anotaciones::get_document_annotations`], que es lo
/// que alimenta el panel de comentarios, el contador del diálogo de
/// exportar, el `.txt` y el resumen en PDF (AC-071). Mientras el XFDF
/// filtraba y el panel no, un campo de formulario se listaba como
/// «comentario», contaba en «N comentarios» y **Supr lo borraba**: nadie
/// tiene motivo para sospechar que borrar un comentario vacío le rompe el
/// formulario.
///
/// La comparación no mira mayúsculas a propósito: los nombres del spec
/// (`/StrikeOut`, `/PolyLine`) y los que devuelve PDFium por su enum
/// (`Strikeout`, `Polyline`) no se escriben igual, y son el mismo subtipo.
pub fn es_comentario(subtipo: &str) -> bool {
    SUBTIPOS.iter().any(|s| s.eq_ignore_ascii_case(subtipo))
}

/// Exporta los comentarios a **XFDF**, que es el formato con el que un
/// revisor le devuelve la revisión a quien le mandó el documento: XML plano
/// que Acrobat, Foxit y Vitela saben importar sobre **su** copia del PDF.
///
/// Se escribe leyendo los diccionarios con lopdf, no con PDFium: así viaja
/// todo lo que la anotación lleva escrito —`/T`, `/M`, `/CreationDate`,
/// `/IRT`, el `/StateModel` de la revisión, los quads de una marca, los
/// vértices de un polígono— sin pasar por ningún modelo intermedio que se
/// deje algo por el camino.
///
/// El dibujo de un trazo (`/Ink`) se saca de su apariencia, que es donde
/// PDFium lo guarda (no escribe `/InkList`), y sale como el `<inklist>` del
/// formato. Lo que no lleva el formato no se puede recrear: un sello llega
/// con su sitio, su autor y su texto, pero sin su dibujo.
///
/// Escribe fuera del documento: no muta nada ni deja paso de deshacer.
#[tauri::command(async)]
pub fn export_comments_xfdf(work_path: String, dest_path: String) -> Result<u32, String> {
    let xml = crate::on_pdfium_thread(move || {
        crate::with_lopdf(&work_path, |doc| Ok(xfdf_de(doc)))
    })?;
    let (xml, n) = xml;
    if n == 0 {
        return Err("El documento no tiene comentarios que exportar".into());
    }
    std::fs::write(&dest_path, xml).map_err(|e| {
        crate::mensaje_llano(format!("No se ha podido escribir {dest_path}: {e}"))
    })?;
    Ok(n)
}

/// El XFDF del documento y cuántos comentarios lleva.
fn xfdf_de(doc: &lopdf::Document) -> (String, u32) {
    use lopdf::Object;
    let mut out = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <xfdf xmlns=\"http://ns.adobe.com/xfdf/\" xml:space=\"preserve\">\n\
         <annots>\n",
    );
    let mut n = 0u32;
    let paginas = doc.get_pages().len() as u16;
    for page_index in 0..paginas {
        let Some(lista) = crate::anotaciones::lista_annots(doc, page_index) else {
            continue;
        };
        // el nombre con el que una respuesta señala a su comentario: el
        // `/NM` si lo trae, y si no uno nuestro con la página y el sitio
        let nombre_de = |i: usize| -> String {
            let Some(Object::Reference(id)) = lista.get(i) else {
                return format!("v{page_index}-{i}");
            };
            doc.get_object(*id)
                .ok()
                .and_then(|o| o.as_dict().ok())
                .and_then(|d| d.get(b"NM").ok())
                .map(crate::anotaciones::texto_de_cadena_pdf)
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| format!("v{page_index}-{i}"))
        };
        for (i, o) in lista.iter().enumerate() {
            let Object::Reference(id) = o else { continue };
            let Ok(d) = doc.get_object(*id).and_then(|x| x.as_dict()) else {
                continue;
            };
            let subtipo = d
                .get(b"Subtype")
                .and_then(|s| s.as_name())
                .map(|s| String::from_utf8_lossy(s).into_owned())
                .unwrap_or_default();
            if !SUBTIPOS.contains(&subtipo.as_str()) {
                continue;
            }
            out.push_str(&elemento(doc, d, &subtipo, page_index, &nombre_de(i), &lista, &nombre_de));
            n += 1;
        }
    }
    out.push_str("</annots>\n</xfdf>\n");
    (out, n)
}

/// Un elemento `<text>`, `<highlight>`… del XFDF.
fn elemento(
    doc: &lopdf::Document,
    d: &lopdf::Dictionary,
    subtipo: &str,
    page_index: u16,
    nombre: &str,
    lista: &[lopdf::Object],
    nombre_de: &dyn Fn(usize) -> String,
) -> String {
    use lopdf::Object;
    let etiqueta = subtipo.to_lowercase();
    let mut at: Vec<String> = vec![
        format!("page=\"{page_index}\""),
        format!("name=\"{}\"", escapa_xml(nombre)),
    ];
    if let Some(r) = numeros(d.get(b"Rect").ok()) {
        at.push(format!("rect=\"{}\"", lista_num(&r)));
    }
    if let Some(c) = numeros(d.get(b"C").ok()) {
        if c.len() >= 3 {
            at.push(format!(
                "color=\"#{:02X}{:02X}{:02X}\"",
                (c[0] * 255.0).round() as u8,
                (c[1] * 255.0).round() as u8,
                (c[2] * 255.0).round() as u8
            ));
        }
    }
    for (clave, nombre_at) in [
        (&b"T"[..], "title"),
        (&b"Subj"[..], "subject"),
        (&b"M"[..], "date"),
        (&b"CreationDate"[..], "creationdate"),
    ] {
        if let Ok(v) = d.get(clave) {
            let texto = crate::anotaciones::texto_de_cadena_pdf(v);
            if !texto.is_empty() {
                at.push(format!("{nombre_at}=\"{}\"", escapa_xml(&texto)));
            }
        }
    }
    if let Ok(Object::Name(icono)) = d.get(b"Name") {
        at.push(format!("icon=\"{}\"", escapa_xml(&String::from_utf8_lossy(icono))));
    }
    // el hilo: a quién contesta y con qué intención (respuesta o estado de
    // revisión), que es lo que hace que la conversación llegue entera
    if let Ok(padre) = d.get(b"IRT").and_then(|o| o.as_reference()) {
        if let Some(i) = lista
            .iter()
            .position(|o| matches!(o, Object::Reference(r) if *r == padre))
        {
            at.push(format!("inreplyto=\"{}\"", escapa_xml(&nombre_de(i))));
        }
    }
    if let Ok(Object::Name(rt)) = d.get(b"RT") {
        at.push(format!("replyType=\"{}\"", escapa_xml(&String::from_utf8_lossy(rt))));
    }
    if let Ok(Object::Name(sm)) = d.get(b"StateModel") {
        at.push(format!("statemodel=\"{}\"", escapa_xml(&String::from_utf8_lossy(sm))));
    }
    if let Ok(estado) = d.get(b"State") {
        let texto = crate::anotaciones::texto_de_cadena_pdf(estado);
        if !texto.is_empty() {
            at.push(format!("state=\"{}\"", escapa_xml(&texto)));
        }
    }
    // la geometría propia de cada tipo
    if let Some(q) = numeros(d.get(b"QuadPoints").ok()) {
        at.push(format!("coords=\"{}\"", lista_num(&q)));
    }
    if let Some(v) = numeros(d.get(b"Vertices").ok()) {
        at.push(format!("vertices=\"{}\"", pares(&v)));
    }
    if let Some(l) = numeros(d.get(b"L").ok()) {
        if l.len() == 4 {
            at.push(format!("start=\"{},{}\" end=\"{},{}\"", l[0], l[1], l[2], l[3]));
        }
    }
    at.push("flags=\"print\"".into());

    let mut hijos = String::new();
    let contenido = d
        .get(b"Contents")
        .map(crate::anotaciones::texto_de_cadena_pdf)
        .unwrap_or_default();
    if !contenido.is_empty() {
        hijos.push_str(&format!("<contents>{}</contents>", escapa_xml(&contenido)));
    }
    if subtipo == "Ink" {
        let trazos = trazos_de(doc, d);
        if !trazos.is_empty() {
            hijos.push_str("<inklist>");
            for t in &trazos {
                let puntos: Vec<String> =
                    t.iter().map(|p| format!("{},{}", p.0, p.1)).collect();
                hijos.push_str(&format!("<gesture>{}</gesture>", puntos.join(";")));
            }
            hijos.push_str("</inklist>");
        }
    }
    format!("<{etiqueta} {}>{hijos}</{etiqueta}>\n", at.join(" "))
}

/// Los trazos de un `/Ink`: del `/InkList` si lo lleva y, si no, del dibujo
/// de su apariencia, que es donde los guarda PDFium.
fn trazos_de(doc: &lopdf::Document, d: &lopdf::Dictionary) -> Vec<Vec<(f32, f32)>> {
    use lopdf::Object;
    if let Ok(Object::Array(lista)) = d.get(b"InkList") {
        let trazos: Vec<Vec<(f32, f32)>> = lista
            .iter()
            .filter_map(|t| numeros(Some(t)))
            .map(|v| v.chunks(2).filter(|c| c.len() == 2).map(|c| (c[0], c[1])).collect())
            .collect();
        if !trazos.is_empty() {
            return trazos;
        }
    }
    let Some(id) = d
        .get(b"AP")
        .ok()
        .and_then(|ap| match ap {
            Object::Dictionary(x) => x.get(b"N").ok().cloned(),
            Object::Reference(r) => doc.get_object(*r).ok()?.as_dict().ok()?.get(b"N").ok().cloned(),
            _ => None,
        })
        .and_then(|n| n.as_reference().ok())
    else {
        return Vec::new();
    };
    let Ok(stream) = doc.get_object(id).and_then(|o| o.as_stream()) else {
        return Vec::new();
    };
    let datos = stream
        .decompressed_content()
        .unwrap_or_else(|_| stream.content.clone());
    crate::anotaciones2::parte_el_camino(&datos).1
}

fn numeros(o: Option<&lopdf::Object>) -> Option<Vec<f32>> {
    use lopdf::Object;
    let a = match o? {
        Object::Array(a) => a,
        _ => return None,
    };
    let v: Vec<f32> = a
        .iter()
        .filter_map(|x| match x {
            Object::Integer(i) => Some(*i as f32),
            Object::Real(r) => Some(*r),
            _ => None,
        })
        .collect();
    (!v.is_empty()).then_some(v)
}

fn lista_num(v: &[f32]) -> String {
    v.iter().map(|n| format!("{n}")).collect::<Vec<_>>().join(",")
}

fn pares(v: &[f32]) -> String {
    v.chunks(2)
        .filter(|c| c.len() == 2)
        .map(|c| format!("{},{}", c[0], c[1]))
        .collect::<Vec<_>>()
        .join(";")
}

pub(crate) fn escapa_xml(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(c),
        }
    }
    out
}

/// Importa comentarios de un XFDF sobre el documento abierto. **Añade**, no
/// sustituye: los que ya había se quedan, que es lo que espera quien recibe
/// la revisión de dos personas distintas. Todo en **una** mutación, así que
/// un ⌘Z devuelve el lote entero. Devuelve cuántos ha añadido.
///
/// Las respuestas se enganchan a su comentario por el `name`/`inreplyto`
/// del formato, así que un hilo llega entero y anidado. Los que traen
/// quads, vértices o extremos se les vuelve a dibujar la apariencia: PDFium
/// no la genera, y sin ella el resaltado importado no existiría fuera de
/// Vitela.
///
/// Un comentario cuya página no está en este documento se salta: el XFDF no
/// dice de qué PDF viene, y escribirlo en otra página sería inventárselo.
#[tauri::command(async)]
pub fn import_comments_xfdf(work_path: String, src_path: String) -> Result<u32, String> {
    let xml = std::fs::read_to_string(&src_path).map_err(|e| {
        crate::mensaje_llano(format!("No se ha podido leer {src_path}: {e}"))
    })?;
    let leidos = lee_xfdf(&xml)?;
    if leidos.is_empty() {
        return Err("Ese fichero no trae ningún comentario".into());
    }
    let cuantos = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
    let hechos = cuantos.clone();
    crate::cirugia(&work_path, move |doc| {
        let paginas = doc.get_pages();
        let mut por_nombre: std::collections::HashMap<String, lopdf::ObjectId> = Default::default();
        let mut pendientes: Vec<(lopdf::ObjectId, String)> = Vec::new();
        let mut nuevas: Vec<(u16, lopdf::ObjectId, Anot)> = Vec::new();
        let mut n = 0u32;
        for a in &leidos {
            let Some(page_id) = paginas.get(&(a.page as u32 + 1)).copied() else {
                continue;
            };
            let id = escribe_annot(doc, a);
            crate::formularios2::anade_a_annots(doc, page_id, id)?;
            if !a.name.is_empty() {
                por_nombre.insert(a.name.clone(), id);
            }
            if !a.inreplyto.is_empty() {
                pendientes.push((id, a.inreplyto.clone()));
            }
            nuevas.push((a.page, id, a.clone()));
            n += 1;
        }
        // el hilo, cuando ya están todos: un `inreplyto` puede señalar a un
        // comentario que aparece después en el fichero
        for (id, a_quien) in pendientes {
            if let Some(padre) = por_nombre.get(&a_quien) {
                if let Ok(d) = doc.get_object_mut(id).and_then(|o| o.as_dict_mut()) {
                    d.set("IRT", lopdf::Object::Reference(*padre));
                }
            }
        }
        // la apariencia de lo que Vitela sabe dibujar; sin ella el
        // resaltado importado no se vería fuera de aquí
        for (page_index, id, a) in &nuevas {
            let indice = crate::anotaciones::lista_annots(doc, *page_index)
                .and_then(|l| {
                    l.iter()
                        .position(|o| matches!(o, lopdf::Object::Reference(r) if r == id))
                });
            let (Some(indice), Some(estilo)) = (indice, estilo_de_marca(&a.subtipo)) else {
                continue;
            };
            crate::anotaciones::escribe_apariencia_marca(doc, *page_index, indice, estilo)?;
        }
        // las notas importadas llevan su ventana, como las de aquí
        crate::anotaciones::repon_popups(doc)?;
        hechos.store(n, std::sync::atomic::Ordering::SeqCst);
        Ok(())
    })?;
    Ok(cuantos.load(std::sync::atomic::Ordering::SeqCst))
}

/// El estilo con el que se redibuja una marca de texto importada, o `None`
/// si ese subtipo no es una marca.
fn estilo_de_marca(subtipo: &str) -> Option<crate::anotaciones::EstiloMarca> {
    use crate::anotaciones::EstiloMarca;
    match subtipo {
        "Highlight" => Some(EstiloMarca::Resaltado),
        "Underline" => Some(EstiloMarca::Subrayado),
        "StrikeOut" => Some(EstiloMarca::Tachado),
        _ => None,
    }
}

/// Una anotación leída del XFDF.
#[derive(Clone, Default, Debug)]
struct Anot {
    subtipo: String,
    page: u16,
    rect: Vec<f32>,
    color: Option<[f32; 3]>,
    title: String,
    subject: String,
    date: String,
    creationdate: String,
    icon: String,
    name: String,
    inreplyto: String,
    reply_type: String,
    state_model: String,
    state: String,
    coords: Vec<f32>,
    vertices: Vec<f32>,
    linea: Vec<f32>,
    trazos: Vec<Vec<f32>>,
    contents: String,
}

/// El diccionario de una anotación importada.
fn escribe_annot(doc: &mut lopdf::Document, a: &Anot) -> lopdf::ObjectId {
    use lopdf::{Dictionary, Object};
    let mut d = Dictionary::new();
    d.set("Type", Object::Name(b"Annot".to_vec()));
    d.set("Subtype", Object::Name(a.subtipo.as_bytes().to_vec()));
    d.set(
        "Rect",
        Object::Array(a.rect.iter().map(|n| (*n).into()).collect()),
    );
    d.set("F", 4i64); // Print
    if let Some([r, g, b]) = a.color {
        d.set("C", Object::Array(vec![r.into(), g.into(), b.into()]));
    }
    if !a.contents.is_empty() {
        d.set("Contents", crate::documento::cadena_pdf(&a.contents));
    }
    for (clave, valor) in [
        ("T", &a.title),
        ("Subj", &a.subject),
        ("State", &a.state),
    ] {
        if !valor.is_empty() {
            d.set(clave, crate::documento::cadena_pdf(valor));
        }
    }
    for (clave, valor) in [("M", &a.date), ("CreationDate", &a.creationdate)] {
        if !valor.is_empty() {
            d.set(clave, Object::string_literal(valor.clone()));
        }
    }
    for (clave, valor) in [
        ("Name", &a.icon),
        ("RT", &a.reply_type),
        ("StateModel", &a.state_model),
    ] {
        if !valor.is_empty() {
            d.set(clave, Object::Name(valor.as_bytes().to_vec()));
        }
    }
    if !a.name.is_empty() {
        d.set("NM", crate::documento::cadena_pdf(&a.name));
    }
    if !a.coords.is_empty() {
        d.set(
            "QuadPoints",
            Object::Array(a.coords.iter().map(|n| (*n).into()).collect()),
        );
    }
    if !a.vertices.is_empty() {
        d.set(
            "Vertices",
            Object::Array(a.vertices.iter().map(|n| (*n).into()).collect()),
        );
    }
    if a.linea.len() == 4 {
        d.set(
            "L",
            Object::Array(a.linea.iter().map(|n| (*n).into()).collect()),
        );
    }
    if !a.trazos.is_empty() {
        d.set(
            "InkList",
            Object::Array(
                a.trazos
                    .iter()
                    .map(|t| Object::Array(t.iter().map(|n| (*n).into()).collect()))
                    .collect(),
            ),
        );
    }
    doc.add_object(d)
}

/// Lee un XFDF. Solo se mira `<annots>`: un XFDF puede traer también los
/// campos de un formulario (`<fields>`), que es otra función y otro día.
fn lee_xfdf(xml: &str) -> Result<Vec<Anot>, String> {
    use quick_xml::events::Event;
    let mut lector = quick_xml::Reader::from_str(xml);
    let dec = lector.decoder();
    let mut out: Vec<Anot> = Vec::new();
    let mut actual: Option<Anot> = None;
    let mut dentro_de = String::new();
    let mut buffer = String::new();
    // lee los atributos de un elemento de anotación
    let lee = |e: &quick_xml::events::BytesStart, subtipo: &str| -> Anot {
        let mut a = Anot { subtipo: subtipo.to_string(), ..Default::default() };
        for at in e.attributes().flatten() {
            let clave = String::from_utf8_lossy(at.key.local_name().as_ref()).to_lowercase();
            let valor = at
                .decoded_and_normalized_value(quick_xml::XmlVersion::Explicit1_0, dec)
                .unwrap_or_default()
                .into_owned();
            pon_atributo(&mut a, &clave, &valor);
        }
        a
    };
    loop {
        match lector.read_event() {
            Err(e) => {
                return Err(crate::mensaje_llano(format!(
                    "Ese fichero no es un XFDF que se pueda leer: {e}"
                )))
            }
            Ok(Event::Eof) => break,
            // un elemento que se abre y se cierra de una vez: un comentario
            // sin texto, que es legal
            Ok(Event::Empty(e)) => {
                let nombre = String::from_utf8_lossy(e.local_name().as_ref()).to_lowercase();
                if let Some(subtipo) = subtipo_de(&nombre) {
                    let a = lee(&e, subtipo);
                    if a.rect.len() == 4 {
                        out.push(a);
                    }
                }
            }
            Ok(Event::Start(e)) => {
                let nombre = String::from_utf8_lossy(e.local_name().as_ref()).to_lowercase();
                match subtipo_de(&nombre) {
                    Some(subtipo) => actual = Some(lee(&e, subtipo)),
                    None => {
                        dentro_de = nombre;
                        buffer.clear();
                    }
                }
            }
            Ok(Event::Text(t)) => {
                let crudo = t.decode().unwrap_or_default().into_owned();
                let limpio = quick_xml::escape::unescape(&crudo)
                    .map(|c| c.into_owned())
                    .unwrap_or_else(|_| crudo.clone());
                buffer.push_str(&limpio);
            }
            Ok(Event::CData(t)) => {
                buffer.push_str(&String::from_utf8_lossy(t.as_ref()));
            }
            Ok(Event::End(e)) => {
                let nombre = String::from_utf8_lossy(e.local_name().as_ref()).to_lowercase();
                if let Some(a) = actual.as_mut() {
                    if nombre == "contents" && dentro_de == "contents" {
                        a.contents = buffer.clone();
                    } else if nombre == "gesture" && dentro_de == "gesture" {
                        let puntos: Vec<f32> = buffer
                            .split([';', ','])
                            .filter_map(|n| n.trim().parse().ok())
                            .collect();
                        if puntos.len() >= 4 {
                            a.trazos.push(puntos);
                        }
                    }
                }
                buffer.clear();
                dentro_de.clear();
                if subtipo_de(&nombre).is_some() {
                    if let Some(a) = actual.take() {
                        if a.rect.len() == 4 {
                            out.push(a);
                        }
                    }
                }
            }
            _ => {}
        }
    }
    Ok(out)
}

/// El subtipo de PDF que le toca a un elemento del XFDF, o `None` si ese
/// elemento no es una anotación.
fn subtipo_de(etiqueta: &str) -> Option<&'static str> {
    SUBTIPOS
        .iter()
        .find(|s| s.to_lowercase() == etiqueta)
        .copied()
}

fn pon_atributo(a: &mut Anot, clave: &str, valor: &str) {
    let numeros = |s: &str| -> Vec<f32> {
        s.split([',', ';', ' '])
            .filter_map(|n| n.trim().parse().ok())
            .collect()
    };
    match clave {
        "page" => a.page = valor.trim().parse().unwrap_or(0),
        "rect" => a.rect = numeros(valor),
        "coords" => a.coords = numeros(valor),
        "vertices" => a.vertices = numeros(valor),
        "start" => {
            let p = numeros(valor);
            a.linea.splice(0..a.linea.len().min(2), p);
        }
        "end" => {
            while a.linea.len() < 2 {
                a.linea.push(0.0);
            }
            a.linea.truncate(2);
            a.linea.extend(numeros(valor));
        }
        "color" => {
            let h = valor.trim_start_matches('#');
            if h.len() == 6 {
                let byte = |i: usize| {
                    u8::from_str_radix(&h[i..i + 2], 16).unwrap_or(0) as f32 / 255.0
                };
                a.color = Some([byte(0), byte(2), byte(4)]);
            }
        }
        "title" => a.title = valor.to_string(),
        "subject" => a.subject = valor.to_string(),
        "date" => a.date = valor.to_string(),
        "creationdate" => a.creationdate = valor.to_string(),
        "icon" => a.icon = valor.to_string(),
        "name" => a.name = valor.to_string(),
        "inreplyto" => a.inreplyto = valor.to_string(),
        "replytype" => a.reply_type = valor.to_string(),
        "statemodel" => a.state_model = valor.to_string(),
        "state" => a.state = valor.to_string(),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::crea_pdf;

    /// Un documento con un hilo de tres (una nota y dos respuestas), un
    /// resaltado y un dibujo, que es lo que hay que poder devolverle a quien
    /// pidió la revisión.
    fn documento_revisado(nombre: &str) -> String {
        let tmp = std::env::temp_dir().join(nombre);
        crea_pdf(&["Contrato de prueba", "Segunda página"], &tmp);
        let work = tmp.to_string_lossy().into_owned();
        crate::anotaciones::add_note(
            work.clone(),
            0,
            80.0,
            120.0,
            "Esto hay que revisarlo".into(),
            Some("Jorge".into()),
        )
        .expect("la nota");
        crate::comentarios::reply_annotation(
            work.clone(),
            0,
            0,
            "Revisado, falta la fecha".into(),
            Some("Ana".into()),
        )
        .expect("la primera respuesta");
        crate::comentarios::reply_annotation(
            work.clone(),
            0,
            0,
            "Fecha puesta".into(),
            Some("Jorge".into()),
        )
        .expect("la segunda respuesta");
        crate::anotaciones2::add_markup(
            work.clone(),
            0,
            vec![crate::Rect { x: 50.0, y: 130.0, w: 120.0, h: 14.0 }],
            "highlight".into(),
            Some([255, 220, 0, 255]),
            Some("Ana".into()),
        )
        .expect("el resaltado");
        crate::anotaciones::add_stroke(
            work.clone(),
            1,
            (0..6).map(|i| [100.0 + i as f32 * 20.0, 200.0]).collect(),
            None,
            None,
            Some("Ana".into()),
        )
        .expect("el dibujo");
        work
    }

    /// **AC-071.** La criba de subtipos es **una sola** para el XFDF y para
    /// el panel: mientras cada mitad tenía la suya, el XFDF salía limpio y
    /// el panel listaba los campos del formulario como comentarios.
    #[test]
    fn un_campo_un_enlace_y_una_firma_no_son_comentarios() {
        for si in ["Text", "Highlight", "Ink", "FreeText", "Stamp", "FileAttachment"] {
            assert!(es_comentario(si), "{si} es un comentario");
        }
        for no in ["Widget", "Link", "Popup", "Sig", "Screen", "Unknown"] {
            assert!(!es_comentario(no), "{no} no es un comentario");
        }
        // el spec escribe `/StrikeOut` y `/PolyLine`; PDFium, `Strikeout` y
        // `Polyline`. Son el mismo subtipo.
        for uno in ["StrikeOut", "Strikeout", "PolyLine", "Polyline"] {
            assert!(es_comentario(uno), "{uno} es un comentario");
        }
    }

    /// **H9.** «Crear resumen de comentarios» produce **un PDF** con una
    /// fila por comentario, las respuestas sangradas bajo el suyo y el
    /// orden que se pida. Es lo que se imprime y lo que se manda por correo.
    #[test]
    fn el_resumen_de_comentarios_sale_en_un_pdf_que_se_puede_abrir() {
        let work = documento_revisado("comentarios2-resumen.pdf");
        let dest = std::env::temp_dir().join("comentarios2-resumen-salida.pdf");
        let d = dest.to_string_lossy().into_owned();

        let n = export_comments_pdf(work.clone(), d.clone(), "pagina".into(), Some("acta.pdf".into()))
            .expect("el resumen");
        assert_eq!(n, 5, "las cinco filas: la nota, sus dos respuestas, el resaltado y el dibujo");

        // se abre como cualquier PDF y dice lo que tiene que decir
        let texto = crate::tests::textos_de(&dest).join(" ");
        assert!(texto.contains("Comentarios de acta.pdf"), "la cabecera: {texto}");
        assert!(texto.contains("Esto hay que revisarlo"), "el texto de la nota: {texto}");
        assert!(texto.contains("Revisado, falta la fecha"), "las respuestas: {texto}");
        assert!(texto.contains("En respuesta"), "y se ven como respuestas: {texto}");
        assert!(texto.contains("Página 1"), "cada fila dice en qué página cae: {texto}");
        assert!(texto.contains("Resaltado") && texto.contains("Dibujo"), "el tipo en español: {texto}");
        assert!(texto.contains("Jorge") && texto.contains("Ana"), "quién lo dijo: {texto}");
        // el nombre del temporal no se enseña nunca
        assert!(!texto.contains("vitela-"), "el nombre del temporal: {texto}");

        // ordenar por autor no parte los hilos: la respuesta va detrás de su
        // comentario aunque sea de otra persona
        let d2 = std::env::temp_dir().join("comentarios2-resumen-autor.pdf");
        let d2s = d2.to_string_lossy().into_owned();
        export_comments_pdf(work.clone(), d2s, "autor".into(), None).expect("por autor");
        let texto = crate::tests::textos_de(&d2).join(" ");
        let nota = texto.find("Esto hay que revisarlo").expect("la nota");
        let respuesta = texto.find("Revisado, falta la fecha").expect("la respuesta");
        assert!(respuesta > nota, "el hilo sigue junto y en orden");

        // un documento sin comentarios lo dice, no escribe un PDF vacío
        let liso = std::env::temp_dir().join("comentarios2-liso.pdf");
        crea_pdf(&["Sin comentarios"], &liso);
        let err = export_comments_pdf(
            liso.to_string_lossy().into_owned(),
            d.clone(),
            "pagina".into(),
            None,
        )
        .unwrap_err();
        assert!(err.contains("no tiene comentarios"), "{err}");

        for p in [&dest, &d2, &liso] {
            std::fs::remove_file(p).ok();
        }
        std::fs::remove_file(&work).ok();
    }

    /// **H9.** Un XFDF exportado y vuelto a importar sobre una copia limpia
    /// devuelve **los mismos comentarios**: autores, fechas, textos y
    /// anidamiento. Es lo que cierra el flujo de revisión que abrió el ciclo
    /// 5: hasta aquí se podía revisar en Vitela y no se podía devolver la
    /// revisión a quien la pidió.
    #[test]
    fn los_comentarios_van_y_vuelven_en_xfdf_con_su_hilo() {
        let work = documento_revisado("comentarios2-xfdf.pdf");
        let antes = crate::anotaciones::get_document_annotations(work.clone()).expect("listar");
        let xfdf = std::env::temp_dir().join("comentarios2-revision.xfdf");
        let x = xfdf.to_string_lossy().into_owned();
        let n = export_comments_xfdf(work.clone(), x.clone()).expect("exportar");
        assert_eq!(
            n as usize,
            antes.len(),
            "los cinco comentarios; el /Popup de la nota no es un comentario y no viaja"
        );

        let xml = std::fs::read_to_string(&xfdf).expect("leer el xfdf");
        assert!(xml.starts_with("<?xml"), "es XML: {xml}");
        assert!(xml.contains("<text "), "la nota: {xml}");
        assert!(xml.contains("inreplyto="), "el hilo viaja: {xml}");
        assert!(xml.contains("Esto hay que revisarlo"), "el texto: {xml}");
        assert!(xml.contains("title=\"Jorge\""), "el autor: {xml}");
        assert!(xml.contains("<inklist>"), "el dibujo sale como gestos: {xml}");

        // una copia limpia del mismo documento, sin un solo comentario
        let limpio = std::env::temp_dir().join("comentarios2-xfdf-limpio.pdf");
        crea_pdf(&["Contrato de prueba", "Segunda página"], &limpio);
        let l = limpio.to_string_lossy().into_owned();
        assert!(crate::anotaciones::get_document_annotations(l.clone())
            .expect("listar")
            .is_empty());

        let pasos = crate::historial::history_state(l.clone()).expect("historial").undo;
        let puestos = import_comments_xfdf(l.clone(), x.clone()).expect("importar");
        assert_eq!(puestos, n, "vuelven todos");
        assert_eq!(
            crate::historial::history_state(l.clone()).expect("historial").undo,
            pasos + 1,
            "el lote entero es un solo paso de deshacer"
        );

        let despues = crate::anotaciones::get_document_annotations(l.clone()).expect("listar");
        assert_eq!(despues.len(), antes.len(), "los mismos comentarios: {despues:?}");
        for (a, b) in antes.iter().zip(despues.iter()) {
            assert_eq!(a.annot.kind, b.annot.kind);
            assert_eq!(a.annot.contents, b.annot.contents);
            assert_eq!(a.annot.author, b.annot.author, "el autor de {:?}", a.annot.contents);
            assert_eq!(a.annot.modified, b.annot.modified, "la fecha de {:?}", a.annot.contents);
            assert_eq!(a.page_index, b.page_index);
        }
        // el anidamiento: las dos respuestas siguen colgando de la nota
        let respuestas: Vec<&crate::anotaciones::AnotacionDoc> =
            despues.iter().filter(|c| c.annot.in_reply_to.is_some()).collect();
        assert_eq!(respuestas.len(), 2, "las dos respuestas: {despues:?}");
        let nota = despues
            .iter()
            .find(|c| c.annot.contents.contains("hay que revisarlo"))
            .expect("la nota");
        for r in &respuestas {
            assert_eq!(r.annot.in_reply_to, Some(nota.annot.index), "cuelgan de la nota");
        }
        // el resaltado vuelve con su apariencia, o fuera de Vitela no
        // existiría
        crate::render_page_png(l.clone(), 0, 200, true).expect("render con lo importado");

        // importar **añade**: hacerlo dos veces no sustituye a nadie
        import_comments_xfdf(l.clone(), x.clone()).expect("importar otra vez");
        assert_eq!(
            crate::anotaciones::get_document_annotations(l.clone())
                .expect("listar")
                .len(),
            antes.len() * 2,
            "la segunda tanda se suma a la primera"
        );

        // un fichero que no es un XFDF se dice en llano
        let basura = std::env::temp_dir().join("comentarios2-basura.xfdf");
        std::fs::write(&basura, b"esto no es XML").expect("escribir");
        let err = import_comments_xfdf(l.clone(), basura.to_string_lossy().into_owned())
            .unwrap_err();
        assert!(err.contains("comentario") || err.contains("XFDF"), "el aviso: {err}");
        assert!(!err.contains("os error"), "jerga: {err}");

        for p in [&xfdf, &limpio, &basura] {
            std::fs::remove_file(p).ok();
        }
        std::fs::remove_file(&work).ok();
    }
}
