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
        "edit_text_block" => cmd!(texto::edit_text_block, { work_path: String, page_index: u16, object_index: u32, new_text: String, color: Option<[u8; 4]>, align: Option<String>, line_height: Option<f32> }),
        "add_text_block" => cmd!(texto::add_text_block, { work_path: String, page_index: u16, x: f32, y: f32, text: String, font_size: f32, font: Option<String>, color: Option<[u8; 4]>, align: Option<String>, line_height: Option<f32> }),
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
                        out.push((ruta.to_string_lossy().into_owned(), texto));
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

    /// Los comandos que la UI llama de verdad: `invoke("x", …)`, con o sin
    /// parámetro de tipo (`invoke<PageText>("get_page_text", …)`).
    fn comandos_que_llama_la_ui(fuentes: &[(String, String)]) -> Vec<(String, String)> {
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
                let Some((nombre, _)) = tras.split_once('"') else { continue };
                fuera.push((nombre.to_string(), fichero.clone()));
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
    /// Los comandos que nadie llama **avisan pero no fallan**: hay funciones
    /// que se registran antes de que la UI las use (`unmark_all_redactions`
    /// esperó un ciclo entero). El aviso sale por stderr con
    /// `cargo test -- --nocapture`.
    #[test]
    fn los_comandos_estan_en_el_handler_en_el_puente_y_en_la_ui() {
        let handler = comandos_del_handler();
        let puente = comandos_del_puente();
        let fuentes = fuentes_de_la_ui();
        let ui = comandos_que_llama_la_ui(&fuentes);

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

        let inventados: Vec<String> = ui
            .iter()
            .filter(|(c, _)| !handler.contains(c))
            .map(|(c, f)| format!("{c} ({f})"))
            .collect();
        assert!(
            inventados.is_empty(),
            "la UI llama a comandos que no existen en el backend: {inventados:?}"
        );

        let nadie: Vec<&String> = handler
            .iter()
            .filter(|c| !ui.iter().any(|(u, _)| u == *c))
            .collect();
        if !nadie.is_empty() {
            eprintln!(
                "[aviso] comandos registrados que la UI no llama todavía: {nadie:?}"
            );
        }
    }
}
