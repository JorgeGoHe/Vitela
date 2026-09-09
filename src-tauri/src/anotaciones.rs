//! Anotaciones básicas: resaltado, trazo (Ink), nota, listado y borrado.

use crate::{cirugia_en_hilo, on_pdfium_thread, pdfium, save_and_close, with_doc, with_lopdf, Geo, Rect};
use crate::historial::mutacion;
use pdfium_render::prelude::*;
use serde::Serialize;


/// Añade un trazo a mano alzada como anotación Ink con su apariencia
/// (un path dentro de la anotación), de modo que se puede borrar
/// individualmente. Los puntos vienen en coords de UI (puntos PDF,
/// origen arriba-izquierda).
#[tauri::command(async)]
pub fn add_stroke(
    work_path: String,
    page_index: u16,
    points: Vec<[f32; 2]>,
    color: Option<[u8; 4]>,
    width: Option<f32>,
    author: Option<String>,
) -> Result<(), String> {
    if points.len() < 2 {
        return Err("Trazo demasiado corto".into());
    }
    let c = color.unwrap_or([226, 61, 61, 255]);
    let w = width.unwrap_or(2.0).clamp(0.5, 12.0);
    mutacion(work_path, |work_path| on_pdfium_thread(move || {
        let pdfium = pdfium()?;
        let doc = pdfium
            .load_pdf_from_file(&work_path, None)
            .map_err(|e| e.to_string())?;
        let mut page = doc.pages().get(page_index).map_err(|e| e.to_string())?;
        let geo = Geo::de_pagina(&page).propia();
        let puntos: Vec<(f32, f32)> = points.iter().map(|p| geo.ui_a_pdf(p[0], p[1])).collect();
        let mut annot = page
            .annotations_mut()
            .create_ink_annotation()
            .map_err(|e| e.to_string())?;
        annot.set_is_printed(true).map_err(|e| e.to_string())?;
        // /C antes de añadir el path (con /AP PDFium ya no deja fijarlo);
        // es lo que lee get_annotations, el color del path no se ve fuera
        annot
            .set_stroke_color(PdfColor::new(c[0], c[1], c[2], c[3]))
            .map_err(|e| e.to_string())?;
        const MARGIN: f32 = 3.0;
        let min_x = puntos.iter().map(|p| p.0).fold(f32::MAX, f32::min) - MARGIN;
        let max_x = puntos.iter().map(|p| p.0).fold(f32::MIN, f32::max) + MARGIN;
        let min_y = puntos.iter().map(|p| p.1).fold(f32::MAX, f32::min) - MARGIN;
        let max_y = puntos.iter().map(|p| p.1).fold(f32::MIN, f32::max) + MARGIN;
        annot
            .set_bounds(PdfRect::new(
                PdfPoints::new(min_y),
                PdfPoints::new(min_x),
                PdfPoints::new(max_y),
                PdfPoints::new(max_x),
            ))
            .map_err(|e| e.to_string())?;
        let mut path = PdfPagePathObject::new(
            &doc,
            PdfPoints::new(puntos[0].0),
            PdfPoints::new(puntos[0].1),
            Some(PdfColor::new(c[0], c[1], c[2], c[3])),
            Some(PdfPoints::new(w)),
            None,
        )
        .map_err(|e| e.to_string())?;
        for p in &puntos[1..] {
            path.line_to(PdfPoints::new(p.0), PdfPoints::new(p.1))
                .map_err(|e| e.to_string())?;
        }
        annot
            .objects_mut()
            .add_path_object(path)
            .map_err(|e| e.to_string())?;
        drop(page);
        save_and_close(doc, &work_path)?;
        remata_annot(&work_path, page_index, None, author)
    }))
}

/// Crea una nota (anotación de texto) en el punto dado (coords de UI).
#[tauri::command(async)]
pub fn add_note(
    work_path: String,
    page_index: u16,
    x: f32,
    y: f32,
    text: String,
    author: Option<String>,
) -> Result<(), String> {
    if text.trim().is_empty() {
        return Err("La nota está vacía".into());
    }
    mutacion(work_path, |work_path| on_pdfium_thread(move || {
        let pdfium = pdfium()?;
        let doc = pdfium
            .load_pdf_from_file(&work_path, None)
            .map_err(|e| e.to_string())?;
        let mut page = doc.pages().get(page_index).map_err(|e| e.to_string())?;
        let geo = Geo::de_pagina(&page).propia();
        let mut annot = page
            .annotations_mut()
            .create_text_annotation(&text)
            .map_err(|e| e.to_string())?;
        annot.set_is_printed(true).map_err(|e| e.to_string())?;
        const ICON: f32 = 22.0;
        annot
            .set_bounds(geo.ui_rect_a_pdf(&Rect {
                x,
                y,
                w: ICON,
                h: ICON,
            }))
            .map_err(|e| e.to_string())?;
        drop(page);
        save_and_close(doc, &work_path)?;
        remata_annot(&work_path, page_index, None, author)?;
        // el post-it de Acrobat: icono de bocadillo y ventana emergente
        cirugia_en_hilo(&work_path, move |doc| {
            let i = ultima_annot(doc, page_index)?;
            let id = annot_id(doc, page_index, i)?;
            {
                let annot = doc
                    .get_object_mut(id)
                    .and_then(|o| o.as_dict_mut())
                    .map_err(|e| e.to_string())?;
                annot.set("Name", lopdf::Object::Name(b"Comment".to_vec()));
            }
            anade_popup(doc, page_index, id)
        })
    }))
}

/// Ventana emergente de un comentario (`/Popup` con `/Open false`), a la
/// derecha del icono: es lo que abren Acrobat y Vista Previa al pulsarlo.
/// El popup no es un comentario — `get_annotations` no lo lista — y se
/// borra con su nota.
fn anade_popup(
    doc: &mut lopdf::Document,
    page_index: u16,
    padre: lopdf::ObjectId,
) -> Result<(), String> {
    use lopdf::{Dictionary, Object};
    let rect: Vec<f32> = doc
        .get_object(padre)
        .and_then(|o| o.as_dict())
        .and_then(|d| d.get(b"Rect"))
        .and_then(|o| o.as_array())
        .map_err(|e| e.to_string())?
        .iter()
        .filter_map(numero)
        .collect();
    if rect.len() != 4 {
        return Err("La nota no tiene rectángulo".into());
    }
    const ANCHO: f32 = 180.0;
    const ALTO: f32 = 100.0;
    let mut popup = Dictionary::new();
    popup.set("Type", Object::Name(b"Annot".to_vec()));
    popup.set("Subtype", Object::Name(b"Popup".to_vec()));
    popup.set(
        "Rect",
        Object::Array(vec![
            rect[2].into(),
            (rect[3] - ALTO).into(),
            (rect[2] + ANCHO).into(),
            rect[3].into(),
        ]),
    );
    popup.set("Parent", Object::Reference(padre));
    popup.set("Open", Object::Boolean(false));
    popup.set("F", 28i64); // Print + NoZoom + NoRotate, como Acrobat
    let popup_id = doc.add_object(popup);
    let page_id = *doc
        .get_pages()
        .get(&(page_index as u32 + 1))
        .ok_or("Página fuera de rango")?;
    crate::formularios2::anade_a_annots(doc, page_id, popup_id)?;
    doc.get_object_mut(padre)
        .and_then(|o| o.as_dict_mut())
        .map_err(|e| e.to_string())?
        .set("Popup", Object::Reference(popup_id));
    Ok(())
}

/// Estilo de la apariencia de una marca de texto.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum EstiloMarca {
    /// Rectángulo por quad en `/BM /Multiply`, como Acrobat.
    Resaltado,
    /// Línea fina en la base del quad.
    Subrayado,
    /// Línea fina a media altura del quad.
    Tachado,
}

/// Grosor de la línea de subrayado/tachado, en puntos (el de Acrobat).
const GROSOR_LINEA: f32 = 1.0;

fn numero(o: &lopdf::Object) -> Option<f32> {
    match o {
        lopdf::Object::Integer(i) => Some(*i as f32),
        lopdf::Object::Real(r) => Some(*r),
        _ => None,
    }
}

/// Quads de una anotación de marcado como rectángulos PDF
/// `(x0, y0, x1, y1)` con origen abajo-izquierda.
fn quads_de(annot: &lopdf::Dictionary) -> Vec<(f32, f32, f32, f32)> {
    let Ok(lopdf::Object::Array(qp)) = annot.get(b"QuadPoints") else {
        return Vec::new();
    };
    qp.chunks(8)
        .filter(|c| c.len() == 8)
        .filter_map(|c| {
            let v: Vec<f32> = c.iter().filter_map(numero).collect();
            if v.len() != 8 {
                return None;
            }
            let xs = [v[0], v[2], v[4], v[6]];
            let ys = [v[1], v[3], v[5], v[7]];
            Some((
                xs.iter().copied().fold(f32::MAX, f32::min),
                ys.iter().copied().fold(f32::MAX, f32::min),
                xs.iter().copied().fold(f32::MIN, f32::max),
                ys.iter().copied().fold(f32::MIN, f32::max),
            ))
        })
        .collect()
}

/// Escribe a mano el `/AP` (Form XObject de apariencia normal) de la marca
/// de texto que está en el índice `annot_index` de `/Annots`, más `/F 4`
/// (Print) y `/CA`.
///
/// Por qué a mano: PDFium genera la apariencia de las marcas en memoria al
/// cargar el documento —por eso se ven en `render_page`— pero NO la escribe
/// al guardar, así que el resaltado no existe para ningún otro visor, ni al
/// imprimir desde otra aplicación, ni tras `flatten_pdf`. Acrobat siempre
/// escribe el `/AP`; esto lo iguala.
///
/// El truco de coordenadas es el habitual: `/BBox` igual al `/Rect` de la
/// anotación y `/Matrix` implícita (identidad), de modo que dentro del
/// stream se dibuja directamente en coordenadas de página.
pub(crate) fn escribe_apariencia_marca(
    doc: &mut lopdf::Document,
    page_index: u16,
    annot_index: usize,
    estilo: EstiloMarca,
) -> Result<(), String> {
    use lopdf::{Dictionary, Object, Stream};

    let annot_id = crate::anotaciones::annot_id(doc, page_index, annot_index)?;
    let annot = doc
        .get_object(annot_id)
        .and_then(|o| o.as_dict())
        .map_err(|e| e.to_string())?
        .clone();

    let quads = quads_de(&annot);
    if quads.is_empty() {
        return Ok(()); // sin quads no hay nada que pintar
    }
    let color = match annot.get(b"C") {
        Ok(Object::Array(c)) if c.len() == 3 => [
            numero(&c[0]).unwrap_or(0.0),
            numero(&c[1]).unwrap_or(0.0),
            numero(&c[2]).unwrap_or(0.0),
        ],
        Ok(Object::Array(c)) if c.len() == 1 => {
            let g = numero(&c[0]).unwrap_or(0.0);
            [g, g, g]
        }
        _ => [1.0, 0.86, 0.0],
    };

    // BBox = envolvente de los quads, con holgura para el grosor de línea
    let x0 = quads.iter().map(|q| q.0).fold(f32::MAX, f32::min);
    let y0 = quads.iter().map(|q| q.1).fold(f32::MAX, f32::min);
    let x1 = quads.iter().map(|q| q.2).fold(f32::MIN, f32::max);
    let y1 = quads.iter().map(|q| q.3).fold(f32::MIN, f32::max);

    let mut ops = String::new();
    if estilo == EstiloMarca::Resaltado {
        ops.push_str("/GSm gs\n");
    }
    ops.push_str(&format!("{:.4} {:.4} {:.4} rg\n", color[0], color[1], color[2]));
    for (qx0, qy0, qx1, qy1) in &quads {
        let (x, y, w, h) = match estilo {
            EstiloMarca::Resaltado => (*qx0, *qy0, qx1 - qx0, qy1 - qy0),
            // Acrobat deja el subrayado justo por debajo de la línea base
            EstiloMarca::Subrayado => (*qx0, *qy0, qx1 - qx0, GROSOR_LINEA),
            EstiloMarca::Tachado => (
                *qx0,
                (qy0 + qy1) / 2.0 - GROSOR_LINEA / 2.0,
                qx1 - qx0,
                GROSOR_LINEA,
            ),
        };
        ops.push_str(&format!("{x:.4} {y:.4} {w:.4} {h:.4} re\n"));
    }
    ops.push_str("f\n");

    let mut recursos = Dictionary::new();
    if estilo == EstiloMarca::Resaltado {
        // Multiply: el amarillo deja leer el texto que hay debajo, igual
        // que Acrobat (que por eso usa opacidad 1 y no transparencia)
        let mut gs = Dictionary::new();
        gs.set("Type", Object::Name(b"ExtGState".to_vec()));
        gs.set("BM", Object::Name(b"Multiply".to_vec()));
        gs.set("CA", Object::Real(1.0));
        gs.set("ca", Object::Real(1.0));
        let mut estados = Dictionary::new();
        estados.set("GSm", Object::Dictionary(gs));
        recursos.set("ExtGState", Object::Dictionary(estados));
    }

    let mut forma = Dictionary::new();
    forma.set("Type", Object::Name(b"XObject".to_vec()));
    forma.set("Subtype", Object::Name(b"Form".to_vec()));
    forma.set("FormType", 1i64);
    forma.set(
        "BBox",
        Object::Array(vec![x0.into(), y0.into(), x1.into(), y1.into()]),
    );
    forma.set("Resources", Object::Dictionary(recursos));
    if estilo == EstiloMarca::Resaltado {
        // grupo de transparencia: sin él algunos visores ignoran el /BM
        let mut grupo = Dictionary::new();
        grupo.set("S", Object::Name(b"Transparency".to_vec()));
        grupo.set("CS", Object::Name(b"DeviceRGB".to_vec()));
        forma.set("Group", Object::Dictionary(grupo));
    }
    let ap_id = doc.add_object(Stream::new(forma, ops.into_bytes()));

    let annot = doc
        .get_object_mut(annot_id)
        .and_then(|o| o.as_dict_mut())
        .map_err(|e| e.to_string())?;
    let mut ap = Dictionary::new();
    ap.set("N", Object::Reference(ap_id));
    annot.set("AP", Object::Dictionary(ap));
    annot.set("F", 4i64); // Print
    annot.set("CA", Object::Real(1.0));
    Ok(())
}

/// Id del objeto de la anotación `index` de la página, resolviendo el
/// `/Annots` esté por referencia o en línea. Si la entrada es un
/// diccionario directo lo convierte en objeto propio para poder mutarlo.
pub(crate) fn annot_id(
    doc: &mut lopdf::Document,
    page_index: u16,
    index: usize,
) -> Result<lopdf::ObjectId, String> {
    use lopdf::Object;
    let page_id = *doc
        .get_pages()
        .get(&(page_index as u32 + 1))
        .ok_or("Página fuera de rango")?;
    let annots_ref = doc
        .get_object(page_id)
        .and_then(|o| o.as_dict())
        .map_err(|e| e.to_string())?
        .get(b"Annots")
        .map_err(|_| "La página no tiene anotaciones".to_string())?
        .clone();
    let (lista, contenedor) = match &annots_ref {
        Object::Reference(rid) => (
            doc.get_object(*rid)
                .and_then(|o| o.as_array())
                .map_err(|e| e.to_string())?
                .clone(),
            Some(*rid),
        ),
        Object::Array(a) => (a.clone(), None),
        _ => return Err("El /Annots de la página no es una lista".into()),
    };
    let entrada = lista.get(index).ok_or("Anotación fuera de rango")?;
    match entrada {
        Object::Reference(rid) => Ok(*rid),
        Object::Dictionary(d) => {
            // anotación en línea: se promueve a objeto indirecto
            let nuevo = doc.add_object(Object::Dictionary(d.clone()));
            match contenedor {
                Some(rid) => {
                    doc.get_object_mut(rid)
                        .and_then(|o| o.as_array_mut())
                        .map_err(|e| e.to_string())?[index] = Object::Reference(nuevo);
                }
                None => {
                    doc.get_object_mut(page_id)
                        .and_then(|o| o.as_dict_mut())
                        .and_then(|d| d.get_mut(b"Annots"))
                        .and_then(|o| o.as_array_mut())
                        .map_err(|e| e.to_string())?[index] = Object::Reference(nuevo);
                }
            }
            Ok(nuevo)
        }
        _ => Err("Entrada de /Annots inesperada".into()),
    }
}

/// Número de anotaciones de una página según el fichero en disco: el índice
/// de la que acaba de crear PDFium es el último.
pub(crate) fn ultima_annot(doc: &lopdf::Document, page_index: u16) -> Result<usize, String> {
    use lopdf::Object;
    let page_id = *doc
        .get_pages()
        .get(&(page_index as u32 + 1))
        .ok_or("Página fuera de rango")?;
    let annots = doc
        .get_object(page_id)
        .and_then(|o| o.as_dict())
        .map_err(|e| e.to_string())?
        .get(b"Annots")
        .map_err(|_| "La página no tiene anotaciones".to_string())?;
    let n = match annots {
        Object::Reference(rid) => doc
            .get_object(*rid)
            .and_then(|o| o.as_array())
            .map_err(|e| e.to_string())?
            .len(),
        Object::Array(a) => a.len(),
        _ => return Err("El /Annots de la página no es una lista".into()),
    };
    n.checked_sub(1).ok_or_else(|| "La página no tiene anotaciones".into())
}

/// Autor de un comentario: el que manda la UI (preferencia del usuario) o,
/// si no lo manda, el nombre de usuario del sistema — Acrobat tampoco
/// pregunta la primera vez.
pub(crate) fn autor_o_sistema(author: Option<String>) -> String {
    let dado = author.map(|a| a.trim().to_string()).filter(|a| !a.is_empty());
    dado
        .or_else(|| std::env::var("USER").ok())
        .or_else(|| std::env::var("USERNAME").ok())
        .or_else(|| std::env::var("LOGNAME").ok())
        .map(|a| a.trim().to_string())
        .filter(|a| !a.is_empty())
        .unwrap_or_else(|| "Usuario".into())
}

/// Fecha actual en el formato de fecha del PDF, con el desfase horario:
/// `D:YYYYMMDDHHmmSS+HH'mm'` (o `…Z` en UTC). Sin la zona, un comentario
/// hecho en Madrid y otro en México se ordenan mal en cualquier revisor.
pub(crate) fn fecha_pdf_ahora() -> String {
    let ahora = chrono::Local::now();
    format!(
        "{}{}",
        ahora.format("D:%Y%m%d%H%M%S"),
        desfase_pdf(ahora.offset().local_minus_utc())
    )
}

/// Desfase horario en el formato del spec PDF: `Z`, `+HH'mm'` o `-HH'mm'`.
fn desfase_pdf(segundos: i32) -> String {
    if segundos == 0 {
        return "Z".into();
    }
    let signo = if segundos < 0 { '-' } else { '+' };
    let minutos = segundos.abs() / 60;
    format!("{signo}{:02}'{:02}'", minutos / 60, minutos % 60)
}

/// `D:YYYYMMDDHHmmSS+HH'mm'` → ISO 8601 (`YYYY-MM-DDTHH:MM:SS+HH:MM`). Lo
/// que no encaje se devuelve vacío: la UI solo tiene que saber pintarlo o
/// no. La zona se conserva: sin ella dos comentarios de husos distintos se
/// ordenan mal.
pub(crate) fn fecha_pdf_a_iso(fecha: &str) -> String {
    let resto = fecha.trim_start_matches("D:");
    let d: Vec<char> = resto.chars().take_while(|c| c.is_ascii_digit()).collect();
    if d.len() < 8 {
        return String::new();
    }
    let t: String = d.iter().collect();
    let mut iso = format!("{}-{}-{}", &t[0..4], &t[4..6], &t[6..8]);
    if d.len() >= 14 {
        iso.push_str(&format!("T{}:{}:{}", &t[8..10], &t[10..12], &t[12..14]));
        iso.push_str(&zona_a_iso(&resto[d.len()..]));
    }
    iso
}

/// `+HH'mm'` → `+HH:MM`, `Z`/`Z00'00'` → `Z`. Sin zona reconocible, nada.
fn zona_a_iso(zona: &str) -> String {
    if zona.starts_with('Z') {
        return "Z".into();
    }
    let b = zona.as_bytes();
    if b.len() >= 6 && matches!(b[0], b'+' | b'-') && zona[1..3].chars().all(|c| c.is_ascii_digit())
    {
        let minutos = if zona[4..6].chars().all(|c| c.is_ascii_digit()) {
            &zona[4..6]
        } else {
            "00"
        };
        return format!("{}{}:{}", zona.chars().next().unwrap(), &zona[1..3], minutos);
    }
    String::new()
}

/// Segundo pase con lopdf sobre la anotación recién creada por PDFium (la
/// última de la página): su apariencia, si es una marca de texto, y la
/// firma `/T` + `/M` que Acrobat pone en todos los comentarios.
///
/// Debe llamarse desde dentro de la `mutacion` y del hilo de PDFium, justo
/// después de `save_and_close`.
pub(crate) fn remata_annot(
    work_path: &str,
    page_index: u16,
    estilo: Option<EstiloMarca>,
    author: Option<String>,
) -> Result<(), String> {
    remata_annot_en(work_path, page_index, None, estilo, author)
}

/// Como [`remata_annot`] pero sobre una anotación concreta (`None` = la
/// última, la que acaba de crear PDFium). El `/T` solo se escribe si llega
/// un autor o si la anotación aún no lo tenía: refrescar la fecha al mover
/// un comentario no puede cambiarle el autor.
pub(crate) fn remata_annot_en(
    work_path: &str,
    page_index: u16,
    annot_index: Option<usize>,
    estilo: Option<EstiloMarca>,
    author: Option<String>,
) -> Result<(), String> {
    let explicito = author.is_some();
    let autor = autor_o_sistema(author);
    let fecha = fecha_pdf_ahora();
    cirugia_en_hilo(work_path, move |doc| {
        let i = match annot_index {
            Some(i) => i,
            None => ultima_annot(doc, page_index)?,
        };
        if let Some(estilo) = estilo {
            escribe_apariencia_marca(doc, page_index, i, estilo)?;
        }
        let id = annot_id(doc, page_index, i)?;
        let annot = doc
            .get_object_mut(id)
            .and_then(|o| o.as_dict_mut())
            .map_err(|e| e.to_string())?;
        if explicito || !annot.has(b"T") {
            annot.set("T", crate::documento::cadena_pdf(&autor));
        }
        if annot_index.is_none() {
            // recién creada: PDFium le pone un /CreationDate suyo en UTC y
            // quedaban dos horas distintas en la misma anotación
            annot.set("CreationDate", lopdf::Object::string_literal(fecha.clone()));
        }
        annot.set("M", lopdf::Object::string_literal(fecha));
        Ok(())
    })
}

/// Decodifica una cadena PDF (literal en PDFDocEncoding o UTF-16BE con BOM).
pub(crate) fn texto_de_cadena_pdf(o: &lopdf::Object) -> String {
    let lopdf::Object::String(bytes, _) = o else {
        return String::new();
    };
    if bytes.starts_with(&[0xFE, 0xFF]) {
        let unidades: Vec<u16> = bytes[2..]
            .chunks(2)
            .filter(|c| c.len() == 2)
            .map(|c| u16::from_be_bytes([c[0], c[1]]))
            .collect();
        String::from_utf16_lossy(&unidades)
    } else {
        // PDFDocEncoding coincide con Latin-1 en todo lo imprimible
        bytes.iter().map(|b| *b as char).collect()
    }
}

#[derive(Serialize, Debug)]
pub struct AnnotationInfo {
    pub index: u16,
    pub kind: String,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub contents: String,
    /// Para resaltados: un rect por línea (los quads), en coords de UI.
    pub rects: Vec<Rect>,
    /// Color de trazo de la anotación (para que la UI pinte los overlays
    /// con el color real, no uno fijo).
    pub color: Option<[u8; 4]>,
    /// Autor del comentario (`/T`); vacío si la anotación no lo lleva.
    pub author: String,
    /// Fecha de modificación (`/M`) en ISO 8601; vacía si no la lleva.
    pub modified: String,
}

/// Lo que se lee con lopdf de cada anotación de la página: el color (que
/// PDFium no puede leer sin arriesgar un SIGSEGV) y la firma del autor.
#[derive(Default, Clone)]
pub struct DatosAnnot {
    pub color: Option<[u8; 4]>,
    pub author: String,
    pub modified: String,
}

/// Lista las anotaciones de una página (bounds en coords de UI). La UI las
/// usa para pintar los iconos de nota, los rects de los resaltados (PDFium no
/// genera apariencia automática para Text ni Highlight) y para borrar con clic.
#[tauri::command(async)]
pub fn get_annotations(path: String, page_index: u16) -> Result<Vec<AnnotationInfo>, String> {
    on_pdfium_thread(move || {
        with_doc(&path, |doc| {
            let mut out = lee_annots(doc, page_index)?;
            aplica_datos(&path, page_index, &mut out);
            Ok(out)
        })
    })
}

/// Las anotaciones de una página, tal como las ve PDFium (sin el color ni
/// la firma, que salen de lopdf en `aplica_datos`).
fn lee_annots(
    doc: &PdfDocument<'static>,
    page_index: u16,
) -> Result<Vec<AnnotationInfo>, String> {
    let page = doc.pages().get(page_index).map_err(|e| e.to_string())?;
    let geo = Geo::de_pagina(&page);
    let annotations = page.annotations();
    let mut out = Vec::new();
    for i in 0..annotations.len() {
        let Ok(mut a) = annotations.get(i) else {
            continue;
        };
        let kind = format!("{:?}", a.annotation_type());
        // el /Popup de una nota no es un comentario: es su ventana
        if kind == "Popup" {
            continue;
        }
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
                                rects.push(geo.pdf_rect_a_ui(&PdfRect::new(
                                    q.bottom(),
                                    q.left(),
                                    q.top(),
                                    q.right(),
                                )));
                            }
                        }
                    }
                };
            }
            lee_quads!(a.as_highlight_annotation_mut());
            lee_quads!(a.as_underline_annotation_mut());
            lee_quads!(a.as_strikeout_annotation_mut());
        }
        let caja = geo.pdf_rect_a_ui(&b);
        out.push(AnnotationInfo {
            index: i as u16,
            kind,
            x: caja.x,
            y: caja.y,
            w: caja.w,
            h: caja.h,
            contents: a.contents().unwrap_or_default(),
            rects,
            color: None,
            author: String::new(),
            modified: String::new(),
        });
    }
    Ok(out)
}

/// Completa con lo que solo sabe lopdf: color, autor y fecha.
fn aplica_datos(path: &str, page_index: u16, out: &mut [AnnotationInfo]) {
    let Some(datos) = datos_annots_lopdf(path, page_index) else {
        return;
    };
    for a in out.iter_mut() {
        if let Some(d) = datos.get(a.index as usize) {
            a.color = d.color;
            a.author.clone_from(&d.author);
            a.modified.clone_from(&d.modified);
        }
    }
}

/// Una anotación con la página en la que está, para el panel de
/// comentarios.
#[derive(Serialize, Debug)]
pub struct AnotacionDoc {
    #[serde(flatten)]
    pub annot: AnnotationInfo,
    pub page_index: u16,
}

/// Todas las anotaciones del documento, ordenadas por página y, dentro de
/// cada una, en el orden de `/Annots`. Una sola pasada: en un PDF de 300
/// páginas, pedirlas página a página serían 300 viajes por el canal del
/// hilo de PDFium.
#[tauri::command(async)]
pub fn get_document_annotations(path: String) -> Result<Vec<AnotacionDoc>, String> {
    on_pdfium_thread(move || {
        with_doc(&path, |doc| {
            let paginas = doc.pages().len();
            let mut out = Vec::new();
            for p in 0..paginas {
                let Ok(mut anots) = lee_annots(doc, p) else {
                    continue;
                };
                aplica_datos(&path, p, &mut anots);
                out.extend(anots.into_iter().map(|annot| AnotacionDoc {
                    annot,
                    page_index: p,
                }));
            }
            Ok(out)
        })
    })
}

/// Color (`/C` + `/CA` como alfa), autor (`/T`) y fecha (`/M`) de las
/// anotaciones de una página leídos con lopdf, alineados por índice con el
/// orden de /Annots (el mismo que recorre PDFium).
///
/// Para el color es la ÚNICA fuente, a propósito: `stroke_color()` de
/// pdfium-render 0.8 castea el handle de la anotación a objeto de página
/// cuando FPDFAnnot_GetColor falla (anotaciones con appearance stream:
/// formas, sellos, Ink, marcas de texto, y cualquiera tras un render), y en
/// Linux ese cast es un SIGSEGV de toda la app.
pub fn datos_annots_lopdf(path: &str, page_index: u16) -> Option<Vec<DatosAnnot>> {
    with_lopdf(path, |doc| Ok(datos_annots(doc, page_index)))
        .ok()
        .flatten()
}

/// El array `/Annots` de una página, esté por referencia o en línea.
pub(crate) fn lista_annots(doc: &lopdf::Document, page_index: u16) -> Option<Vec<lopdf::Object>> {
    use lopdf::Object;
    let page_id = *doc.get_pages().get(&(page_index as u32 + 1))?;
    let page = doc.get_object(page_id).ok()?.as_dict().ok()?;
    match page.get(b"Annots").ok()? {
        Object::Reference(rid) => Some(doc.get_object(*rid).ok()?.as_array().ok()?.clone()),
        Object::Array(a) => Some(a.clone()),
        _ => None,
    }
}

pub fn datos_annots(doc: &lopdf::Document, page_index: u16) -> Option<Vec<DatosAnnot>> {
    use lopdf::Object;
    let annots = lista_annots(doc, page_index)?;
    Some(
        annots
            .iter()
            .map(|a| {
                let dict = match a {
                    Object::Reference(rid) => {
                        match doc.get_object(*rid).ok().and_then(|o| o.as_dict().ok()) {
                            Some(d) => d,
                            None => return DatosAnnot::default(),
                        }
                    }
                    Object::Dictionary(d) => d,
                    _ => return DatosAnnot::default(),
                };
                DatosAnnot {
                    color: color_annot(dict),
                    author: dict
                        .get(b"T")
                        .map(texto_de_cadena_pdf)
                        .unwrap_or_default(),
                    modified: dict
                        .get(b"M")
                        .map(|o| fecha_pdf_a_iso(&texto_de_cadena_pdf(o)))
                        .unwrap_or_default(),
                }
            })
            .collect(),
    )
}

/// Color de una anotación a partir de `/C` (+ `/CA` como alfa).
fn color_annot(dict: &lopdf::Dictionary) -> Option<[u8; 4]> {
    use lopdf::Object;
    let c = match dict.get(b"C").ok()? {
        Object::Array(v) => v,
        _ => return None,
    };
    let alpha = dict
        .get(b"CA")
        .ok()
        .and_then(numero)
        .map(|a| (a * 255.0) as u8)
        .unwrap_or(255);
    match c.len() {
        3 => Some([
            (numero(&c[0])? * 255.0) as u8,
            (numero(&c[1])? * 255.0) as u8,
            (numero(&c[2])? * 255.0) as u8,
            alpha,
        ]),
        1 => {
            let g = (numero(&c[0])? * 255.0) as u8;
            Some([g, g, g, alpha])
        }
        _ => None,
    }
}

/// Índice del `/Popup` de una anotación dentro del `/Annots` de la página,
/// si lo tiene: borrar la nota sin él dejaría una ventana huérfana
/// apuntando a un objeto que ya no existe.
fn indice_popup(path: &str, page_index: u16, annot_index: usize) -> Option<usize> {
    use lopdf::Object;
    with_lopdf(path, |doc| {
        let Some(lista) = lista_annots(doc, page_index) else {
            return Ok(None);
        };
        let Some(Object::Reference(annot_ref)) = lista.get(annot_index) else {
            return Ok(None);
        };
        let popup = doc
            .get_object(*annot_ref)
            .and_then(|o| o.as_dict())
            .and_then(|d| d.get(b"Popup"))
            .and_then(|o| o.as_reference())
            .ok();
        Ok(popup.and_then(|p| {
            lista
                .iter()
                .position(|o| matches!(o, Object::Reference(r) if *r == p))
        }))
    })
    .ok()
    .flatten()
}

/// Elimina la anotación con el índice dado (y su ventana emergente, si la
/// tiene: en Acrobat el post-it se va entero).
#[tauri::command(async)]
pub fn remove_annotation(work_path: String, page_index: u16, annot_index: u16) -> Result<(), String> {
    mutacion(work_path, |work_path| on_pdfium_thread(move || {
        let mut indices = vec![annot_index as usize];
        if let Some(p) = indice_popup(&work_path, page_index, annot_index as usize) {
            indices.push(p);
        }
        // de mayor a menor: borrar no invalida los índices que quedan
        indices.sort_unstable();
        indices.dedup();
        indices.reverse();
        let pdfium = pdfium()?;
        let doc = pdfium
            .load_pdf_from_file(&work_path, None)
            .map_err(|e| e.to_string())?;
        let mut page = doc.pages().get(page_index).map_err(|e| e.to_string())?;
        for i in indices {
            let annotations = page.annotations_mut();
            let annot = annotations.get(i).map_err(|e| e.to_string())?;
            annotations
                .delete_annotation(annot)
                .map_err(|e| e.to_string())?;
        }
        drop(page);
        save_and_close(doc, &work_path)?;
        Ok(())
    }))
}

/// Quita una anotación del `/Annots` de su página con lopdf (para los
/// comandos que ya están dentro de una `cirugia` y no pueden abrir PDFium a
/// mitad). El objeto se queda huérfano: lopdf lo descarta al guardar.
pub(crate) fn quita_annot(
    doc: &mut lopdf::Document,
    page_index: u16,
    annot_index: usize,
) -> Result<(), String> {
    use lopdf::Object;
    let page_id = *doc
        .get_pages()
        .get(&(page_index as u32 + 1))
        .ok_or("Página fuera de rango")?;
    let por_referencia = {
        let page = doc
            .get_object(page_id)
            .and_then(|o| o.as_dict())
            .map_err(|e| e.to_string())?;
        match page.get(b"Annots") {
            Ok(Object::Reference(rid)) => Some(*rid),
            _ => None,
        }
    };
    let quita = |arr: &mut Vec<Object>| -> Result<(), String> {
        if annot_index >= arr.len() {
            return Err("Esa anotación ya no está".into());
        }
        arr.remove(annot_index);
        Ok(())
    };
    match por_referencia {
        Some(rid) => quita(
            doc.get_object_mut(rid)
                .and_then(|o| o.as_array_mut())
                .map_err(|e| e.to_string())?,
        ),
        None => {
            let page = doc
                .get_object_mut(page_id)
                .and_then(|o| o.as_dict_mut())
                .map_err(|e| e.to_string())?;
            match page.get_mut(b"Annots") {
                Ok(Object::Array(arr)) => quita(arr),
                _ => Err("La página no tiene anotaciones".into()),
            }
        }
    }
}

/// Reescribe el `/AP` de un cuadro de texto con lo que diga ahora su
/// `/Contents`, su `/DA` y su `/BS`. Sin esto, corregir el texto cambiaría
/// el dato y no lo que se ve. Para el resto de tipos no hay nada que hacer:
/// ni las notas ni las marcas sacan su apariencia del texto.
pub(crate) fn regenera_freetext(doc: &mut lopdf::Document, id: lopdf::ObjectId) -> Result<(), String> {
    use lopdf::Object;
    let annot = doc
        .get_object(id)
        .and_then(|o| o.as_dict())
        .map_err(|e| e.to_string())?
        .clone();
    if annot.get(b"Subtype").and_then(|o| o.as_name()).unwrap_or_default() != b"FreeText" {
        return Ok(());
    }
    let rect: Vec<f32> = annot
        .get(b"Rect")
        .and_then(|o| o.as_array())
        .map_err(|e| e.to_string())?
        .iter()
        .filter_map(numero)
        .collect();
    if rect.len() != 4 {
        return Ok(());
    }
    let (w, h) = ((rect[2] - rect[0]).abs(), (rect[3] - rect[1]).abs());
    let da = annot
        .get(b"DA")
        .map(texto_de_cadena_pdf)
        .unwrap_or_default();
    let (size, color) = crate::anotaciones2::lee_da(&da);
    let border = annot
        .get(b"BS")
        .and_then(|o| o.as_dict())
        .ok()
        .and_then(|d| d.get(b"W").ok())
        .and_then(numero)
        .map(|w| w > 0.0)
        .unwrap_or(true);
    let texto = annot
        .get(b"Contents")
        .map(texto_de_cadena_pdf)
        .unwrap_or_default();
    let ap_id =
        crate::anotaciones2::apariencia_freetext(doc, w, h, &texto, size, color, border);
    let mut ap = lopdf::Dictionary::new();
    ap.set("N", Object::Reference(ap_id));
    doc.get_object_mut(id)
        .and_then(|o| o.as_dict_mut())
        .map_err(|e| e.to_string())?
        .set("AP", Object::Dictionary(ap));
    Ok(())
}

/// Cambia el texto de un comentario ya creado y refresca su fecha (en
/// Acrobat, doble clic sobre el post-it y a escribir). Los cuadros de texto
/// vuelven a dibujar su apariencia con el texto nuevo.
#[tauri::command(async)]
pub fn set_annotation_contents(
    work_path: String,
    page_index: u16,
    annot_index: u16,
    contents: String,
    author: Option<String>,
) -> Result<(), String> {
    let explicito = author.is_some();
    let autor = autor_o_sistema(author);
    let fecha = fecha_pdf_ahora();
    crate::cirugia(&work_path, move |doc| {
        let id = annot_id(doc, page_index, annot_index as usize)?;
        let annot = doc
            .get_object_mut(id)
            .and_then(|o| o.as_dict_mut())
            .map_err(|e| e.to_string())?;
        annot.set("Contents", crate::documento::cadena_pdf(&contents));
        if explicito || !annot.has(b"T") {
            annot.set("T", crate::documento::cadena_pdf(&autor));
        }
        annot.set("M", lopdf::Object::string_literal(fecha));
        regenera_freetext(doc, id)
    })
}

/// Cambia el color de una anotación, al instante y sin «Aceptar», como las
/// propiedades de comentario de Acrobat.
///
/// Son dos pases: el dibujo que llevan dentro los trazos, las formas y los
/// sellos se recolorea con PDFium (si no, cambiar el color no se vería), y
/// el `/C` —que es de donde lo lee la UI— se escribe con lopdf. Las marcas
/// de texto además regeneran su `/AP`: sin eso el PDF exportado seguiría
/// amarillo fuera de Vitela.
#[tauri::command(async)]
pub fn set_annotation_color(
    work_path: String,
    page_index: u16,
    annot_index: u16,
    color: [u8; 4],
) -> Result<(), String> {
    mutacion(work_path, move |work_path| on_pdfium_thread(move || {
        recolorea_objetos(&work_path, page_index, annot_index, color)?;
        let fecha = fecha_pdf_ahora();
        cirugia_en_hilo(&work_path, move |doc| {
            let i = annot_index as usize;
            let id = annot_id(doc, page_index, i)?;
            let subtipo = doc
                .get_object(id)
                .and_then(|o| o.as_dict())
                .and_then(|d| d.get(b"Subtype"))
                .and_then(|o| o.as_name())
                .map(|n| n.to_vec())
                .unwrap_or_default();
            {
                let annot = doc
                    .get_object_mut(id)
                    .and_then(|o| o.as_dict_mut())
                    .map_err(|e| e.to_string())?;
                annot.set(
                    "C",
                    lopdf::Object::Array(vec![
                        (color[0] as f32 / 255.0).into(),
                        (color[1] as f32 / 255.0).into(),
                        (color[2] as f32 / 255.0).into(),
                    ]),
                );
                if color[3] < 255 {
                    annot.set("CA", lopdf::Object::Real(color[3] as f32 / 255.0));
                }
                annot.set("M", lopdf::Object::string_literal(fecha));
            }
            let estilo = match subtipo.as_slice() {
                b"Highlight" => Some(EstiloMarca::Resaltado),
                b"Underline" => Some(EstiloMarca::Subrayado),
                b"StrikeOut" => Some(EstiloMarca::Tachado),
                _ => None,
            };
            if let Some(estilo) = estilo {
                escribe_apariencia_marca(doc, page_index, i, estilo)?;
            }
            if subtipo.as_slice() == b"FreeText" {
                // el color del cuadro vive en su /DA, no solo en /C
                let da = doc
                    .get_object(id)
                    .and_then(|o| o.as_dict())
                    .and_then(|d| d.get(b"DA"))
                    .map(texto_de_cadena_pdf)
                    .unwrap_or_default();
                let (size, _) = crate::anotaciones2::lee_da(&da);
                doc.get_object_mut(id)
                    .and_then(|o| o.as_dict_mut())
                    .map_err(|e| e.to_string())?
                    .set(
                        "DA",
                        lopdf::Object::string_literal(format!(
                            "/Helv {size:.2} Tf {:.4} {:.4} {:.4} rg",
                            color[0] as f32 / 255.0,
                            color[1] as f32 / 255.0,
                            color[2] as f32 / 255.0
                        )),
                    );
                regenera_freetext(doc, id)?;
            }
            Ok(())
        })
    }))
}

/// Recolorea el dibujo que llevan dentro las anotaciones con apariencia
/// embebida (Ink y Stamp: trazos, formas y sellos). Las demás no tienen
/// nada que recolorear y se dejan como están.
fn recolorea_objetos(
    work_path: &str,
    page_index: u16,
    annot_index: u16,
    color: [u8; 4],
) -> Result<(), String> {
    let pdfium = pdfium()?;
    let doc = pdfium
        .load_pdf_from_file(work_path, None)
        .map_err(|e| e.to_string())?;
    let mut page = doc.pages().get(page_index).map_err(|e| e.to_string())?;
    let c = PdfColor::new(color[0], color[1], color[2], color[3]);
    let mut tocado = false;
    {
        let mut annot = page
            .annotations_mut()
            .get(annot_index as usize)
            .map_err(|e| e.to_string())?;
        let objects = match (
            annot.as_stamp_annotation_mut().is_some(),
            annot.as_ink_annotation_mut().is_some(),
        ) {
            (true, _) => Some(annot.as_stamp_annotation_mut().unwrap().objects_mut()),
            (_, true) => Some(annot.as_ink_annotation_mut().unwrap().objects_mut()),
            _ => None,
        };
        if let Some(objects) = objects {
            for i in 0..objects.len() {
                let Ok(mut obj) = objects.get(i) else { continue };
                if obj.as_text_object().is_some() {
                    let _ = obj.set_fill_color(c);
                } else {
                    let _ = obj.set_stroke_color(c);
                    // solo se recolorea el relleno si lo había
                    if obj.fill_color().map(|f| f.alpha() > 0).unwrap_or(false) {
                        let _ = obj.set_fill_color(c);
                    }
                }
                tocado = true;
            }
        }
    }
    drop(page);
    if tocado {
        save_and_close(doc, work_path)
    } else {
        drop(doc);
        crate::invalidate_doc_cache();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine;
    #[allow(unused_imports)]
    use crate::tests::{crea_pdf, textos_de};
    #[allow(unused_imports)]
    use crate::{get_page_sizes, open_pdf, render_page_b64, render_page_png};

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
        let antes = decode(render_page_b64(work.clone(), 0, 200, None).unwrap());
        // trazo horizontal que pasa por (150, 120) pt
        add_stroke(work.clone(), 0, vec![[50.0, 120.0], [250.0, 120.0]], None, None, None).expect("trazo");
        let despues = decode(render_page_b64(work.clone(), 0, 200, None).unwrap());
        let px = (150.0f32 * 200.0 / 595.0) as u32;
        let py = (120.0f32 * 200.0 / 595.0) as u32;
        let mut cambiado = false;
        for dy in 0..3 {
            if antes.get_pixel(px, py + dy) != despues.get_pixel(px, py + dy) {
                cambiado = true;
            }
        }
        assert!(cambiado, "el trazo no cambió ningún píxel");
        // el color del trazo tiene que llegar a la UI en get_annotations
        add_stroke(
            work.clone(),
            0,
            vec![[50.0, 200.0], [250.0, 200.0]],
            Some([46, 160, 67, 255]),
            None,
            None,
        )
        .expect("trazo con color");
        let annots = get_annotations(work, 0).expect("annots");
        let ultimo = annots.last().expect("hay anotaciones");
        assert_eq!(ultimo.kind, "Ink");
        assert_eq!(ultimo.color, Some([46, 160, 67, 255]));
        std::fs::remove_file(&tmp).ok();
    }

    #[test]
    fn resaltado_devuelve_sus_rects() {
        // PDFium no genera apariencia para Highlight: la UI lo pinta con los
        // rects que devuelve get_annotations. Verificamos ese contrato.
        let tmp = std::env::temp_dir().join("editor_pdf_test_resaltado_rects.pdf");
        crea_pdf(&["Hola"], &tmp);
        let work = tmp.to_string_lossy().into_owned();
        crate::anotaciones2::add_markup(
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
            "highlight".into(),
            None,
            None,
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
        crate::anotaciones2::add_markup(
            work.clone(),
            0,
            vec![Rect {
                x: 50.0,
                y: 700.0,
                w: 100.0,
                h: 14.0,
            }],
            "highlight".into(),
            None,
            None,
        )
        .expect("resaltar");
        add_note(work.clone(), 0, 200.0, 100.0, "Una nota".into(), None).expect("añadir nota");
        let annots = get_annotations(work.clone(), 0).expect("listar anotaciones");
        assert_eq!(annots.len(), 2, "anotaciones: {:?}", annots.len());
        let nota = annots.iter().find(|a| a.kind == "Text").expect("nota");
        assert_eq!(nota.contents, "Una nota");

        // trazo como anotación Ink
        add_stroke(
            work.clone(),
            0,
            vec![[10.0, 10.0], [50.0, 40.0], [90.0, 10.0]],
            None,
            None,
            None,
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
        render_page_b64(work.clone(), 0, 200, None).expect("render con anotaciones");

        std::fs::remove_file(&tmp).ok();
    }
}

#[cfg(test)]
mod tests_apariencia {
    use super::*;
    use crate::render_page_png;
    use crate::tests::crea_pdf;

    /// Píxel de un PNG en las coordenadas de UI dadas (puntos PDF) para un
    /// render del ancho indicado sobre una página A4.
    fn pixel(png: &[u8], ancho_px: u32, x_pt: f32, y_pt: f32) -> [u8; 4] {
        let img = image::load_from_memory(png).expect("PNG").to_rgba8();
        let escala = ancho_px as f32 / 595.0;
        img.get_pixel((x_pt * escala) as u32, (y_pt * escala) as u32).0
    }

    /// «Comentarios y formularios: solo el documento» del diálogo de
    /// impresión de Acrobat: lo mismo pero sin las marcas. `render_page`
    /// pasaba siempre la bandera FPDF_ANNOT, así que no había forma de
    /// imprimir sin los resaltados.
    #[test]
    fn el_render_puede_dejar_fuera_las_anotaciones() {
        let tmp = std::env::temp_dir().join("render-sin-anotaciones-test.pdf");
        crea_pdf(&["Hola"], &tmp);
        let work = tmp.to_string_lossy().into_owned();
        crate::anotaciones2::add_markup(
            work.clone(),
            0,
            vec![crate::Rect { x: 40.0, y: 300.0, w: 200.0, h: 30.0 }],
            "highlight".into(),
            None,
            None,
        )
        .expect("resaltar");

        let con = crate::render_page_png(work.clone(), 0, 300, true).expect("render con marcas");
        let sin = crate::render_page_png(work.clone(), 0, 300, false).expect("render sin marcas");
        // el centro del rect resaltado, en puntos PDF
        let (px, py) = (140.0, 315.0);
        let a = pixel(&con, 300, px, py);
        let b = pixel(&sin, 300, px, py);
        assert_ne!(a, b, "sin marcas el resaltado no puede seguir ahí");
        assert!(
            b[0] > 240 && b[1] > 240 && b[2] > 240,
            "sin marcas queda el papel: {b:?}"
        );
        std::fs::remove_file(&tmp).ok();
    }

    /// Diccionario de la anotación `i` de la primera página, leído del
    /// fichero ya guardado (no del documento en memoria de PDFium).
    fn annot_guardada(work: &str, i: usize) -> lopdf::Dictionary {
        let doc = lopdf::Document::load(work).expect("cargar con lopdf");
        let page_id = *doc.get_pages().get(&1).expect("página 1");
        let annots = doc
            .get_object(page_id)
            .and_then(|o| o.as_dict())
            .and_then(|d| d.get(b"Annots"))
            .and_then(|o| o.as_array())
            .expect("Annots")
            .clone();
        match &annots[i] {
            lopdf::Object::Reference(rid) => doc.get_object(*rid).unwrap().as_dict().unwrap().clone(),
            lopdf::Object::Dictionary(d) => d.clone(),
            otro => panic!("anotación inesperada: {otro:?}"),
        }
    }

    /// La apariencia normal (`/AP /N`) tiene que ser un stream de verdad,
    /// no una referencia colgante.
    fn tiene_ap_con_stream(work: &str, i: usize) -> bool {
        let doc = lopdf::Document::load(work).expect("cargar con lopdf");
        let annot = annot_guardada(work, i);
        let Ok(ap) = annot.get(b"AP").and_then(|o| o.as_dict()) else {
            return false;
        };
        let Ok(n) = ap.get(b"N") else { return false };
        let obj = match n {
            lopdf::Object::Reference(rid) => doc.get_object(*rid).expect("stream de apariencia"),
            otro => otro,
        };
        obj.as_stream().map(|s| !s.content.is_empty()).unwrap_or(false)
    }

    /// `/M` de la anotación `i` tal como está en el fichero, sin traducir.
    fn fecha_guardada(work: &str, i: usize) -> String {
        let annot = annot_guardada(work, i);
        match annot.get(b"M") {
            Ok(lopdf::Object::String(bytes, _)) => bytes.iter().map(|b| *b as char).collect(),
            otro => panic!("la anotación no lleva /M: {otro:?}"),
        }
    }

    /// La fecha va en el formato del spec PDF y con el desfase horario:
    /// `D:YYYYMMDDHHmmSS` + `Z` o `+HH'mm'`. La que escribe PDFium por su
    /// cuenta al guardar (`…Z00'00'`, siempre UTC) no cumple.
    fn fecha_pdf_completa(m: &str) -> bool {
        let Some(resto) = m.strip_prefix("D:") else {
            return false;
        };
        if resto.len() < 14 || !resto[..14].chars().all(|c| c.is_ascii_digit()) {
            return false;
        }
        let zona = &resto[14..];
        zona == "Z"
            || (zona.len() == 7
                && matches!(zona.as_bytes()[0], b'+' | b'-')
                && zona.as_bytes()[3] == b'\''
                && zona.as_bytes()[6] == b'\''
                && zona[1..3].chars().all(|c| c.is_ascii_digit())
                && zona[4..6].chars().all(|c| c.is_ascii_digit()))
    }

    /// Envejece el `/M` de una anotación: el reloj no avanza dentro de un
    /// test y la fecha va en segundos.
    fn envejece(work: &str, i: usize) {
        crate::cirugia(work, move |doc| {
            let id = annot_id(doc, 0, i)?;
            let annot = doc
                .get_object_mut(id)
                .and_then(|o| o.as_dict_mut())
                .map_err(|e| e.to_string())?;
            annot.set("M", lopdf::Object::string_literal("D:20200101000000+00'00'"));
            Ok(())
        })
        .expect("envejecer la fecha");
    }

    /// El `/Annots` de la primera página tal como está en el fichero.
    fn annots_guardadas(work: &str) -> Vec<lopdf::Object> {
        let doc = lopdf::Document::load(work).expect("cargar con lopdf");
        let page_id = *doc.get_pages().get(&1).expect("página 1");
        doc.get_object(page_id)
            .and_then(|o| o.as_dict())
            .and_then(|d| d.get(b"Annots"))
            .and_then(|o| o.as_array())
            .cloned()
            .unwrap_or_default()
    }

    fn caja(y: f32) -> Rect {
        Rect { x: 200.0, y, w: 200.0, h: 20.0 }
    }

    #[test]
    fn resaltado_se_ve_en_el_render_y_conserva_ap() {
        let tmp = std::env::temp_dir().join("editor_pdf_test_ap_resaltado.pdf");
        crea_pdf(&["Hola"], &tmp);
        let work = tmp.to_string_lossy().into_owned();
        // zona en blanco de la página, lejos del texto
        crate::anotaciones2::add_markup(work.clone(), 0, vec![caja(300.0)], "highlight".into(), None, None).expect("resaltar");

        let png = render_page_png(work.clone(), 0, 300, true).expect("render");
        let [r, g, b, _] = pixel(&png, 300, 300.0, 310.0);
        assert!(
            r > 200 && g > 150 && b < 120,
            "el resaltado no se ve en el render: rgb({r},{g},{b})"
        );

        // lo que PDFium genera al vuelo no queda en el fichero: el /AP
        // escrito a mano sí, y es lo que verá cualquier otro visor
        assert!(
            tiene_ap_con_stream(&work, 0),
            "el resaltado guardado debe llevar /AP con su stream"
        );
        let annot = annot_guardada(&work, 0);
        assert_eq!(annot.get(b"F").and_then(|o| o.as_i64()).unwrap_or(0), 4);
        assert!(annot.get(b"CA").is_ok(), "el resaltado debe llevar /CA");
        std::fs::remove_file(&tmp).ok();
    }

    #[test]
    fn autor_y_fecha_en_las_anotaciones() {
        let tmp = std::env::temp_dir().join("editor_pdf_test_autor.pdf");
        crea_pdf(&["Hola"], &tmp);
        let work = tmp.to_string_lossy().into_owned();

        // sin autor: el nombre de usuario del sistema, nunca vacío
        add_note(work.clone(), 0, 200.0, 100.0, "Una nota".into(), None).expect("nota");
        let a = &get_annotations(work.clone(), 0).expect("listar")[0];
        assert!(!a.author.is_empty(), "el autor no puede quedar vacío");
        assert!(
            a.modified.starts_with(&format!("{}-", chrono::Local::now().format("%Y"))),
            "fecha ISO 8601 esperada, llegó {:?}",
            a.modified
        );

        // con autor explícito, en todos los creadores de anotaciones
        crate::anotaciones2::add_markup(work.clone(), 0, vec![caja(300.0)], "highlight".into(), None, Some("Jorge".into())).expect("resaltar");
        add_stroke(
            work.clone(),
            0,
            vec![[10.0, 10.0], [50.0, 40.0]],
            None,
            None,
            Some("Jorge".into()),
        )
        .expect("trazo");
        crate::anotaciones2::add_markup(
            work.clone(),
            0,
            vec![caja(400.0)],
            "underline".into(),
            None,
            Some("Jorge".into()),
        )
        .expect("subrayar");
        crate::anotaciones2::add_shape(
            work.clone(),
            0,
            "rect".into(),
            10.0,
            500.0,
            80.0,
            560.0,
            [0, 0, 0, 255],
            None,
            1.0,
            Some("Jorge".into()),
        )
        .expect("forma");
        crate::anotaciones2::add_stamp(
            work.clone(),
            0,
            "APROBADO".into(),
            [0, 0, 0, 255],
            10.0,
            600.0,
            14.0,
            Some("Jorge".into()),
        )
        .expect("sello");

        let annots = get_annotations(work.clone(), 0).expect("listar");
        assert_eq!(annots.len(), 6);
        for a in annots.iter().skip(1) {
            assert_eq!(a.author, "Jorge", "anotación {:?} sin autor", a.kind);
            assert!(!a.modified.is_empty(), "anotación {:?} sin fecha", a.kind);
        }
        std::fs::remove_file(&tmp).ok();
    }

    #[test]
    fn aplanar_conserva_el_resaltado() {
        // con /AP escrito, aplanar ya no borra las marcas: es lo que
        // permite quitar del diálogo la advertencia de que se pierden
        let tmp = std::env::temp_dir().join("editor_pdf_test_ap_aplanado.pdf");
        crea_pdf(&["Hola"], &tmp);
        let work = tmp.to_string_lossy().into_owned();
        crate::anotaciones2::add_markup(work.clone(), 0, vec![caja(300.0)], "highlight".into(), None, None).expect("resaltar");
        crate::seguridad::flatten_pdf(work.clone()).expect("aplanar");

        let png = render_page_png(work.clone(), 0, 300, true).expect("render");
        let [r, g, b, _] = pixel(&png, 300, 300.0, 310.0);
        assert!(
            r > 200 && g > 150 && b < 120,
            "el resaltado se perdió al aplanar: rgb({r},{g},{b})"
        );
        assert!(
            get_annotations(work, 0).expect("listar").is_empty(),
            "aplanar debe dejar la marca como contenido, no como anotación"
        );
        std::fs::remove_file(&tmp).ok();
    }

    #[test]
    fn subrayado_pinta_la_base_y_no_el_centro() {
        let tmp = std::env::temp_dir().join("editor_pdf_test_ap_subrayado.pdf");
        crea_pdf(&["Hola"], &tmp);
        let work = tmp.to_string_lossy().into_owned();
        crate::anotaciones2::add_markup(
            work.clone(),
            0,
            vec![caja(300.0)],
            "underline".into(),
            Some([0, 0, 255, 255]),
            None,
        )
        .expect("subrayar");

        let png = render_page_png(work.clone(), 0, 300, true).expect("render");
        let [_, _, base, _] = pixel(&png, 300, 300.0, 319.0);
        assert!(base > 150, "el subrayado no se ve en la base del quad");
        let [r, g, b, _] = pixel(&png, 300, 300.0, 308.0);
        assert!(
            r > 240 && g > 240 && b > 240,
            "el subrayado no debe rellenar el quad: rgb({r},{g},{b})"
        );
        assert!(tiene_ap_con_stream(&work, 0), "el subrayado debe llevar /AP");
        std::fs::remove_file(&tmp).ok();
    }

    #[test]
    fn tachado_pinta_el_centro_y_no_la_base() {
        let tmp = std::env::temp_dir().join("editor_pdf_test_ap_tachado.pdf");
        crea_pdf(&["Hola"], &tmp);
        let work = tmp.to_string_lossy().into_owned();
        crate::anotaciones2::add_markup(
            work.clone(),
            0,
            vec![caja(300.0)],
            "strikeout".into(),
            Some([255, 0, 0, 255]),
            None,
        )
        .expect("tachar");

        let png = render_page_png(work.clone(), 0, 300, true).expect("render");
        let [medio, _, _, _] = pixel(&png, 300, 300.0, 310.0);
        assert!(medio > 150, "el tachado no se ve a media altura");
        let [r, g, b, _] = pixel(&png, 300, 300.0, 302.0);
        assert!(
            r > 240 && g > 240 && b > 240,
            "el tachado no debe rellenar el quad: rgb({r},{g},{b})"
        );
        assert!(tiene_ap_con_stream(&work, 0), "el tachado debe llevar /AP");
        std::fs::remove_file(&tmp).ok();
    }

    /// La zona horaria llega a la UI: `modified` es ISO 8601 completo.
    #[test]
    fn la_fecha_iso_conserva_la_zona() {
        assert_eq!(
            fecha_pdf_a_iso("D:20260909143012+02'00'"),
            "2026-09-09T14:30:12+02:00"
        );
        assert_eq!(
            fecha_pdf_a_iso("D:20260909143012-05'30'"),
            "2026-09-09T14:30:12-05:30"
        );
        // la que escribe PDFium por su cuenta al guardar
        assert_eq!(
            fecha_pdf_a_iso("D:20260909143012Z00'00'"),
            "2026-09-09T14:30:12Z"
        );
        // sin zona (documentos de antes) se queda sin ella, no inventa
        assert_eq!(fecha_pdf_a_iso("D:20260909143012"), "2026-09-09T14:30:12");
        assert_eq!(fecha_pdf_a_iso("basura"), "");
    }

    /// `/M` con desfase horario: sin él, un comentario hecho en Madrid y
    /// otro en México se ordenan mal en cualquier revisor (el formato de
    /// fecha del spec PDF lleva la zona).
    #[test]
    fn la_fecha_lleva_la_zona_horaria() {
        let tmp = std::env::temp_dir().join("editor_pdf_test_zona_horaria.pdf");
        crea_pdf(&["Hola"], &tmp);
        let work = tmp.to_string_lossy().into_owned();
        add_note(work.clone(), 0, 200.0, 100.0, "Una nota".into(), None).expect("nota");

        let m = fecha_guardada(&work, 0);
        assert!(
            fecha_pdf_completa(&m),
            "el desfase horario debe ir como +HH'mm' o Z, llegó {m:?}"
        );
        std::fs::remove_file(&tmp).ok();
    }

    /// Mover un comentario actualiza su fecha de modificación (Acrobat lo
    /// hace) sin tocar el autor.
    #[test]
    fn mover_una_anotacion_refresca_la_fecha() {
        let tmp = std::env::temp_dir().join("editor_pdf_test_m_transform.pdf");
        crea_pdf(&["Hola"], &tmp);
        let work = tmp.to_string_lossy().into_owned();
        crate::anotaciones2::add_stamp(
            work.clone(),
            0,
            "APROBADO".into(),
            [0, 0, 0, 255],
            200.0,
            300.0,
            14.0,
            Some("Jorge".into()),
        )
        .expect("sello");

        // fecha vieja a mano: el reloj no avanza dentro de un test
        crate::cirugia(&work, |doc| {
            let id = annot_id(doc, 0, 0)?;
            let annot = doc
                .get_object_mut(id)
                .and_then(|o| o.as_dict_mut())
                .map_err(|e| e.to_string())?;
            annot.set("M", lopdf::Object::string_literal("D:20200101000000+00'00'"));
            Ok(())
        })
        .expect("envejecer la fecha");
        assert_eq!(fecha_guardada(&work, 0), "D:20200101000000+00'00'");

        crate::anotaciones2::transform_annotation(work.clone(), 0, 0, 100.0, 100.0, 120.0, 40.0)
            .expect("mover");

        let m = fecha_guardada(&work, 0);
        assert!(
            m.starts_with(&format!("D:{}", chrono::Local::now().format("%Y"))),
            "mover debe refrescar /M, sigue en {m}"
        );
        assert!(
            fecha_pdf_completa(&m),
            "la fecha tras mover es la de PDFium, no la nuestra: {m:?}"
        );
        let annots = get_annotations(work, 0).expect("listar");
        assert_eq!(annots[0].author, "Jorge", "mover no debe tocar el autor");
        std::fs::remove_file(&tmp).ok();
    }

    /// Corregir el texto de un comentario ya creado (doble clic en Acrobat):
    /// el texto nuevo llega a `get_annotations` y la fecha se refresca.
    #[test]
    fn editar_el_texto_de_un_comentario() {
        let tmp = std::env::temp_dir().join("editor_pdf_test_editar_nota.pdf");
        crea_pdf(&["Hola"], &tmp);
        let work = tmp.to_string_lossy().into_owned();
        add_note(work.clone(), 0, 200.0, 100.0, "Primera".into(), Some("Jorge".into()))
            .expect("nota");
        envejece(&work, 0);

        set_annotation_contents(
            work.clone(),
            0,
            0,
            "Corregida\nen dos líneas".into(),
            None,
        )
        .expect("corregir");

        let a = &get_annotations(work.clone(), 0).expect("listar")[0];
        assert_eq!(a.contents, "Corregida\nen dos líneas");
        assert_eq!(a.author, "Jorge", "corregir no cambia el autor");
        let m = fecha_guardada(&work, 0);
        assert!(
            m.starts_with(&format!("D:{}", chrono::Local::now().format("%Y"))),
            "corregir debe refrescar /M, sigue en {m}"
        );
        std::fs::remove_file(&tmp).ok();
    }

    /// Cambiar el color de un resaltado tiene que reescribir su `/AP`: sin
    /// eso el PDF exportado seguiría amarillo fuera de Vitela.
    #[test]
    fn recolorear_un_resaltado_reescribe_su_apariencia() {
        let tmp = std::env::temp_dir().join("editor_pdf_test_recolorear.pdf");
        crea_pdf(&["Hola"], &tmp);
        let work = tmp.to_string_lossy().into_owned();
        crate::anotaciones2::add_markup(work.clone(), 0, vec![caja(300.0)], "highlight".into(), None, None).expect("resaltar");

        set_annotation_color(work.clone(), 0, 0, [90, 200, 250, 255]).expect("recolorear");

        let [r, g, b, _] = pixel(&render_page_png(work.clone(), 0, 300, true).expect("render"), 300, 300.0, 310.0);
        assert!(
            b > 200 && r < 160,
            "el resaltado sigue sin ser azul: rgb({r},{g},{b})"
        );
        assert!(tiene_ap_con_stream(&work, 0), "el resaltado debe conservar /AP");
        let a = &get_annotations(work, 0).expect("listar")[0];
        assert_eq!(a.color, Some([90, 200, 250, 255]));
        std::fs::remove_file(&tmp).ok();
    }

    /// La nota es un post-it de verdad: icono de bocadillo (`/Name /Comment`)
    /// y ventana emergente (`/Popup` con `/Open false`), que es lo que
    /// enseñan Acrobat y Vista Previa. El popup no es un comentario: no sale
    /// en la lista, y se va con la nota al borrarla.
    #[test]
    fn la_nota_lleva_icono_y_popup() {
        let tmp = std::env::temp_dir().join("editor_pdf_test_popup.pdf");
        crea_pdf(&["Hola"], &tmp);
        let work = tmp.to_string_lossy().into_owned();
        add_note(work.clone(), 0, 200.0, 100.0, "Una nota".into(), None).expect("nota");

        let nota = annot_guardada(&work, 0);
        assert_eq!(
            nota.get(b"Name").and_then(|o| o.as_name()).unwrap_or_default(),
            b"Comment"
        );
        assert!(nota.get(b"Popup").is_ok(), "la nota debe llevar /Popup");
        let popup = annot_guardada(&work, 1);
        assert_eq!(
            popup.get(b"Subtype").and_then(|o| o.as_name()).unwrap_or_default(),
            b"Popup"
        );
        assert!(
            !popup.get(b"Open").and_then(|o| o.as_bool()).unwrap_or(true),
            "el popup nace cerrado"
        );

        let annots = get_annotations(work.clone(), 0).expect("listar");
        assert_eq!(annots.len(), 1, "el popup no es un comentario: {annots:?}");
        remove_annotation(work.clone(), 0, annots[0].index).expect("borrar");
        assert!(
            get_annotations(work.clone(), 0).expect("listar").is_empty(),
            "borrar la nota debe llevarse su popup"
        );
        assert!(
            annots_guardadas(&work).is_empty(),
            "el popup huérfano se queda en el fichero"
        );
        std::fs::remove_file(&tmp).ok();
    }

    /// El panel de comentarios pide todo el documento de una vez: una sola
    /// pasada, con la página de cada anotación y el orden estable.
    #[test]
    fn todas_las_anotaciones_del_documento_de_una_vez() {
        let tmp = std::env::temp_dir().join("editor_pdf_test_annots_doc.pdf");
        crea_pdf(&["Uno", "Dos", "Tres"], &tmp);
        let work = tmp.to_string_lossy().into_owned();
        add_note(work.clone(), 0, 200.0, 100.0, "En la 1".into(), Some("Jorge".into()))
            .expect("nota 1");
        add_note(work.clone(), 2, 200.0, 100.0, "En la 3".into(), Some("Jorge".into()))
            .expect("nota 3");
        crate::anotaciones2::add_stamp(
            work.clone(),
            2,
            "APROBADO".into(),
            [192, 57, 43, 255],
            300.0,
            400.0,
            18.0,
            None,
        )
        .expect("sello 3");

        let todas = get_document_annotations(work.clone()).expect("listar el documento");
        assert_eq!(todas.len(), 3, "una en la 1 y dos en la 3: {todas:?}");
        assert_eq!(
            todas.iter().map(|a| a.page_index).collect::<Vec<_>>(),
            vec![0, 2, 2],
            "ordenadas por página"
        );
        assert_eq!(todas[0].annot.contents, "En la 1");
        assert_eq!(todas[1].annot.contents, "En la 3");
        assert_eq!(todas[2].annot.kind, "Stamp");
        assert_eq!(todas[0].annot.author, "Jorge");
        assert!(!todas[0].annot.modified.is_empty());
        // el JSON va plano: page_index junto a los campos de la anotación
        let json = serde_json::to_value(&todas[0]).expect("serializar");
        assert_eq!(json["page_index"], 0);
        assert_eq!(json["kind"], "Text");
        // documento sin comentarios: lista vacía, no un error
        let vacio = std::env::temp_dir().join("editor_pdf_test_annots_doc_vacio.pdf");
        crea_pdf(&["Sin nada"], &vacio);
        assert!(
            get_document_annotations(vacio.to_string_lossy().into_owned())
                .expect("listar")
                .is_empty()
        );
        std::fs::remove_file(&tmp).ok();
        std::fs::remove_file(&vacio).ok();
    }

    /// `/M` y `/CreationDate` de la misma anotación tienen que decir la
    /// misma hora: PDFium escribía la suya en UTC y en el mismo objeto
    /// convivían las 12:46 y las 10:46 de la misma nota.
    #[test]
    fn la_fecha_de_creacion_y_la_de_cambio_dicen_lo_mismo() {
        let tmp = std::env::temp_dir().join("editor_pdf_test_fechas_coherentes.pdf");
        crea_pdf(&["Hola"], &tmp);
        let work = tmp.to_string_lossy().into_owned();
        add_note(work.clone(), 0, 200.0, 100.0, "Una nota".into(), None).expect("nota");
        crate::anotaciones2::add_shape(
            work.clone(),
            0,
            "rect".into(),
            50.0,
            50.0,
            150.0,
            120.0,
            [0, 0, 0, 255],
            None,
            1.0,
            None,
        )
        .expect("forma");

        for i in [0usize, 2] {
            let annot = annot_guardada(&work, i);
            let cadena = |clave: &[u8]| match annot.get(clave) {
                Ok(lopdf::Object::String(b, _)) => b.iter().map(|c| *c as char).collect::<String>(),
                otro => panic!("falta la fecha {clave:?}: {otro:?}"),
            };
            let m = cadena(b"M");
            let creacion = cadena(b"CreationDate");
            assert!(fecha_pdf_completa(&m), "/M: {m}");
            assert!(fecha_pdf_completa(&creacion), "/CreationDate: {creacion}");
            // misma hora local hasta el minuto y misma zona
            assert_eq!(&m[..14], &creacion[..14], "{m} vs {creacion}");
            assert_eq!(&m[16..], &creacion[16..], "zonas distintas");
        }
        std::fs::remove_file(&tmp).ok();
    }
}
