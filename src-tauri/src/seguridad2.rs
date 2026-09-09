//! Redacción en dos fases y limpieza de información oculta.
//!
//! Acrobat separa **marcar** de **aplicar**: se dibujan rectángulos rojos
//! revisables, que se mueven y se borran, y solo al pulsar «Aplicar»
//! desaparece el contenido y queda la caja negra. Vitela hacía la segunda
//! mitad (`redact_area`) sin la primera, así que censurar era irreversible
//! al primer arrastre.
//!
//! Las marcas son anotaciones `/Square` normales —`/C [1 0 0]` de borde y
//! `/IC [0 0 0]` de relleno, con su `/AP`— más una clave propia
//! `/Vitela /Redact`. Por eso sobreviven a guardar, se ven en cualquier
//! visor como lo que son (una propuesta, no una censura) y la UI las mueve
//! y las borra con `transform_annotation` y `remove_annotation`, como
//! cualquier otro comentario.

use crate::historial::mutacion;
use crate::{cirugia, on_pdfium_thread, Rect};
use lopdf::{Dictionary, Document as LoDoc, Object};
use serde::Serialize;

/// Marca de agua de Vitela en la anotación: la clave que distingue una
/// marca de redacción de un rectángulo cualquiera.
const CLAVE: &[u8] = b"Vitela";
const VALOR: &[u8] = b"Redact";

/// ¿Es esta anotación una marca de redacción nuestra?
fn es_marca(annot: &Dictionary) -> bool {
    annot.get(b"Subtype").and_then(|o| o.as_name()).unwrap_or_default() == b"Square"
        && annot.get(CLAVE).and_then(|o| o.as_name()).unwrap_or_default() == VALOR
}

/// Apariencia de la marca: el borde rojo que se ve mientras es solo una
/// propuesta. PDFium no escribe el `/AP` de las anotaciones de marcado, así
/// que sin esto la marca no existiría fuera de Vitela.
fn apariencia_marca(doc: &mut LoDoc, w: f32, h: f32) -> lopdf::ObjectId {
    use lopdf::Stream;
    let ops = format!("q 1 0 0 RG 1.5 w 0.75 0.75 {:.2} {:.2} re S Q\n", w - 1.5, h - 1.5);
    let mut forma = Dictionary::new();
    forma.set("Type", Object::Name(b"XObject".to_vec()));
    forma.set("Subtype", Object::Name(b"Form".to_vec()));
    forma.set("FormType", 1i64);
    forma.set(
        "BBox",
        Object::Array(vec![0.into(), 0.into(), w.into(), h.into()]),
    );
    forma.set("Resources", Object::Dictionary(Dictionary::new()));
    doc.add_object(Stream::new(forma, ops.into_bytes()))
}

/// Marca una zona para redactar. No borra nada: deja el rectángulo rojo
/// revisable que se aplica después. Devuelve el índice de la marca dentro
/// de `/Annots` de esa página, que es el mismo que usan `get_annotations`,
/// `transform_annotation` y `remove_annotation`.
#[tauri::command(async)]
pub fn mark_redaction(work_path: String, page_index: u16, rect: Rect) -> Result<u16, String> {
    if rect.w < 2.0 || rect.h < 2.0 {
        return Err("La zona que se quiere tapar es demasiado pequeña".into());
    }
    let indice = std::sync::Arc::new(std::sync::Mutex::new(0u16));
    let salida = indice.clone();
    cirugia(&work_path, move |doc| {
        let page_id = *doc
            .get_pages()
            .get(&(page_index as u32 + 1))
            .ok_or("Página fuera de rango")?;
        let geo = crate::formularios2::geo_pagina(doc, page_id)?;
        let caja = geo.ui_rect_a_pdf(&rect);
        let (w, h) = (
            caja.right().value - caja.left().value,
            caja.top().value - caja.bottom().value,
        );
        let ap_id = apariencia_marca(doc, w, h);
        let mut annot = Dictionary::new();
        annot.set("Type", Object::Name(b"Annot".to_vec()));
        annot.set("Subtype", Object::Name(b"Square".to_vec()));
        annot.set(
            "Rect",
            Object::Array(vec![
                caja.left().value.into(),
                caja.bottom().value.into(),
                caja.right().value.into(),
                caja.top().value.into(),
            ]),
        );
        annot.set("C", Object::Array(vec![1.into(), 0.into(), 0.into()]));
        annot.set("IC", Object::Array(vec![0.into(), 0.into(), 0.into()]));
        annot.set("F", 4i64); // Print
        annot.set("Contents", crate::documento::cadena_pdf("Marca de redacción"));
        annot.set(CLAVE, Object::Name(VALOR.to_vec()));
        let mut ap = Dictionary::new();
        ap.set("N", Object::Reference(ap_id));
        annot.set("AP", Object::Dictionary(ap));
        let annot_id = doc.add_object(annot);
        crate::formularios2::anade_a_annots(doc, page_id, annot_id)?;
        let n = crate::anotaciones::lista_annots(doc, page_index)
            .map(|a| a.len())
            .unwrap_or(1);
        *indice.lock().unwrap() = n.saturating_sub(1) as u16;
        Ok(())
    })?;
    let n = *salida.lock().unwrap();
    Ok(n)
}

#[derive(Serialize, Debug)]
pub struct Marca {
    pub page_index: u16,
    /// Índice dentro de `/Annots` de esa página: el mismo que manejan
    /// `get_annotations` y `remove_annotation`.
    pub annot_index: u16,
    /// En el espacio de la página VISTA, como todo lo que lee anotaciones.
    pub rect: Rect,
}

/// Las marcas de redacción de todo el documento, para la fila contextual
/// («Aplicar redacción (3 zonas)») y para pintarlas.
#[tauri::command(async)]
pub fn list_redactions(work_path: String) -> Result<Vec<Marca>, String> {
    on_pdfium_thread(move || crate::with_lopdf(&work_path, |doc| Ok(marcas_de(doc))))
}

fn marcas_de(doc: &LoDoc) -> Vec<Marca> {
    let mut out = Vec::new();
    for (n, page_id) in doc.get_pages().values().enumerate() {
        let Some(annots) = crate::anotaciones::lista_annots(doc, n as u16) else {
            continue;
        };
        let Ok(geo) = crate::formularios2::geo_vista(doc, *page_id) else {
            continue;
        };
        for (i, a) in annots.iter().enumerate() {
            let Some(annot) = dict_de(doc, a) else { continue };
            if !es_marca(&annot) {
                continue;
            }
            let Some(caja) = caja_de(&annot) else { continue };
            out.push(Marca {
                page_index: n as u16,
                annot_index: i as u16,
                rect: geo.pdf_rect_a_ui(&pdfium_render::prelude::PdfRect::new(
                    pdfium_render::prelude::PdfPoints::new(caja[1]),
                    pdfium_render::prelude::PdfPoints::new(caja[0]),
                    pdfium_render::prelude::PdfPoints::new(caja[3]),
                    pdfium_render::prelude::PdfPoints::new(caja[2]),
                )),
            });
        }
    }
    out
}

/// Las cajas de las marcas en coordenadas PDF, por página: es lo que
/// necesita `tapa_zonas`, que trabaja con PDFium y no con el rect de la UI.
fn cajas_de_marcas(doc: &LoDoc) -> Vec<(u16, [f32; 4])> {
    let mut out = Vec::new();
    for n in 0..doc.get_pages().len() as u16 {
        let Some(annots) = crate::anotaciones::lista_annots(doc, n) else {
            continue;
        };
        for a in &annots {
            let Some(annot) = dict_de(doc, a) else { continue };
            if !es_marca(&annot) {
                continue;
            }
            if let Some(c) = caja_de(&annot) {
                out.push((n, c));
            }
        }
    }
    out
}

fn dict_de(doc: &LoDoc, o: &Object) -> Option<Dictionary> {
    match o {
        Object::Dictionary(d) => Some(d.clone()),
        Object::Reference(id) => doc.get_object(*id).ok()?.as_dict().ok().cloned(),
        _ => None,
    }
}

/// `/Rect` normalizado (x0 ≤ x1, y0 ≤ y1) en coordenadas PDF.
fn caja_de(annot: &Dictionary) -> Option<[f32; 4]> {
    let v: Vec<f32> = annot
        .get(b"Rect")
        .and_then(|o| o.as_array())
        .ok()?
        .iter()
        .filter_map(|o| o.as_float().ok().or_else(|| o.as_i64().ok().map(|n| n as f32)))
        .collect();
    if v.len() != 4 {
        return None;
    }
    Some([
        v[0].min(v[2]),
        v[1].min(v[3]),
        v[0].max(v[2]),
        v[1].max(v[3]),
    ])
}

/// Quita una marca sin aplicarla («Quitar todas las marcas» va llamando a
/// esta). `mark_index` es el índice de la anotación en la página, el mismo
/// que devuelve `mark_redaction` y trae `list_redactions`.
#[tauri::command(async)]
pub fn unmark_redaction(work_path: String, page_index: u16, mark_index: u16) -> Result<(), String> {
    cirugia(&work_path, move |doc| {
        let annots = crate::anotaciones::lista_annots(doc, page_index)
            .ok_or("La página no tiene anotaciones")?;
        let a = annots
            .get(mark_index as usize)
            .ok_or("Esa marca ya no está")?;
        let annot = dict_de(doc, a).ok_or("Esa marca ya no está")?;
        if !es_marca(&annot) {
            return Err("Esa anotación no es una marca de redacción".into());
        }
        crate::anotaciones::quita_annot(doc, page_index, mark_index as usize)
    })
}

/// Aplica todas las marcas del documento: por cada zona borra los objetos
/// que la tocan, pinta el rectángulo negro y quita la marca. Todo el lote
/// en UNA mutación, así que un solo ⌘Z lo devuelve entero.
///
/// Con `dry_run` no se toca nada: solo se cuenta lo que caería, que es el
/// aviso que la UI enseña antes de la confirmación destructiva.
#[tauri::command(async)]
pub fn apply_redactions(work_path: String, dry_run: bool) -> Result<InformeRedaccion, String> {
    let cuerpo = move |work_path: String| {
        let zonas: Vec<(u16, [f32; 4])> = on_pdfium_thread({
            let w = work_path.clone();
            move || crate::with_lopdf(&w, |doc| Ok(cajas_de_marcas(doc)))
        })?;
        if zonas.is_empty() {
            return Err("No hay ninguna zona marcada para tapar".into());
        }
        let total = zonas.len() as u32;
        let informe = on_pdfium_thread({
            let w = work_path.clone();
            move || tapa_zonas(&w, &zonas, dry_run)
        })?;
        Ok(InformeRedaccion {
            zonas: total,
            textos: informe.0,
            imagenes: informe.1,
        })
    };
    if dry_run {
        cuerpo(work_path).map_err(crate::mensaje_llano)
    } else {
        mutacion(work_path, cuerpo)
    }
}

#[derive(Serialize, Debug)]
pub struct InformeRedaccion {
    pub zonas: u32,
    pub textos: u32,
    pub imagenes: u32,
}

/// El trabajo sucio: borrar los objetos de cada zona y pintar el negro.
/// Devuelve (textos, imágenes).
fn tapa_zonas(
    work_path: &str,
    zonas: &[(u16, [f32; 4])],
    dry_run: bool,
) -> Result<(u32, u32), String> {
    use pdfium_render::prelude::*;
    let pdfium = crate::pdfium()?;
    let doc = pdfium
        .load_pdf_from_file(work_path, None)
        .map_err(crate::mensaje_llano)?;
    let (mut textos, mut imagenes) = (0u32, 0u32);
    let mut paginas: Vec<u16> = zonas.iter().map(|z| z.0).collect();
    paginas.sort_unstable();
    paginas.dedup();
    for p in paginas {
        let mut page = doc.pages().get(p).map_err(crate::mensaje_llano)?;
        // las cajas ya vienen en coordenadas PDF: son el /Rect de la marca
        let cajas: Vec<PdfRect> = zonas
            .iter()
            .filter(|z| z.0 == p)
            .map(|z| {
                PdfRect::new(
                    PdfPoints::new(z.1[1]),
                    PdfPoints::new(z.1[0]),
                    PdfPoints::new(z.1[3]),
                    PdfPoints::new(z.1[2]),
                )
            })
            .collect();
        let mut caen: Vec<usize> = Vec::new();
        {
            let objects = page.objects();
            for i in 0..objects.len() {
                let Ok(obj) = objects.get(i) else { continue };
                let es_texto = obj.as_text_object().is_some();
                let es_imagen = obj.as_image_object().is_some();
                if !es_texto && !es_imagen {
                    continue;
                }
                let Ok(b) = obj.bounds() else { continue };
                let solapa = cajas.iter().any(|c| {
                    b.left().value < c.right().value
                        && b.right().value > c.left().value
                        && b.bottom().value < c.top().value
                        && b.top().value > c.bottom().value
                });
                if solapa {
                    caen.push(i);
                    if es_texto {
                        textos += 1;
                    } else {
                        imagenes += 1;
                    }
                }
            }
        }
        if dry_run {
            continue;
        }
        for &i in caen.iter().rev() {
            let removed = page
                .objects_mut()
                .remove_object_at_index(i)
                .map_err(crate::mensaje_llano)?;
            // regla del proyecto: su Drop llama a FPDFPageObj_Destroy y
            // PDFium casca — fuga puntual asumida
            std::mem::forget(removed);
        }
        for c in &cajas {
            let negro = PdfPagePathObject::new_rect(
                &doc,
                *c,
                None,
                None,
                Some(PdfColor::new(0, 0, 0, 255)),
            )
            .map_err(crate::mensaje_llano)?;
            page.objects_mut()
                .add_path_object(negro)
                .map_err(crate::mensaje_llano)?;
        }
        page.regenerate_content().map_err(crate::mensaje_llano)?;
    }
    if dry_run {
        drop(doc);
        crate::invalidate_doc_cache();
        return Ok((textos, imagenes));
    }
    crate::save_and_close(doc, work_path)?;
    // y fuera las marcas: ya no proponen nada, el contenido no está
    crate::cirugia_en_hilo(work_path, |doc| {
        let marcas = marcas_de(doc);
        // de mayor a menor para que los índices no se muevan bajo los pies
        let mut por_pagina: std::collections::BTreeMap<u16, Vec<u16>> = Default::default();
        for m in marcas {
            por_pagina.entry(m.page_index).or_default().push(m.annot_index);
        }
        for (p, mut indices) in por_pagina {
            indices.sort_unstable();
            for i in indices.into_iter().rev() {
                crate::anotaciones::quita_annot(doc, p, i as usize)?;
            }
        }
        Ok(())
    })?;
    Ok((textos, imagenes))
}

#[derive(Serialize, Debug, Default)]
pub struct SanitizeReport {
    pub metadatos: u32,
    pub scripts: u32,
    pub adjuntos: u32,
    pub capas: u32,
    pub formularios: u32,
}

/// «Quitar información oculta»: lo que el documento lleva escrito y no se
/// ve. Con `dry_run` solo cuenta, que es el ensayo previo que la UI enseña
/// («Se quitarán: autor y título, 2 adjuntos, 1 script»).
///
/// Se van: `/Info` y el XMP del catálogo; los `/JavaScript` del `/Names` y
/// los `/OpenAction` y `/AA` que ejecuten código; los `/EmbeddedFiles` y
/// las anotaciones `/FileAttachment`; las capas (`/OCProperties`, y las
/// anotaciones que colgaran de una capa apagada); y los campos de
/// formulario con lo que lleven escrito.
#[tauri::command(async)]
pub fn sanitize_pdf(work_path: String, dry_run: bool) -> Result<SanitizeReport, String> {
    if dry_run {
        return on_pdfium_thread(move || {
            crate::with_lopdf(&work_path, |doc| Ok(cuenta_oculto(doc).0))
        });
    }
    let informe = std::sync::Arc::new(std::sync::Mutex::new(SanitizeReport::default()));
    let salida = informe.clone();
    cirugia(&work_path, move |doc| {
        let (r, fuera) = cuenta_oculto(doc);
        quita_oculto(doc, &fuera);
        *informe.lock().unwrap() = r;
        Ok(())
    })?;
    let r = std::mem::take(&mut *salida.lock().unwrap());
    Ok(r)
}

/// Cuenta la información oculta y, de paso, apunta qué anotaciones hay que
/// quitar (adjuntos, widgets de formulario y las que ejecutan código).
fn cuenta_oculto(doc: &LoDoc) -> (SanitizeReport, Vec<(u16, usize)>) {
    let mut r = SanitizeReport::default();
    let mut fuera: Vec<(u16, usize)> = Vec::new();
    let Ok(root) = doc.trailer.get(b"Root").and_then(|o| o.as_reference()) else {
        return (r, fuera);
    };

    // metadatos: cada entrada de /Info con contenido, más el XMP
    if let Ok(info_ref) = doc.trailer.get(b"Info").and_then(|o| o.as_reference()) {
        if let Some(info) = dict_de(doc, &Object::Reference(info_ref)) {
            r.metadatos += info.iter().filter(|(k, _)| *k != b"Producer").count() as u32;
        }
    }
    let catalogo = dict_de(doc, &Object::Reference(root)).unwrap_or_default();
    if catalogo.has(b"Metadata") {
        r.metadatos += 1;
    }

    // scripts y adjuntos: el /Names del catálogo, más /OpenAction y /AA
    if let Some(names) = catalogo.get(b"Names").ok().and_then(|o| dict_de(doc, o)) {
        r.scripts += cuenta_arbol(doc, names.get(b"JavaScript").ok());
        r.adjuntos += cuenta_arbol(doc, names.get(b"EmbeddedFiles").ok());
    }
    if catalogo.has(b"OpenAction") {
        r.scripts += 1;
    }
    if catalogo.has(b"AA") {
        r.scripts += 1;
    }

    // capas y formularios
    if let Some(oc) = catalogo.get(b"OCProperties").ok().and_then(|o| dict_de(doc, o)) {
        r.capas += match oc.get(b"OCGs") {
            Ok(Object::Array(a)) => a.len() as u32,
            _ => 1,
        };
    }
    if let Some(form) = catalogo.get(b"AcroForm").ok().and_then(|o| dict_de(doc, o)) {
        r.formularios += match form.get(b"Fields") {
            Ok(Object::Array(a)) => a.len() as u32,
            Ok(Object::Reference(id)) => doc
                .get_object(*id)
                .and_then(|o| o.as_array())
                .map(|a| a.len() as u32)
                .unwrap_or(0),
            _ => 0,
        };
    }

    // lo que cuelga de las páginas: adjuntos, campos y anotaciones con
    // código detrás
    for p in 0..doc.get_pages().len() as u16 {
        let Some(annots) = crate::anotaciones::lista_annots(doc, p) else {
            continue;
        };
        for (i, a) in annots.iter().enumerate() {
            let Some(annot) = dict_de(doc, a) else { continue };
            let subtipo = annot.get(b"Subtype").and_then(|o| o.as_name()).unwrap_or_default();
            if subtipo == b"FileAttachment" {
                r.adjuntos += 1;
            } else if subtipo == b"Widget" {
                // ya contado por /Fields
            } else if annot.has(b"AA") || annot.has(b"JS") {
                r.scripts += 1;
            } else {
                continue;
            }
            fuera.push((p, i));
        }
    }
    (r, fuera)
}

/// Quita de verdad lo que `cuenta_oculto` encontró.
fn quita_oculto(doc: &mut LoDoc, fuera: &[(u16, usize)]) {
    let Ok(root) = doc.trailer.get(b"Root").and_then(|o| o.as_reference()) else {
        return;
    };
    doc.trailer.remove(b"Info");
    // de mayor a menor: los índices no se mueven bajo los pies
    let mut fuera = fuera.to_vec();
    fuera.sort_unstable();
    for (p, i) in fuera.into_iter().rev() {
        let _ = crate::anotaciones::quita_annot(doc, p, i);
    }
    let nombres = doc
        .get_object(root)
        .and_then(|o| o.as_dict())
        .ok()
        .and_then(|c| c.get(b"Names").ok().cloned());
    if let Ok(cat) = doc.get_object_mut(root).and_then(|o| o.as_dict_mut()) {
        for clave in [
            &b"Metadata"[..],
            b"OpenAction",
            b"AA",
            b"OCProperties",
            b"AcroForm",
        ] {
            cat.remove(clave);
        }
        if let Ok(Object::Dictionary(names)) = cat.get_mut(b"Names") {
            names.remove(b"JavaScript");
            names.remove(b"EmbeddedFiles");
        }
    }
    if let Some(Object::Reference(id)) = nombres {
        if let Ok(names) = doc.get_object_mut(id).and_then(|o| o.as_dict_mut()) {
            names.remove(b"JavaScript");
            names.remove(b"EmbeddedFiles");
        }
    }
    // quitar la referencia no basta: el script y el adjunto seguirían en el
    // fichero, solo que sin nadie que apunte a ellos. «Quitar información
    // oculta» tiene que dejarlos fuera de verdad.
    doc.prune_objects();
}

/// Cuenta las hojas de un árbol de nombres (`/Names` o `/Kids`).
fn cuenta_arbol(doc: &LoDoc, nodo: Option<&Object>) -> u32 {
    let Some(nodo) = nodo else { return 0 };
    let Some(d) = dict_de(doc, nodo) else { return 0 };
    let mut n = match d.get(b"Names") {
        Ok(Object::Array(a)) => (a.len() / 2) as u32,
        _ => 0,
    };
    if let Ok(Object::Array(kids)) = d.get(b"Kids") {
        for k in kids.clone() {
            n += cuenta_arbol(doc, Some(&k));
        }
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::crea_pdf;

    fn pasos(work: &str) -> u16 {
        crate::historial::history_state(work.to_string()).expect("historial").undo
    }

    /// ¿Es negro el píxel del centro de ese rect en el render?
    fn negro_en(work: &str, pagina: u16, r: &Rect) -> bool {
        let sizes = crate::get_page_sizes(work.to_string()).expect("tamaños");
        let escala = 600.0 / sizes[pagina as usize].width;
        let png = crate::render_page_png(work.to_string(), pagina, 600, true).expect("render");
        let img = image::load_from_memory(&png).expect("PNG").to_rgba8();
        let (x, y) = (
            ((r.x + r.w / 2.0) * escala) as u32,
            ((r.y + r.h / 2.0) * escala) as u32,
        );
        let p = img.get_pixel(x.min(img.width() - 1), y.min(img.height() - 1)).0;
        p[0] < 40 && p[1] < 40 && p[2] < 40
    }

    /// Acrobat separa marcar de aplicar: se dibujan rectángulos rojos que se
    /// revisan, se mueven y se borran, y solo al pulsar «Aplicar» desaparece
    /// el contenido. Marcar no destruye nada; aplicar es un solo ⌘Z.
    #[test]
    fn marcar_revisar_y_aplicar_la_redaccion() {
        let pdf = std::env::temp_dir().join("seguridad2-redaccion-test.pdf");
        crea_pdf(&["Confidencial", "Segunda"], &pdf);
        let work = pdf.to_string_lossy().into_owned();
        // el texto de crea_pdf va en (50, 700) desde abajo: en coords de UI,
        // arriba del todo de una A4
        let zona = Rect { x: 40.0, y: 120.0, w: 160.0, h: 40.0 };
        let otra = Rect { x: 300.0, y: 400.0, w: 100.0, h: 30.0 };

        let i = mark_redaction(work.clone(), 0, zona.clone()).expect("marcar");
        mark_redaction(work.clone(), 1, otra.clone()).expect("marcar en la otra página");
        let marcas = list_redactions(work.clone()).expect("listar");
        assert_eq!(marcas.len(), 2, "dos zonas marcadas");
        assert_eq!(marcas[0].page_index, 0);
        assert_eq!(marcas[0].annot_index, i);
        assert!(
            (marcas[0].rect.x - zona.x).abs() < 1.0 && (marcas[0].rect.y - zona.y).abs() < 1.0,
            "la marca se lee en ({:.1},{:.1})",
            marcas[0].rect.x,
            marcas[0].rect.y
        );
        // marcar NO borra: el texto sigue ahí
        let texto: String = crate::busqueda::get_page_text(work.clone(), 0)
            .expect("texto")
            .chars
            .iter()
            .map(|c| c.ch.as_str())
            .collect();
        assert!(texto.contains("Confidencial"), "marcar no puede borrar nada");

        // quitar una marca deja la otra
        unmark_redaction(work.clone(), 1, marcas[1].annot_index).expect("quitar marca");
        assert_eq!(list_redactions(work.clone()).expect("listar").len(), 1);

        // el ensayo previo cuenta y no toca nada
        let antes = pasos(&work);
        let previo = apply_redactions(work.clone(), true).expect("ensayo");
        assert_eq!(previo.zonas, 1);
        assert!(previo.textos >= 1, "el ensayo cuenta el texto que caería");
        assert_eq!(pasos(&work), antes, "el ensayo no deja paso de deshacer");
        assert_eq!(list_redactions(work.clone()).expect("listar").len(), 1);

        // aplicar: el texto desaparece, queda negro y la marca se va
        let informe = apply_redactions(work.clone(), false).expect("aplicar");
        assert_eq!(informe.zonas, 1);
        let texto: String = crate::busqueda::get_page_text(work.clone(), 0)
            .expect("texto")
            .chars
            .iter()
            .map(|c| c.ch.as_str())
            .collect();
        assert!(
            !texto.contains("Confidencial"),
            "el texto redactado sigue en el documento: {texto:?}"
        );
        assert!(negro_en(&work, 0, &zona), "tiene que quedar negro donde estaba");
        assert!(
            list_redactions(work.clone()).expect("listar").is_empty(),
            "la marca aplicada ya no propone nada"
        );

        // y todo el lote se deshace de una vez
        assert_eq!(pasos(&work), antes + 1, "aplicar es UN paso");
        crate::historial::undo(work.clone()).expect("deshacer");
        let texto: String = crate::busqueda::get_page_text(work.clone(), 0)
            .expect("texto")
            .chars
            .iter()
            .map(|c| c.ch.as_str())
            .collect();
        assert!(texto.contains("Confidencial"), "⌘Z devuelve el lote entero");
        std::fs::remove_file(&pdf).ok();
    }

    /// «Quitar información oculta»: el ensayo previo cuenta por categoría y,
    /// al aplicarlo, lo que el documento llevaba escondido deja de estar.
    #[test]
    fn sanitizar_cuenta_antes_y_limpia_despues() {
        let pdf = std::env::temp_dir().join("seguridad2-sanitizar-test.pdf");
        crea_pdf(&["Documento"], &pdf);
        let work = pdf.to_string_lossy().into_owned();

        // un documento con de todo: autor, JavaScript, adjunto y un
        // /OpenAction
        crate::cirugia(&work, |doc| {
            use lopdf::{Dictionary, Object, Stream};
            let mut info = Dictionary::new();
            info.set("Author", Object::string_literal("Jorge"));
            info.set("Title", Object::string_literal("Secreto"));
            let info_id = doc.add_object(info);
            doc.trailer.set("Info", Object::Reference(info_id));

            let js = doc.add_object(Stream::new(Dictionary::new(), b"app.alert('hola')".to_vec()));
            let mut accion = Dictionary::new();
            accion.set("S", Object::Name(b"JavaScript".to_vec()));
            accion.set("JS", Object::Reference(js));
            let accion_id = doc.add_object(accion);
            let mut js_tree = Dictionary::new();
            js_tree.set(
                "Names",
                Object::Array(vec![
                    Object::string_literal("hola"),
                    Object::Reference(accion_id),
                ]),
            );
            let mut adjunto = Dictionary::new();
            adjunto.set("Type", Object::Name(b"Filespec".to_vec()));
            adjunto.set("F", Object::string_literal("nomina.txt"));
            let adjunto_id = doc.add_object(adjunto);
            let mut ef_tree = Dictionary::new();
            ef_tree.set(
                "Names",
                Object::Array(vec![
                    Object::string_literal("nomina.txt"),
                    Object::Reference(adjunto_id),
                ]),
            );
            let mut names = Dictionary::new();
            names.set("JavaScript", Object::Dictionary(js_tree));
            names.set("EmbeddedFiles", Object::Dictionary(ef_tree));
            let root = doc
                .trailer
                .get(b"Root")
                .and_then(|o| o.as_reference())
                .map_err(|e| e.to_string())?;
            let cat = doc
                .get_object_mut(root)
                .and_then(|o| o.as_dict_mut())
                .map_err(|e| e.to_string())?;
            cat.set("Names", Object::Dictionary(names));
            cat.set("OpenAction", Object::Reference(accion_id));
            Ok(())
        })
        .expect("ensuciar el documento");

        let previo = sanitize_pdf(work.clone(), true).expect("ensayo");
        assert!(previo.metadatos >= 2, "autor y título: {previo:?}");
        assert_eq!(previo.scripts, 2, "el /Names y el /OpenAction: {previo:?}");
        assert_eq!(previo.adjuntos, 1, "un adjunto: {previo:?}");

        let antes = pasos(&work);
        let hecho = sanitize_pdf(work.clone(), false).expect("limpiar");
        assert_eq!(hecho.scripts, previo.scripts);
        assert_eq!(pasos(&work), antes + 1, "limpiar es un paso de deshacer");

        let bytes = std::fs::read(&pdf).expect("leer");
        assert!(
            !bytes.windows(11).any(|w| w == b"/JavaScript"),
            "el documento se queda sin JavaScript"
        );
        assert!(
            !bytes.windows(14).any(|w| w == b"/EmbeddedFiles"),
            "el documento se queda sin adjuntos"
        );
        let queda = sanitize_pdf(work.clone(), true).expect("segundo ensayo");
        assert_eq!(queda.scripts, 0);
        assert_eq!(queda.adjuntos, 0);
        assert_eq!(queda.metadatos, 0);
        std::fs::remove_file(&pdf).ok();
    }
}
