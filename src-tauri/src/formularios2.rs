//! Creación de campos de formulario y enlaces (PDFium no los crea: cirugía
//! con lopdf, mismo patrón que el campo de firma de firma.rs). El borrado de
//! campos no se ofrece en v1 (dejaría huérfanos en /Fields); los enlaces son
//! anotaciones normales y se borran con remove_annotation.

use crate::{cirugia, on_pdfium_thread, Rect};
use lopdf::{Dictionary, Document as LoDoc, Object, ObjectId, Stream, StringFormat};
use pdfium_render::prelude::*;
use serde::Serialize;

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
pub(crate) fn anade_a_annots(
    doc: &mut LoDoc,
    page_id: ObjectId,
    annot_id: ObjectId,
) -> Result<(), String> {
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

/// La `/MediaBox` de la página, en coordenadas del papel, para lo que
/// necesita la caja y no el cambio de espacio (el fondo, que se pinta a
/// sangre sobre ella).
pub(crate) fn caja_de_pagina(doc: &LoDoc, page_id: ObjectId) -> Result<[f32; 4], String> {
    let mb = media_box(doc, page_id)?;
    Ok([
        mb[0].min(mb[2]),
        mb[1].min(mb[3]),
        mb[0].max(mb[2]),
        mb[1].max(mb[3]),
    ])
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
        cx + r,
        cy,
        cx + r,
        cy + k,
        cx + k,
        cy + r,
        cx,
        cy + r,
        cx - k,
        cy + r,
        cx - r,
        cy + k,
        cx - r,
        cy,
        cx - r,
        cy - k,
        cx - k,
        cy - r,
        cx,
        cy - r,
        cx + k,
        cy - r,
        cx + r,
        cy - k,
        cx + r,
        cy,
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
/// El mismo bit, para quien tenga que reconocer un campo bloqueado desde
/// fuera del módulo (los tres comandos que rellenan).
pub(crate) const SOLO_LECTURA_FF: i64 = SOLO_LECTURA;
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
    let campo = CampoNuevo {
        page_index,
        kind,
        rect,
        name,
        group,
        export_value,
        options,
        props,
    };
    cirugia(&work_path, move |doc| crea_campo(doc, campo))
}

/// Un campo por crear: lo que [`create_form_field`] recibe suelto y lo que
/// [`create_form_fields`] recibe en lista.
#[derive(serde::Deserialize, Clone, Debug)]
pub struct CampoNuevo {
    pub page_index: u16,
    pub kind: String,
    pub rect: Rect,
    pub name: String,
    #[serde(default)]
    pub group: Option<String>,
    #[serde(default)]
    pub export_value: Option<String>,
    #[serde(default)]
    pub options: Option<Vec<String>>,
    #[serde(default)]
    pub props: Option<PropsCampo>,
}

/// Crea **un lote** de campos en una sola cirugía, para que un ⌘Z devuelva
/// el formulario entero. Es lo que necesita «Reconocer campos…»: aceptar
/// ocho propuestas es un gesto, no ocho.
///
/// Si uno falla no se escribe ninguno: la cirugía guarda al final y una
/// mutación fallida retira su paso de deshacer. Devuelve cuántos ha creado.
#[tauri::command(async)]
pub fn create_form_fields(work_path: String, fields: Vec<CampoNuevo>) -> Result<u16, String> {
    if fields.is_empty() {
        return Err("No hay ningún campo que crear".into());
    }
    let cuantos = fields.len() as u16;
    cirugia(&work_path, move |doc| {
        for campo in fields {
            crea_campo(doc, campo)?;
        }
        Ok(())
    })?;
    Ok(cuantos)
}

/// El cuerpo de [`create_form_field`], sobre un documento ya abierto por
/// lopdf: así el lote entero cabe en una sola cirugía.
fn crea_campo(doc: &mut LoDoc, campo: CampoNuevo) -> Result<(), String> {
    let CampoNuevo {
        page_index,
        kind,
        rect,
        name,
        group,
        export_value,
        options,
        props,
    } = campo;
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
    {
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
                    Object::Name(if elegida {
                        export.as_bytes().to_vec()
                    } else {
                        b"Off".to_vec()
                    }),
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
                        let v = if elegida {
                            export.as_bytes().to_vec()
                        } else {
                            b"Off".to_vec()
                        };
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
    }
}

/// Claves de campo que se heredan por la cadena de `/Parent` (spec
/// 12.7.3.2), más el `/TU` que Vitela escribe en el padre de un grupo de
/// radios: son las que hay que bajar al widget para que el campo quepa
/// entero en un solo diccionario.
const HEREDABLES: [&[u8]; 9] = [
    b"FT", b"Ff", b"V", b"DV", b"DA", b"Q", b"MaxLen", b"Opt", b"TU",
];

/// Nombre completo de un campo: los `/T` de la cadena unidos con puntos,
/// de la raíz al widget, que es como lo escribe el spec y como lo leen los
/// demás programas.
fn nombre_completo(doc: &LoDoc, cadena: &[ObjectId], propio: Option<&Object>) -> String {
    let mut partes: Vec<String> = cadena
        .iter()
        .rev()
        .filter_map(|id| {
            let d = doc.get_object(*id).ok()?.as_dict().ok()?;
            let t = crate::anotaciones::texto_de_cadena_pdf(d.get(b"T").ok()?);
            (!t.is_empty()).then_some(t)
        })
        .collect();
    if let Some(t) = propio.map(crate::anotaciones::texto_de_cadena_pdf) {
        if !t.is_empty() {
            partes.push(t);
        }
    }
    partes.join(".")
}

/// La cadena de padres de un objeto, del más cercano al más lejano.
/// **Corta los ciclos**: un `/Parent` que vuelve sobre sus pasos es
/// exactamente lo que hay que sobrevivir aquí.
fn cadena_de_padres(doc: &LoDoc, id: ObjectId) -> Vec<ObjectId> {
    let mut out = Vec::new();
    let mut visto = std::collections::HashSet::new();
    visto.insert(id);
    let mut actual = id;
    while let Ok(d) = doc.get_object(actual).and_then(|o| o.as_dict()) {
        match d.get(b"Parent") {
            Ok(Object::Reference(p)) if visto.insert(*p) => {
                out.push(*p);
                actual = *p;
            }
            _ => break,
        }
    }
    out
}

/// **AC-096.** Aplana el árbol `/AcroForm` de un documento que va a servir
/// de **origen** de una importación: baja a cada `/Widget` las claves que
/// heredaba de su campo padre, le pone el nombre completo y le quita el
/// `/Parent`.
///
/// El motivo es el mismo que el de las ventanas de las notas (AC-046): un
/// grupo de botones de radio es **un campo con `/Kids`** cuyos hijos
/// apuntan al padre, y `FPDF_ImportPages` recorre el grafo de la anotación
/// recursivamente hasta comerse la pila — el proceso entero se iba con un
/// SIGSEGV. Sin `/Parent` no hay ciclo, y el campo viaja igual porque
/// ahora cabe en el propio widget; [`repon_acroform`] lo vuelve a montar
/// en el destino, como `repon_popups` repone las ventanas.
///
/// Devuelve si ha tocado algo (si no, no hace falta copiar el fichero).
pub(crate) fn aplana_campos(doc: &mut LoDoc) -> bool {
    let paginas: Vec<u32> = doc.get_pages().keys().copied().collect();
    let mut deberes: Vec<(ObjectId, Dictionary)> = Vec::new();
    for numero in paginas {
        let Some(lista) = crate::anotaciones::lista_annots(doc, (numero - 1) as u16) else {
            continue;
        };
        for entrada in lista {
            let Object::Reference(rid) = entrada else {
                continue;
            };
            let Ok(d) = doc.get_object(rid).and_then(|o| o.as_dict()) else {
                continue;
            };
            if d.get(b"Subtype").and_then(|o| o.as_name()).ok() != Some(b"Widget") {
                continue;
            }
            let cadena = cadena_de_padres(doc, rid);
            if cadena.is_empty() {
                continue;
            }
            let mut baja = Dictionary::new();
            let nombre = nombre_completo(doc, &cadena, d.get(b"T").ok());
            if !nombre.is_empty() {
                baja.set("T", Object::string_literal(nombre));
            }
            for clave in HEREDABLES {
                if d.has(clave) {
                    continue;
                }
                let heredado = cadena.iter().find_map(|id| {
                    doc.get_object(*id)
                        .ok()?
                        .as_dict()
                        .ok()?
                        .get(clave)
                        .ok()
                        .cloned()
                });
                if let Some(v) = heredado {
                    baja.set(clave.to_vec(), v);
                }
            }
            deberes.push((rid, baja));
        }
    }
    if deberes.is_empty() {
        return false;
    }
    for (rid, baja) in deberes {
        if let Ok(d) = doc.get_object_mut(rid).and_then(|o| o.as_dict_mut()) {
            for (clave, valor) in baja.iter() {
                d.set(clave.to_vec(), valor.clone());
            }
            d.remove(b"Parent");
        }
    }
    // el árbol de campos se queda sin nadie que apunte a él: quitarlo del
    // catálogo evita que ningún otro recorrido vuelva a pisar el ciclo
    if let Ok(catalog_id) = doc.trailer.get(b"Root").and_then(|o| o.as_reference()) {
        if let Ok(c) = doc.get_object_mut(catalog_id).and_then(|o| o.as_dict_mut()) {
            c.remove(b"AcroForm");
        }
    }
    true
}

/// Los campos (y sus descendientes) que ya cuelgan del `/AcroForm`.
fn campos_colgados(doc: &LoDoc) -> std::collections::HashSet<ObjectId> {
    let mut vistos = std::collections::HashSet::new();
    let mut pila: Vec<ObjectId> = acroform_de(doc)
        .and_then(|(_, f)| f.get(b"Fields").ok().cloned())
        .map(|o| referencias_de(doc, &o))
        .unwrap_or_default();
    while let Some(id) = pila.pop() {
        if !vistos.insert(id) {
            continue;
        }
        if let Ok(d) = doc.get_object(id).and_then(|o| o.as_dict()) {
            if let Ok(kids) = d.get(b"Kids") {
                pila.extend(referencias_de(doc, kids));
            }
        }
    }
    vistos
}

/// Las referencias de un array, resolviéndolo si el propio array va por
/// referencia.
fn referencias_de(doc: &LoDoc, o: &Object) -> Vec<ObjectId> {
    let arr = match o {
        Object::Array(a) => a.clone(),
        Object::Reference(rid) => match doc.get_object(*rid).and_then(|o| o.as_array()) {
            Ok(a) => a.clone(),
            Err(_) => return Vec::new(),
        },
        _ => return Vec::new(),
    };
    arr.iter().filter_map(|o| o.as_reference().ok()).collect()
}

/// El `/AcroForm` del catálogo: su id (si va por referencia) y su
/// diccionario.
fn acroform_de(doc: &LoDoc) -> Option<(Option<ObjectId>, Dictionary)> {
    let catalog_id = doc
        .trailer
        .get(b"Root")
        .and_then(|o| o.as_reference())
        .ok()?;
    let catalog = doc.get_object(catalog_id).ok()?.as_dict().ok()?;
    match catalog.get(b"AcroForm") {
        Ok(Object::Dictionary(d)) => Some((None, d.clone())),
        Ok(Object::Reference(rid)) => Some((
            Some(*rid),
            doc.get_object(*rid).ok()?.as_dict().ok()?.clone(),
        )),
        _ => None,
    }
}

/// **AC-096 y AC-104.** Vuelve a montar el `/AcroForm` de un documento que
/// acaba de recibir páginas importadas: `FPDF_ImportPages` copia los
/// `/Widget` de la página y **no copia el `/AcroForm` del catálogo**, así
/// que el campo llegaba dibujado pero muerto —no se podía rellenar y
/// `get_form_fields` devolvía la lista vacía—.
///
/// Recorre los widgets de todas las páginas que no cuelgan ya de
/// `/Fields`, los agrupa por nombre (dos widgets con el mismo `/T` son el
/// mismo campo, que es lo que hace un grupo de radios), renombra el que
/// choque con un campo que ya estaba y los mete en `/Fields`, fundiéndolo
/// con el que hubiera. Es idempotente: pasarlo dos veces no duplica nada.
pub(crate) fn repon_acroform(doc: &mut LoDoc) -> Result<(), String> {
    let colgados = campos_colgados(doc);
    let mut usados: Vec<String> = campos_por_nombre(doc).into_iter().map(|(n, _)| n).collect();
    // los huérfanos, en orden de página y de /Annots, agrupados por nombre
    let mut grupos: Vec<(String, Vec<ObjectId>)> = Vec::new();
    let paginas: Vec<u32> = doc.get_pages().keys().copied().collect();
    for numero in paginas {
        let Some(lista) = crate::anotaciones::lista_annots(doc, (numero - 1) as u16) else {
            continue;
        };
        for entrada in lista {
            let Object::Reference(rid) = entrada else {
                continue;
            };
            if colgados.contains(&rid) {
                continue;
            }
            let Ok(d) = doc.get_object(rid).and_then(|o| o.as_dict()) else {
                continue;
            };
            if d.get(b"Subtype").and_then(|o| o.as_name()).ok() != Some(b"Widget") {
                continue;
            }
            if d.has(b"Parent") {
                continue;
            }
            let nombre = d
                .get(b"T")
                .map(crate::anotaciones::texto_de_cadena_pdf)
                .unwrap_or_default();
            if nombre.is_empty() {
                continue;
            }
            match grupos.iter_mut().find(|(n, _)| *n == nombre) {
                Some((_, ids)) => ids.push(rid),
                None => grupos.push((nombre, vec![rid])),
            }
        }
    }
    if grupos.is_empty() {
        return Ok(());
    }
    let mut nuevos: Vec<ObjectId> = Vec::new();
    for (nombre, ids) in grupos {
        // dos campos distintos no pueden llamarse igual: el que llega de
        // fuera se renombra, como hace `crea_campo`
        let mut final_ = nombre.clone();
        let mut n = 2;
        while usados.contains(&final_) {
            final_ = format!("{nombre}-{n}");
            n += 1;
        }
        usados.push(final_.clone());
        if ids.len() == 1 {
            if final_ != nombre {
                doc.get_object_mut(ids[0])
                    .and_then(|o| o.as_dict_mut())
                    .map_err(|e| e.to_string())?
                    .set("T", Object::string_literal(final_));
            }
            nuevos.push(ids[0]);
            continue;
        }
        // varios widgets con el mismo nombre son UN campo con /Kids: es lo
        // que hace que marcar un radio desmarque a sus hermanos
        let mut padre = Dictionary::new();
        padre.set("T", Object::string_literal(final_));
        if let Ok(d) = doc.get_object(ids[0]).and_then(|o| o.as_dict()) {
            for clave in HEREDABLES {
                if let Ok(v) = d.get(clave) {
                    padre.set(clave.to_vec(), v.clone());
                }
            }
        }
        let padre_id = doc.add_object(Object::Dictionary(padre));
        let hijos: Vec<Object> = ids.iter().map(|id| Object::Reference(*id)).collect();
        doc.get_object_mut(padre_id)
            .and_then(|o| o.as_dict_mut())
            .map_err(|e| e.to_string())?
            .set("Kids", Object::Array(hijos));
        for id in &ids {
            if let Ok(d) = doc.get_object_mut(*id).and_then(|o| o.as_dict_mut()) {
                d.set("Parent", Object::Reference(padre_id));
                for clave in [&b"T"[..], b"FT", b"Ff", b"V", b"DV", b"Opt", b"TU"] {
                    d.remove(clave);
                }
            }
        }
        nuevos.push(padre_id);
    }
    // y el /AcroForm: el que hubiera, con los campos nuevos detrás
    let (form_id, mut form) = acroform_de(doc).unwrap_or((None, Dictionary::new()));
    let mut campos: Vec<Object> = match form.get(b"Fields") {
        Ok(o) => referencias_de(doc, o)
            .into_iter()
            .map(Object::Reference)
            .collect(),
        Err(_) => Vec::new(),
    };
    campos.extend(nuevos.into_iter().map(Object::Reference));
    form.set("Fields", Object::Array(campos));
    if !form.has(b"DA") {
        form.set("DA", Object::string_literal("/Helv 0 Tf 0 g"));
    }
    if !form.has(b"DR") {
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
    }
    form.set("NeedAppearances", Object::Boolean(true));
    match form_id {
        Some(rid) => {
            *doc.get_object_mut(rid).map_err(|e| e.to_string())? = Object::Dictionary(form);
        }
        None => {
            let catalog_id = doc
                .trailer
                .get(b"Root")
                .and_then(|o| o.as_reference())
                .map_err(|e| e.to_string())?;
            doc.get_object_mut(catalog_id)
                .and_then(|o| o.as_dict_mut())
                .map_err(|e| e.to_string())?
                .set("AcroForm", Object::Dictionary(form));
        }
    }
    Ok(())
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
    let con_esquema = if u.contains(':') {
        u.to_string()
    } else {
        format!("https://{u}")
    };
    let esquema = con_esquema
        .split(':')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    if matches!(esquema.as_str(), "http" | "https" | "mailto") {
        Ok(con_esquema)
    } else {
        Err(format!(
            "Solo se admiten enlaces http, https o mailto (no «{esquema}»)"
        ))
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
    let uri = uri
        .filter(|u| !u.trim().is_empty())
        .map(|u| normaliza_uri(&u))
        .transpose()?;
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
        link.set("Border", Object::Array(vec![0.into(), 0.into(), 0.into()]));
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

/// Un campo que la detección **propone**. Nada de esto está escrito en el
/// documento: es lo que la UI pinta sobre la página para que el usuario lo
/// repase, lo renombre o lo quite antes de crear nada.
#[derive(Serialize, Debug, Clone)]
pub struct CampoPropuesto {
    pub page_index: u16,
    /// En el espacio **propio** de la página, origen arriba a la izquierda:
    /// exactamente lo que `create_form_field` espera recibir, para que la
    /// UI pueda aceptar una propuesta sin convertir nada.
    pub rect: Rect,
    /// `"text"`, `"checkbox"` o `"radio"`.
    pub kind: String,
    pub name: String,
    /// El `/T` del grupo cuando `kind` es `"radio"`; vacío en los demás.
    /// No estaba en el contrato del analista, pero un radio sin grupo no se
    /// puede crear: `create_form_field` lo exige.
    pub group: String,
    /// De 0 a 1. Una heurística no acierta siempre y no puede fingir que
    /// sí: por debajo de 0,6 la UI avisa de que hay que repasarlo.
    pub confianza: f32,
}

/// Reconoce los campos de un formulario **impreso** y los propone: líneas
/// de subrayado, casillas, grupos de radios y cajas rectangulares, con el
/// nombre sacado del texto de al lado.
///
/// **No escribe nada.** Es la diferencia con Acrobat, donde «Preparar
/// formulario» crea los campos sin preguntar y quitar los que sobran cuesta
/// más que dibujarlos a mano. Aquí se propone, el usuario repasa y crea el
/// lote con `create_form_fields`, que es **un** paso de deshacer.
///
/// La heurística trabaja sobre lo que Vitela ya sabe leer: los bloques de
/// texto de la página y la **caja** de sus objetos de camino (pdfium-render
/// 0.8 no expone los segmentos de un camino, y para reconocer una raya o un
/// recuadro basta con su caja). Lo que ya tiene un widget encima no se
/// propone: proponer un campo donde ya hay uno es ruido.
#[tauri::command(async)]
pub fn detect_form_fields(
    work_path: String,
    page_indices: Option<Vec<u16>>,
) -> Result<Vec<CampoPropuesto>, String> {
    on_pdfium_thread(move || {
        crate::with_doc(&work_path, |doc| {
            let total = doc.pages().len();
            let paginas: Vec<u16> = match page_indices {
                Some(v) => v.into_iter().filter(|p| *p < total).collect(),
                None => (0..total).collect(),
            };
            let mut out: Vec<CampoPropuesto> = Vec::new();
            let mut usados: Vec<String> = Vec::new();
            for p in paginas {
                propone_en(doc, p, &mut out, &mut usados);
            }
            Ok(out)
        })
    })
}

/// Una raya, un recuadro o una casilla ya reconocidos, con su caja en el
/// espacio propio de la página.
struct Marca {
    rect: Rect,
    clase: Clase,
}

#[derive(PartialEq, Clone, Copy)]
enum Clase {
    /// Raya horizontal larga y fina: encima va un campo de texto.
    Raya,
    /// Cuadrado pequeño: casilla, o radio si tiene compañeros en su fila.
    Cuadro,
    /// Recuadro grande: campo de texto dentro.
    Caja,
}

/// Alto máximo de una raya (más que esto ya es un recuadro aplastado).
const RAYA_ALTO: f32 = 3.5;
/// Ancho mínimo de una raya para que sea un renglón y no un adorno.
const RAYA_ANCHO: f32 = 36.0;
/// Lado máximo de una casilla.
const CASILLA_LADO: f32 = 22.0;
/// Alto de un campo de texto cuando lo único que hay es la raya: el cuerpo
/// de la letra de al lado por esto, que es la proporción de Acrobat.
const ALTO_POR_CUERPO: f32 = 1.5;

fn propone_en(
    doc: &PdfDocument<'static>,
    page_index: u16,
    out: &mut Vec<CampoPropuesto>,
    usados: &mut Vec<String>,
) {
    let Ok(page) = doc.pages().get(page_index) else {
        return;
    };
    let geo = crate::Geo::de_pagina(&page).propia();
    let bloques = crate::texto::bloques_de(doc, page_index);

    // lo que ya es un campo: no se propone encima
    let ocupado: Vec<Rect> = {
        let annots = page.annotations();
        (0..annots.len())
            .filter_map(|i| {
                let a = annots.get(i).ok()?;
                a.as_widget_annotation()?;
                let b = a.bounds().ok()?;
                Some(geo.pdf_rect_a_ui(&b))
            })
            .collect()
    };

    let mut marcas: Vec<Marca> = Vec::new();
    let objetos = page.objects();
    for i in 0..objetos.len() {
        let Ok(obj) = objetos.get(i) else { continue };
        if obj.as_path_object().is_none() {
            continue;
        }
        let Ok(b) = obj.bounds() else { continue };
        let r = geo.pdf_rect_a_ui(&PdfRect::new(b.bottom(), b.left(), b.top(), b.right()));
        if let Some(clase) = clase_de(&r, geo.ancho()) {
            marcas.push(Marca { rect: r, clase });
        }
    }
    // las corridas de «_» son la otra forma de imprimir un renglón, y son
    // texto, no camino
    for b in &bloques {
        let t = b.text.trim();
        if t.len() >= 4 && t.chars().all(|c| c == '_') {
            marcas.push(Marca {
                rect: Rect {
                    x: b.x,
                    y: b.y + b.h,
                    w: b.w,
                    h: 1.0,
                },
                clase: Clase::Raya,
            });
        }
    }

    // los cuadros que comparten fila con otros son las opciones de un mismo
    // grupo de radios; uno solo es una casilla
    let filas = agrupa_en_filas(&marcas);

    for (indice, marca) in marcas.iter().enumerate() {
        let caja = match marca.clase {
            // el campo va ENCIMA de la raya, con el alto del cuerpo de al lado
            Clase::Raya => {
                let cuerpo = cuerpo_cerca(&bloques, &marca.rect).unwrap_or(11.0);
                let alto = (cuerpo * ALTO_POR_CUERPO).clamp(12.0, 30.0);
                Rect {
                    x: marca.rect.x,
                    y: (marca.rect.y - alto).max(0.0),
                    w: marca.rect.w,
                    h: alto,
                }
            }
            _ => marca.rect.clone(),
        };
        if caja.w < 8.0 || caja.h < 8.0 {
            continue;
        }
        if ocupado.iter().any(|o| se_pisan(o, &caja)) {
            continue;
        }
        if out
            .iter()
            .any(|c| c.page_index == page_index && se_pisan(&c.rect, &caja))
        {
            continue;
        }
        let fila = filas.get(&indice).cloned().unwrap_or_default();
        let hermanos = fila.len().max(1);
        let (kind, etiqueta, confianza) = match marca.clase {
            Clase::Raya => (
                "text",
                etiqueta_de(&bloques, &marca.rect, Lado::Izquierda),
                0.9,
            ),
            Clase::Caja => (
                "text",
                etiqueta_de(&bloques, &marca.rect, Lado::Izquierda),
                0.65,
            ),
            Clase::Cuadro if hermanos > 1 => (
                "radio",
                etiqueta_de(&bloques, &marca.rect, Lado::Derecha),
                0.7,
            ),
            Clase::Cuadro => (
                "checkbox",
                etiqueta_de(&bloques, &marca.rect, Lado::Derecha),
                0.8,
            ),
        };
        let (etiqueta, tenia) = match etiqueta {
            Some(t) => (t, true),
            // sin texto al lado el campo sigue existiendo, pero el nombre es
            // un apaño y la confianza lo dice
            None => (format!("campo_{}", out.len() + 1), false),
        };
        let base = nombre_de_campo(&etiqueta);
        let name = unico(&base, usados);
        let group = if kind == "radio" {
            // el grupo lo encabeza el texto que va a la izquierda del
            // PRIMER cuadro de la fila («Sexo: ○ Hombre ○ Mujer»): tomarlo
            // del vecino de cada uno daría tres grupos de uno
            let primero = fila
                .iter()
                .copied()
                .min_by(|a, b| marcas[*a].rect.x.total_cmp(&marcas[*b].rect.x))
                .unwrap_or(indice);
            etiqueta_de(&bloques, &marcas[primero].rect, Lado::Izquierda)
                .map(|t| nombre_de_campo(&t))
                .unwrap_or_else(|| format!("grupo_{}", out.len() + 1))
        } else {
            String::new()
        };
        out.push(CampoPropuesto {
            page_index,
            rect: caja,
            kind: kind.to_string(),
            name,
            group,
            confianza: if tenia { confianza } else { confianza - 0.3 },
        });
    }
}

/// Qué es esta caja, si es que es algo. `ancho_pagina` sirve para descartar
/// los marcos y las tablas que ocupan la hoja entera.
fn clase_de(r: &Rect, ancho_pagina: f32) -> Option<Clase> {
    let (w, h) = (r.w, r.h);
    if w >= RAYA_ANCHO && h <= RAYA_ALTO && w < ancho_pagina * 0.95 {
        return Some(Clase::Raya);
    }
    let lado = w.min(h);
    if (7.0..=CASILLA_LADO).contains(&lado) && (w / h).clamp(0.1, 10.0) > 0.7 && w / h < 1.4 {
        return Some(Clase::Cuadro);
    }
    if w >= RAYA_ANCHO && (14.0..=120.0).contains(&h) && w < ancho_pagina * 0.95 {
        return Some(Clase::Caja);
    }
    None
}

/// Qué cuadros comparten fila con cada cuadro (él incluido): dos o más
/// alineados y del mismo tamaño son las opciones de un grupo de radios, que
/// es como se imprime «Sí / No / No sabe». El del extremo izquierdo es el
/// que lleva delante el texto que da nombre al grupo.
fn agrupa_en_filas(marcas: &[Marca]) -> std::collections::HashMap<usize, Vec<usize>> {
    let mut out = std::collections::HashMap::new();
    for (i, a) in marcas.iter().enumerate() {
        if a.clase != Clase::Cuadro {
            continue;
        }
        let fila: Vec<usize> = marcas
            .iter()
            .enumerate()
            .filter(|(_, b)| {
                b.clase == Clase::Cuadro
                    && (b.rect.y - a.rect.y).abs() < a.rect.h * 0.6
                    && (b.rect.h - a.rect.h).abs() < 3.0
            })
            .map(|(j, _)| j)
            .collect();
        out.insert(i, fila);
    }
    out
}

enum Lado {
    Izquierda,
    Derecha,
}

/// El texto que da nombre al campo: el más cercano por la izquierda (o por
/// la derecha, en las casillas) dentro de la misma línea, y si no lo hay,
/// el de encima. Es de donde lo saca Acrobat y de donde lo sacaría una
/// persona.
fn etiqueta_de(bloques: &[crate::texto::TextBlock], r: &Rect, lado: Lado) -> Option<String> {
    let centro = r.y + r.h / 2.0;
    let misma_linea = |b: &crate::texto::TextBlock| {
        let c = b.y + b.h / 2.0;
        (c - centro).abs() < (b.h.max(r.h)) * 1.2
    };
    let util = |b: &crate::texto::TextBlock| {
        let t = b.text.trim();
        !t.is_empty() && !t.chars().all(|c| c == '_' || c == '.')
    };
    let candidato = match lado {
        Lado::Izquierda => bloques
            .iter()
            .filter(|b| util(b) && misma_linea(b) && b.x + b.w <= r.x + 2.0)
            .min_by(|a, b| (r.x - (a.x + a.w)).total_cmp(&(r.x - (b.x + b.w)))),
        Lado::Derecha => bloques
            .iter()
            .filter(|b| util(b) && misma_linea(b) && b.x + 2.0 >= r.x + r.w)
            .min_by(|a, b| (a.x - (r.x + r.w)).total_cmp(&(b.x - (r.x + r.w)))),
    };
    let candidato = candidato.or_else(|| {
        // nada al lado: lo de encima, si cae sobre la misma columna
        bloques
            .iter()
            .filter(|b| {
                util(b)
                    && b.y + b.h <= r.y + 1.0
                    && r.y - (b.y + b.h) < b.h * 2.5
                    && b.x < r.x + r.w
                    && b.x + b.w > r.x
            })
            .max_by(|a, b| (a.y + a.h).total_cmp(&(b.y + b.h)))
    })?;
    let t = candidato.text.trim();
    (!t.is_empty()).then(|| t.to_string())
}

/// El cuerpo de letra del texto más cercano a una raya, que es el alto que
/// tiene que tener el campo que se escriba encima.
fn cuerpo_cerca(bloques: &[crate::texto::TextBlock], r: &Rect) -> Option<f32> {
    bloques
        .iter()
        .filter(|b| !b.text.trim().is_empty() && (b.y + b.h / 2.0 - r.y).abs() < 40.0)
        .min_by(|a, b| {
            let d =
                |t: &crate::texto::TextBlock| ((t.x - r.x).powi(2) + (t.y - r.y).powi(2)).sqrt();
            d(a).total_cmp(&d(b))
        })
        .map(|b| b.font_size)
        .filter(|s| *s > 1.0)
}

/// Dos cajas que se pisan de verdad (más de un pelo).
fn se_pisan(a: &Rect, b: &Rect) -> bool {
    let solape = (a.x + a.w).min(b.x + b.w) - a.x.max(b.x);
    let alto = (a.y + a.h).min(b.y + b.h) - a.y.max(b.y);
    solape > 2.0 && alto > 2.0
}

/// El nombre del campo a partir del texto de al lado: «Nombre y
/// apellidos:» → `nombre_y_apellidos`. Sin tildes ni eñes porque el `/T` de
/// un campo lo leen otros programas y una cadena literal con acentos se lee
/// mal fuera de aquí.
fn nombre_de_campo(texto: &str) -> String {
    let limpio: String = texto
        .trim()
        .trim_end_matches([':', '.', '·', '-', '—', ' '])
        .trim()
        .to_lowercase()
        .chars()
        .map(sin_tilde)
        .collect();
    let mut out = String::new();
    for c in limpio.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.ends_with('_') {
            out.push('_');
        }
    }
    let out = out.trim_matches('_').to_string();
    let out: String = out.chars().take(40).collect();
    if out.is_empty() {
        "campo".to_string()
    } else {
        out
    }
}

fn sin_tilde(c: char) -> char {
    match c {
        'á' | 'à' | 'ä' | 'â' => 'a',
        'é' | 'è' | 'ë' | 'ê' => 'e',
        'í' | 'ì' | 'ï' | 'î' => 'i',
        'ó' | 'ò' | 'ö' | 'ô' => 'o',
        'ú' | 'ù' | 'ü' | 'û' => 'u',
        'ñ' => 'n',
        'ç' => 'c',
        otro => otro,
    }
}

/// Desempata los nombres repetidos: `nombre`, `nombre_2`, `nombre_3`…
fn unico(base: &str, usados: &mut Vec<String>) -> String {
    let mut nombre = base.to_string();
    let mut n = 2;
    while usados.contains(&nombre) {
        nombre = format!("{base}_{n}");
        n += 1;
    }
    usados.push(nombre.clone());
    nombre
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::crea_pdf;

    /// Un formulario **impreso** de prueba: rótulos con dos puntos, tres
    /// renglones de subrayado, dos casillas y una fila de tres opciones,
    /// que es como se imprimen los formularios de verdad.
    fn formulario_impreso(dest: &std::path::Path) {
        let dest = dest.to_path_buf();
        crate::historial::borra_instantaneas_en_disco(&dest.to_string_lossy());
        on_pdfium_thread(move || {
            let pdfium = crate::pdfium().expect("libpdfium");
            let mut doc = pdfium.create_new_pdf().expect("documento");
            let font = doc.fonts_mut().helvetica();
            let mut page = doc
                .pages_mut()
                .create_page_at_end(PdfPagePaperSize::a4())
                .expect("página");
            let negro = PdfColor::new(0, 0, 0, 255);
            let texto = |page: &mut PdfPage<'static>, t: &str, x: f32, y: f32| {
                let mut obj =
                    PdfPageTextObject::new(&doc, t, font, PdfPoints::new(11.0)).expect("texto");
                obj.translate(PdfPoints::new(x), PdfPoints::new(y))
                    .expect("colocar");
                page.objects_mut().add_text_object(obj).expect("añadir");
            };
            // tres renglones con su rótulo delante
            for (i, rotulo) in ["Nombre y apellidos:", "Domicilio:", "DNI:"]
                .iter()
                .enumerate()
            {
                let y = 700.0 - i as f32 * 40.0;
                texto(&mut page, rotulo, 60.0, y);
                let linea = PdfPagePathObject::new_line(
                    &doc,
                    PdfPoints::new(200.0),
                    PdfPoints::new(y - 2.0),
                    PdfPoints::new(460.0),
                    PdfPoints::new(y - 2.0),
                    negro,
                    PdfPoints::new(1.0),
                )
                .expect("renglón");
                page.objects_mut().add_path_object(linea).expect("añadir");
            }
            // dos casillas, cada una con su texto a la derecha
            for (i, etiqueta) in ["Acepto las condiciones", "Quiero recibir avisos"]
                .iter()
                .enumerate()
            {
                let y = 560.0 - i as f32 * 30.0;
                let caja = PdfPagePathObject::new_rect(
                    &doc,
                    PdfRect::new(
                        PdfPoints::new(y),
                        PdfPoints::new(60.0),
                        PdfPoints::new(y + 12.0),
                        PdfPoints::new(72.0),
                    ),
                    Some(negro),
                    Some(PdfPoints::new(1.0)),
                    None,
                )
                .expect("casilla");
                page.objects_mut().add_path_object(caja).expect("añadir");
                texto(&mut page, etiqueta, 80.0, y + 2.0);
            }
            // una fila de tres opciones, encabezada por su rótulo
            texto(&mut page, "Sexo:", 60.0, 470.0);
            for (i, opcion) in ["Hombre", "Mujer", "Otro"].iter().enumerate() {
                let x = 110.0 + i as f32 * 90.0;
                let caja = PdfPagePathObject::new_rect(
                    &doc,
                    PdfRect::new(
                        PdfPoints::new(468.0),
                        PdfPoints::new(x),
                        PdfPoints::new(478.0),
                        PdfPoints::new(x + 10.0),
                    ),
                    Some(negro),
                    Some(PdfPoints::new(1.0)),
                    None,
                )
                .expect("opción");
                page.objects_mut().add_path_object(caja).expect("añadir");
                texto(&mut page, opcion, x + 16.0, 470.0);
            }
            page.regenerate_content().expect("contenido");
            drop(page);
            doc.save_to_file(&dest).expect("guardar");
        })
    }

    /// **H7.** «Preparar formulario» de Acrobat pasa el documento, propone
    /// los campos que ha encontrado con el nombre sacado del texto de al
    /// lado y deja corregir **antes** de aceptar. Aquí, además, no se
    /// escribe nada hasta que se dice que sí: una heurística no acierta
    /// siempre y no puede fingir que sí.
    #[test]
    fn reconocer_campos_propone_sin_escribir_nada() {
        let pdf = std::env::temp_dir().join("formularios2-reconocer.pdf");
        formulario_impreso(&pdf);
        let work = pdf.to_string_lossy().into_owned();

        let propuestas = detect_form_fields(work.clone(), None).expect("reconocer");
        assert_eq!(
            propuestas.len(),
            8,
            "tres renglones, dos casillas y tres opciones: {:?}",
            propuestas
                .iter()
                .map(|c| (c.kind.as_str(), c.name.as_str()))
                .collect::<Vec<_>>()
        );

        // el tipo sale de la forma: renglón → texto, cuadro suelto →
        // casilla, cuadros alineados → opciones de un grupo
        let de = |k: &str| -> Vec<&CampoPropuesto> {
            propuestas.iter().filter(|c| c.kind == k).collect()
        };
        assert_eq!(de("text").len(), 3);
        assert_eq!(de("checkbox").len(), 2);
        assert_eq!(de("radio").len(), 3);

        // el nombre sale del texto de al lado, limpio de dos puntos y de
        // espacios, y sin tildes: el `/T` lo leen otros programas
        let nombres: Vec<&str> = propuestas.iter().map(|c| c.name.as_str()).collect();
        assert!(
            nombres.contains(&"nombre_y_apellidos"),
            "los nombres: {nombres:?}"
        );
        assert!(nombres.contains(&"domicilio"), "los nombres: {nombres:?}");
        assert!(nombres.contains(&"dni"), "los nombres: {nombres:?}");
        assert!(
            nombres.contains(&"acepto_las_condiciones"),
            "la casilla toma el texto de su derecha: {nombres:?}"
        );

        // las tres opciones son del MISMO grupo, y el grupo lo encabeza el
        // texto que va delante de la fila
        let grupos: Vec<&str> = de("radio").iter().map(|c| c.group.as_str()).collect();
        assert_eq!(grupos, vec!["sexo"; 3], "un solo grupo: {grupos:?}");
        let opciones: Vec<&str> = de("radio").iter().map(|c| c.name.as_str()).collect();
        assert!(opciones.contains(&"hombre") && opciones.contains(&"mujer"));

        // el campo de texto va ENCIMA del renglón, no debajo ni encima del
        // rótulo: se escribe donde se escribiría a mano
        let renglon = de("text")
            .into_iter()
            .find(|c| c.name == "nombre_y_apellidos")
            .expect("el renglón del nombre");
        assert!(renglon.rect.x > 190.0, "empieza donde empieza la raya");
        assert!(
            renglon.rect.h > 10.0 && renglon.rect.h < 30.0,
            "alto de un renglón"
        );
        assert!(
            propuestas
                .iter()
                .all(|c| c.confianza > 0.0 && c.confianza <= 1.0),
            "la confianza va de 0 a 1"
        );

        // **nada se ha escrito**: el documento sigue sin formulario
        assert!(
            get_form_fields_vacio(&work),
            "reconocer no puede tocar el documento"
        );

        // y crear el lote entero es UN paso de deshacer
        let pasos = crate::historial::history_state(work.clone())
            .expect("historial")
            .undo;
        let lote: Vec<CampoNuevo> = propuestas
            .iter()
            .map(|c| CampoNuevo {
                page_index: c.page_index,
                kind: c.kind.clone(),
                rect: c.rect.clone(),
                name: c.name.clone(),
                group: (!c.group.is_empty()).then(|| c.group.clone()),
                export_value: Some(c.name.clone()),
                options: None,
                props: None,
            })
            .collect();
        let creados = create_form_fields(work.clone(), lote).expect("crear el lote");
        assert_eq!(creados, 8);
        assert_eq!(
            crate::historial::history_state(work.clone())
                .expect("historial")
                .undo,
            pasos + 1,
            "ocho campos, un solo ⌘Z"
        );
        let campos = crate::formularios::get_form_fields(work.clone(), 0).expect("campos");
        assert_eq!(campos.len(), 8, "los ocho widgets cuelgan de la página");
        crate::historial::undo(work.clone()).expect("deshacer");
        assert!(
            get_form_fields_vacio(&work),
            "un ⌘Z devuelve el formulario entero"
        );

        // y un documento sin nada que parezca campo devuelve la lista vacía
        // sin error, que es lo que hay que contestar
        let liso = std::env::temp_dir().join("formularios2-sin-campos.pdf");
        crea_pdf(&["Solo texto corrido, sin renglones ni cuadros"], &liso);
        let vacio = detect_form_fields(liso.to_string_lossy().into_owned(), None)
            .expect("un documento liso no es un error");
        assert!(vacio.is_empty(), "no había nada que proponer: {vacio:?}");
        std::fs::remove_file(&liso).ok();
        std::fs::remove_file(&pdf).ok();
    }

    /// **R48b.** Aceptar un lote de propuestas es **una cirugía**, no N
    /// llamadas fundidas después: si el sexto campo no se puede crear, no
    /// se escribe ninguno. Un formulario a medias —cinco campos escritos y
    /// un error— es peor que un formulario que no se creó, porque el
    /// documento se queda en un estado que nadie pidió y no hay un ⌘Z que
    /// lo deshaga del todo.
    #[test]
    fn un_lote_que_falla_a_medias_no_escribe_ninguno() {
        let pdf = std::env::temp_dir().join("formularios2-lote-roto.pdf");
        crea_pdf(&["Solicitud"], &pdf);
        let work = pdf.to_string_lossy().into_owned();
        let pasos = crate::historial::history_state(work.clone())
            .expect("historial")
            .undo;

        let campo = |nombre: &str, y: f32| CampoNuevo {
            page_index: 0,
            kind: "text".into(),
            rect: Rect {
                x: 100.0,
                y,
                w: 160.0,
                h: 20.0,
            },
            name: nombre.into(),
            group: None,
            export_value: None,
            options: None,
            props: None,
        };
        // el tercero no tiene nombre: `crea_campo` se niega
        let lote = vec![
            campo("nombre", 200.0),
            campo("apellidos", 240.0),
            campo("", 280.0),
        ];
        let error = create_form_fields(work.clone(), lote).expect_err("el lote no vale");
        assert!(error.contains("nombre"), "el aviso dice qué falta: {error}");

        assert!(
            get_form_fields_vacio(&work),
            "ni los dos buenos: la cirugía guarda al final o no guarda"
        );
        assert_eq!(
            crate::historial::history_state(work.clone())
                .expect("historial")
                .undo,
            pasos,
            "una mutación fallida no deja paso de deshacer"
        );
        std::fs::remove_file(&pdf).ok();
    }

    /// **Exportar e importar los datos del formulario** (XFDF). Es lo que
    /// se manda de vuelta a quien repartió el formulario: unos kilobytes
    /// con las respuestas en vez del documento entero. Y al recibirlas, lo
    /// que hay que saber es cuántas se han colocado y **cuántas venían de
    /// un formulario que ya no es este**: importar rellena, no crea campos.
    #[test]
    fn los_datos_del_formulario_salen_y_vuelven_en_un_xfdf() {
        let dir = std::env::temp_dir();
        let campos = |y: f32, nombre: &str, kind: &str| CampoNuevo {
            page_index: 0,
            kind: kind.into(),
            rect: Rect {
                x: 80.0,
                y,
                w: 160.0,
                h: 20.0,
            },
            name: nombre.into(),
            group: None,
            export_value: None,
            options: None,
            props: None,
        };
        let prepara = |nombre: &str| {
            let pdf = dir.join(nombre);
            crea_pdf(&["Solicitud"], &pdf);
            let work = pdf.to_string_lossy().into_owned();
            create_form_fields(
                work.clone(),
                vec![
                    campos(200.0, "nombre", "text"),
                    campos(240.0, "acepto", "checkbox"),
                ],
            )
            .expect("crear el formulario");
            work
        };

        let origen = prepara("formularios2-xfdf-origen.pdf");
        crate::formularios::set_form_text(origen.clone(), 0, 0, "Jorge Gómez".into())
            .expect("rellenar");
        crate::formularios::set_form_checked(origen.clone(), 0, 1, true).expect("marcar");

        let xfdf = dir.join("formularios2-datos.xfdf");
        let d = xfdf.to_string_lossy().into_owned();
        assert_eq!(
            export_form_data_xfdf(origen.clone(), d.clone(), None).expect("exportar"),
            2,
            "los dos campos, con valor o sin él"
        );
        let xml = std::fs::read_to_string(&xfdf).expect("leer el xfdf");
        assert!(xml.contains("name=\"nombre\""), "{xml}");
        assert!(xml.contains("<value>Jorge Gómez</value>"), "{xml}");
        assert!(xml.contains("name=\"acepto\""), "{xml}");
        // **AC-102**: el `<f href>` lleva el nombre del documento, no el
        // de la copia de trabajo, que no significa nada para quien recibe
        // el fichero
        assert!(
            xml.contains("<f href=\"formularios2-xfdf-origen.pdf\"/>"),
            "el nombre del documento en el href: {xml}"
        );
        let con_nombre = dir.join("formularios2-datos-nombre.xfdf");
        export_form_data_xfdf(
            origen.clone(),
            con_nombre.to_string_lossy().into_owned(),
            Some("/Users/jorge/Documentos/Solicitud de beca.pdf".into()),
        )
        .expect("exportar con nombre");
        let xml2 = std::fs::read_to_string(&con_nombre).expect("leer");
        assert!(
            xml2.contains("<f href=\"Solicitud de beca.pdf\"/>"),
            "el que manda la interfaz, sin su carpeta: {xml2}"
        );

        // y vuelven a otro ejemplar del mismo formulario, en blanco
        let destino = prepara("formularios2-xfdf-destino.pdf");
        let pasos = crate::historial::history_state(destino.clone())
            .expect("historial")
            .undo;
        assert_eq!(
            import_form_data_xfdf(destino.clone(), d.clone()).expect("importar"),
            ImportacionFormulario {
                rellenados: 2,
                sin_campo: 0
            }
        );
        assert_eq!(
            crate::historial::history_state(destino.clone())
                .expect("historial")
                .undo,
            pasos + 1,
            "el fichero entero es un solo ⌘Z"
        );
        let campos_ahora = crate::formularios::get_form_fields(destino.clone(), 0).expect("campos");
        assert_eq!(campos_ahora[0].value, "Jorge Gómez");
        assert!(campos_ahora[1].checked, "la casilla marcada se ve marcada");

        // un fichero de otra versión del formulario: lo que no cabe se
        // cuenta y se dice, que es la pregunta de quien recibe respuestas
        let ajeno = dir.join("formularios2-datos-ajeno.xfdf");
        std::fs::write(
            &ajeno,
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
             <xfdf xmlns=\"http://ns.adobe.com/xfdf/\">\n<fields>\n\
             <field name=\"nombre\"><value>Ana</value></field>\n\
             <field name=\"telefono\"><value>600 000 000</value></field>\n\
             </fields></xfdf>",
        )
        .expect("escribir el xfdf ajeno");
        assert_eq!(
            import_form_data_xfdf(destino.clone(), ajeno.to_string_lossy().into_owned())
                .expect("importar"),
            ImportacionFormulario {
                rellenados: 1,
                sin_campo: 1
            }
        );

        // un fichero que no trae datos no se traga en silencio
        let vacio = dir.join("formularios2-datos-vacio.xfdf");
        std::fs::write(&vacio, "<xfdf><fields></fields></xfdf>").expect("escribir");
        assert!(
            import_form_data_xfdf(destino.clone(), vacio.to_string_lossy().into_owned())
                .unwrap_err()
                .contains("datos de formulario")
        );
        // y un documento sin campos tampoco escribe un fichero vacío
        let liso = dir.join("formularios2-xfdf-liso.pdf");
        crea_pdf(&["Sin campos"], &liso);
        assert!(
            export_form_data_xfdf(liso.to_string_lossy().into_owned(), d.clone(), None)
                .unwrap_err()
                .contains("campos de formulario")
        );

        for f in [&xfdf, &ajeno, &vacio, &liso] {
            std::fs::remove_file(f).ok();
        }
    }

    fn get_form_fields_vacio(work: &str) -> bool {
        crate::formularios::get_form_fields(work.to_string(), 0)
            .map(|c| c.is_empty())
            .unwrap_or(true)
    }

    /// **R42b (AC-065 y AC-066).** Un campo de solo lectura no se rellena,
    /// y el texto de ayuda que se escribe en el fichero se puede leer.
    ///
    /// Hasta el ciclo 6, `get_form_fields` devolvía `required` y nada más:
    /// la UI no podía saber que un campo estaba bloqueado y lo dejaba
    /// cambiar, y el `/TU` viajaba al PDF sin que hubiera forma de verlo.
    #[test]
    fn un_campo_de_solo_lectura_no_se_rellena_y_su_ayuda_se_lee() {
        let pdf = std::env::temp_dir().join("formularios2-solo-lectura.pdf");
        crea_pdf(&["Formulario"], &pdf);
        let work = pdf.to_string_lossy().into_owned();
        create_form_field(
            work.clone(),
            0,
            "text".into(),
            Rect {
                x: 60.0,
                y: 200.0,
                w: 200.0,
                h: 20.0,
            },
            "expediente".into(),
            None,
            None,
            None,
            Some(PropsCampo {
                tooltip: Some("El número que sale en la carta".into()),
                solo_lectura: true,
                valor_defecto: Some("2026/0001".into()),
                ..Default::default()
            }),
        )
        .expect("crear el campo bloqueado");
        create_form_field(
            work.clone(),
            0,
            "text".into(),
            Rect {
                x: 60.0,
                y: 240.0,
                w: 200.0,
                h: 20.0,
            },
            "comentario".into(),
            None,
            None,
            None,
            None,
        )
        .expect("crear el campo normal");

        let campos = crate::formularios::get_form_fields(work.clone(), 0).expect("campos");
        let bloqueado = campos
            .iter()
            .find(|c| c.name == "expediente")
            .expect("el bloqueado");
        let libre = campos
            .iter()
            .find(|c| c.name == "comentario")
            .expect("el libre");
        assert!(bloqueado.read_only, "el bit 1 del /Ff se lee");
        assert!(!libre.read_only);
        assert_eq!(bloqueado.tooltip, "El número que sale en la carta");
        assert_eq!(libre.tooltip, "", "sin /TU, sin ayuda que enseñar");

        // y no se deja cambiar, aunque la interfaz se despiste
        let err = crate::formularios::set_form_text(
            work.clone(),
            0,
            bloqueado.annot_index,
            "otra cosa".into(),
        )
        .unwrap_err();
        assert!(err.contains("solo lectura"), "el aviso: {err}");
        assert!(
            !err.contains("Ff") && !err.contains("os error"),
            "jerga: {err}"
        );
        let campos = crate::formularios::get_form_fields(work.clone(), 0).expect("campos");
        assert_eq!(
            campos
                .iter()
                .find(|c| c.name == "expediente")
                .unwrap()
                .value,
            "2026/0001",
            "el valor no ha cambiado"
        );

        // el campo normal sí
        crate::formularios::set_form_text(work.clone(), 0, libre.annot_index, "vale".into())
            .expect("rellenar el campo normal");

        // una lista bloqueada tampoco: es el caso con el que se encontró
        create_form_field(
            work.clone(),
            0,
            "list".into(),
            Rect {
                x: 60.0,
                y: 300.0,
                w: 200.0,
                h: 20.0,
            },
            "idioma".into(),
            None,
            None,
            Some(vec!["Castellano".into(), "Euskera".into()]),
            Some(PropsCampo {
                solo_lectura: true,
                ..Default::default()
            }),
        )
        .expect("crear la lista bloqueada");
        let lista = crate::formularios::get_form_fields(work.clone(), 0)
            .expect("campos")
            .into_iter()
            .find(|c| c.name == "idioma")
            .expect("la lista");
        assert!(lista.read_only);
        let err = crate::formularios::set_form_choice(
            work.clone(),
            0,
            lista.annot_index,
            "Euskera".into(),
        )
        .unwrap_err();
        assert!(err.contains("solo lectura"), "el aviso: {err}");

        // y una casilla bloqueada
        create_form_field(
            work.clone(),
            0,
            "checkbox".into(),
            Rect {
                x: 300.0,
                y: 200.0,
                w: 16.0,
                h: 16.0,
            },
            "leido".into(),
            None,
            None,
            None,
            Some(PropsCampo {
                solo_lectura: true,
                ..Default::default()
            }),
        )
        .expect("crear la casilla bloqueada");
        let casilla = crate::formularios::get_form_fields(work.clone(), 0)
            .expect("campos")
            .into_iter()
            .find(|c| c.name == "leido")
            .expect("la casilla");
        let err = crate::formularios::set_form_checked(work.clone(), 0, casilla.annot_index, true)
            .unwrap_err();
        assert!(err.contains("solo lectura"), "el aviso: {err}");
        std::fs::remove_file(&pdf).ok();
    }

    /// **R40b (AC-063).** Un grupo de radios recién creado tiene que salir
    /// **sin marcar**, y marcar uno tiene que apagar a sus hermanos.
    ///
    /// Hasta el ciclo 6 los tres salían marcados: `is_checked()` de
    /// pdfium-render 0.8 devuelve `true` para todas las opciones cuando el
    /// campo está en `/V /Off` (el estado «Off» cuenta como estado presente
    /// en el `/AP`). Con eso la UI conmutaba `!checked` —siempre `false`—
    /// y volvía a escribir `/Off`: el formulario no se podía rellenar
    /// nunca, ni al primer clic ni al décimo.
    #[test]
    fn un_grupo_de_radios_sale_sin_marcar_y_marcar_uno_apaga_a_los_demas() {
        let pdf = std::env::temp_dir().join("formularios2-radios-marcar.pdf");
        crea_pdf(&["Radios"], &pdf);
        let work = pdf.to_string_lossy().into_owned();
        for i in 0..3 {
            create_form_field(
                work.clone(),
                0,
                "radio".into(),
                Rect {
                    x: 60.0,
                    y: 250.0 + i as f32 * 40.0,
                    w: 20.0,
                    h: 20.0,
                },
                format!("op{i}"),
                Some("sexo".into()),
                Some(format!("op{i}")),
                None,
                None,
            )
            .expect("crear la opción");
        }

        let campos = crate::formularios::get_form_fields(work.clone(), 0).expect("campos");
        assert_eq!(campos.len(), 3);
        assert!(
            campos.iter().all(|c| !c.checked),
            "un grupo recién creado no tiene nada elegido: {:?}",
            campos.iter().map(|c| c.checked).collect::<Vec<_>>()
        );

        // marcar el segundo: solo el segundo
        crate::formularios::set_form_checked(work.clone(), 0, campos[1].annot_index, true)
            .expect("marcar el segundo");
        let campos = crate::formularios::get_form_fields(work.clone(), 0).expect("campos");
        assert_eq!(
            campos.iter().map(|c| c.checked).collect::<Vec<_>>(),
            vec![false, true, false],
            "solo el segundo"
        );

        // y marcar el tercero apaga al segundo: eso es lo que distingue un
        // grupo de radios de tres casillas sueltas
        crate::formularios::set_form_checked(work.clone(), 0, campos[2].annot_index, true)
            .expect("marcar el tercero");
        let campos = crate::formularios::get_form_fields(work.clone(), 0).expect("campos");
        assert_eq!(
            campos.iter().map(|c| c.checked).collect::<Vec<_>>(),
            vec![false, false, true],
            "el tercero, y el segundo se ha apagado"
        );

        // una casilla suelta sí se conmuta, que es lo suyo
        create_form_field(
            work.clone(),
            0,
            "checkbox".into(),
            Rect {
                x: 200.0,
                y: 250.0,
                w: 16.0,
                h: 16.0,
            },
            "acepto".into(),
            None,
            None,
            None,
            None,
        )
        .expect("crear la casilla");
        let casilla = crate::formularios::get_form_fields(work.clone(), 0)
            .expect("campos")
            .into_iter()
            .find(|c| c.name == "acepto")
            .expect("la casilla");
        assert!(!casilla.checked, "recién creada, sin marcar");
        crate::formularios::set_form_checked(work.clone(), 0, casilla.annot_index, true)
            .expect("marcar");
        let casilla = crate::formularios::get_form_fields(work.clone(), 0)
            .expect("campos")
            .into_iter()
            .find(|c| c.name == "acepto")
            .expect("la casilla");
        assert!(casilla.checked, "la casilla queda marcada");
        std::fs::remove_file(&pdf).ok();
    }

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
                Rect {
                    x: 60.0,
                    y: 200.0 + i as f32 * 30.0,
                    w: 18.0,
                    h: 18.0,
                },
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
        let root = doc
            .trailer
            .get(b"Root")
            .and_then(|o| o.as_reference())
            .unwrap();
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
                doc.get_object(id)
                    .unwrap()
                    .as_dict()
                    .unwrap()
                    .get(b"AS")
                    .unwrap()
                    .as_name()
                    .unwrap(),
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
        let opciones = vec![
            "España".to_string(),
            "Portugal".to_string(),
            "Francia".to_string(),
        ];
        create_form_field(
            work.clone(),
            0,
            "combo".into(),
            Rect {
                x: 60.0,
                y: 200.0,
                w: 160.0,
                h: 24.0,
            },
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
            Rect {
                x: 60.0,
                y: 260.0,
                w: 160.0,
                h: 60.0,
            },
            "provincias".into(),
            None,
            None,
            Some(vec!["Álava".into(), "Burgos".into()]),
            None,
        )
        .expect("crear lista");

        let campos = crate::formularios::get_form_fields(work.clone(), 0).expect("listar");
        let combo = campos
            .iter()
            .find(|c| c.name == "pais")
            .expect("el desplegable");
        assert_eq!(combo.kind, "ComboBox", "{}", combo.kind);
        assert_eq!(combo.options, opciones, "las opciones se leen enteras");
        assert_eq!(combo.value, "Portugal", "el valor por defecto sale puesto");
        let lista = campos
            .iter()
            .find(|c| c.name == "provincias")
            .expect("la lista");
        assert_eq!(lista.kind, "ListBox", "{}", lista.kind);
        assert_eq!(lista.options.len(), 2);

        // y se puede elegir otra opción con el comando de siempre
        crate::formularios::set_form_choice(work.clone(), 0, combo.annot_index, "Francia".into())
            .expect("elegir");
        let campos = crate::formularios::get_form_fields(work.clone(), 0).expect("listar");
        assert_eq!(
            campos.iter().find(|c| c.name == "pais").unwrap().value,
            "Francia"
        );

        // un desplegable sin opciones se dice, no se crea vacío
        assert!(create_form_field(
            work.clone(),
            0,
            "combo".into(),
            Rect {
                x: 60.0,
                y: 400.0,
                w: 100.0,
                h: 24.0
            },
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
            Rect {
                x: 60.0,
                y: 200.0,
                w: 200.0,
                h: 24.0,
            },
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
            Rect {
                x: 60.0,
                y: 260.0,
                w: 200.0,
                h: 24.0,
            },
            "tratamiento".into(),
            None,
            None,
            None,
            Some(PropsCampo {
                orden_tab: Some(0),
                ..Default::default()
            }),
        )
        .expect("crear el segundo");

        let campos = crate::formularios::get_form_fields(work.clone(), 0).expect("listar");
        assert_eq!(
            campos[0].name, "tratamiento",
            "el orden de tabulación es el de /Annots: {campos:?}"
        );
        let obligatorio = campos
            .iter()
            .find(|c| c.name == "nombre")
            .expect("el campo");
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
        assert_eq!(
            campo.get(b"Ff").unwrap().as_i64().unwrap() & OBLIGATORIO,
            OBLIGATORIO
        );
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
            doc.get_object(page_id)
                .unwrap()
                .as_dict()
                .unwrap()
                .get(b"Tabs")
                .unwrap()
                .as_name()
                .unwrap(),
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
        let area = Rect {
            x: 100.0,
            y: 400.0,
            w: 150.0,
            h: 30.0,
        };
        create_form_field(
            work.clone(),
            0,
            "text".into(),
            area.clone(),
            "nombre".into(),
            None,
            None,
            None,
            None,
        )
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
        create_form_field(
            work.clone(),
            0,
            "checkbox".into(),
            r.clone(),
            "acepto".into(),
            None,
            None,
            None,
            None,
        )
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
        assert!(
            nombres.contains(&"acepto") && nombres.contains(&"acepto-2"),
            "{nombres:?}"
        );
        let idx = campos
            .iter()
            .find(|c| c.name == "acepto")
            .unwrap()
            .annot_index;
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
            Rect {
                x: 60.0,
                y: 200.0,
                w: 140.0,
                h: 22.0,
            },
            "efimero".into(),
            None,
            None,
            None,
            None,
        )
        .expect("crear");
        assert_eq!(
            crate::formularios::get_form_fields(work.clone(), 0)
                .unwrap()
                .len(),
            1
        );
        delete_form_field(work.clone(), "efimero".into()).expect("borrar");
        assert_eq!(
            crate::formularios::get_form_fields(work.clone(), 0)
                .unwrap()
                .len(),
            0
        );
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
        create_link(work.clone(), 0, Rect { y: 130.0, ..r }, None, Some(1)).expect("enlace página");
        let links = crate::documento::get_links(work.clone(), 0).expect("listar");
        assert_eq!(links.len(), 2);
        assert!(links
            .iter()
            .any(|l| l.uri.as_deref() == Some("https://ejemplo.es")));
        assert!(links.iter().any(|l| l.dest_page == Some(1)));
        // exactamente uno de los dos parámetros
        assert!(create_link(work.clone(), 0, r.clone(), None, None).is_err());
        // esquemas peligrosos fuera; sin esquema se asume https
        assert!(create_link(
            work.clone(),
            0,
            r.clone(),
            Some("file:///etc/passwd".into()),
            None
        )
        .is_err());
        assert!(create_link(
            work.clone(),
            0,
            r.clone(),
            Some("javascript:alert(1)".into()),
            None
        )
        .is_err());
        create_link(
            work.clone(),
            0,
            r.clone(),
            Some("ejemplo.org/x".into()),
            None,
        )
        .expect("sin esquema");
        let links = crate::documento::get_links(work.clone(), 0).expect("listar");
        assert!(links
            .iter()
            .any(|l| l.uri.as_deref() == Some("https://ejemplo.org/x")));
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

// ---------------------------------------------------------------------------
// Los datos del formulario, fuera del PDF (XFDF)
// ---------------------------------------------------------------------------

/// Lo que ha pasado al importar unas respuestas.
#[derive(serde::Serialize, Debug, Default, PartialEq, Eq)]
pub struct ImportacionFormulario {
    /// Campos del documento que han recibido su valor.
    pub rellenados: u16,
    /// Campos que venían en el fichero y **no existen en este documento**.
    /// Es la pregunta real de quien recibe respuestas: si el que contestó
    /// tenía otra versión del formulario, hay respuestas que no se pueden
    /// colocar y hay que decirlo, no tragárselas.
    pub sin_campo: u16,
}

/// Los campos terminales del `/AcroForm` con su **nombre completo** (el
/// `/T` de cada nivel unido con puntos, como lo escribe el spec y como lo
/// espera cualquier otro programa) y el id de su objeto.
///
/// Un campo es terminal cuando no tiene `/Kids` o cuando sus hijos son
/// widgets sin nombre propio (un grupo de radios es **un** campo con tres
/// hijos, no tres campos).
fn campos_por_nombre(doc: &LoDoc) -> Vec<(String, lopdf::ObjectId)> {
    fn baja(
        doc: &LoDoc,
        id: lopdf::ObjectId,
        prefijo: &str,
        hondo: u8,
        vistos: &mut Vec<lopdf::ObjectId>,
        out: &mut Vec<(String, lopdf::ObjectId)>,
    ) {
        if hondo > 16 || vistos.contains(&id) {
            return; // un /Kids con un ciclo no puede colgar la app
        }
        vistos.push(id);
        let Ok(d) = doc.get_object(id).and_then(|o| o.as_dict()) else {
            return;
        };
        let propio = d
            .get(b"T")
            .and_then(|o| o.as_str())
            .map(|s| String::from_utf8_lossy(s).into_owned())
            .unwrap_or_default();
        let nombre = match (prefijo.is_empty(), propio.is_empty()) {
            (_, true) => prefijo.to_string(),
            (true, false) => propio,
            (false, false) => format!("{prefijo}.{propio}"),
        };
        let kids: Vec<lopdf::ObjectId> = d
            .get(b"Kids")
            .and_then(|o| o.as_array())
            .map(|a| a.iter().filter_map(|o| o.as_reference().ok()).collect())
            .unwrap_or_default();
        // hijos con nombre propio = subárbol; sin nombre = widgets del campo
        let con_nombre: Vec<lopdf::ObjectId> = kids
            .iter()
            .copied()
            .filter(|k| {
                doc.get_object(*k)
                    .and_then(|o| o.as_dict())
                    .map(|d| d.has(b"T"))
                    .unwrap_or(false)
            })
            .collect();
        if con_nombre.is_empty() {
            if !nombre.is_empty() {
                out.push((nombre, id));
            }
            return;
        }
        for k in con_nombre {
            baja(doc, k, &nombre, hondo + 1, vistos, out);
        }
    }

    let mut out = Vec::new();
    let Ok(catalog) = doc.catalog() else {
        return out;
    };
    let form = match catalog.get(b"AcroForm") {
        Ok(Object::Reference(rid)) => doc.get_object(*rid).and_then(|o| o.as_dict()).ok(),
        Ok(Object::Dictionary(d)) => Some(d),
        _ => None,
    };
    let Some(form) = form else { return out };
    let Ok(campos) = form.get(b"Fields").and_then(|o| match o {
        Object::Reference(rid) => doc.get_object(*rid).and_then(|o| o.as_array()),
        otro => otro.as_array(),
    }) else {
        return out;
    };
    let mut vistos = Vec::new();
    for c in campos {
        if let Ok(id) = c.as_reference() {
            baja(doc, id, "", 0, &mut vistos, &mut out);
        }
    }
    out
}

/// El valor de un campo, tal como se escribe en un XFDF: una cadena por
/// valor (una lista de selección múltiple trae varias).
fn valores_de(doc: &LoDoc, id: lopdf::ObjectId) -> Vec<String> {
    // el valor de un botón es un **nombre** (`/Yes`), y PDFium lo escribe a
    // veces como la cadena «/Yes»: en el XFDF va sin la barra, que es lo que
    // entiende Acrobat. En un campo de texto la barra es un carácter más y
    // no se toca.
    let es_boton = heredado_ft(doc, id).as_deref() == Some("Btn");
    let texto = move |o: &Object| {
        let s = match o {
            // una cadena PDF puede venir en UTF-16BE: leerla como bytes
            // sueltos convertía «Gómez» en «G?mez» en el fichero que se
            // manda de vuelta
            Object::String(..) => crate::anotaciones::texto_de_cadena_pdf(o),
            Object::Name(n) => String::from_utf8_lossy(n).into_owned(),
            _ => return None,
        };
        Some(if es_boton {
            s.trim_start_matches('/').to_string()
        } else {
            s
        })
    };
    // el /V puede estar heredado del padre
    let mut actual = Some(id);
    let mut hondo = 0;
    while let Some(oid) = actual {
        hondo += 1;
        if hondo > 16 {
            break;
        }
        let Ok(d) = doc.get_object(oid).and_then(|o| o.as_dict()) else {
            break;
        };
        if let Ok(v) = d.get(b"V") {
            let v = match v {
                Object::Reference(rid) => doc.get_object(*rid).unwrap_or(v),
                otro => otro,
            };
            return match v {
                Object::Array(a) => a.iter().filter_map(texto).collect(),
                otro => texto(otro).into_iter().collect(),
            };
        }
        actual = d.get(b"Parent").and_then(|o| o.as_reference()).ok();
    }
    Vec::new()
}

/// ¿Es un campo de firma? Los `/Sig` no son datos del formulario: no se
/// exportan ni se rellenan (ni se podrían: una firma no es un valor).
fn es_firma(doc: &LoDoc, id: lopdf::ObjectId) -> bool {
    let mut actual = Some(id);
    let mut hondo = 0;
    while let Some(oid) = actual {
        hondo += 1;
        if hondo > 16 {
            return false;
        }
        let Ok(d) = doc.get_object(oid).and_then(|o| o.as_dict()) else {
            return false;
        };
        if let Ok(ft) = d.get(b"FT").and_then(|o| o.as_name()) {
            return ft == b"Sig";
        }
        actual = d.get(b"Parent").and_then(|o| o.as_reference()).ok();
    }
    false
}

/// **Exportar los datos del formulario** a un XFDF, como «Más ▸ Exportar
/// datos» de Acrobat: solo lo que alguien ha rellenado, sin el documento.
/// Es lo que se manda de vuelta a quien repartió el formulario, y pesa unos
/// kilobytes en vez de unos megas.
///
/// Se escribe a mano (el XML de salida es cuatro etiquetas) y se lee con
/// `quick-xml`, por el mismo camino que estrenó el XFDF de los comentarios.
/// Los campos de firma no salen: una firma no es un dato que se rellene.
#[tauri::command(async)]
pub fn export_form_data_xfdf(
    work_path: String,
    dest_path: String,
    document_name: Option<String>,
) -> Result<u16, String> {
    // AC-102: el `<f href>` decía «vitela-c-formulario-178910…​.pdf», que
    // es el nombre de la copia de trabajo. Quien recibe el fichero no
    // tiene por qué ver el temporal: se reusa lo del resumen de
    // comentarios, que ya sabía sacar el nombre de verdad
    let nombre = crate::comentarios::nombre_de_documento(document_name.as_deref(), &work_path);
    let (xml, n) = crate::on_pdfium_thread(move || {
        crate::with_lopdf(&work_path, |doc| {
            let mut out = format!(
                "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
                 <xfdf xmlns=\"http://ns.adobe.com/xfdf/\" xml:space=\"preserve\">\n\
                 <f href=\"{}\"/>\n<fields>\n",
                crate::comentarios2::escapa_xml(&nombre)
            );
            let mut n = 0u16;
            for (nombre, id) in campos_por_nombre(doc) {
                if es_firma(doc, id) {
                    continue;
                }
                out.push_str(&format!(
                    "<field name=\"{}\">\n",
                    crate::comentarios2::escapa_xml(&nombre)
                ));
                for v in valores_de(doc, id) {
                    out.push_str(&format!(
                        "<value>{}</value>\n",
                        crate::comentarios2::escapa_xml(&v)
                    ));
                }
                out.push_str("</field>\n");
                n += 1;
            }
            out.push_str("</fields>\n</xfdf>\n");
            Ok((out, n))
        })
    })?;
    if n == 0 {
        return Err("Este documento no tiene campos de formulario".into());
    }
    std::fs::write(&dest_path, xml)
        .map_err(|e| crate::mensaje_llano(format!("No se ha podido escribir {dest_path}: {e}")))?;
    Ok(n)
}

/// Lee los pares nombre → valores de un XFDF de datos.
fn lee_xfdf_datos(xml: &str) -> Result<Vec<(String, Vec<String>)>, String> {
    use quick_xml::events::Event;
    let mut lector = quick_xml::Reader::from_str(xml);
    let mut out: Vec<(String, Vec<String>)> = Vec::new();
    let mut actual: Option<(String, Vec<String>)> = None;
    let mut en_valor = false;
    let mut buffer = String::new();
    loop {
        match lector.read_event() {
            Err(e) => {
                return Err(crate::mensaje_llano(format!(
                    "Ese fichero no es un XFDF que se pueda leer: {e}"
                )))
            }
            Ok(Event::Eof) => break,
            Ok(Event::Start(e)) => {
                let etiqueta = String::from_utf8_lossy(e.local_name().as_ref()).to_lowercase();
                match etiqueta.as_str() {
                    "field" => {
                        let nombre = e
                            .attributes()
                            .flatten()
                            .find(|a| a.key.local_name().as_ref() == b"name")
                            .and_then(|a| String::from_utf8(a.value.to_vec()).ok())
                            .unwrap_or_default();
                        if let Some(campo) = actual.take() {
                            out.push(campo);
                        }
                        actual = Some((nombre, Vec::new()));
                    }
                    "value" => {
                        en_valor = true;
                        buffer.clear();
                    }
                    _ => {}
                }
            }
            Ok(Event::Text(t)) if en_valor => {
                buffer.push_str(&t.decode().unwrap_or_default());
            }
            Ok(Event::End(e)) => {
                let etiqueta = String::from_utf8_lossy(e.local_name().as_ref()).to_lowercase();
                match etiqueta.as_str() {
                    "value" => {
                        en_valor = false;
                        if let Some((_, valores)) = actual.as_mut() {
                            valores.push(buffer.clone());
                        }
                    }
                    "field" => {
                        if let Some(campo) = actual.take() {
                            out.push(campo);
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
    if let Some(campo) = actual.take() {
        out.push(campo);
    }
    out.retain(|(n, _)| !n.is_empty());
    Ok(out)
}

/// **Importar los datos de un formulario** desde un XFDF: rellena los
/// campos que ya existen y **no crea ninguno**. Es lo que hace Acrobat, y
/// es lo correcto: un fichero de respuestas no puede cambiar el formulario
/// que se repartió.
///
/// Dice cuántos ha rellenado y cuántos venían en el fichero y no existen en
/// el documento, que es la pregunta de quien recibe respuestas de una
/// versión anterior del formulario. Todo en **una** mutación.
#[tauri::command(async)]
pub fn import_form_data_xfdf(
    work_path: String,
    src_path: String,
) -> Result<ImportacionFormulario, String> {
    let xml = std::fs::read_to_string(&src_path)
        .map_err(|e| crate::mensaje_llano(format!("No se ha podido leer {src_path}: {e}")))?;
    let datos = lee_xfdf_datos(&xml)?;
    if datos.is_empty() {
        return Err("Ese fichero no trae datos de formulario".into());
    }
    let hecho = std::sync::Arc::new(std::sync::Mutex::new(ImportacionFormulario::default()));
    let cuenta = hecho.clone();
    cirugia(&work_path, move |doc| {
        let campos: std::collections::HashMap<String, lopdf::ObjectId> =
            campos_por_nombre(doc).into_iter().collect();
        let mut informe = ImportacionFormulario::default();
        for (nombre, valores) in datos {
            let Some(id) = campos.get(&nombre).copied() else {
                informe.sin_campo += 1;
                continue;
            };
            if es_firma(doc, id) {
                informe.sin_campo += 1;
                continue;
            }
            rellena_campo(doc, id, &valores)?;
            informe.rellenados += 1;
        }
        pide_apariencias(doc)?;
        *cuenta.lock().unwrap_or_else(|e| e.into_inner()) = informe;
        Ok(())
    })?;
    let informe = std::mem::take(&mut *hecho.lock().unwrap_or_else(|e| e.into_inner()));
    Ok(informe)
}

/// Escribe el `/V` de un campo y, si es un botón, el `/AS` de sus widgets:
/// el `/AS` es el estado que el visor pinta, y sin él una casilla marcada
/// se ve sin marcar (AC-063).
fn rellena_campo(doc: &mut LoDoc, id: lopdf::ObjectId, valores: &[String]) -> Result<(), String> {
    let (es_boton, kids) = {
        let d = doc
            .get_object(id)
            .and_then(|o| o.as_dict())
            .map_err(|e| e.to_string())?;
        let ft = heredado_ft(doc, id);
        let kids: Vec<lopdf::ObjectId> = d
            .get(b"Kids")
            .and_then(|o| o.as_array())
            .map(|a| a.iter().filter_map(|o| o.as_reference().ok()).collect())
            .unwrap_or_default();
        (ft.as_deref() == Some("Btn"), kids)
    };
    let valor = valores.first().cloned().unwrap_or_default();
    let nuevo = if es_boton {
        Object::Name(valor.as_bytes().to_vec())
    } else if valores.len() > 1 {
        Object::Array(
            valores
                .iter()
                .map(|v| crate::documento::cadena_pdf(v))
                .collect(),
        )
    } else {
        crate::documento::cadena_pdf(&valor)
    };
    doc.get_object_mut(id)
        .and_then(|o| o.as_dict_mut())
        .map_err(|e| e.to_string())?
        .set("V", nuevo);
    if es_boton {
        // el hijo cuyo /AP tiene ese estado se enciende; los hermanos, /Off
        let objetivo = valor.as_bytes().to_vec();
        let hijos = if kids.is_empty() { vec![id] } else { kids };
        for k in hijos {
            let suyo = estados_de_ap(doc, k).contains(&valor);
            if let Ok(d) = doc.get_object_mut(k).and_then(|o| o.as_dict_mut()) {
                d.set(
                    "AS",
                    Object::Name(if suyo {
                        objetivo.clone()
                    } else {
                        b"Off".to_vec()
                    }),
                );
            }
        }
    }
    Ok(())
}

/// El `/FT` del campo, heredándolo del padre como dice el spec.
fn heredado_ft(doc: &LoDoc, id: lopdf::ObjectId) -> Option<String> {
    let mut actual = Some(id);
    let mut hondo = 0;
    while let Some(oid) = actual {
        hondo += 1;
        if hondo > 16 {
            return None;
        }
        let d = doc.get_object(oid).and_then(|o| o.as_dict()).ok()?;
        if let Ok(ft) = d.get(b"FT").and_then(|o| o.as_name()) {
            return Some(String::from_utf8_lossy(ft).into_owned());
        }
        actual = d.get(b"Parent").and_then(|o| o.as_reference()).ok();
    }
    None
}

/// Los estados que el `/AP /N` de un widget sabe pintar («Yes», «Off»…).
fn estados_de_ap(doc: &LoDoc, id: lopdf::ObjectId) -> Vec<String> {
    let Ok(d) = doc.get_object(id).and_then(|o| o.as_dict()) else {
        return Vec::new();
    };
    let ap = match d.get(b"AP") {
        Ok(Object::Reference(rid)) => doc.get_object(*rid).and_then(|o| o.as_dict()).ok(),
        Ok(Object::Dictionary(d)) => Some(d),
        _ => None,
    };
    let Some(ap) = ap else { return Vec::new() };
    let n = match ap.get(b"N") {
        Ok(Object::Reference(rid)) => doc.get_object(*rid).and_then(|o| o.as_dict()).ok(),
        Ok(Object::Dictionary(d)) => Some(d),
        _ => None,
    };
    n.map(|d| {
        d.iter()
            .map(|(k, _)| String::from_utf8_lossy(k).into_owned())
            .collect()
    })
    .unwrap_or_default()
}
