//! Formularios AcroForm: leer campos y rellenar texto y casillas.

use crate::{on_pdfium_thread, pdfium, save_and_close, with_doc};
use pdfium_render::prelude::*;
use serde::Serialize;

#[derive(Serialize)]
pub struct FormFieldInfo {
    pub annot_index: u16,
    pub name: String,
    pub kind: String,
    pub value: String,
    pub checked: bool,
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
#[tauri::command(async)]
pub fn set_form_text(
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
#[tauri::command(async)]
pub fn set_form_checked(
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

        render_page_b64(work.clone(), 0, 200).expect("render con formulario");
        std::fs::remove_file(&tmp).ok();
    }
}
