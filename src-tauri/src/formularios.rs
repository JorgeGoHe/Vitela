//! Formularios AcroForm: leer campos y rellenar texto y casillas.

use crate::{on_pdfium_thread, pdfium, save_and_close, with_doc};
use crate::historial::mutacion;
use pdfium_render::prelude::*;
use serde::Serialize;

#[derive(Serialize, Debug)]
pub struct FormFieldInfo {
    pub annot_index: u16,
    pub name: String,
    pub kind: String,
    pub value: String,
    pub checked: bool,
    /// Opciones de un desplegable o una lista (`/Opt`); vacío en el resto.
    pub options: Vec<String>,
    /// Campo obligatorio (`/Ff` bit 2, heredado del padre si el widget no
    /// lo lleva): la UI le pinta el borde del acento, como Acrobat.
    pub required: bool,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// Lista los campos de formulario (widgets) de una página, con bounds en
/// coords de UI.
#[tauri::command(async)]
pub fn get_form_fields(path: String, page_index: u16) -> Result<Vec<FormFieldInfo>, String> {
    on_pdfium_thread(move || {
        // el bit de «obligatorio» lo lee lopdf: pdfium-render 0.8 no expone
        // el /Ff del campo, y sin él la UI no puede resaltar lo que hay que
        // rellenar antes de enviar
        let obligatorios = crate::with_lopdf(&path, |doc| Ok(banderas_de_annots(doc, page_index)))
            .unwrap_or_default();
        with_doc(&path, |doc| {
            let page = doc.pages().get(page_index).map_err(|e| e.to_string())?;
            let geo = crate::Geo::de_pagina(&page);
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
                // un campo de firma NO es un campo que se rellene: con él
                // en la lista, un PDF que solo lleva una firma se anunciaba
                // como «este documento se puede rellenar», justo cuando lo
                // que el usuario está mirando es la firma. Acrobat también
                // los separa.
                if field.field_type() == PdfFormFieldType::Signature {
                    continue;
                }
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
                    // desplegables y listas: el valor elegido, y sus
                    // opciones aparte
                    PdfFormFieldType::ComboBox => (
                        field
                            .as_combo_box_field()
                            .and_then(|c| c.value())
                            .unwrap_or_default(),
                        false,
                    ),
                    PdfFormFieldType::ListBox => (
                        field
                            .as_list_box_field()
                            .and_then(|l| l.value())
                            .unwrap_or_default(),
                        false,
                    ),
                    _ => (String::new(), false),
                };
                let options: Vec<String> = match field.field_type() {
                    PdfFormFieldType::ComboBox => field
                        .as_combo_box_field()
                        .map(|c| etiquetas(c.options()))
                        .unwrap_or_default(),
                    PdfFormFieldType::ListBox => field
                        .as_list_box_field()
                        .map(|l| etiquetas(l.options()))
                        .unwrap_or_default(),
                    _ => Vec::new(),
                };
                let caja = geo.pdf_rect_a_ui(&b);
                out.push(FormFieldInfo {
                    annot_index: i as u16,
                    name: field.name().unwrap_or_default(),
                    kind,
                    value,
                    checked,
                    options,
                    required: obligatorios
                        .get(i)
                        .is_some_and(|f| f & crate::formularios2::OBLIGATORIO != 0),
                    x: caja.x,
                    y: caja.y,
                    w: caja.w,
                    h: caja.h,
                });
            }
            Ok(out)
        })
    })
}

/// Escribe el valor de un campo de texto de formulario.
#[tauri::command(async)]
pub fn set_form_text(
    work_path: String,
    page_index: u16,
    annot_index: u16,
    value: String,
) -> Result<(), String> {
    mutacion(work_path, |work_path| on_pdfium_thread(move || {
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
    }))
}

/// El `/Ff` de cada anotación de la página, en el orden de `/Annots` (el
/// mismo `annot_index` que devuelve `get_form_fields`), heredándolo del
/// `/Parent` cuando el widget no lo lleva —que es justo el caso de los
/// grupos de radios, donde el campo es el padre—.
fn banderas_de_annots(doc: &lopdf::Document, page_index: u16) -> Vec<i64> {
    use lopdf::Object;
    let Some(lista) = crate::anotaciones::lista_annots(doc, page_index) else {
        return Vec::new();
    };
    lista
        .iter()
        .map(|o| {
            let Object::Reference(id) = o else { return 0 };
            let Ok(d) = doc.get_object(*id).and_then(|o| o.as_dict()) else {
                return 0;
            };
            if let Ok(ff) = d.get(b"Ff").and_then(|o| o.as_i64()) {
                return ff;
            }
            d.get(b"Parent")
                .and_then(|o| o.as_reference())
                .ok()
                .and_then(|p| doc.get_object(p).ok()?.as_dict().ok()?.get(b"Ff").ok()?.as_i64().ok())
                .unwrap_or(0)
        })
        .collect()
}

/// Etiquetas de las opciones de un desplegable o una lista, en su orden.
fn etiquetas(opciones: &PdfFormFieldOptions) -> Vec<String> {
    opciones
        .iter()
        .map(|o| o.label().cloned().unwrap_or_default())
        .collect()
}

/// Marca o desmarca una casilla, o elige un botón de radio.
///
/// **Los radios van por lopdf**, no por PDFium: un grupo de radios es un
/// solo campo con un `/Kids` por opción, y elegir uno es escribir el `/V`
/// del padre y el `/AS` de cada hijo (el elegido con su valor de
/// exportación, los hermanos a `/Off`). Dejárselo a PDFium hacía que
/// desmarcar los hermanos dependiera de cómo estuviera armado el
/// documento; así es explícito y se ve en los bytes.
#[tauri::command(async)]
pub fn set_form_checked(
    work_path: String,
    page_index: u16,
    annot_index: u16,
    checked: bool,
) -> Result<(), String> {
    let radio = {
        let w = work_path.clone();
        on_pdfium_thread(move || {
            crate::with_lopdf(&w, |doc| Ok(padre_de_radio(doc, page_index, annot_index)))
        })?
    };
    if let Some((widget_id, padre_id)) = radio {
        return elige_radio(work_path, widget_id, padre_id, checked);
    }
    mutacion(work_path, |work_path| on_pdfium_thread(move || {
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
    }))
}

/// Si el widget de esa posición es una opción de un grupo de radios,
/// devuelve `(widget, campo del grupo)`. El campo es el `/Parent` (los
/// hermanos comparten uno) o el propio widget cuando el grupo tiene una
/// sola opción y va todo en el mismo diccionario.
fn padre_de_radio(
    doc: &lopdf::Document,
    page_index: u16,
    annot_index: u16,
) -> Option<(lopdf::ObjectId, lopdf::ObjectId)> {
    use lopdf::Object;
    let lista = crate::anotaciones::lista_annots(doc, page_index)?;
    let Object::Reference(widget_id) = lista.get(annot_index as usize)? else {
        return None;
    };
    let w = doc.get_object(*widget_id).ok()?.as_dict().ok()?;
    let campo_id = match w.get(b"Parent") {
        Ok(Object::Reference(rid)) => *rid,
        _ => *widget_id,
    };
    let campo = doc.get_object(campo_id).ok()?.as_dict().ok()?;
    let es_radio = campo.get(b"FT").and_then(|o| o.as_name()).ok()? == b"Btn"
        && campo.get(b"Ff").and_then(|o| o.as_i64()).unwrap_or(0)
            & crate::formularios2::RADIO_FF
            != 0;
    es_radio.then_some((*widget_id, campo_id))
}

/// Escribe la elección de un grupo de radios: el `/V` del campo y el `/AS`
/// de cada hijo.
fn elige_radio(
    work_path: String,
    widget_id: lopdf::ObjectId,
    padre_id: lopdf::ObjectId,
    checked: bool,
) -> Result<(), String> {
    crate::cirugia(&work_path, move |doc| {
        use lopdf::Object;
        // el valor de exportación de esta opción: el estado de su /AP que
        // no es «Off»
        let export = doc
            .get_object(widget_id)
            .and_then(|o| o.as_dict())
            .ok()
            .and_then(|w| w.get(b"AP").ok().cloned())
            .and_then(|ap| match ap {
                Object::Dictionary(d) => d.get(b"N").ok().cloned(),
                Object::Reference(rid) => doc
                    .get_object(rid)
                    .ok()?
                    .as_dict()
                    .ok()?
                    .get(b"N")
                    .ok()
                    .cloned(),
                _ => None,
            })
            .and_then(|n| match n {
                Object::Dictionary(d) => d
                    .iter()
                    .map(|(k, _)| String::from_utf8_lossy(k).into_owned())
                    .find(|k| k != "Off"),
                _ => None,
            })
            .ok_or("Esa opción no tiene valor de exportación")?;

        let kids: Vec<lopdf::ObjectId> = doc
            .get_object(padre_id)
            .and_then(|o| o.as_dict())
            .ok()
            .and_then(|d| d.get(b"Kids").ok().cloned())
            .and_then(|k| match k {
                Object::Array(a) => Some(
                    a.iter()
                        .filter_map(|o| o.as_reference().ok())
                        .collect::<Vec<_>>(),
                ),
                _ => None,
            })
            .unwrap_or_else(|| vec![widget_id]);

        // los hermanos se apagan: es lo que distingue un grupo de radios de
        // tres casillas sueltas
        for kid in kids {
            let encendido = checked && kid == widget_id;
            let estado = if encendido { export.as_bytes().to_vec() } else { b"Off".to_vec() };
            if let Ok(d) = doc.get_object_mut(kid).and_then(|o| o.as_dict_mut()) {
                d.set("AS", Object::Name(estado));
            }
        }
        let v = if checked { export.as_bytes().to_vec() } else { b"Off".to_vec() };
        doc.get_object_mut(padre_id)
            .and_then(|o| o.as_dict_mut())
            .map_err(|e| e.to_string())?
            .set("V", Object::Name(v));
        crate::formularios2::pide_apariencias(doc)
    })
}

/// Elige una opción de un desplegable o una lista. `field_index` es el
/// `annot_index` que devuelve `get_form_fields`.
///
/// Va con lopdf: pdfium-render 0.8 lee las opciones pero no deja escribir el
/// valor. Se escribe `/V` (y `/I` en las listas, que es donde el spec quiere
/// el índice) y se enciende `NeedAppearances`, como ya hacen los campos
/// creados por Vitela, para que el visor vuelva a dibujar el campo.
#[tauri::command(async)]
pub fn set_form_choice(
    work_path: String,
    page_index: u16,
    field_index: u16,
    value: String,
) -> Result<(), String> {
    crate::cirugia(&work_path, move |doc| {
        use lopdf::Object;
        let lista = crate::anotaciones::lista_annots(doc, page_index)
            .ok_or("La página no tiene campos de formulario")?;
        let widget_id = match lista.get(field_index as usize) {
            Some(Object::Reference(rid)) => *rid,
            _ => return Err("Ese campo ya no está en la página".into()),
        };
        // el campo puede ser el propio widget o su padre (widgets hermanos)
        let campo_id = {
            let w = doc
                .get_object(widget_id)
                .and_then(|o| o.as_dict())
                .map_err(|e| e.to_string())?;
            if w.has(b"FT") {
                widget_id
            } else {
                w.get(b"Parent")
                    .and_then(|o| o.as_reference())
                    .map_err(|_| "Ese campo no es un desplegable".to_string())?
            }
        };
        let campo = doc
            .get_object(campo_id)
            .and_then(|o| o.as_dict())
            .map_err(|e| e.to_string())?
            .clone();
        if campo.get(b"FT").and_then(|o| o.as_name()).unwrap_or_default() != b"Ch" {
            return Err("Ese campo no es un desplegable ni una lista".into());
        }
        let opciones = campo
            .get(b"Opt")
            .and_then(|o| o.as_array())
            .map_err(|_| "El desplegable no tiene opciones".to_string())?
            .clone();
        // cada /Opt es una cadena o [exportación, etiqueta]
        let par = |o: &Object| -> (String, String) {
            match o {
                Object::Array(a) if a.len() >= 2 => (
                    crate::anotaciones::texto_de_cadena_pdf(&a[0]),
                    crate::anotaciones::texto_de_cadena_pdf(&a[1]),
                ),
                otro => {
                    let t = crate::anotaciones::texto_de_cadena_pdf(otro);
                    (t.clone(), t)
                }
            }
        };
        let elegido = opciones
            .iter()
            .enumerate()
            .find(|(_, o)| {
                let (exportacion, etiqueta) = par(o);
                etiqueta == value || exportacion == value
            })
            .map(|(i, o)| (i, par(o).0))
            .ok_or_else(|| format!("«{value}» no es una de las opciones"))?;
        // bit 18 de /Ff (131072): desplegable; sin él, lista
        let es_lista = campo
            .get(b"Ff")
            .and_then(|o| o.as_i64())
            .map(|f| f & 131_072 == 0)
            .unwrap_or(true);
        {
            let campo = doc
                .get_object_mut(campo_id)
                .and_then(|o| o.as_dict_mut())
                .map_err(|e| e.to_string())?;
            campo.set("V", crate::documento::cadena_pdf(&elegido.1));
            if es_lista {
                campo.set("I", Object::Array(vec![Object::Integer(elegido.0 as i64)]));
            } else {
                campo.remove(b"I");
            }
        }
        // la apariencia guardada es la del valor viejo
        if let Ok(w) = doc.get_object_mut(widget_id).and_then(|o| o.as_dict_mut()) {
            w.remove(b"AP");
        }
        crate::formularios2::pide_apariencias(doc)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[allow(unused_imports)]
    use crate::tests::{crea_pdf, textos_de};
    #[allow(unused_imports)]
    use crate::{get_page_sizes, open_pdf, render_page_b64, render_page_png};

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
        escribe_pdf(&objs, dest);
    }

    /// Serializa una lista de objetos numerados como un PDF con su xref.
    fn escribe_pdf(objs: &[(u32, String)], dest: &std::path::Path) {
        let mut out: Vec<u8> = b"%PDF-1.7\n".to_vec();
        let mut offsets = vec![0usize; objs.len() + 1];
        for (num, body) in objs {
            offsets[*num as usize] = out.len();
            out.extend_from_slice(format!("{num} 0 obj\n{body}\nendobj\n").as_bytes());
        }
        let xref_pos = out.len();
        out.extend_from_slice(format!("xref\n0 {}\n", objs.len() + 1).as_bytes());
        out.extend_from_slice(b"0000000000 65535 f \n");
        for off in offsets.iter().skip(1).take(objs.len()) {
            out.extend_from_slice(format!("{off:010} 00000 n \n").as_bytes());
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

    /// PDF con un desplegable (ComboBox) sin valor y una lista (ListBox) con
    /// «Verde» elegido. PDFium no crea campos: se escribe a mano.
    fn crea_pdf_desplegables(dest: &std::path::Path) {
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
                "<</Type/Annot/Subtype/Widget/FT/Ch/Ff 131072/T(ciudad)\
                 /Rect[50 600 250 620]/F 4/DA(/Helv 12 Tf 0 g)\
                 /Opt[(Madrid)(Barcelona)(Sevilla)]>>"
                    .into(),
            ),
            (
                5,
                "<</Type/Annot/Subtype/Widget/FT/Ch/T(color)\
                 /Rect[50 500 250 560]/F 4/DA(/Helv 12 Tf 0 g)\
                 /Opt[(Rojo)(Verde)(Azul)]/V(Verde)/I[1]>>"
                    .into(),
            ),
            (6, "<</Type/Font/Subtype/Type1/BaseFont/Helvetica>>".into()),
        ];
        escribe_pdf(&objs, dest);
    }

    /// Un PDF rellenable se nota que lo es: el desplegable ofrece las
    /// opciones del documento y lo elegido sobrevive a guardar.
    #[test]
    fn desplegables_y_listas() {
        let tmp = std::env::temp_dir().join("editor_pdf_test_desplegable.pdf");
        crea_pdf_desplegables(&tmp);
        let work = tmp.to_string_lossy().into_owned();

        let campos = get_form_fields(work.clone(), 0).expect("listar");
        let ciudad = campos.iter().find(|c| c.name == "ciudad").expect("desplegable");
        assert_eq!(ciudad.kind, "ComboBox");
        assert_eq!(ciudad.options, vec!["Madrid", "Barcelona", "Sevilla"]);
        assert_eq!(ciudad.value, "");
        let color = campos.iter().find(|c| c.name == "color").expect("lista");
        assert_eq!(color.kind, "ListBox");
        assert_eq!(color.options, vec!["Rojo", "Verde", "Azul"]);
        assert_eq!(color.value, "Verde");

        // sin valor, el desplegable está en blanco
        let tinta = |w: &str| {
            let png = render_page_png(w.to_string(), 0, 600, true).expect("render");
            let img = image::load_from_memory(&png).expect("PNG").to_rgba8();
            let escala = 600.0 / 595.0;
            let mut n = 0;
            for y in (222.0 * escala) as u32..(242.0 * escala) as u32 {
                for x in (52.0 * escala) as u32..(248.0 * escala) as u32 {
                    let p = img.get_pixel(x, y).0;
                    if p[0] < 200 && p[1] < 200 && p[2] < 200 {
                        n += 1;
                    }
                }
            }
            n
        };
        let vacio = tinta(&work);

        set_form_choice(work.clone(), 0, ciudad.annot_index, "Sevilla".into())
            .expect("elegir ciudad");
        set_form_choice(work.clone(), 0, color.annot_index, "Azul".into()).expect("elegir color");

        let campos = get_form_fields(work.clone(), 0).expect("relistar");
        assert_eq!(
            campos.iter().find(|c| c.name == "ciudad").unwrap().value,
            "Sevilla"
        );
        assert_eq!(
            campos.iter().find(|c| c.name == "color").unwrap().value,
            "Azul"
        );
        assert!(
            tinta(&work) > vacio,
            "lo elegido tiene que verse en el render ({vacio} píxeles antes, {} después)",
            tinta(&work)
        );

        // el índice de la lista acompaña al valor, como pide el spec
        let doc = lopdf::Document::load(&work).expect("cargar");
        let lista = doc
            .objects
            .values()
            .filter_map(|o| o.as_dict().ok())
            .find(|d| {
                matches!(d.get(b"T"), Ok(lopdf::Object::String(t, _)) if t == b"color")
            })
            .expect("campo color");
        assert_eq!(
            lista.get(b"I").and_then(|o| o.as_array()).expect("/I"),
            &vec![lopdf::Object::Integer(2)]
        );

        // una opción que no está no se acepta
        assert!(set_form_choice(work.clone(), 0, ciudad.annot_index, "Bilbao".into()).is_err());
        std::fs::remove_file(&tmp).ok();
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

        render_page_b64(work.clone(), 0, 200, None).expect("render con formulario");
        std::fs::remove_file(&tmp).ok();
    }
}
