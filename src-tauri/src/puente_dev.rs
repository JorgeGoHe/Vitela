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
//!   → {"path":"/tmp/editor-pdf-fixture-….pdf"}
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
            let path = std::env::temp_dir().join(format!("editor-pdf-fixture-{nanos}.pdf"));
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
    use crate::{anotaciones2, documento, exportar, firmas_visuales, imagenes, paginas2, seguridad};
    match cmd {
        "open_pdf" => cmd!(crate::open_pdf, { path: String, password: Option<String> }),
        "render_page" => cmd!(crate::render_page, { path: String, page_index: u16, width: i32 }),
        "get_page_text" => cmd!(crate::get_page_text, { path: String, page_index: u16 }),
        "search_pdf" => cmd!(crate::search_pdf, { path: String, query: String }),
        "delete_page" => cmd!(crate::delete_page, { work_path: String, page_index: u16 }),
        "rotate_page" => cmd!(crate::rotate_page, { work_path: String, page_index: u16 }),
        "move_page" => cmd!(crate::move_page, { work_path: String, from_index: u16, to_index: u16 }),
        "merge_pdf" => cmd!(crate::merge_pdf, { work_path: String, other_path: String }),
        "extract_pages" => cmd!(crate::extract_pages, { work_path: String, page_indices: Vec<u16>, dest_path: String }),
        "save_pdf" => cmd!(crate::save_pdf, { work_path: String, dest_path: String }),
        "add_highlight" => cmd!(crate::add_highlight, { work_path: String, page_index: u16, rects: Vec<crate::Rect> }),
        "add_stroke" => cmd!(crate::add_stroke, { work_path: String, page_index: u16, points: Vec<[f32; 2]>, color: Option<[u8; 4]>, width: Option<f32> }),
        "add_note" => cmd!(crate::add_note, { work_path: String, page_index: u16, x: f32, y: f32, text: String }),
        "get_annotations" => cmd!(crate::get_annotations, { path: String, page_index: u16 }),
        "remove_annotation" => cmd!(crate::remove_annotation, { work_path: String, page_index: u16, annot_index: u16 }),
        "get_form_fields" => cmd!(crate::get_form_fields, { path: String, page_index: u16 }),
        "set_form_text" => cmd!(crate::set_form_text, { work_path: String, page_index: u16, annot_index: u16, value: String }),
        "set_form_checked" => cmd!(crate::set_form_checked, { work_path: String, page_index: u16, annot_index: u16, checked: bool }),
        "get_text_blocks" => cmd!(crate::get_text_blocks, { path: String, page_index: u16 }),
        "edit_text_block" => cmd!(crate::edit_text_block, { work_path: String, page_index: u16, object_index: u32, new_text: String }),
        "add_text_block" => cmd!(crate::add_text_block, { work_path: String, page_index: u16, x: f32, y: f32, text: String, font_size: f32, font: Option<String> }),
        "delete_text_block" => cmd!(crate::delete_text_block, { work_path: String, page_index: u16, object_index: u32 }),
        "get_images" => cmd!(crate::get_images, { path: String, page_index: u16 }),
        "add_image" => cmd!(crate::add_image, { work_path: String, page_index: u16, image_path: String, x: f32, y: f32 }),
        "transform_image" => cmd!(crate::transform_image, { work_path: String, page_index: u16, object_index: u32, x: f32, y: f32, w: f32, h: f32 }),
        "replace_image" => cmd!(crate::replace_image, { work_path: String, page_index: u16, object_index: u32, image_path: String }),
        "delete_image" => cmd!(crate::delete_image, { work_path: String, page_index: u16, object_index: u32 }),
        "sign_pdf" => cmd!(crate::sign_pdf, { work_path: String, dest_path: String, cert_pem_path: String, key_pem_path: String, reason: Option<String> }),
        "sign_pdf_p12" => cmd!(crate::sign_pdf_p12, { work_path: String, dest_path: String, p12_path: String, password: String, reason: Option<String> }),
        "stamp_signature" => cmd!(firmas_visuales::stamp_signature, { work_path: String, page_index: u16, png_base64: String, x: f32, y: f32, w: f32, h: f32 }),
        "import_signature_file" => cmd!(firmas_visuales::import_signature_file, { image_path: String }),
        "save_stored_signature" => cmd!(firmas_visuales::save_stored_signature, { name: String, png_base64: String }),
        "list_stored_signatures" => firmas_visuales::list_stored_signatures()
            .and_then(|v| serde_json::to_value(v).map_err(|e| e.to_string())),
        "delete_stored_signature" => cmd!(firmas_visuales::delete_stored_signature, { id: String }),
        "get_image_data" => cmd!(imagenes::get_image_data, { path: String, page_index: u16, object_index: u32 }),
        "add_markup" => cmd!(anotaciones2::add_markup, { work_path: String, page_index: u16, rects: Vec<crate::Rect>, kind: String, color: Option<[u8; 4]> }),
        "add_shape" => cmd!(anotaciones2::add_shape, { work_path: String, page_index: u16, kind: String, x1: f32, y1: f32, x2: f32, y2: f32, stroke: [u8; 4], fill: Option<[u8; 4]>, stroke_width: f32 }),
        "add_stamp" => cmd!(anotaciones2::add_stamp, { work_path: String, page_index: u16, text: String, color: [u8; 4], x: f32, y: f32, font_size: f32 }),
        "add_blank_page" => cmd!(paginas2::add_blank_page, { work_path: String, index: u16 }),
        "duplicate_page" => cmd!(paginas2::duplicate_page, { work_path: String, page_index: u16 }),
        "insert_pdf_at" => cmd!(paginas2::insert_pdf_at, { work_path: String, other_path: String, index: u16 }),
        "crop_page" => cmd!(paginas2::crop_page, { work_path: String, page_index: u16, rect: crate::Rect, all_pages: bool }),
        "add_watermark" => cmd!(paginas2::add_watermark, { work_path: String, text: String, font_size: f32, color: [u8; 4], diagonal: bool }),
        "remove_marginal_text" => cmd!(paginas2::remove_marginal_text, { work_path: String, zona: String, dry_run: bool }),
        "add_header_footer" => cmd!(paginas2::add_header_footer, { work_path: String, header_left: Option<String>, header_center: Option<String>, header_right: Option<String>, footer_left: Option<String>, footer_center: Option<String>, footer_right: Option<String>, font_size: f32 }),
        "get_outline" => cmd!(documento::get_outline, { path: String }),
        "set_outline" => cmd!(documento::set_outline, { work_path: String, nodes: Vec<documento::OutlineNode> }),
        "get_metadata" => cmd!(documento::get_metadata, { path: String }),
        "set_metadata" => cmd!(documento::set_metadata, { work_path: String, meta: documento::Metadata }),
        "get_links" => cmd!(documento::get_links, { path: String, page_index: u16 }),
        "encrypt_pdf" => cmd!(seguridad::encrypt_pdf, { work_path: String, dest_path: String, user_password: String, owner_password: Option<String> }),
        "flatten_pdf" => cmd!(seguridad::flatten_pdf, { work_path: String }),
        "redact_area" => cmd!(seguridad::redact_area, { work_path: String, page_index: u16, rect: crate::Rect, dry_run: bool }),
        "export_pages_png" => cmd!(exportar::export_pages_png, { path: String, dest_dir: String, dpi: u16, format: String }),
        "export_text" => cmd!(exportar::export_text, { path: String, dest_path: String }),
        "compress_pdf" => cmd!(exportar::compress_pdf, { work_path: String, quality: u8, max_dpi: u16 }),
        "create_form_field" => cmd!(crate::formularios2::create_form_field, { work_path: String, page_index: u16, kind: String, rect: crate::Rect, name: String }),
        "delete_form_field" => cmd!(crate::formularios2::delete_form_field, { work_path: String, name: String }),
        "create_link" => cmd!(crate::formularios2::create_link, { work_path: String, page_index: u16, rect: crate::Rect, uri: Option<String>, dest_page: Option<u16> }),
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
}
