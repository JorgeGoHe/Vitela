//! Creación de campos de formulario y enlaces (PDFium no los crea: cirugía
//! con lopdf, mismo patrón que el campo de firma de firma.rs). El borrado de
//! campos no se ofrece en v1 (dejaría huérfanos en /Fields); los enlaces son
//! anotaciones normales y se borran con remove_annotation.

use crate::{cirugia, Rect};
use lopdf::{Dictionary, Document as LoDoc, Object, ObjectId, Stream, StringFormat};

/// MediaBox de una página, buscando en el propio dict o heredado del árbol.
fn media_box(doc: &LoDoc, page_id: ObjectId) -> Result<[f32; 4], String> {
    let mut actual = page_id;
    for _ in 0..32 {
        let dict = doc
            .get_object(actual)
            .and_then(|o| o.as_dict())
            .map_err(|e| e.to_string())?;
        if let Ok(mb) = dict.get(b"MediaBox") {
            let mb = match mb {
                Object::Reference(rid) => doc
                    .get_object(*rid)
                    .and_then(|o| o.as_array())
                    .map_err(|e| e.to_string())?,
                Object::Array(a) => a,
                _ => return Err("MediaBox inválido".into()),
            };
            let v: Vec<f32> = mb
                .iter()
                .map(|o| match o {
                    Object::Integer(i) => *i as f32,
                    Object::Real(r) => *r,
                    _ => 0.0,
                })
                .collect();
            if v.len() == 4 {
                return Ok([v[0], v[1], v[2], v[3]]);
            }
        }
        match dict.get(b"Parent") {
            Ok(Object::Reference(rid)) => actual = *rid,
            _ => break,
        }
    }
    Err("La página no tiene MediaBox".into())
}

/// Añade una anotación al array Annots de la página (directo o referencia).
pub(crate) fn anade_a_annots(doc: &mut LoDoc, page_id: ObjectId, annot_id: ObjectId) -> Result<(), String> {
    anade_a_annots_en(doc, page_id, annot_id, None)
}

/// Como [`anade_a_annots`], pero pudiendo colocar la anotación **en una
/// posición** del array. El orden de `/Annots` es el orden de tabulación de
/// la página cuando lleva `/Tabs /S`, que es lo que hace Acrobat: por eso
/// «orden de tabulación» en el diálogo de propiedades es colocar el widget
/// en la lista, no un número que se guarde en ningún sitio.
pub(crate) fn anade_a_annots_en(
    doc: &mut LoDoc,
    page_id: ObjectId,
    annot_id: ObjectId,
    posicion: Option<usize>,
) -> Result<(), String> {
    let mete = |arr: &mut Vec<Object>| {
        let pos = posicion.unwrap_or(arr.len()).min(arr.len());
        arr.insert(pos, Object::Reference(annot_id));
    };
    let annots_ref = {
        let page = doc
            .get_object(page_id)
            .and_then(|o| o.as_dict())
            .map_err(|e| e.to_string())?;
        match page.get(b"Annots") {
            Ok(Object::Reference(rid)) => Some(*rid),
            _ => None,
        }
    };
    if let Some(rid) = annots_ref {
        mete(
            doc.get_object_mut(rid)
                .and_then(|o| o.as_array_mut())
                .map_err(|e| e.to_string())?,
        );
    } else {
        let page = doc
            .get_object_mut(page_id)
            .and_then(|o| o.as_dict_mut())
            .map_err(|e| e.to_string())?;
        match page.get_mut(b"Annots") {
            Ok(Object::Array(arr)) => mete(arr),
            _ => page.set("Annots", Object::Array(vec![Object::Reference(annot_id)])),
        }
    }
    Ok(())
}

/// Geometría de la página para convertir las coordenadas que manda la UI.
/// Los comandos que escriben trabajan en el espacio PROPIO de la página
/// (sin rotar), que es la caja de la página con `/Rotate` a cero.
pub(crate) fn geo_pagina(doc: &LoDoc, page_id: ObjectId) -> Result<crate::Geo, String> {
    let mb = media_box(doc, page_id)?;
    Ok(crate::Geo::nueva(&mb, 0))
}

/// Como [`geo_pagina`] pero con el `/Rotate` de la página puesto: es el
/// espacio de la página VISTA, el que devuelven los comandos que leen
/// anotaciones.
pub(crate) fn geo_vista(doc: &LoDoc, page_id: ObjectId) -> Result<crate::Geo, String> {
    let mb = media_box(doc, page_id)?;
    Ok(crate::Geo::nueva(&mb, rotacion(doc, page_id)))
}

/// `/Rotate` de la página, heredado del árbol de páginas si hace falta.
fn rotacion(doc: &LoDoc, page_id: ObjectId) -> u16 {
    let mut actual = page_id;
    for _ in 0..32 {
        let Ok(dict) = doc.get_object(actual).and_then(|o| o.as_dict()) else {
            return 0;
        };
        if let Ok(r) = dict.get(b"Rotate").and_then(|o| o.as_i64()) {
            return r.rem_euclid(360) as u16;
        }
        match dict.get(b"Parent").and_then(|o| o.as_reference()) {
            Ok(p) => actual = p,
            Err(_) => return 0,
        }
    }
    0
}

/// Rect de UI (origen arriba-izquierda) a array Rect PDF de la página dada.
fn rect_pdf(rect: &Rect, geo: &crate::Geo) -> Object {
    let r = geo.ui_rect_a_pdf(rect);
    Object::Array(vec![
        r.left().value.into(),
        r.bottom().value.into(),
        r.right().value.into(),
        r.top().value.into(),
    ])
}

/// Las propiedades que Acrobat pone en el primer panel de «Preparar
/// formulario». Llegan anidadas y **en snake_case**: Tauri solo traduce
/// camelCase a snake_case en los argumentos de primer nivel, no dentro de
/// las estructuras.
#[derive(serde::Deserialize, Default, Clone, Debug)]
#[serde(default)]
pub struct PropsCampo {
    /// `/TU`: el texto de ayuda que sale al pasar el ratón.
    pub tooltip: Option<String>,
    /// `/Ff` bit 2: sin rellenar, el formulario no se puede enviar.
    pub obligatorio: bool,
    /// `/Ff` bit 1.
    pub solo_lectura: bool,
    /// `/DV` (y `/V` al crearlo, para que se vea).
    pub valor_defecto: Option<String>,
    /// Dónde entra el campo en el orden de tabulación, que en un PDF es la
    /// posición dentro del `/Annots` de la página (con `/Tabs /S`).
    pub orden_tab: Option<u16>,
}

/// Un `/AP /N` de dos estados dibujado a mano. El marco y la marca van
/// **dentro** de la apariencia: si se dejan en manos del visor (`/MK`),
/// aplanar se queda sin nada que copiar y el campo sin marcar desaparece.
fn apariencia_dos_estados(
    doc: &mut LoDoc,
    w: f32,
    h: f32,
    encendido: &str,
    marco: &str,
    marca: &str,
) -> Object {
    let forma = |c: &str| {
        let mut d = Dictionary::new();
        d.set("Type", Object::Name(b"XObject".to_vec()));
        d.set("Subtype", Object::Name(b"Form".to_vec()));
        d.set(
            "BBox",
            Object::Array(vec![0.into(), 0.into(), w.into(), h.into()]),
        );
        d.set("Resources", Object::Dictionary(Dictionary::new()));
        Stream::new(d, c.as_bytes().to_vec())
    };
    let off_id = doc.add_object(forma(marco));
    let on_id = doc.add_object(forma(&format!("{marco}{marca}")));
    let mut estados = Dictionary::new();
    estados.set("Off", Object::Reference(off_id));
    estados.set(encendido, Object::Reference(on_id));
    let mut ap = Dictionary::new();
    ap.set("N", Object::Dictionary(estados));
    Object::Dictionary(ap)
}

/// Un círculo por cuatro bézieres, que es como se dibuja un círculo en un
/// content stream (no hay operador de arco).
fn circulo(cx: f32, cy: f32, r: f32) -> String {
    let k = r * 0.5523;
    format!(
        "{:.2} {:.2} m {:.2} {:.2} {:.2} {:.2} {:.2} {:.2} c \
         {:.2} {:.2} {:.2} {:.2} {:.2} {:.2} c \
         {:.2} {:.2} {:.2} {:.2} {:.2} {:.2} c \
         {:.2} {:.2} {:.2} {:.2} {:.2} {:.2} c ",
        cx + r, cy,
        cx + r, cy + k, cx + k, cy + r, cx, cy + r,
        cx - k, cy + r, cx - r, cy + k, cx - r, cy,
        cx - r, cy - k, cx - k, cy - r, cx, cy - r,
        cx + k, cy - r, cx + r, cy - k, cx + r, cy,
    )
}

/// El campo de un grupo de radios que ya existe: `/FT /Btn` con el bit de
/// Radio puesto y el `/T` del grupo. Es lo que hace que marcar uno desmarque
/// los hermanos: **una sola entrada en `/Fields` con un `/Kids` por opción**,
/// y no tres campos independientes, que es el defecto de Acrobat que aquí no
/// se copia.
fn grupo_de_radios(doc: &LoDoc, grupo: &str) -> Option<ObjectId> {
    doc.objects.iter().find_map(|(id, o)| {
        let d = o.as_dict().ok()?;
        if d.get(b"FT").and_then(|o| o.as_name()).ok()? != b"Btn" {
            return None;
        }
        if d.get(b"Ff").and_then(|o| o.as_i64()).unwrap_or(0) & RADIO == 0 {
            return None;
        }
        (crate::anotaciones::texto_de_cadena_pdf(d.get(b"T").ok()?) == grupo).then_some(*id)
    })
}

/// `/Ff` bit 16: botón de radio.
const RADIO: i64 = 32_768;
/// El mismo bit, para quien tenga que reconocer un grupo de radios desde
/// fuera del módulo (`set_form_checked`).
pub(crate) const RADIO_FF: i64 = RADIO;
/// `/Ff` bit 18: desplegable (sin él, lista).
const COMBO: i64 = 131_072;
/// `/Ff` bit 1: solo lectura.
const SOLO_LECTURA: i64 = 1;
/// `/Ff` bit 2: obligatorio.
pub(crate) const OBLIGATORIO: i64 = 2;

/// Crea un campo de formulario en la página: texto, casilla, **botón de
/// radio**, **desplegable** o **lista**, con las propiedades que Acrobat
/// pone en el primer panel de «Preparar formulario».
///
/// - `radio` necesita `group` (el nombre del grupo, que es el `/T` del
///   campo) y `export_value` (el valor de esa opción; sin él, `name`). Los
///   widgets del mismo grupo cuelgan de **una sola entrada de `/Fields`**
///   por su `/Kids`, que es lo que hace que desmarcar los hermanos sea
///   automático y no una casualidad del visor.
/// - `combo` y `list` necesitan `options`.
/// - `props` lleva tooltip (`/TU`), obligatorio y solo lectura (`/Ff` bits
///   2 y 1), valor por defecto (`/DV`) y orden de tabulación (la posición
///   en el `/Annots` de la página, que es lo que ordena la tabulación
///   cuando la página lleva `/Tabs /S`).
#[tauri::command(async)]
#[allow(clippy::too_many_arguments)]
pub fn create_form_field(
    work_path: String,
    page_index: u16,
    kind: String,
    rect: Rect,
    name: String,
    group: Option<String>,
    export_value: Option<String>,
    options: Option<Vec<String>>,
    props: Option<PropsCampo>,
) -> Result<(), String> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err("El campo necesita un nombre".into());
    }
    if rect.w < 8.0 || rect.h < 8.0 {
        return Err("El área del campo es demasiado pequeña".into());
    }
    let props = props.unwrap_or_default();
    let grupo = group.map(|g| g.trim().to_string()).unwrap_or_default();
    if kind == "radio" && grupo.is_empty() {
        return Err("Un botón de radio necesita el nombre de su grupo".into());
    }
    let opciones: Vec<String> = options
        .unwrap_or_default()
        .into_iter()
        .map(|o| o.trim().to_string())
        .filter(|o| !o.is_empty())
        .collect();
    if matches!(kind.as_str(), "combo" | "list") && opciones.is_empty() {
        return Err("Un desplegable necesita al menos una opción".into());
    }
    cirugia(&work_path, move |doc| {
        let page_id = *doc
            .get_pages()
            .get(&(page_index as u32 + 1))
            .ok_or("Página fuera de rango")?;
        let geo = geo_pagina(doc, page_id)?;
        let mut banderas = 0i64;
        if props.obligatorio {
            banderas |= OBLIGATORIO;
        }
        if props.solo_lectura {
            banderas |= SOLO_LECTURA;
        }

        // nombres existentes para garantizar unicidad de T
        let existentes: Vec<String> = doc
            .objects
            .values()
            .filter_map(|o| o.as_dict().ok())
            .filter(|d| d.has(b"T"))
            .filter_map(|d| d.get(b"T").ok())
            .filter_map(|t| match t {
                Object::String(b, _) => Some(String::from_utf8_lossy(b).into_owned()),
                _ => None,
            })
            .collect();
        let mut nombre = name.clone();
        let mut n = 2;
        while existentes.contains(&nombre) {
            nombre = format!("{name}-{n}");
            n += 1;
        }

        let mut widget = Dictionary::new();
        widget.set("Type", Object::Name(b"Annot".to_vec()));
        widget.set("Subtype", Object::Name(b"Widget".to_vec()));
        widget.set("Rect", rect_pdf(&rect, &geo));
        widget.set("F", 4i64); // Print
        widget.set("DA", Object::string_literal("/Helv 0 Tf 0 g"));
        let mut mk = Dictionary::new();
        mk.set("BC", Object::Array(vec![0.into(), 0.into(), 0.into()]));
        widget.set("MK", Object::Dictionary(mk));
        let mut bs = Dictionary::new();
        bs.set("W", Object::Integer(1));
        bs.set("S", Object::Name(b"S".to_vec()));
        widget.set("BS", Object::Dictionary(bs));
        let (ancho, alto) = (rect.w, rect.h);
        let marco_rect = format!(
            "q 0 0 0 RG 1 w 0.5 0.5 {:.2} {:.2} re S Q\n",
            ancho - 1.0,
            alto - 1.0
        );

        // el campo que va en /Fields: el widget, o el padre del grupo de
        // radios (que es uno para las tres opciones)
        let campo_id: ObjectId;
        let widget_id: ObjectId;
        match kind.as_str() {
            "text" => {
                widget.set("T", Object::string_literal(nombre.clone()));
                widget.set("FT", Object::Name(b"Tx".to_vec()));
                let valor = props.valor_defecto.clone().unwrap_or_default();
                widget.set("V", crate::documento::cadena_pdf(&valor));
                if props.valor_defecto.is_some() {
                    widget.set("DV", crate::documento::cadena_pdf(&valor));
                }
                pon_comunes(&mut widget, &props, banderas);
                widget_id = doc.add_object(widget);
                campo_id = widget_id;
            }
            "checkbox" => {
                widget.set("T", Object::string_literal(nombre.clone()));
                widget.set("FT", Object::Name(b"Btn".to_vec()));
                let marcada = props
                    .valor_defecto
                    .as_deref()
                    .is_some_and(|v| matches!(v, "Yes" | "on" | "sí" | "si" | "true"));
                let estado = if marcada { "Yes" } else { "Off" };
                widget.set("V", Object::Name(estado.as_bytes().to_vec()));
                widget.set("AS", Object::Name(estado.as_bytes().to_vec()));
                if props.valor_defecto.is_some() {
                    widget.set("DV", Object::Name(estado.as_bytes().to_vec()));
                }
                let aspa = format!(
                    "q 0 g 1.5 w 2 2 m {} {} l S 2 {} m {} 2 l S Q",
                    ancho - 2.0,
                    alto - 2.0,
                    alto - 2.0,
                    ancho - 2.0
                );
                let ap = apariencia_dos_estados(doc, ancho, alto, "Yes", &marco_rect, &aspa);
                widget.set("AP", ap);
                pon_comunes(&mut widget, &props, banderas);
                widget_id = doc.add_object(widget);
                campo_id = widget_id;
            }
            "radio" => {
                let export = export_value
                    .as_deref()
                    .map(str::trim)
                    .filter(|v| !v.is_empty())
                    .unwrap_or(&name)
                    .to_string();
                if export == "Off" {
                    return Err("«Off» no puede ser el valor de una opción".into());
                }
                let elegida = props.valor_defecto.as_deref() == Some(export.as_str());
                widget.set(
                    "AS",
                    Object::Name(if elegida { export.as_bytes().to_vec() } else { b"Off".to_vec() }),
                );
                let r = (ancho.min(alto) / 2.0 - 1.0).max(1.0);
                let (cx, cy) = (ancho / 2.0, alto / 2.0);
                let marco = format!("q 0 0 0 RG 1 w {}S Q\n", circulo(cx, cy, r));
                let punto = format!("q 0 g {}f Q\n", circulo(cx, cy, r * 0.5));
                let ap = apariencia_dos_estados(doc, ancho, alto, &export, &marco, &punto);
                widget.set("AP", ap);
                // el padre: se reutiliza el del grupo si ya lo hay, y sus
                // opciones se desmarcan solas porque son /Kids del MISMO
                // campo
                match grupo_de_radios(doc, &grupo) {
                    Some(padre) => {
                        widget.set("Parent", Object::Reference(padre));
                        widget_id = doc.add_object(widget);
                        let p = doc
                            .get_object_mut(padre)
                            .and_then(|o| o.as_dict_mut())
                            .map_err(|e| e.to_string())?;
                        match p.get_mut(b"Kids") {
                            Ok(Object::Array(k)) => k.push(Object::Reference(widget_id)),
                            _ => p.set("Kids", Object::Array(vec![Object::Reference(widget_id)])),
                        }
                        if elegida {
                            p.set("V", Object::Name(export.as_bytes().to_vec()));
                            p.set("DV", Object::Name(export.as_bytes().to_vec()));
                        }
                        // el grupo ya está en /Fields: no se vuelve a meter
                        return remata_acroform(doc, page_id, widget_id, None, &props);
                    }
                    None => {
                        let mut padre = Dictionary::new();
                        padre.set("FT", Object::Name(b"Btn".to_vec()));
                        padre.set("T", Object::string_literal(grupo.clone()));
                        padre.set("Ff", Object::Integer(RADIO | banderas));
                        let v = if elegida { export.as_bytes().to_vec() } else { b"Off".to_vec() };
                        padre.set("V", Object::Name(v.clone()));
                        padre.set("DV", Object::Name(v));
                        if let Some(t) = props.tooltip.as_deref().filter(|t| !t.trim().is_empty()) {
                            padre.set("TU", crate::documento::cadena_pdf(t));
                        }
                        let padre_id = doc.add_object(padre);
                        widget.set("Parent", Object::Reference(padre_id));
                        widget_id = doc.add_object(widget);
                        doc.get_object_mut(padre_id)
                            .and_then(|o| o.as_dict_mut())
                            .map_err(|e| e.to_string())?
                            .set("Kids", Object::Array(vec![Object::Reference(widget_id)]));
                        campo_id = padre_id;
                    }
                }
            }
            "combo" | "list" => {
                widget.set("T", Object::string_literal(nombre.clone()));
                widget.set("FT", Object::Name(b"Ch".to_vec()));
                widget.set(
                    "Opt",
                    Object::Array(
                        opciones
                            .iter()
                            .map(|o| crate::documento::cadena_pdf(o))
                            .collect(),
                    ),
                );
                if kind == "combo" {
                    banderas |= COMBO;
                }
                let valor = props
                    .valor_defecto
                    .clone()
                    .filter(|v| opciones.contains(v))
                    .unwrap_or_default();
                widget.set("V", crate::documento::cadena_pdf(&valor));
                if !valor.is_empty() {
                    widget.set("DV", crate::documento::cadena_pdf(&valor));
                    if let Some(i) = opciones.iter().position(|o| *o == valor) {
                        widget.set("I", Object::Array(vec![Object::Integer(i as i64)]));
                    }
                }
                pon_comunes(&mut widget, &props, banderas);
                widget_id = doc.add_object(widget);
                campo_id = widget_id;
            }
            otro => return Err(format!("Tipo de campo desconocido: {otro}")),
        }
        remata_acroform(doc, page_id, widget_id, Some(campo_id), &props)
    })
}

/// Tooltip y banderas, que son iguales en todos los campos menos en el
/// radio (donde van en el padre, que es el campo de verdad).
fn pon_comunes(widget: &mut Dictionary, props: &PropsCampo, banderas: i64) {
    if let Some(t) = props.tooltip.as_deref().filter(|t| !t.trim().is_empty()) {
        widget.set("TU", crate::documento::cadena_pdf(t));
    }
    if banderas != 0 {
        widget.set("Ff", Object::Integer(banderas));
    }
}

/// Cuelga el widget de la página (en su sitio del orden de tabulación) y
/// mete el campo en el `/AcroForm`, creándolo si no lo había.
fn remata_acroform(
    doc: &mut LoDoc,
    page_id: ObjectId,
    widget_id: ObjectId,
    campo_id: Option<ObjectId>,
    props: &PropsCampo,
) -> Result<(), String> {
    anade_a_annots_en(doc, page_id, widget_id, props.orden_tab.map(|n| n as usize))?;
    // el orden de tabulación de la página es el de /Annots, y hay que
    // pedirlo: sin /Tabs el visor tabula como quiere
    doc.get_object_mut(page_id)
        .and_then(|o| o.as_dict_mut())
        .map_err(|e| e.to_string())?
        .set("Tabs", Object::Name(b"S".to_vec()));

    let catalog_id = doc
        .trailer
        .get(b"Root")
        .and_then(|o| o.as_reference())
        .map_err(|e| e.to_string())?;
    let existente: Option<Dictionary> = {
        let catalog = doc
            .get_object(catalog_id)
            .and_then(|o| o.as_dict())
            .map_err(|e| e.to_string())?;
        match catalog.get(b"AcroForm") {
            Ok(Object::Dictionary(d)) => Some(d.clone()),
            Ok(Object::Reference(rid)) => doc
                .get_object(*rid)
                .ok()
                .and_then(|o| o.as_dict().ok())
                .cloned(),
            _ => None,
        }
    };
    let mut form = existente.unwrap_or_default();
    if let Some(campo_id) = campo_id {
        match form.get_mut(b"Fields") {
            Ok(Object::Array(arr)) => arr.push(Object::Reference(campo_id)),
            _ => form.set("Fields", Object::Array(vec![Object::Reference(campo_id)])),
        }
    }
    form.set("DA", Object::string_literal("/Helv 0 Tf 0 g"));
    form.set("NeedAppearances", Object::Boolean(true));
    let mut helv = Dictionary::new();
    helv.set("Type", Object::Name(b"Font".to_vec()));
    helv.set("Subtype", Object::Name(b"Type1".to_vec()));
    helv.set("BaseFont", Object::Name(b"Helvetica".to_vec()));
    helv.set("Encoding", Object::Name(b"WinAnsiEncoding".to_vec()));
    let mut fuentes = Dictionary::new();
    fuentes.set("Helv", Object::Dictionary(helv));
    let mut dr = Dictionary::new();
    dr.set("Font", Object::Dictionary(fuentes));
    form.set("DR", Object::Dictionary(dr));
    doc.get_object_mut(catalog_id)
        .and_then(|o| o.as_dict_mut())
        .map_err(|e| e.to_string())?
        .set("AcroForm", Object::Dictionary(form));
    Ok(())
}

/// Enciende `NeedAppearances` en el AcroForm (creándolo si no lo hay): le
/// dice al visor que vuelva a dibujar los campos porque su valor ha
/// cambiado. Es lo que ya hacen los campos que crea Vitela.
pub(crate) fn pide_apariencias(doc: &mut LoDoc) -> Result<(), String> {
    let catalog_id = doc
        .trailer
        .get(b"Root")
        .and_then(|o| o.as_reference())
        .map_err(|e| e.to_string())?;
    let form_ref = doc
        .get_object(catalog_id)
        .and_then(|o| o.as_dict())
        .map_err(|e| e.to_string())?
        .get(b"AcroForm")
        .cloned();
    match form_ref {
        Ok(Object::Reference(rid)) => {
            doc.get_object_mut(rid)
                .and_then(|o| o.as_dict_mut())
                .map_err(|e| e.to_string())?
                .set("NeedAppearances", Object::Boolean(true));
        }
        Ok(Object::Dictionary(mut d)) => {
            d.set("NeedAppearances", Object::Boolean(true));
            doc.get_object_mut(catalog_id)
                .and_then(|o| o.as_dict_mut())
                .map_err(|e| e.to_string())?
                .set("AcroForm", Object::Dictionary(d));
        }
        _ => return Err("El documento no tiene formulario".into()),
    }
    Ok(())
}

/// Solo se escriben enlaces web y de correo; sin esquema se asume https.
fn normaliza_uri(u: &str) -> Result<String, String> {
    let u = u.trim();
    let con_esquema = if u.contains(':') { u.to_string() } else { format!("https://{u}") };
    let esquema = con_esquema.split(':').next().unwrap_or("").to_ascii_lowercase();
    if matches!(esquema.as_str(), "http" | "https" | "mailto") {
        Ok(con_esquema)
    } else {
        Err(format!("Solo se admiten enlaces http, https o mailto (no «{esquema}»)"))
    }
}

/// Crea un enlace en la página: a una URL externa o a otra página.
#[tauri::command(async)]
pub fn create_link(
    work_path: String,
    page_index: u16,
    rect: Rect,
    uri: Option<String>,
    dest_page: Option<u16>,
) -> Result<(), String> {
    let uri = uri.filter(|u| !u.trim().is_empty()).map(|u| normaliza_uri(&u)).transpose()?;
    if uri.is_some() == dest_page.is_some() {
        return Err("Indica o una URL o una página de destino (solo una)".into());
    }
    cirugia(&work_path, move |doc| {
        let paginas = doc.get_pages();
        let page_id = *paginas
            .get(&(page_index as u32 + 1))
            .ok_or("Página fuera de rango")?;
        let geo = geo_pagina(doc, page_id)?;
        let mut link = Dictionary::new();
        link.set("Type", Object::Name(b"Annot".to_vec()));
        link.set("Subtype", Object::Name(b"Link".to_vec()));
        link.set("Rect", rect_pdf(&rect, &geo));
        link.set(
            "Border",
            Object::Array(vec![0.into(), 0.into(), 0.into()]),
        );
        if let Some(u) = uri {
            let mut a = Dictionary::new();
            a.set("S", Object::Name(b"URI".to_vec()));
            a.set(
                "URI",
                Object::String(u.trim().as_bytes().to_vec(), StringFormat::Literal),
            );
            link.set("A", Object::Dictionary(a));
        } else if let Some(p) = dest_page {
            let destino = *paginas
                .get(&(p as u32 + 1))
                .ok_or("Página de destino fuera de rango")?;
            link.set(
                "Dest",
                Object::Array(vec![
                    Object::Reference(destino),
                    Object::Name(b"XYZ".to_vec()),
                    Object::Null,
                    Object::Null,
                    Object::Null,
                ]),
            );
        }
        let link_id = doc.add_object(link);
        anade_a_annots(doc, page_id, link_id)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::crea_pdf;

    /// **H2.** Un grupo de tres radios es **un solo campo** con tres
    /// `/Kids`, no tres campos que se marcan a la vez. Ese es el defecto de
    /// Acrobat que aquí no se copia: allí, crear tres radios sin entender
    /// los grupos produce tres campos independientes.
    #[test]
    fn tres_radios_de_un_grupo_son_un_campo_con_tres_kids() {
        let pdf = std::env::temp_dir().join("formularios2-radios.pdf");
        crea_pdf(&["Encuesta"], &pdf);
        let work = pdf.to_string_lossy().to_string();
        for (i, opcion) in ["mujer", "hombre", "otro"].iter().enumerate() {
            create_form_field(
                work.clone(),
                0,
                "radio".into(),
                Rect { x: 60.0, y: 200.0 + i as f32 * 30.0, w: 18.0, h: 18.0 },
                format!("sexo-{opcion}"),
                Some("sexo".into()),
                Some((*opcion).into()),
                None,
                None,
            )
            .expect("crear radio");
        }

        // en los bytes: UNA entrada en /Fields, con tres /Kids
        let doc = lopdf::Document::load(&work).expect("releer");
        let root = doc.trailer.get(b"Root").and_then(|o| o.as_reference()).unwrap();
        let form = match doc
            .get_object(root)
            .and_then(|o| o.as_dict())
            .unwrap()
            .get(b"AcroForm")
            .unwrap()
        {
            Object::Dictionary(d) => d.clone(),
            Object::Reference(rid) => doc.get_object(*rid).unwrap().as_dict().unwrap().clone(),
            _ => panic!("sin AcroForm"),
        };
        let campos = form.get(b"Fields").unwrap().as_array().unwrap();
        assert_eq!(campos.len(), 1, "un solo campo para las tres opciones");
        let campo_id = campos[0].as_reference().unwrap();
        let campo = doc.get_object(campo_id).unwrap().as_dict().unwrap();
        assert_eq!(campo.get(b"FT").unwrap().as_name().unwrap(), b"Btn");
        assert!(
            campo.get(b"Ff").unwrap().as_i64().unwrap() & RADIO != 0,
            "el bit 16 es lo que lo hace un radio y no una casilla"
        );
        let kids = campo.get(b"Kids").unwrap().as_array().unwrap();
        assert_eq!(kids.len(), 3, "una opción por hijo");
        assert_eq!(campo.get(b"V").unwrap().as_name().unwrap(), b"Off");

        // marcar el segundo
        let campos_ui = crate::formularios::get_form_fields(work.clone(), 0).expect("listar");
        assert_eq!(campos_ui.len(), 3, "tres widgets en la página");
        assert_eq!(campos_ui[1].kind, "RadioButton", "{:?}", campos_ui[1].kind);
        crate::formularios::set_form_checked(work.clone(), 0, 1, true).expect("marcar");

        let doc = lopdf::Document::load(&work).expect("releer");
        let campo = doc.get_object(campo_id).unwrap().as_dict().unwrap();
        assert_eq!(
            campo.get(b"V").unwrap().as_name().unwrap(),
            b"hombre",
            "el /V del campo es el valor de exportación del elegido"
        );
        let kids: Vec<_> = campo
            .get(b"Kids")
            .unwrap()
            .as_array()
            .unwrap()
            .iter()
            .map(|o| o.as_reference().unwrap())
            .collect();
        let estado = |id: lopdf::ObjectId| {
            String::from_utf8_lossy(
                doc.get_object(id).unwrap().as_dict().unwrap().get(b"AS").unwrap().as_name().unwrap(),
            )
            .into_owned()
        };
        assert_eq!(estado(kids[0]), "Off", "los hermanos se apagan solos");
        assert_eq!(estado(kids[1]), "hombre");
        assert_eq!(estado(kids[2]), "Off");
        // y PDFium lo lee igual que cualquier visor
        let campos_ui = crate::formularios::get_form_fields(work.clone(), 0).expect("listar");
        assert!(!campos_ui[0].checked && campos_ui[1].checked && !campos_ui[2].checked);

        // elegir otro apaga el anterior sin tocar nada más
        crate::formularios::set_form_checked(work.clone(), 0, 2, true).expect("marcar el tercero");
        let campos_ui = crate::formularios::get_form_fields(work.clone(), 0).expect("listar");
        assert!(!campos_ui[1].checked && campos_ui[2].checked);

        // ⌘Z devuelve la elección anterior
        crate::historial::undo(work.clone()).expect("deshacer");
        let campos_ui = crate::formularios::get_form_fields(work.clone(), 0).expect("listar");
        assert!(campos_ui[1].checked && !campos_ui[2].checked);

        crate::render_page_png(work.clone(), 0, 200, true).expect("render con radios");
        std::fs::remove_file(&pdf).ok();
    }

    /// **H2.** Un desplegable se crea con sus opciones y se lee con ellas;
    /// una lista, igual pero sin el bit de desplegable.
    #[test]
    fn un_desplegable_se_crea_con_sus_opciones_y_se_lee_con_ellas() {
        let pdf = std::env::temp_dir().join("formularios2-desplegable.pdf");
        crea_pdf(&["Pedido"], &pdf);
        let work = pdf.to_string_lossy().to_string();
        let opciones = vec!["España".to_string(), "Portugal".to_string(), "Francia".to_string()];
        create_form_field(
            work.clone(),
            0,
            "combo".into(),
            Rect { x: 60.0, y: 200.0, w: 160.0, h: 24.0 },
            "pais".into(),
            None,
            None,
            Some(opciones.clone()),
            Some(PropsCampo {
                valor_defecto: Some("Portugal".into()),
                ..Default::default()
            }),
        )
        .expect("crear desplegable");
        create_form_field(
            work.clone(),
            0,
            "list".into(),
            Rect { x: 60.0, y: 260.0, w: 160.0, h: 60.0 },
            "provincias".into(),
            None,
            None,
            Some(vec!["Álava".into(), "Burgos".into()]),
            None,
        )
        .expect("crear lista");

        let campos = crate::formularios::get_form_fields(work.clone(), 0).expect("listar");
        let combo = campos.iter().find(|c| c.name == "pais").expect("el desplegable");
        assert_eq!(combo.kind, "ComboBox", "{}", combo.kind);
        assert_eq!(combo.options, opciones, "las opciones se leen enteras");
        assert_eq!(combo.value, "Portugal", "el valor por defecto sale puesto");
        let lista = campos.iter().find(|c| c.name == "provincias").expect("la lista");
        assert_eq!(lista.kind, "ListBox", "{}", lista.kind);
        assert_eq!(lista.options.len(), 2);

        // y se puede elegir otra opción con el comando de siempre
        crate::formularios::set_form_choice(work.clone(), 0, combo.annot_index, "Francia".into())
            .expect("elegir");
        let campos = crate::formularios::get_form_fields(work.clone(), 0).expect("listar");
        assert_eq!(campos.iter().find(|c| c.name == "pais").unwrap().value, "Francia");

        // un desplegable sin opciones se dice, no se crea vacío
        assert!(create_form_field(
            work.clone(),
            0,
            "combo".into(),
            Rect { x: 60.0, y: 400.0, w: 100.0, h: 24.0 },
            "vacio".into(),
            None,
            None,
            None,
            None,
        )
        .is_err());
        std::fs::remove_file(&pdf).ok();
    }

    /// **H2.** Las propiedades del primer panel de Acrobat: tooltip,
    /// obligatorio, solo lectura, valor por defecto y orden de tabulación.
    #[test]
    fn un_campo_obligatorio_lleva_su_bit_y_su_tooltip() {
        let pdf = std::env::temp_dir().join("formularios2-props.pdf");
        crea_pdf(&["Solicitud"], &pdf);
        let work = pdf.to_string_lossy().to_string();
        create_form_field(
            work.clone(),
            0,
            "text".into(),
            Rect { x: 60.0, y: 200.0, w: 200.0, h: 24.0 },
            "nombre".into(),
            None,
            None,
            None,
            Some(PropsCampo {
                tooltip: Some("Nombre y dos apellidos".into()),
                obligatorio: true,
                solo_lectura: false,
                valor_defecto: Some("Ada".into()),
                orden_tab: None,
            }),
        )
        .expect("crear campo obligatorio");
        // y otro que se cuela DELANTE en el orden de tabulación
        create_form_field(
            work.clone(),
            0,
            "text".into(),
            Rect { x: 60.0, y: 260.0, w: 200.0, h: 24.0 },
            "tratamiento".into(),
            None,
            None,
            None,
            Some(PropsCampo { orden_tab: Some(0), ..Default::default() }),
        )
        .expect("crear el segundo");

        let campos = crate::formularios::get_form_fields(work.clone(), 0).expect("listar");
        assert_eq!(
            campos[0].name, "tratamiento",
            "el orden de tabulación es el de /Annots: {campos:?}"
        );
        let obligatorio = campos.iter().find(|c| c.name == "nombre").expect("el campo");
        assert!(obligatorio.required, "el bit 2 de /Ff llega hasta la UI");
        assert!(!campos[0].required, "y el otro no es obligatorio");
        assert_eq!(obligatorio.value, "Ada", "el valor por defecto sale puesto");

        let doc = lopdf::Document::load(&work).expect("releer");
        let campo = doc
            .objects
            .values()
            .filter_map(|o| o.as_dict().ok())
            .find(|d| {
                d.get(b"T")
                    .map(crate::anotaciones::texto_de_cadena_pdf)
                    .unwrap_or_default()
                    == "nombre"
            })
            .expect("el campo en los bytes");
        assert_eq!(campo.get(b"Ff").unwrap().as_i64().unwrap() & OBLIGATORIO, OBLIGATORIO);
        assert_eq!(
            crate::anotaciones::texto_de_cadena_pdf(campo.get(b"TU").unwrap()),
            "Nombre y dos apellidos"
        );
        assert_eq!(
            crate::anotaciones::texto_de_cadena_pdf(campo.get(b"DV").unwrap()),
            "Ada"
        );
        // la página pide tabular por el orden de /Annots
        let page_id = *doc.get_pages().get(&1).unwrap();
        assert_eq!(
            doc.get_object(page_id).unwrap().as_dict().unwrap().get(b"Tabs").unwrap().as_name().unwrap(),
            b"S"
        );
        std::fs::remove_file(&pdf).ok();
    }

    /// En una página rotada, el campo y el enlace se escriben en el espacio
    /// PROPIO de la página (sin rotar, que es la UI quien convierte) y se
    /// leen en el de la página vista, que es donde la UI los pinta.
    #[test]
    fn campos_y_enlaces_en_una_pagina_rotada() {
        let pdf = std::env::temp_dir().join("formularios2-rotada-test.pdf");
        crea_pdf(&["Solicitud"], &pdf);
        let work = pdf.to_string_lossy().to_string();
        crate::paginas::rotate_page(work.clone(), 0).expect("girar 90°");

        // rect en el espacio propio de la página (A4 sin rotar, 595×842)
        let area = Rect { x: 100.0, y: 400.0, w: 150.0, h: 30.0 };
        create_form_field(work.clone(), 0, "text".into(), area.clone(), "nombre".into(), None, None, None, None)
            .expect("crear campo");
        create_link(
            work.clone(),
            0,
            area.clone(),
            Some("https://ejemplo.org".into()),
            None,
        )
        .expect("crear enlace");

        // leídos en la página vista (842×595): (100,400) propia → (412,100)
        let esperado = |x: f32, y: f32, w: f32, h: f32, que: &str| {
            assert!(
                (x - 412.0).abs() < 1.0
                    && (y - 100.0).abs() < 1.0
                    && (w - 30.0).abs() < 1.0
                    && (h - 150.0).abs() < 1.0,
                "{que} se lee en ({x},{y}) {w}×{h}"
            );
        };
        let campo = &crate::formularios::get_form_fields(work.clone(), 0).expect("listar")[0];
        esperado(campo.x, campo.y, campo.w, campo.h, "el campo");
        let enlace = &crate::documento::get_links(work.clone(), 0).expect("enlaces")[0];
        esperado(enlace.x, enlace.y, enlace.w, enlace.h, "el enlace");

        // y en el fichero, el /Rect es el de la página sin rotar
        let doc = LoDoc::load(&work).expect("cargar");
        let page_id = *doc.get_pages().get(&1).expect("página 1");
        let annots = doc
            .get_object(page_id)
            .and_then(|o| o.as_dict())
            .and_then(|d| d.get(b"Annots"))
            .and_then(|o| o.as_array())
            .expect("Annots")
            .clone();
        let rid = annots[0].as_reference().expect("referencia");
        let r: Vec<f32> = doc
            .get_object(rid)
            .and_then(|o| o.as_dict())
            .and_then(|d| d.get(b"Rect"))
            .and_then(|o| o.as_array())
            .expect("Rect")
            .iter()
            .map(|o| match o {
                Object::Integer(i) => *i as f32,
                Object::Real(v) => *v,
                _ => 0.0,
            })
            .collect();
        assert!(
            (r[0] - 100.0).abs() < 1.0
                && (r[1] - 412.0).abs() < 1.0
                && (r[2] - 250.0).abs() < 1.0
                && (r[3] - 442.0).abs() < 1.0,
            "/Rect del campo: {r:?}"
        );
        std::fs::remove_file(&pdf).ok();
    }

    #[test]
    fn campo_de_texto_visible_y_rellenable_por_pdfium() {
        let pdf = std::env::temp_dir().join("formularios2-texto-test.pdf");
        crea_pdf(&["Solicitud"], &pdf);
        let work = pdf.to_string_lossy().to_string();
        create_form_field(
            work.clone(),
            0,
            "text".into(),
            Rect {
                x: 100.0,
                y: 200.0,
                w: 180.0,
                h: 24.0,
            },
            "nombre".into(),
            None,
            None,
            None,
            None,
        )
        .expect("crear campo");
        let campos = crate::formularios::get_form_fields(work.clone(), 0).expect("listar");
        assert_eq!(campos.len(), 1);
        assert_eq!(campos[0].name, "nombre");
        assert_eq!(campos[0].kind, "Text");
        assert!((campos[0].x - 100.0).abs() < 1.0);
        crate::formularios::set_form_text(work.clone(), 0, campos[0].annot_index, "Jorge".into())
            .expect("rellenar");
        let campos = crate::formularios::get_form_fields(work, 0).expect("relistar");
        assert_eq!(campos[0].value, "Jorge");
    }

    #[test]
    fn casilla_marcable_y_nombres_unicos() {
        let pdf = std::env::temp_dir().join("formularios2-casilla-test.pdf");
        crea_pdf(&["Consentimiento"], &pdf);
        let work = pdf.to_string_lossy().to_string();
        let r = Rect {
            x: 80.0,
            y: 300.0,
            w: 16.0,
            h: 16.0,
        };
        create_form_field(work.clone(), 0, "checkbox".into(), r.clone(), "acepto".into(), None, None, None, None)
            .expect("crear casilla");
        // mismo nombre otra vez: debe renombrarse a acepto-2
        create_form_field(
            work.clone(),
            0,
            "checkbox".into(),
            Rect { y: 330.0, ..r },
            "acepto".into(),
            None,
            None,
            None,
            None,
        )
        .expect("segunda casilla");
        let campos = crate::formularios::get_form_fields(work.clone(), 0).expect("listar");
        assert_eq!(campos.len(), 2);
        let nombres: Vec<&str> = campos.iter().map(|c| c.name.as_str()).collect();
        assert!(nombres.contains(&"acepto") && nombres.contains(&"acepto-2"), "{nombres:?}");
        let idx = campos.iter().find(|c| c.name == "acepto").unwrap().annot_index;
        crate::formularios::set_form_checked(work.clone(), 0, idx, true).expect("marcar");
        let campos = crate::formularios::get_form_fields(work, 0).expect("relistar");
        assert!(campos.iter().find(|c| c.name == "acepto").unwrap().checked);
    }

    #[test]
    fn campo_creado_y_borrado() {
        let pdf = std::env::temp_dir().join("formularios2-borrar-test.pdf");
        crea_pdf(&["Baja"], &pdf);
        let work = pdf.to_string_lossy().to_string();
        create_form_field(
            work.clone(),
            0,
            "text".into(),
            Rect { x: 60.0, y: 200.0, w: 140.0, h: 22.0 },
            "efimero".into(),
            None,
            None,
            None,
            None,
        )
        .expect("crear");
        assert_eq!(crate::formularios::get_form_fields(work.clone(), 0).unwrap().len(), 1);
        delete_form_field(work.clone(), "efimero".into()).expect("borrar");
        assert_eq!(crate::formularios::get_form_fields(work.clone(), 0).unwrap().len(), 0);
        assert!(delete_form_field(work, "no-existe".into()).is_err());
    }

    #[test]
    fn enlaces_uri_y_pagina_y_borrado() {
        let pdf = std::env::temp_dir().join("formularios2-enlaces-test.pdf");
        crea_pdf(&["Uno", "Dos"], &pdf);
        let work = pdf.to_string_lossy().to_string();
        let r = Rect {
            x: 50.0,
            y: 100.0,
            w: 120.0,
            h: 18.0,
        };
        create_link(
            work.clone(),
            0,
            r.clone(),
            Some("https://ejemplo.es".into()),
            None,
        )
        .expect("enlace uri");
        create_link(work.clone(), 0, Rect { y: 130.0, ..r }, None, Some(1))
            .expect("enlace página");
        let links = crate::documento::get_links(work.clone(), 0).expect("listar");
        assert_eq!(links.len(), 2);
        assert!(links.iter().any(|l| l.uri.as_deref() == Some("https://ejemplo.es")));
        assert!(links.iter().any(|l| l.dest_page == Some(1)));
        // exactamente uno de los dos parámetros
        assert!(create_link(work.clone(), 0, r.clone(), None, None).is_err());
        // esquemas peligrosos fuera; sin esquema se asume https
        assert!(create_link(work.clone(), 0, r.clone(), Some("file:///etc/passwd".into()), None)
            .is_err());
        assert!(create_link(work.clone(), 0, r.clone(), Some("javascript:alert(1)".into()), None)
            .is_err());
        create_link(work.clone(), 0, r.clone(), Some("ejemplo.org/x".into()), None)
            .expect("sin esquema");
        let links = crate::documento::get_links(work.clone(), 0).expect("listar");
        assert!(links.iter().any(|l| l.uri.as_deref() == Some("https://ejemplo.org/x")));
        // el annot_index de get_links es el mismo que da get_annotations
        let annots = crate::anotaciones::get_annotations(work.clone(), 0).expect("annots");
        let de_annots: Vec<u16> = annots
            .iter()
            .filter(|a| a.kind == "Link")
            .map(|a| a.index)
            .collect();
        let de_links: Vec<u16> = links.iter().map(|l| l.annot_index).collect();
        assert_eq!(de_annots, de_links);
        // borrar el primero con ese índice (es lo que hace la UI)
        crate::anotaciones::remove_annotation(work.clone(), 0, links[0].annot_index)
            .expect("borrar");
        let quedan = crate::documento::get_links(work, 0).expect("relistar");
        assert_eq!(quedan.len(), 2);
        assert!(quedan.iter().all(|l| l.uri != links[0].uri));
    }
}

/// Borra un campo de formulario por nombre: quita el widget de los Annots de
/// su página y la referencia de /Fields del AcroForm.
#[tauri::command(async)]
pub fn delete_form_field(work_path: String, name: String) -> Result<(), String> {
    cirugia(&work_path, move |doc| {
        // localizar el widget por su T
        let widget_id = doc
            .objects
            .iter()
            .find(|(_, o)| {
                o.as_dict()
                    .map(|d| {
                        matches!(d.get(b"Subtype").and_then(|s| s.as_name()), Ok(b"Widget"))
                            && matches!(
                                d.get(b"T"),
                                Ok(Object::String(t, _)) if String::from_utf8_lossy(t) == name
                            )
                    })
                    .unwrap_or(false)
            })
            .map(|(id, _)| *id)
            .ok_or_else(|| format!("No existe el campo «{name}»"))?;
        // quitarlo de los Annots de todas las páginas (directo o referencia)
        let paginas: Vec<ObjectId> = doc.get_pages().values().copied().collect();
        for page_id in paginas {
            let annots_ref = {
                let Ok(page) = doc.get_object(page_id).and_then(|o| o.as_dict()) else {
                    continue;
                };
                match page.get(b"Annots") {
                    Ok(Object::Reference(rid)) => Some(*rid),
                    _ => None,
                }
            };
            if let Some(rid) = annots_ref {
                if let Ok(arr) = doc.get_object_mut(rid).and_then(|o| o.as_array_mut()) {
                    arr.retain(|o| o.as_reference().ok() != Some(widget_id));
                }
            } else if let Ok(page) = doc.get_object_mut(page_id).and_then(|o| o.as_dict_mut()) {
                if let Ok(Object::Array(arr)) = page.get_mut(b"Annots") {
                    arr.retain(|o| o.as_reference().ok() != Some(widget_id));
                }
            }
        }
        // y de /Fields del AcroForm
        let catalog_id = doc
            .trailer
            .get(b"Root")
            .and_then(|o| o.as_reference())
            .map_err(|e| e.to_string())?;
        let form_es_ref = {
            let catalog = doc
                .get_object(catalog_id)
                .and_then(|o| o.as_dict())
                .map_err(|e| e.to_string())?;
            match catalog.get(b"AcroForm") {
                Ok(Object::Reference(rid)) => Some(*rid),
                _ => None,
            }
        };
        let quita = |form: &mut Dictionary| {
            if let Ok(Object::Array(arr)) = form.get_mut(b"Fields") {
                arr.retain(|o| o.as_reference().ok() != Some(widget_id));
            }
        };
        if let Some(rid) = form_es_ref {
            if let Ok(form) = doc.get_object_mut(rid).and_then(|o| o.as_dict_mut()) {
                quita(form);
            }
        } else if let Ok(catalog) = doc.get_object_mut(catalog_id).and_then(|o| o.as_dict_mut()) {
            if let Ok(Object::Dictionary(form)) = catalog.get_mut(b"AcroForm") {
                quita(form);
            }
        }
        Ok(())
    })
}
