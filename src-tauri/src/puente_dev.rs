//! Puente HTTP de desarrollo para QA "como usuario real": expone los
//! comandos de la app por HTTP para que la UI servida por Vite
//! (http://localhost:1420) funcione entera en un navegador normal, donde no
//! existe el IPC de Tauri. Solo se compila en debug.
//!
//! Sesión de QA típica:
//! ```text
//! Terminal 1: bun run dev          # vite en :1420
//! Terminal 2: bun run qa:puente    # este puente + PDFium en :1422, sin ventana
//!
//! curl -X POST localhost:1422/qa/fixture -d '{"pages":["Uno","Dos","Tres"]}'
//!   → {"path":"/tmp/vitela-fixture-….pdf"}
//! curl -X POST localhost:1422/qa/dialogo -d '{"value":"<esa ruta>"}'
//!   → encola la respuesta del siguiente diálogo de abrir/guardar
//! browse goto http://localhost:1420 ; click en "Abrir PDF" …
//! ```
//! Los diálogos del sistema no existen en el navegador: el shim de
//! `src/dialogos.ts` pide al puente la siguiente respuesta encolada
//! (cola vacía → null = usuario canceló).

use serde_json::{json, Value};
use std::collections::VecDeque;
use std::sync::Mutex;

static COLA_DIALOGOS: Mutex<VecDeque<Value>> = Mutex::new(VecDeque::new());

/// Puerto del puente (EDITOR_PDF_PUENTE_PUERTO o 1422; el 1421 lo usa el
/// HMR de Vite).
pub fn puerto() -> u16 {
    std::env::var("EDITOR_PDF_PUENTE_PUERTO")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(1422)
}

/// Arranque para el binario `puente`: fija el directorio de datos a temp
/// (biblioteca de firmas aislada de la real) y sirve en primer plano.
pub fn arrancar_bin() {
    let datos = std::env::temp_dir().join("editor_pdf_qa_datos");
    let _ = std::fs::create_dir_all(&datos);
    let _ = crate::firmas_visuales::DIR_DATOS.set(datos);
    arrancar(puerto());
}

/// Sirve el puente en 127.0.0.1:puerto (bloquea el hilo actual).
pub fn arrancar(puerto: u16) {
    let server = match tiny_http::Server::http(("127.0.0.1", puerto)) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("[puente] no se pudo escuchar en 127.0.0.1:{puerto}: {e}");
            return;
        }
    };
    eprintln!("[puente] listo en http://127.0.0.1:{puerto} (salud: GET /salud)");
    for request in server.incoming_requests() {
        std::thread::spawn(move || atender(request));
    }
}

fn con_cors(mut r: tiny_http::Response<std::io::Cursor<Vec<u8>>>) -> tiny_http::Response<std::io::Cursor<Vec<u8>>> {
    for (k, v) in [
        ("Access-Control-Allow-Origin", "*"),
        ("Access-Control-Allow-Headers", "content-type"),
        ("Access-Control-Allow-Methods", "POST, GET, OPTIONS"),
        ("Content-Type", "application/json"),
    ] {
        r = r.with_header(tiny_http::Header::from_bytes(k.as_bytes(), v.as_bytes()).unwrap());
    }
    r
}

fn responde(request: tiny_http::Request, codigo: u16, body: Value) {
    let r = tiny_http::Response::from_string(body.to_string()).with_status_code(codigo);
    let _ = request.respond(con_cors(r));
}

fn atender(mut request: tiny_http::Request) {
    if request.method() == &tiny_http::Method::Options {
        return responde(request, 204, json!({}));
    }
    let mut body_raw = String::new();
    let _ = std::io::Read::read_to_string(request.as_reader(), &mut body_raw);
    let body: Value = if body_raw.trim().is_empty() {
        json!({})
    } else {
        match serde_json::from_str(&body_raw) {
            Ok(v) => v,
            Err(e) => return responde(request, 400, json!({"error": format!("JSON inválido: {e}")})),
        }
    };
    let url = request.url().to_string();
    match url.as_str() {
        "/salud" => responde(request, 200, json!({"ok": true})),
        "/qa/dialogo" => {
            let value = body.get("value").cloned().unwrap_or(Value::Null);
            let mut cola = COLA_DIALOGOS.lock().unwrap();
            cola.push_back(value);
            let n = cola.len();
            responde(request, 200, json!({"pendientes": n}))
        }
        "/qa/dialogo/siguiente" => {
            let value = COLA_DIALOGOS
                .lock()
                .unwrap()
                .pop_front()
                .unwrap_or(Value::Null);
            responde(request, 200, json!({"value": value}))
        }
        "/qa/fixture" => {
            let paginas: Vec<String> = body
                .get("pages")
                .and_then(|p| serde_json::from_value(p.clone()).ok())
                .unwrap_or_else(|| vec!["Página de prueba".into()]);
            let refs: Vec<&str> = paginas.iter().map(String::as_str).collect();
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0);
            let path = std::env::temp_dir().join(format!("vitela-fixture-{nanos}.pdf"));
            crate::crea_pdf(&refs, &path);
            responde(request, 200, json!({"path": path.to_string_lossy()}))
        }
        _ => {
            if let Some(cmd) = url.strip_prefix("/invoke/") {
                match despachar(cmd, body) {
                    Ok(v) => responde(request, 200, v),
                    Err(e) => responde(request, 400, json!({"error": e})),
                }
            } else {
                responde(request, 404, json!({"error": format!("Ruta desconocida: {url}")}))
            }
        }
    }
}

/// Despacha un comando por nombre con los mismos argumentos camelCase que
/// envía la UI. Mantener sincronizado con `generate_handler!` de lib.rs.
pub(crate) fn despachar(cmd: &str, body: Value) -> Result<Value, String> {
    macro_rules! cmd {
        ($f:path, { $($n:ident : $t:ty),* $(,)? }) => {{
            #[derive(serde::Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct Args {
                $($n: $t),*
            }
            let a: Args = serde_json::from_value(body)
                .map_err(|e| format!("argumentos inválidos para {}: {e}", stringify!($f)))?;
            $f($(a.$n),*)
                .and_then(|v| serde_json::to_value(v).map_err(|e| e.to_string()))
        }};
    }
    use crate::{
        anotaciones, anotaciones2, busqueda, documento, exportar, firma, firmas_visuales, formularios, historial,
        imagenes, paginas, paginas2, seguridad, seguridad2, texto,
    };
    match cmd {
        "open_pdf" => cmd!(crate::open_pdf, { path: String, password: Option<String> }),
        "render_page" => cmd!(crate::render_page_b64, { path: String, page_index: u16, width: i32, with_annotations: Option<bool> }),
        "get_page_text" => cmd!(busqueda::get_page_text, { path: String, page_index: u16 }),
        "get_page_sizes" => cmd!(crate::get_page_sizes, { path: String }),
        "search_pdf" => cmd!(busqueda::search_pdf, { path: String, query: String, match_case: Option<bool>, whole_word: Option<bool>, context: Option<bool> }),
        "delete_page" => cmd!(paginas::delete_page, { work_path: String, page_index: u16 }),
        "rotate_page" => cmd!(paginas::rotate_page, { work_path: String, page_index: u16 }),
        "move_page" => cmd!(paginas::move_page, { work_path: String, from_index: u16, to_index: u16 }),
        "merge_pdf" => cmd!(paginas::merge_pdf, { work_path: String, other_path: String }),
        "extract_pages" => cmd!(paginas::extract_pages, { work_path: String, page_indices: Vec<u16>, dest_path: String, delete_after: Option<bool> }),
        "extract_each_page" => cmd!(paginas::extract_each_page, { work_path: String, page_indices: Vec<u16>, dest_dir: String, delete_after: Option<bool> }),
        "delete_pages" => cmd!(paginas::delete_pages, { work_path: String, page_indices: Vec<u16> }),
        "rotate_pages" => cmd!(paginas::rotate_pages, { work_path: String, page_indices: Vec<u16>, quarter_turns: i8 }),
        "save_pdf" => cmd!(crate::save_pdf, { work_path: String, dest_path: String }),
        "add_stroke" => cmd!(anotaciones::add_stroke, { work_path: String, page_index: u16, points: Vec<[f32; 2]>, color: Option<[u8; 4]>, width: Option<f32>, author: Option<String> }),
        "add_note" => cmd!(anotaciones::add_note, { work_path: String, page_index: u16, x: f32, y: f32, text: String, author: Option<String> }),
        "get_annotations" => cmd!(anotaciones::get_annotations, { path: String, page_index: u16 }),
        "remove_annotation" => cmd!(anotaciones::remove_annotation, { work_path: String, page_index: u16, annot_index: u16 }),
        "set_annotation_contents" => cmd!(anotaciones::set_annotation_contents, { work_path: String, page_index: u16, annot_index: u16, contents: String, author: Option<String> }),
        "set_annotation_color" => cmd!(anotaciones::set_annotation_color, { work_path: String, page_index: u16, annot_index: u16, color: [u8; 4] }),
        "get_document_annotations" => cmd!(anotaciones::get_document_annotations, { path: String }),
        "reply_annotation" => cmd!(crate::comentarios::reply_annotation, { work_path: String, page_index: u16, annot_index: u16, text: String, author: Option<String> }),
        "set_annotation_state" => cmd!(crate::comentarios::set_annotation_state, { work_path: String, page_index: u16, annot_index: u16, state: String, author: Option<String> }),
        "export_comments" => cmd!(crate::comentarios::export_comments, { path: String, dest_path: String, formato: String }),
        "get_form_fields" => cmd!(formularios::get_form_fields, { path: String, page_index: u16 }),
        "set_form_text" => cmd!(formularios::set_form_text, { work_path: String, page_index: u16, annot_index: u16, value: String }),
        "set_form_checked" => cmd!(formularios::set_form_checked, { work_path: String, page_index: u16, annot_index: u16, checked: bool }),
        "set_form_choice" => cmd!(formularios::set_form_choice, { work_path: String, page_index: u16, field_index: u16, value: String }),
        "get_text_blocks" => cmd!(texto::get_text_blocks, { path: String, page_index: u16 }),
        "replace_text" => cmd!(texto::replace_text, { work_path: String, matches: Vec<texto::Reemplazo> }),
        "edit_text_block" => cmd!(texto::edit_text_block, { work_path: String, page_index: u16, object_index: u32, new_text: String, color: Option<[u8; 4]>, align: Option<String>, line_height: Option<f32>, char_spacing: Option<f32> }),
        "add_text_block" => cmd!(texto::add_text_block, { work_path: String, page_index: u16, x: f32, y: f32, text: String, font_size: f32, font: Option<String>, color: Option<[u8; 4]>, align: Option<String>, line_height: Option<f32>, char_spacing: Option<f32> }),
        "delete_text_block" => cmd!(texto::delete_text_block, { work_path: String, page_index: u16, object_index: u32 }),
        "move_text_block" => cmd!(texto::move_text_block, { work_path: String, page_index: u16, object_index: u32, x: f32, y: f32 }),
        "resize_text_block" => cmd!(texto::resize_text_block, { work_path: String, page_index: u16, object_index: u32, w: f32, h: f32 }),
        "get_images" => cmd!(imagenes::get_images, { path: String, page_index: u16 }),
        "add_image" => cmd!(imagenes::add_image, { work_path: String, page_index: u16, image_path: String, x: f32, y: f32 }),
        "transform_image" => cmd!(imagenes::transform_image, { work_path: String, page_index: u16, object_index: u32, x: f32, y: f32, w: f32, h: f32, rotate: Option<i16>, flip_h: Option<bool>, flip_v: Option<bool> }),
        "reorder_image" => cmd!(imagenes::reorder_image, { work_path: String, page_index: u16, object_index: u32, al_frente: bool }),
        "replace_image" => cmd!(imagenes::replace_image, { work_path: String, page_index: u16, object_index: u32, image_path: String }),
        "crop_image" => cmd!(imagenes::crop_image, { work_path: String, page_index: u16, object_index: u32, rect: crate::Rect }),
        "delete_image" => cmd!(imagenes::delete_image, { work_path: String, page_index: u16, object_index: u32 }),
        "sign_pdf" => cmd!(crate::sign_pdf, { work_path: String, dest_path: String, cert_pem_path: String, key_pem_path: String, reason: Option<String>, rect: Option<crate::Rect>, page_index: Option<u16>, signer_name: Option<String>, signature_png: Option<String> }),
        "sign_pdf_p12" => cmd!(crate::sign_pdf_p12, { work_path: String, dest_path: String, p12_path: String, password: String, reason: Option<String>, rect: Option<crate::Rect>, page_index: Option<u16>, signer_name: Option<String>, signature_png: Option<String> }),
        "verify_signatures" => cmd!(firma::verify_signatures, { path: String }),
        "stamp_signature" => cmd!(firmas_visuales::stamp_signature, { work_path: String, page_index: u16, png_base64: String, x: f32, y: f32, w: f32, h: f32 }),
        "import_signature_file" => cmd!(firmas_visuales::import_signature_file, { image_path: String }),
        "save_stored_signature" => cmd!(firmas_visuales::save_stored_signature, { name: String, png_base64: String }),
        "list_stored_signatures" => firmas_visuales::list_stored_signatures()
            .and_then(|v| serde_json::to_value(v).map_err(|e| e.to_string())),
        "delete_stored_signature" => cmd!(firmas_visuales::delete_stored_signature, { id: String }),
        "get_image_data" => cmd!(imagenes::get_image_data, { path: String, page_index: u16, object_index: u32 }),
        "add_markup" => cmd!(anotaciones2::add_markup, { work_path: String, page_index: u16, rects: Vec<crate::Rect>, kind: String, color: Option<[u8; 4]>, author: Option<String> }),
        "add_shape" => cmd!(anotaciones2::add_shape, { work_path: String, page_index: u16, kind: String, x1: f32, y1: f32, x2: f32, y2: f32, stroke: [u8; 4], fill: Option<[u8; 4]>, stroke_width: f32, author: Option<String> }),
        "add_stamp" => cmd!(anotaciones2::add_stamp, { work_path: String, page_index: u16, text: String, color: [u8; 4], x: f32, y: f32, font_size: f32, author: Option<String> }),
        "add_free_text" => cmd!(anotaciones2::add_free_text, { work_path: String, page_index: u16, rect: crate::Rect, text: String, font_size: f32, color: [u8; 4], border: bool, author: Option<String> }),
        "transform_annotation" => cmd!(anotaciones2::transform_annotation, { work_path: String, page_index: u16, annot_index: u16, x: f32, y: f32, w: f32, h: f32 }),
        "add_blank_page" => cmd!(paginas2::add_blank_page, { work_path: String, index: u16 }),
        "pdf_from_images" => cmd!(paginas2::pdf_from_images, { image_paths: Vec<String>, dest_path: String, tamano: String }),
        "duplicate_page" => cmd!(paginas2::duplicate_page, { work_path: String, page_index: u16 }),
        "insert_pdf_at" => cmd!(paginas2::insert_pdf_at, { work_path: String, other_path: String, index: u16 }),
        "replace_pages" => cmd!(paginas2::replace_pages, { work_path: String, page_indices: Vec<u16>, other_path: String, other_indices: Option<Vec<u16>> }),
        "split_pdf" => cmd!(paginas2::split_pdf, { work_path: String, dest_dir: String, modo: String, cada: Option<u16> }),
        "merge_many" => cmd!(paginas2::merge_many, { work_path: String, others: Vec<String>, at: Option<u16> }),
        "crop_page" => cmd!(paginas2::crop_page, { work_path: String, page_index: u16, rect: crate::Rect, all_pages: bool }),
        "add_watermark" => cmd!(paginas2::add_watermark, { work_path: String, text: String, font_size: f32, color: [u8; 4], diagonal: bool, position: Option<String>, page_indices: Option<Vec<u16>>, image_png: Option<String>, opacity: Option<f32>, rotation: Option<f32> }),
        "remove_marginal_text" => cmd!(paginas2::remove_marginal_text, { work_path: String, zona: String, dry_run: bool }),
        "add_header_footer" => cmd!(paginas2::add_header_footer, { work_path: String, header_left: Option<String>, header_center: Option<String>, header_right: Option<String>, footer_left: Option<String>, footer_center: Option<String>, footer_right: Option<String>, font_size: f32, page_indices: Option<Vec<u16>> }),
        "get_outline" => cmd!(documento::get_outline, { path: String }),
        "set_outline" => cmd!(documento::set_outline, { work_path: String, nodes: Vec<documento::OutlineNode> }),
        "pdf_info" => cmd!(documento::pdf_info, { path: String }),
        "get_metadata" => cmd!(documento::get_metadata, { path: String }),
        "set_metadata" => cmd!(documento::set_metadata, { work_path: String, meta: documento::Metadata }),
        "get_links" => cmd!(documento::get_links, { path: String, page_index: u16 }),
        "encrypt_pdf" => cmd!(seguridad::encrypt_pdf, { work_path: String, dest_path: Option<String>, user_password: String, owner_password: Option<String>, permisos: Option<seguridad::Permisos> }),
        "remove_encryption" => cmd!(seguridad::remove_encryption, { work_path: String }),
        "flatten_pdf" => cmd!(seguridad::flatten_pdf, { work_path: String }),
        "redact_area" => cmd!(seguridad::redact_area, { work_path: String, page_index: u16, rect: crate::Rect, dry_run: bool }),
        "mark_redaction" => cmd!(seguridad2::mark_redaction, { work_path: String, page_index: u16, rect: crate::Rect }),
        "list_redactions" => cmd!(seguridad2::list_redactions, { work_path: String }),
        "unmark_redaction" => cmd!(seguridad2::unmark_redaction, { work_path: String, page_index: u16, annot_index: u16 }),
        "unmark_all_redactions" => cmd!(seguridad2::unmark_all_redactions, { work_path: String }),
        "apply_redactions" => cmd!(seguridad2::apply_redactions, { work_path: String, dry_run: bool }),
        "sanitize_pdf" => cmd!(seguridad2::sanitize_pdf, { work_path: String, dry_run: bool }),
        "list_attachments" => cmd!(crate::adjuntos::list_attachments, { path: String }),
        "save_attachment" => cmd!(crate::adjuntos::save_attachment, { path: String, index: u16, dest_path: String }),
        "add_attachment" => cmd!(crate::adjuntos::add_attachment, { work_path: String, file_path: String, description: Option<String> }),
        "list_layers" => cmd!(crate::adjuntos::list_layers, { path: String }),
        "set_layer_visible" => cmd!(crate::adjuntos::set_layer_visible, { work_path: String, index: u16, visible: bool }),
        "export_pages_png" => cmd!(exportar::export_pages_png, { path: String, dest_dir: String, dpi: u16, format: String }),
        "export_text" => cmd!(exportar::export_text, { path: String, dest_path: String }),
        "export_docx" => cmd!(exportar::export_docx, { work_path: String, dest_path: String, page_indices: Option<Vec<u16>> }),
        "compress_pdf" => cmd!(exportar::compress_pdf, { work_path: String, quality: u8, max_dpi: u16 }),
        "create_form_field" => cmd!(crate::formularios2::create_form_field, { work_path: String, page_index: u16, kind: String, rect: crate::Rect, name: String }),
        "delete_form_field" => cmd!(crate::formularios2::delete_form_field, { work_path: String, name: String }),
        "create_link" => cmd!(crate::formularios2::create_link, { work_path: String, page_index: u16, rect: crate::Rect, uri: Option<String>, dest_page: Option<u16> }),
        "close_document" => cmd!(crate::close_document, { work_path: String }),
        "undo" => cmd!(historial::undo, { work_path: String }),
        "redo" => cmd!(historial::redo, { work_path: String }),
        "history_state" => cmd!(historial::history_state, { work_path: String }),
        "squash_history" => cmd!(historial::squash_history, { work_path: String, steps: u16 }),
        "autosave_state" => cmd!(crate::recuperacion::autosave_state, { work_path: String, original_path: Option<String>, modified: bool }),
        "borra_sesion" => cmd!(crate::recuperacion::borra_sesion, { work_path: Option<String> }),
        "recover_session" => crate::recuperacion::recover_session()
            .and_then(|v| serde_json::to_value(v).map_err(|e| e.to_string())),
        "list_recent" => crate::recientes::list_recent()
            .and_then(|v| serde_json::to_value(v).map_err(|e| e.to_string())),
        "touch_recent" => cmd!(crate::recientes::touch_recent, { path: String }),
        "remove_recent" => cmd!(crate::recientes::remove_recent, { path: String }),
        // en el navegador de QA no hay ventana que cerrar
        "confirmar_cierre" => Ok(Value::Null),
        // ni ficheros pendientes del Finder
        "ui_lista" => Ok(Value::Null),
        // ni menú del sistema que atenuar
        "set_menu_state" => Ok(Value::Null),
        otro => Err(format!("Comando desconocido en el puente: {otro}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn despacho_abre_y_renderiza() {
        let pdf = std::env::temp_dir().join("puente-despacho-test.pdf");
        crate::crea_pdf(&["Hola puente"], &pdf);
        let info = despachar(
            "open_pdf",
            json!({"path": pdf.to_string_lossy(), "password": null}),
        )
        .expect("open_pdf");
        assert_eq!(info["page_count"], 1);
        let work = info["work_path"].as_str().expect("work_path");
        let png = despachar(
            "render_page",
            json!({"path": work, "pageIndex": 0, "width": 200}),
        )
        .expect("render_page");
        crate::close_document(work.to_string()).expect("cerrar");
        assert!(png.as_str().unwrap().len() > 100);
        // comando desconocido: error claro con el nombre
        let err = despachar("no_existe", json!({})).unwrap_err();
        assert!(err.contains("no_existe"));
    }

    #[test]
    fn cola_de_dialogos_fifo_y_vacia() {
        {
            let mut cola = COLA_DIALOGOS.lock().unwrap();
            cola.clear();
            cola.push_back(json!("/tmp/a.pdf"));
            cola.push_back(json!(null));
        }
        assert_eq!(
            COLA_DIALOGOS.lock().unwrap().pop_front(),
            Some(json!("/tmp/a.pdf"))
        );
        assert_eq!(COLA_DIALOGOS.lock().unwrap().pop_front(), Some(json!(null)));
        assert_eq!(COLA_DIALOGOS.lock().unwrap().pop_front(), None);
    }

    /// El fuente sin comentarios (`//…` y `/*…*/`), respetando lo que vaya
    /// entre comillas. Todo lo que este test lee —llamadas, tipos y objetos
    /// literales— se analiza sobre el texto limpio: una coma o un `;`
    /// dentro de un comentario partía los bloques por la mitad.
    fn sin_comentarios(texto: &str) -> String {
        let b = texto.as_bytes();
        let mut out: Vec<u8> = Vec::with_capacity(b.len());
        let mut comilla: Option<u8> = None;
        let mut i = 0;
        while i < b.len() {
            let c = b[i];
            match comilla {
                Some(q) => {
                    out.push(c);
                    if c == b'\\' && i + 1 < b.len() {
                        out.push(b[i + 1]);
                        i += 2;
                        continue;
                    }
                    if c == q {
                        comilla = None;
                    }
                    i += 1;
                }
                None if c == b'"' || c == b'\'' || c == b'`' => {
                    comilla = Some(c);
                    out.push(c);
                    i += 1;
                }
                None if c == b'/' && b.get(i + 1) == Some(&b'/') => {
                    while i < b.len() && b[i] != b'\n' {
                        i += 1;
                    }
                }
                None if c == b'/' && b.get(i + 1) == Some(&b'*') => {
                    i += 2;
                    while i + 1 < b.len() && !(b[i] == b'*' && b[i + 1] == b'/') {
                        i += 1;
                    }
                    i = (i + 2).min(b.len());
                    out.push(b' ');
                }
                None => {
                    out.push(c);
                    i += 1;
                }
            }
        }
        String::from_utf8(out).unwrap_or_else(|_| texto.to_string())
    }

    /// El código de la UI (`src/`), leído entero: los ficheros `.ts` y
    /// `.tsx` con su ruta.
    fn fuentes_de_la_ui() -> Vec<(String, String)> {
        fn recorre(dir: &std::path::Path, out: &mut Vec<(String, String)>) {
            let Ok(entradas) = std::fs::read_dir(dir) else { return };
            for e in entradas.flatten() {
                let ruta = e.path();
                if ruta.is_dir() {
                    recorre(&ruta, out);
                } else if matches!(ruta.extension().and_then(|s| s.to_str()), Some("ts") | Some("tsx"))
                {
                    if let Ok(texto) = std::fs::read_to_string(&ruta) {
                        // sin comentarios: un `invoke` de ejemplo dentro de
                        // un doc-comment no es una llamada, y un `;` o una
                        // coma dentro de un comentario partía los tipos
                        out.push((ruta.to_string_lossy().into_owned(), sin_comentarios(&texto)));
                    }
                }
            }
        }
        let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../src");
        let mut out = Vec::new();
        recorre(&src, &mut out);
        assert!(!out.is_empty(), "no se ha encontrado el código de la UI en {src:?}");
        out
    }

    fn fuente(nombre: &str) -> String {
        let ruta = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src")
            .join(nombre);
        std::fs::read_to_string(&ruta).unwrap_or_else(|e| panic!("leer {ruta:?}: {e}"))
    }

    /// Los nombres del `generate_handler![…]` de `lib.rs`, sin su módulo:
    /// eso es exactamente lo que la UI escribe en `invoke`.
    fn comandos_del_handler() -> Vec<String> {
        let lib = fuente("lib.rs");
        let desde = lib.find("generate_handler![").expect("lib.rs sin generate_handler!");
        let cuerpo = &lib[desde + "generate_handler![".len()..];
        let hasta = cuerpo.find(']').expect("generate_handler! sin cerrar");
        let mut nombres: Vec<String> = cuerpo[..hasta]
            .split(',')
            .map(|t| t.trim().rsplit("::").next().unwrap_or("").to_string())
            .filter(|t| !t.is_empty())
            .collect();
        nombres.sort();
        assert!(nombres.len() > 50, "el handler se ha leído a medias: {nombres:?}");
        nombres
    }

    /// Las ramas del `match` de [`despachar`], leídas del propio fuente.
    fn comandos_del_puente() -> Vec<String> {
        let yo = fuente("puente_dev.rs");
        let desde = yo.find("fn despachar").expect("sin despachar");
        let hasta = yo[desde..].find("\n#[cfg(test)]").map(|i| desde + i).unwrap_or(yo.len());
        let mut nombres: Vec<String> = yo[desde..hasta]
            .lines()
            .filter_map(|l| {
                let l = l.trim_start();
                let resto = l.strip_prefix('"')?;
                let (nombre, tras) = resto.split_once('"')?;
                tras.trim_start().starts_with("=>").then(|| nombre.to_string())
            })
            .collect();
        nombres.sort();
        nombres
    }

    /// Comandos registrados que la UI **todavía** no llama, con su motivo.
    /// La lista tiene que estar vacía al cerrar un ciclo: un comando que
    /// nadie llama es trabajo que no ha llegado al usuario. Se admite una
    /// excepción mientras las dos mitades se escriben en paralelo, y se
    /// quita al integrar.
    const NADIE_LLAMA: &[(&str, &str)] = &[
        ("pdf_from_images", "pendiente_ui: R26 lo lleva a Archivo y al estado vacío"),
    ];

    /// Comandos cuyos argumentos **no casan hoy** y su motivo. Cada entrada
    /// es una función rota que el usuario no puede usar, así que la lista
    /// tiene que quedar vacía: está aquí solo mientras el arreglo vive en
    /// la otra mitad.
    const ARGUMENTOS_PENDIENTES: &[(&str, &str)] = &[(
        "reorder_image",
        "pendiente_ui: `api.ts` manda `imageIndex` y el comando espera \
         `objectIndex`, como sus cinco hermanos de imagen; hasta que la UI lo \
         corrija, «Traer al frente» y «Enviar al fondo» no llegan al backend",
    )];

    /// Llamadas cuyos argumentos no son un objeto literal y el test no
    /// puede leer (`invoke("render_page", args, opts)`, que arma el objeto
    /// según las opciones). Se enumeran para que no crezcan en silencio.
    const SIN_LEER: &[&str] = &["render_page"];

    /// Un parámetro de un comando del backend: su nombre en snake_case y si
    /// es obligatorio (los `Option<T>` no lo son: un `invoke` que no mande
    /// el campo tiene que funcionar).
    struct Parametro {
        nombre: String,
        obligatorio: bool,
    }

    /// Los ficheros de Rust del core, con su texto.
    fn fuentes_del_core() -> Vec<String> {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut out = Vec::new();
        let Ok(entradas) = std::fs::read_dir(&dir) else {
            panic!("no se ha encontrado el core en {dir:?}")
        };
        for e in entradas.flatten() {
            let ruta = e.path();
            if ruta.extension().and_then(|s| s.to_str()) == Some("rs") {
                if let Ok(t) = std::fs::read_to_string(&ruta) {
                    out.push(t);
                }
            }
        }
        out
    }

    /// El trozo entre paréntesis (o llaves, o corchetes) que abre en `desde`,
    /// contando anidamiento y saltándose lo que haya dentro de comillas.
    fn hasta_cerrar(texto: &str, desde: usize, abre: char, cierra: char) -> Option<&str> {
        let bytes = texto.as_bytes();
        let mut nivel = 0i32;
        let mut i = desde;
        let mut comilla: Option<u8> = None;
        while i < bytes.len() {
            let b = bytes[i];
            match comilla {
                Some(c) => {
                    if b == b'\\' {
                        i += 2;
                        continue;
                    }
                    if b == c {
                        comilla = None;
                    }
                }
                None => {
                    if b == b'"' || b == b'\'' || b == b'`' {
                        comilla = Some(b);
                    } else if b as char == abre {
                        nivel += 1;
                    } else if b as char == cierra {
                        nivel -= 1;
                        if nivel == 0 {
                            return Some(&texto[desde + 1..i]);
                        }
                    }
                }
            }
            i += 1;
        }
        None
    }

    /// Parte una lista por el separador que se le diga, **al nivel de
    /// arriba**: lo que va dentro de paréntesis, llaves, corchetes o
    /// comillas no cuenta.
    fn trozos(texto: &str, separadores: &[char]) -> Vec<String> {
        let mut out = Vec::new();
        let mut actual = String::new();
        let mut nivel = 0i32;
        let mut comilla: Option<char> = None;
        let mut escapa = false;
        for c in texto.chars() {
            if let Some(q) = comilla {
                actual.push(c);
                if escapa {
                    escapa = false;
                } else if c == '\\' {
                    escapa = true;
                } else if c == q {
                    comilla = None;
                }
                continue;
            }
            match c {
                '"' | '\'' | '`' => {
                    comilla = Some(c);
                    actual.push(c);
                }
                '(' | '[' | '{' | '<' => {
                    nivel += 1;
                    actual.push(c);
                }
                ')' | ']' | '}' | '>' => {
                    nivel -= 1;
                    actual.push(c);
                }
                c if nivel == 0 && separadores.contains(&c) => {
                    out.push(actual.trim().to_string());
                    actual = String::new();
                }
                c => actual.push(c),
            }
        }
        if !actual.trim().is_empty() {
            out.push(actual.trim().to_string());
        }
        out.retain(|t| !t.is_empty());
        out
    }

    /// Los parámetros de cada `#[tauri::command]` del core, leídos del
    /// propio fuente. Los que inyecta Tauri (el `AppHandle`) no los manda la
    /// UI y no cuentan.
    fn parametros_de_los_comandos() -> std::collections::BTreeMap<String, Vec<Parametro>> {
        let mut out = std::collections::BTreeMap::new();
        for texto in fuentes_del_core() {
            let mut i = 0;
            while let Some(j) = texto[i..].find("#[tauri::command") {
                let j = i + j;
                i = j + 1;
                let Some(k) = texto[j..].find("fn ") else { continue };
                let k = j + k + "fn ".len();
                let Some(p) = texto[k..].find('(') else { continue };
                let nombre = texto[k..k + p].trim().to_string();
                let Some(dentro) = hasta_cerrar(&texto, k + p, '(', ')') else {
                    continue;
                };
                let params = trozos(dentro, &[','])
                    .into_iter()
                    .filter_map(|t| {
                        let (nombre, tipo) = t.split_once(':')?;
                        let tipo = tipo.trim();
                        if tipo.contains("AppHandle") || tipo.contains("Window") {
                            return None;
                        }
                        Some(Parametro {
                            nombre: nombre.trim().to_string(),
                            obligatorio: !tipo.starts_with("Option<"),
                        })
                    })
                    .collect();
                out.insert(nombre, params);
            }
        }
        assert!(
            out.len() > 50,
            "los comandos se han leído a medias: {}",
            out.len()
        );
        out
    }

    /// Las claves de un bloque `{ … }` de TypeScript, sea un tipo
    /// (`{ workPath: string; pageIndex?: number }`) o un objeto literal
    /// (`{ workPath, pageIndex: 0 }`). Devuelve también los `...spread` que
    /// no ha sabido resolver quien llama.
    fn claves_del_bloque(bloque: &str) -> (Vec<String>, Vec<String>) {
        let mut claves = Vec::new();
        let mut spreads = Vec::new();
        for t in trozos(bloque, &[',', ';']) {
            let t = t.trim();
            // los comentarios de línea de un tipo se cuelan en el trozo
            let t: String = t
                .lines()
                .map(str::trim)
                .filter(|l| !l.starts_with("//") && !l.starts_with('*') && !l.starts_with("/*"))
                .collect::<Vec<_>>()
                .join(" ");
            let t = t.trim();
            if let Some(nombre) = t.strip_prefix("...") {
                spreads.push(nombre.trim().to_string());
                continue;
            }
            let clave = t.split_once(':').map(|(k, _)| k).unwrap_or(t);
            let clave = clave.trim().trim_end_matches('?').trim();
            if clave.is_empty() || !clave.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                continue;
            }
            claves.push(clave.to_string());
        }
        (claves, spreads)
    }

    /// Resuelve un `...NOMBRE`: el objeto o el tipo con ese nombre declarado
    /// en el mismo fichero (`const SIN_APARIENCIA = { … }`,
    /// `type AparienciaFirma = { … }`), o el parámetro de la función que
    /// envuelve al `invoke` (`function x(args: { … } & Otro)`), que es el
    /// patrón de `api.ts`.
    fn resuelve_spread(texto: &str, antes_de: usize, nombre: &str) -> Option<Vec<String>> {
        // 1) el parámetro de la función que envuelve: el `nombre: {` más
        //    cercano hacia atrás
        let aguja = format!("{nombre}: {{");
        if let Some(pos) = texto[..antes_de].rfind(&aguja) {
            let abre = pos + aguja.len() - 1;
            if let Some(bloque) = hasta_cerrar(texto, abre, '{', '}') {
                let mut claves = claves_del_bloque(bloque).0;
                // `args: { … } & AparienciaFirma`: la intersección también
                // trae claves, y son las que faltaban
                let tras = &texto[abre + bloque.len() + 2..];
                let mut resto = tras.trim_start();
                while let Some(r) = resto.strip_prefix('&') {
                    let r = r.trim_start();
                    let fin = r
                        .find(|c: char| !c.is_ascii_alphanumeric() && c != '_')
                        .unwrap_or(r.len());
                    let tipo = &r[..fin];
                    if let Some(mas) = declaracion(texto, tipo) {
                        claves.extend(mas);
                    }
                    resto = r[fin..].trim_start();
                }
                return Some(claves);
            }
        }
        // 2) una declaración del fichero
        declaracion(texto, nombre)
    }

    /// Las claves del `type X = { … }` o `const X… = { … }` del fichero.
    fn declaracion(texto: &str, nombre: &str) -> Option<Vec<String>> {
        for aguja in [format!("type {nombre} ="), format!("const {nombre}")] {
            let Some(pos) = texto.find(&aguja) else { continue };
            let tras = &texto[pos..];
            let Some(abre) = tras.find('{') else { continue };
            // que la llave sea de esa declaración y no de la siguiente
            if tras[..abre].contains(';') {
                continue;
            }
            if let Some(bloque) = hasta_cerrar(tras, abre, '{', '}') {
                return Some(claves_del_bloque(bloque).0);
            }
        }
        None
    }

    /// De `pageIndex` a `page_index`: es lo que hace Tauri con los
    /// argumentos que llegan de JavaScript.
    fn a_snake(camel: &str) -> String {
        let mut out = String::new();
        for c in camel.chars() {
            if c.is_ascii_uppercase() {
                out.push('_');
                out.push(c.to_ascii_lowercase());
            } else {
                out.push(c);
            }
        }
        out
    }

    /// Una llamada `invoke("cmd", { … })` de la UI, con las claves que
    /// manda. `completa` es falso cuando hay un `...spread` que el test no
    /// ha sabido resolver: entonces se comprueba lo que se ve, pero no se
    /// puede exigir que estén todos los argumentos obligatorios.
    struct Llamada {
        comando: String,
        fichero: String,
        claves: Vec<String>,
        completa: bool,
    }

    /// Los `invoke("…", { … })` de la UI con sus argumentos.
    fn llamadas_de_la_ui(fuentes: &[(String, String)]) -> Vec<Llamada> {
        let mut fuera = Vec::new();
        for (fichero, texto) in fuentes {
            let bytes = texto.as_bytes();
            let mut i = 0;
            while let Some(j) = texto[i..].find("invoke") {
                let j = i + j;
                i = j + "invoke".len();
                // que sea la palabra `invoke` y no el final de otra
                if j > 0 && (bytes[j - 1].is_ascii_alphanumeric() || bytes[j - 1] == b'_') {
                    continue;
                }
                let mut k = i;
                // parámetro de tipo opcional: invoke<PageSize[]>(
                let resto = &texto[k..];
                if resto.starts_with('<') {
                    match resto.find('>') {
                        Some(cierre) => k += cierre + 1,
                        None => continue,
                    }
                }
                let resto = texto[k..].trim_start();
                let Some(tras) = resto.strip_prefix("(") else { continue };
                let tras = tras.trim_start();
                let Some(tras) = tras.strip_prefix('"') else { continue };
                let Some((comando, _)) = tras.split_once('"') else { continue };
                // dónde empieza el objeto de argumentos, si lo hay
                let pos_nombre = texto.len() - tras.len();
                let tras_nombre = &texto[pos_nombre + comando.len() + 1..];
                let despues = tras_nombre.trim_start();
                let (claves, completa) = match despues.strip_prefix(',') {
                    None => (Vec::new(), true),
                    Some(d) => {
                        let d = d.trim_start();
                        if !d.starts_with('{') {
                            // argumentos que no son un objeto literal: no se
                            // pueden leer, pero tampoco los hay en la UI
                            (Vec::new(), false)
                        } else {
                            let abre = texto.len() - d.len();
                            match hasta_cerrar(texto, abre, '{', '}') {
                                None => (Vec::new(), false),
                                Some(bloque) => {
                                    let (mut claves, spreads) = claves_del_bloque(bloque);
                                    let mut completa = true;
                                    for s in spreads {
                                        match resuelve_spread(texto, abre, &s) {
                                            Some(mas) => claves.extend(mas),
                                            None => completa = false,
                                        }
                                    }
                                    (claves, completa)
                                }
                            }
                        }
                    }
                };
                fuera.push(Llamada {
                    comando: comando.to_string(),
                    fichero: fichero.clone(),
                    claves,
                    completa,
                });
            }
        }
        fuera
    }

    /// **El segundo test cruzado** (la regla del programa: toda lista
    /// compartida entre las dos mitades se cruza a máquina). Tres listas que
    /// tienen que decir lo mismo: los comandos registrados en
    /// `generate_handler!`, las ramas del puente de QA y los `invoke("…")`
    /// de la UI.
    ///
    /// Falla si un comando está en el handler y no en el puente (QA en el
    /// navegador se queda sin esa función y nadie se entera hasta la sesión
    /// de pruebas), si está en el puente y no en el handler (en la app de
    /// verdad no existe) o si la UI llama a algo que no está registrado (un
    /// error en tiempo de ejecución que solo salta al pulsar ese botón).
    ///
    /// Desde el ciclo 6 falla también **un comando que nadie llama**: un
    /// `eprintln!` en un CI con salida larga no lo lee nadie, y por eso el
    /// ciclo 5 se cerró creyendo que «Crear PDF desde imágenes» estaba
    /// hecho. Lo que está a medio integrar va en `NADIE_LLAMA` con su
    /// motivo, y esa lista tiene que quedar vacía al cerrar el ciclo.
    #[test]
    fn los_comandos_estan_en_el_handler_en_el_puente_y_en_la_ui() {
        let handler = comandos_del_handler();
        let puente = comandos_del_puente();
        let fuentes = fuentes_de_la_ui();
        let llamadas = llamadas_de_la_ui(&fuentes);

        let sin_puente: Vec<&String> = handler.iter().filter(|c| !puente.contains(c)).collect();
        assert!(
            sin_puente.is_empty(),
            "estos comandos están en generate_handler! y no en puente_dev::despachar, \
             así que la sesión de QA en el navegador no los tiene: {sin_puente:?}"
        );
        let sin_handler: Vec<&String> = puente.iter().filter(|c| !handler.contains(c)).collect();
        assert!(
            sin_handler.is_empty(),
            "estos comandos están en puente_dev::despachar y no en generate_handler!, \
             así que en la app de verdad no existen: {sin_handler:?}"
        );

        let inventados: Vec<String> = llamadas
            .iter()
            .filter(|l| !handler.contains(&l.comando))
            .map(|l| format!("{} ({})", l.comando, l.fichero))
            .collect();
        assert!(
            inventados.is_empty(),
            "la UI llama a comandos que no existen en el backend: {inventados:?}"
        );

        let nadie: Vec<&String> = handler
            .iter()
            .filter(|c| !llamadas.iter().any(|l| &l.comando == *c))
            .filter(|c| !NADIE_LLAMA.iter().any(|(n, _)| *n == c.as_str()))
            .collect();
        assert!(
            nadie.is_empty(),
            "estos comandos están registrados y la UI no los llama, así que \
             el usuario no puede llegar a ellos: {nadie:?}. Si están a medio \
             integrar, van en NADIE_LLAMA con su motivo"
        );
        let sobra: Vec<&str> = NADIE_LLAMA
            .iter()
            .filter(|(n, _)| llamadas.iter().any(|l| l.comando == *n))
            .map(|(n, _)| *n)
            .collect();
        assert!(
            sobra.is_empty(),
            "la UI ya llama a estos comandos: fuera de NADIE_LLAMA {sobra:?}"
        );
        let fantasmas: Vec<&str> = NADIE_LLAMA
            .iter()
            .filter(|(n, _)| !handler.contains(&n.to_string()))
            .map(|(n, _)| *n)
            .collect();
        assert!(
            fantasmas.is_empty(),
            "NADIE_LLAMA nombra comandos que no existen: {fantasmas:?}"
        );
    }

    /// **R25.** El mismo cruce, un nivel más abajo: los **nombres de
    /// argumento**. El ciclo 5 integró media función de espaciado entre
    /// caracteres porque la UI mandaba `charSpacing` y el comando no lo
    /// tenía: Tauri descarta en silencio las claves que no conoce y el
    /// comando devuelve `Ok` sin hacer nada. Aquí se cruzan los parámetros
    /// de cada `#[tauri::command]` (en camelCase, que es como los manda
    /// JavaScript) con las claves del objeto de cada `invoke`.
    ///
    /// Los `Option<T>` no son obligatorios: un `invoke` que no mande el
    /// campo tiene que funcionar. Lo que Tauri inyecta (el `AppHandle`) no
    /// lo manda nadie y no cuenta.
    #[test]
    fn los_argumentos_de_cada_invoke_son_los_del_comando() {
        let comandos = parametros_de_los_comandos();
        let fuentes = fuentes_de_la_ui();
        let llamadas = llamadas_de_la_ui(&fuentes);
        assert!(llamadas.len() > 50, "se han leído {} invoke", llamadas.len());

        let ilegibles: Vec<&str> = llamadas
            .iter()
            .filter(|l| !l.completa && !SIN_LEER.contains(&l.comando.as_str()))
            .map(|l| l.comando.as_str())
            .collect();
        assert!(
            ilegibles.is_empty(),
            "estos `invoke` no mandan un objeto literal, así que este test no              los puede cruzar: {ilegibles:?}. Si tiene que ser así, van en              SIN_LEER; si no, el objeto se escribe en el propio `invoke`"
        );

        let mut fallos: Vec<String> = Vec::new();
        for l in &llamadas {
            let Some(params) = comandos.get(&l.comando) else { continue };
            if ARGUMENTOS_PENDIENTES.iter().any(|(c, _)| *c == l.comando) {
                continue;
            }
            let mandadas: Vec<String> = l.claves.iter().map(|c| a_snake(c)).collect();
            for clave in &mandadas {
                if !params.iter().any(|p| &p.nombre == clave) {
                    fallos.push(format!(
                        "{} manda `{clave}`, que {} no tiene (Tauri lo descarta en silencio) — {}",
                        l.comando, l.comando, l.fichero
                    ));
                }
            }
            if !l.completa {
                continue;
            }
            for p in params.iter().filter(|p| p.obligatorio) {
                if !mandadas.contains(&p.nombre) {
                    fallos.push(format!(
                        "{} no recibe `{}`, que es obligatorio — {}",
                        l.comando, p.nombre, l.fichero
                    ));
                }
            }
        }
        fallos.sort();
        fallos.dedup();
        assert!(
            fallos.is_empty(),
            "los argumentos de la UI y los de los comandos no dicen lo mismo:\n  {}",
            fallos.join("\n  ")
        );
    }
}
