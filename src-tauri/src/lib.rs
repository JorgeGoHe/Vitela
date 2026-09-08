use base64::Engine;
use pdfium_render::prelude::*;
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::io::Cursor;
use std::sync::{mpsc, OnceLock};

type Job = Box<dyn FnOnce() + Send>;

static PDFIUM_TX: OnceLock<mpsc::Sender<Job>> = OnceLock::new();

/// Directorio `lib/` dentro de los resources del bundle (solo en producción;
/// lo fija el setup de Tauri antes de usar PDFium).
static RESOURCE_LIB_DIR: OnceLock<std::path::PathBuf> = OnceLock::new();

/// Ejecuta `f` en el hilo dedicado de PDFium y devuelve su resultado.
///
/// PDFium se cuelga (deadlock, sin error) si se inicializa una segunda
/// instancia mientras otra sigue viva en el proceso, y sus tipos no son
/// `Send`. Un único hilo propietario garantiza por construcción una sola
/// instancia y serializa todo el acceso, venga de donde venga la llamada
/// (comandos de Tauri o tests).
///
/// Es reentrante: si ya estamos en el hilo de PDFium, `f` se ejecuta en
/// línea. Sin esto, un trabajo que por dentro vuelva a llamar aquí se
/// quedaría esperando a un hilo que está ocupado esperándole a él.
pub(crate) fn on_pdfium_thread<R: Send + 'static>(f: impl FnOnce() -> R + Send + 'static) -> R {
    if EN_HILO_PDFIUM.with(|c| c.get()) {
        return f();
    }
    let tx = PDFIUM_TX.get_or_init(|| {
        let (tx, rx) = mpsc::channel::<Job>();
        std::thread::Builder::new()
            .name("pdfium".into())
            .spawn(move || {
                EN_HILO_PDFIUM.with(|c| c.set(true));
                for job in rx {
                    job();
                }
            })
            .expect("no se pudo crear el hilo de PDFium");
        tx
    });
    let (rtx, rrx) = mpsc::channel();
    tx.send(Box::new(move || {
        let _ = rtx.send(f());
    }))
    .expect("el hilo de PDFium ha muerto");
    rrx.recv().expect("el hilo de PDFium ha muerto")
}

thread_local! {
    static EN_HILO_PDFIUM: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static PDFIUM: RefCell<Option<&'static Pdfium>> = const { RefCell::new(None) };
    static DOC_CACHE: RefCell<Option<(String, PdfDocument<'static>)>> = const { RefCell::new(None) };
    static LOPDF_CACHE: RefCell<Option<(String, lopdf::Document)>> = const { RefCell::new(None) };
}

/// Instancia única de PDFium, creada una sola vez y viva todo el proceso.
/// Solo debe llamarse desde el hilo de PDFium (dentro de `on_pdfium_thread`).
/// Orden de búsqueda: resources del bundle (producción) → src-tauri/lib/
/// (dev y tests, donde el cwd es src-tauri) → librería del sistema.
pub(crate) fn pdfium() -> Result<&'static Pdfium, String> {
    PDFIUM.with(|cell| {
        let mut slot = cell.borrow_mut();
        if let Some(p) = *slot {
            return Ok(p);
        }
        let from_resources = RESOURCE_LIB_DIR.get().and_then(|dir| {
            Pdfium::bind_to_library(Pdfium::pdfium_platform_library_name_at_path(&format!(
                "{}/",
                dir.display()
            )))
            .ok()
        });
        let bindings = match from_resources {
            Some(b) => b,
            None => Pdfium::bind_to_library(Pdfium::pdfium_platform_library_name_at_path("./lib/"))
                .or_else(|_| Pdfium::bind_to_system_library())
                .map_err(|e| format!("No se pudo cargar libpdfium: {e}"))?,
        };
        let leaked: &'static Pdfium = Box::leak(Box::new(Pdfium::new(bindings)));
        *slot = Some(leaked);
        Ok(leaked)
    })
}

/// Ejecuta `f` con el documento cacheado para `path`, recargándolo del disco
/// solo si el caché apunta a otro fichero. Solo debe llamarse desde el hilo
/// de PDFium.
pub(crate) fn with_doc<R>(
    path: &str,
    f: impl FnOnce(&PdfDocument<'static>) -> Result<R, String>,
) -> Result<R, String> {
    DOC_CACHE.with(|cell| {
        let mut cache = cell.borrow_mut();
        let stale = !matches!(cache.as_ref(), Some((p, _)) if p == path);
        if stale {
            let doc = pdfium()?
                .load_pdf_from_file(path, None)
                .map_err(|e| e.to_string())?;
            *cache = Some((path.to_string(), doc));
        }
        f(&cache.as_ref().unwrap().1)
    })
}

/// Igual que `with_doc` pero con el documento parseado por lopdf (para lo
/// que PDFium no expone o no lee bien). Solo desde el hilo de PDFium.
pub(crate) fn with_lopdf<R>(
    path: &str,
    f: impl FnOnce(&lopdf::Document) -> Result<R, String>,
) -> Result<R, String> {
    LOPDF_CACHE.with(|cell| {
        let mut cache = cell.borrow_mut();
        let stale = !matches!(cache.as_ref(), Some((p, _)) if p == path);
        if stale {
            let doc = lopdf::Document::load(path)
                .map_err(|e| format!("No se pudo leer el PDF: {e}"))?;
            *cache = Some((path.to_string(), doc));
        }
        f(&cache.as_ref().unwrap().1)
    })
}

/// Descarta los documentos cacheados. Llamar tras cualquier mutación en disco.
pub(crate) fn invalidate_doc_cache() {
    DOC_CACHE.with(|cell| *cell.borrow_mut() = None);
    LOPDF_CACHE.with(|cell| *cell.borrow_mut() = None);
}

#[derive(Serialize, Debug)]
struct DocumentInfo {
    page_count: u16,
    work_path: String,
    /// El original estaba cifrado (la copia de trabajo queda descifrada;
    /// guardar sin re-proteger equivale a quitar la contraseña).
    had_password: bool,
}

/// Copias de trabajo vivas: se borran al cerrar el documento o al salir de
/// la app (una copia abandonada es un PDF entero en temp; si el original
/// iba cifrado, además está en claro).
static COPIAS_ABIERTAS: std::sync::LazyLock<std::sync::Mutex<std::collections::HashSet<String>>> =
    std::sync::LazyLock::new(Default::default);

fn copias_abiertas() -> std::sync::MutexGuard<'static, std::collections::HashSet<String>> {
    COPIAS_ABIERTAS.lock().unwrap_or_else(|e| e.into_inner())
}

/// Borra la copia de trabajo y sus instantáneas de historial. Debe llamarse
/// desde el hilo de PDFium (invalida el caché antes de borrar).
fn borra_copia(work_path: &str) {
    invalidate_doc_cache();
    historial::limpia(work_path);
    let _ = std::fs::remove_file(work_path);
    copias_abiertas().remove(work_path);
}

/// Cierra un documento: borra su copia de trabajo e instantáneas.
#[tauri::command(async)]
fn close_document(work_path: String) -> Result<(), String> {
    on_pdfium_thread(move || {
        borra_copia(&work_path);
        Ok(())
    })
}

/// Al salir de la app: borra las copias que sigan abiertas.
fn borra_copias_abiertas() {
    let restantes: Vec<String> = copias_abiertas().iter().cloned().collect();
    on_pdfium_thread(move || {
        for w in restantes {
            borra_copia(&w);
        }
    });
}

/// Barrido al arrancar: copias de trabajo (`vitela-*.pdf` en temp) e
/// instantáneas (`vitela-historial/`) huérfanas de cierres bruscos. Solo las
/// de hace más de 24 h, para no pisar a otra instancia de la app viva.
fn barre_huerfanos(dir: &std::path::Path, edad_minima: std::time::Duration) -> usize {
    let mut borrados = barre_ficheros(dir, edad_minima, |n| {
        n.starts_with("vitela-") && (n.ends_with(".pdf") || n.ends_with(".pdf.tmp"))
    });
    borrados += barre_ficheros(&dir.join("vitela-historial"), edad_minima, |n| n.contains(".snap"));
    borrados
}

fn barre_ficheros(
    dir: &std::path::Path,
    edad_minima: std::time::Duration,
    es_nuestro: impl Fn(&str) -> bool,
) -> usize {
    let Ok(entradas) = std::fs::read_dir(dir) else { return 0 };
    let ahora = std::time::SystemTime::now();
    let mut borrados = 0;
    for e in entradas.flatten() {
        let nombre = e.file_name().to_string_lossy().to_string();
        if !es_nuestro(&nombre) {
            continue;
        }
        let viejo = e
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|m| ahora.duration_since(m).ok())
            .map(|d| d >= edad_minima)
            .unwrap_or(false);
        if viejo && std::fs::remove_file(e.path()).is_ok() {
            borrados += 1;
        }
    }
    borrados
}

/// Ruta única en temp para la copia de trabajo del documento.
fn work_copy_path(original: &str) -> std::path::PathBuf {
    let name = std::path::Path::new(original)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("documento");
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!("vitela-{name}-{nanos}.pdf"))
}

/// Guarda el documento sobre `path` y lo cierra. PDFium lee el fichero de
/// forma perezosa mientras el documento está abierto, y en Windows no se
/// puede renombrar encima de un fichero abierto, así que el orden importa:
/// escribir a un temporal, cerrar el documento (y el caché, que puede tener
/// otro handle del mismo fichero) y solo entonces renombrar.
pub(crate) fn save_and_close(doc: PdfDocument<'static>, path: &str) -> Result<(), String> {
    let tmp = format!("{path}.tmp");
    doc.save_to_file(&tmp).map_err(|e| e.to_string())?;
    drop(doc);
    invalidate_doc_cache();
    std::fs::rename(&tmp, path).map_err(|e| e.to_string())
}

/// Cirugía con lopdf sobre la copia de trabajo: carga el PDF, ejecuta `f`
/// y guarda con .tmp + rename, todo dentro del hilo de PDFium. El caché se
/// invalida ANTES de leer: PDFium mantiene el fichero abierto de forma
/// perezosa y en Windows el rename fallaría; además, al pasar por el hilo
/// ningún otro comando puede estar escribiendo la copia a la vez.
pub(crate) fn cirugia(
    work_path: &str,
    f: impl FnOnce(&mut lopdf::Document) -> Result<(), String> + Send + 'static,
) -> Result<(), String> {
    historial::mutacion(work_path.to_string(), |work_path| on_pdfium_thread(move || {
        invalidate_doc_cache();
        let mut doc = lopdf::Document::load(&work_path)
            .map_err(|e| format!("No se pudo leer el PDF: {e}"))?;
        if doc.is_encrypted() {
            return Err("El documento está cifrado: quita la contraseña antes".into());
        }
        f(&mut doc)?;
        let tmp = format!("{work_path}.tmp");
        doc.save(&tmp).map_err(|e| format!("No se pudo guardar: {e}"))?;
        std::fs::rename(&tmp, &work_path).map_err(|e| e.to_string())
    }))
}

/// Abre un PDF creando una copia de trabajo en temp. Todas las mutaciones
/// operan sobre la copia; el original solo se toca al guardar. Si el PDF
/// está cifrado hace falta `password`: la copia de trabajo se guarda ya
/// descifrada para que el resto de comandos no tengan que saber nada. El
/// error "PASSWORD_REQUIRED" indica a la UI que pida contraseña.
#[tauri::command(async)]
fn open_pdf(path: String, password: Option<String>) -> Result<DocumentInfo, String> {
    let work = work_copy_path(&path);
    let work_path = work.to_string_lossy().into_owned();
    on_pdfium_thread(move || {
        let pdfium = pdfium()?;
        let doc = match pdfium.load_pdf_from_file(&path, password.as_deref()) {
            Ok(doc) => doc,
            Err(PdfiumError::PdfiumLibraryInternalError(
                PdfiumInternalError::PasswordError,
            )) => return Err("PASSWORD_REQUIRED".into()),
            Err(e) => return Err(e.to_string()),
        };
        let page_count = doc.pages().len();
        let had_password = password.is_some();
        if had_password {
            // copia descifrada (save_to_file conservaría el cifrado)
            drop(doc);
            seguridad::guarda_descifrado(
                &path,
                password.as_deref().unwrap_or(""),
                &work_path,
            )?;
        } else {
            drop(doc);
            std::fs::copy(&path, &work_path)
                .map_err(|e| format!("No se pudo crear la copia de trabajo: {e}"))?;
        }
        copias_abiertas().insert(work_path.clone());
        Ok(DocumentInfo {
            page_count,
            work_path,
            had_password,
        })
    })
}

/// Renderiza una página a PNG (bytes) con el ancho pedido en píxeles.
pub(crate) fn render_page_png(
    path: String,
    page_index: u16,
    width: i32,
) -> Result<Vec<u8>, String> {
    on_pdfium_thread(move || {
        with_doc(&path, |doc| {
            let page = doc.pages().get(page_index).map_err(|e| e.to_string())?;
            let bitmap = page
                .render_with_config(
                    &PdfRenderConfig::new()
                        .set_target_width(width)
                        .render_form_data(true)
                        .render_annotations(true),
                )
                .map_err(|e| e.to_string())?;
            let mut png = Vec::new();
            bitmap
                .as_image()
                .write_to(&mut Cursor::new(&mut png), image::ImageFormat::Png)
                .map_err(|e| e.to_string())?;
            Ok(png)
        })
    })
}

/// Variante en base64 para el puente de QA (que habla JSON) y los tests.
pub(crate) fn render_page_b64(
    path: String,
    page_index: u16,
    width: i32,
) -> Result<String, String> {
    render_page_png(path, page_index, width)
        .map(|png| base64::engine::general_purpose::STANDARD.encode(png))
}

/// Comando de render: los bytes del PNG viajan como IPC binario (sin base64
/// ni JSON — con páginas grandes el parseo de un JSON de varios MB congelaba
/// el hilo del webview y dejaba zonas sin pintar al redimensionar la ventana).
#[tauri::command(async)]
fn render_page(
    path: String,
    page_index: u16,
    width: i32,
) -> Result<tauri::ipc::Response, String> {
    render_page_png(path, page_index, width).map(tauri::ipc::Response::new)
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub(crate) struct Rect {
    pub(crate) x: f32,
    pub(crate) y: f32,
    pub(crate) w: f32,
    pub(crate) h: f32,
}

#[derive(Serialize)]
struct PageSize {
    width: f32,
    height: f32,
}

/// Tamaño de todas las páginas en puntos PDF (para el layout del scroll
/// continuo sin renderizar nada).
#[tauri::command(async)]
fn get_page_sizes(path: String) -> Result<Vec<PageSize>, String> {
    on_pdfium_thread(move || {
        with_doc(&path, |doc| {
            Ok(doc
                .pages()
                .iter()
                .map(|p| PageSize {
                    width: p.width().value,
                    height: p.height().value,
                })
                .collect())
        })
    })
}

mod anotaciones;
mod anotaciones2;
mod busqueda;
mod documento;
mod exportar;
mod firma;
mod firmas_visuales;
mod formularios;
mod formularios2;
mod historial;
mod imagenes;
mod paginas;
mod paginas2;
#[cfg(debug_assertions)]
pub mod puente_dev;
mod seguridad;
mod texto;

/// Firma digitalmente la copia de trabajo y escribe el PDF firmado en
/// `dest_path`. Certificado y clave privada en PEM (RSA sin cifrar).
#[tauri::command(async)]
fn sign_pdf(
    work_path: String,
    dest_path: String,
    cert_pem_path: String,
    key_pem_path: String,
    reason: Option<String>,
) -> Result<(), String> {
    let cert_pem = std::fs::read_to_string(&cert_pem_path)
        .map_err(|e| format!("No se pudo leer el certificado: {e}"))?;
    let key_pem = std::fs::read_to_string(&key_pem_path)
        .map_err(|e| format!("No se pudo leer la clave: {e}"))?;
    let cred = firma::credenciales_pem(&cert_pem, &key_pem)?;
    firmar_en_hilo(work_path, dest_path, cred, reason)
}

/// La firma lee la copia de trabajo: se hace en el hilo de PDFium, con el
/// caché invalidado, para no competir con una mutación concurrente.
fn firmar_en_hilo(
    work_path: String,
    dest_path: String,
    cred: firma::Credenciales,
    reason: Option<String>,
) -> Result<(), String> {
    on_pdfium_thread(move || {
        invalidate_doc_cache();
        firma::sign(&work_path, &dest_path, &cred, reason)
    })
}

/// Igual que `sign_pdf` pero con un contenedor PKCS#12 (.p12/.pfx).
#[tauri::command(async)]
fn sign_pdf_p12(
    work_path: String,
    dest_path: String,
    p12_path: String,
    password: String,
    reason: Option<String>,
) -> Result<(), String> {
    let bytes = std::fs::read(&p12_path).map_err(|e| format!("No se pudo leer el .p12: {e}"))?;
    let cred = firma::credenciales_p12(&bytes, &password)?;
    firmar_en_hilo(work_path, dest_path, cred, reason)
}

/// Crea un PDF de prueba con una página de texto por cada entrada. Lo usan
/// los tests y el puente de QA (fixtures).
#[cfg(any(test, debug_assertions))]
pub(crate) fn crea_pdf(textos: &[&str], dest: &std::path::Path) {
    let textos: Vec<String> = textos.iter().map(|t| t.to_string()).collect();
    let dest = dest.to_path_buf();
    // los tests reutilizan nombres de fixture: fuera las instantáneas de la
    // ejecución anterior, que si no se acumulan en el temp
    historial::borra_instantaneas_en_disco(&dest.to_string_lossy());
    on_pdfium_thread(move || {
        let pdfium = pdfium().expect("no cargó libpdfium");
        let mut doc = pdfium.create_new_pdf().expect("crear documento");
        let font = doc.fonts_mut().helvetica();
        for texto in &textos {
            let mut page = doc
                .pages_mut()
                .create_page_at_end(PdfPagePaperSize::a4())
                .expect("crear página");
            let mut obj = PdfPageTextObject::new(&doc, texto, font, PdfPoints::new(14.0))
                .expect("crear objeto de texto");
            // posición realista (no en la esquina 0,0)
            obj.translate(PdfPoints::new(50.0), PdfPoints::new(700.0))
                .expect("posicionar texto");
            page.objects_mut()
                .add_text_object(obj)
                .expect("añadir texto");
        }
        doc.save_to_file(&dest).expect("guardar PDF de prueba");
    })
}

/// Vuelca la copia de trabajo en el destino (guardar / guardar como).
#[tauri::command(async)]
fn save_pdf(work_path: String, dest_path: String) -> Result<(), String> {
    // en el hilo de PDFium: nadie puede estar renombrando la copia a la vez
    on_pdfium_thread(move || {
        invalidate_doc_cache();
        std::fs::copy(&work_path, &dest_path)
            .map(|_| ())
            .map_err(|e| format!("No se pudo guardar: {e}"))
    })
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            use tauri::Manager;
            if let Ok(dir) = app.path().resource_dir() {
                let _ = RESOURCE_LIB_DIR.set(dir.join("lib"));
            }
            if let Ok(dir) = app.path().app_data_dir() {
                // el identifier cambió con el renombre a Vitela: recuperar
                // las firmas guardadas bajo el identifier antiguo
                firmas_visuales::migrar_datos_antiguos(&dir);
                let _ = firmas_visuales::DIR_DATOS.set(dir);
            }
            #[cfg(debug_assertions)]
            if std::env::var("EDITOR_PDF_PUENTE").as_deref() == Ok("1") {
                std::thread::spawn(|| puente_dev::arrancar(puente_dev::puerto()));
            }
            std::thread::spawn(|| {
                barre_huerfanos(&std::env::temp_dir(), std::time::Duration::from_secs(24 * 3600));
            });
            Ok(())
        })
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            open_pdf,
            render_page,
            busqueda::get_page_text,
            get_page_sizes,
            busqueda::search_pdf,
            paginas::delete_page,
            paginas::rotate_page,
            paginas::move_page,
            paginas::merge_pdf,
            paginas::extract_pages,
            save_pdf,
            anotaciones::add_highlight,
            anotaciones::add_stroke,
            anotaciones::add_note,
            anotaciones::get_annotations,
            anotaciones::remove_annotation,
            formularios::get_form_fields,
            formularios::set_form_text,
            formularios::set_form_checked,
            texto::get_text_blocks,
            texto::edit_text_block,
            texto::add_text_block,
            texto::delete_text_block,
            imagenes::get_images,
            imagenes::add_image,
            imagenes::transform_image,
            imagenes::replace_image,
            imagenes::delete_image,
            sign_pdf,
            sign_pdf_p12,
            firmas_visuales::stamp_signature,
            firmas_visuales::import_signature_file,
            firmas_visuales::save_stored_signature,
            firmas_visuales::list_stored_signatures,
            firmas_visuales::delete_stored_signature,
            imagenes::get_image_data,
            anotaciones2::add_markup,
            anotaciones2::add_shape,
            anotaciones2::add_stamp,
            anotaciones2::transform_annotation,
            paginas2::add_blank_page,
            paginas2::duplicate_page,
            paginas2::insert_pdf_at,
            paginas2::crop_page,
            paginas2::add_watermark,
            paginas2::add_header_footer,
            paginas2::remove_marginal_text,
            documento::get_outline,
            documento::set_outline,
            documento::get_metadata,
            documento::set_metadata,
            documento::get_links,
            seguridad::encrypt_pdf,
            seguridad::flatten_pdf,
            seguridad::redact_area,
            exportar::export_pages_png,
            exportar::export_text,
            exportar::compress_pdf,
            formularios2::create_form_field,
            formularios2::create_link,
            formularios2::delete_form_field,
            historial::undo,
            historial::redo,
            historial::history_state,
            historial::squash_history,
            close_document
        ])
        .build(tauri::generate_context!())
        .expect("error while running tauri application")
        .run(|_app, event| {
            if let tauri::RunEvent::Exit = event {
                borra_copias_abiertas();
            }
        });
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) use crate::crea_pdf;

    /// Texto plano de cada página de un PDF, para verificar orden y contenido.
    pub(crate) fn textos_de(path: &std::path::Path) -> Vec<String> {
        let path = path.to_path_buf();
        on_pdfium_thread(move || {
            let pdfium = pdfium().expect("no cargó libpdfium");
            let doc = pdfium.load_pdf_from_file(&path, None).expect("abrir PDF");
            doc.pages()
                .iter()
                .map(|p| p.text().map(|t| t.all()).unwrap_or_default())
                .collect()
        })
    }

    #[test]
    fn close_document_borra_copia_e_instantaneas() {
        let pdf = std::env::temp_dir().join("editor_pdf_test_cerrar.pdf");
        crea_pdf(&["Uno", "Dos"], &pdf);
        let info = open_pdf(pdf.to_string_lossy().into_owned(), None).expect("abrir");
        let work = info.work_path.clone();
        assert!(copias_abiertas().contains(&work));
        paginas::rotate_page(work.clone(), 0).expect("rotar (deja instantánea)");
        let nombre = std::path::Path::new(&work).file_name().unwrap().to_string_lossy().to_string();
        let snap = historial::directorio().join(format!("{nombre}.snap0"));
        assert!(snap.exists());
        close_document(work.clone()).expect("cerrar");
        assert!(!std::path::Path::new(&work).exists(), "la copia debe desaparecer");
        assert!(!snap.exists());
        assert!(!copias_abiertas().contains(&work));
        std::fs::remove_file(&pdf).ok();
    }

    #[test]
    fn barrido_respeta_recientes_y_ajenos() {
        let dir = std::env::temp_dir().join("editor_pdf_test_barrido");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::create_dir_all(dir.join("vitela-historial")).unwrap();
        for n in ["vitela-a-1.pdf", "vitela-b-2.pdf.tmp", "otro.pdf", "vitela-notas.txt"] {
            std::fs::write(dir.join(n), b"x").unwrap();
        }
        std::fs::write(dir.join("vitela-historial").join("x.pdf.snap3"), b"x").unwrap();
        // con edad mínima cero se borran los nuestros; los ajenos se quedan
        assert_eq!(barre_huerfanos(&dir, std::time::Duration::ZERO), 3);
        assert!(dir.join("otro.pdf").exists());
        assert!(dir.join("vitela-notas.txt").exists());
        std::fs::write(dir.join("vitela-c-3.pdf"), b"x").unwrap();
        // recién creado: con 24 h de margen no se toca
        assert_eq!(barre_huerfanos(&dir, std::time::Duration::from_secs(24 * 3600)), 0);
        assert!(dir.join("vitela-c-3.pdf").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn hilo_pdfium_reentrante() {
        // una llamada anidada se ejecuta en línea y devuelve su valor; sin el
        // guardián de reentrada este test se quedaría colgado para siempre
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let v = on_pdfium_thread(|| on_pdfium_thread(|| 21) * 2);
            let _ = tx.send(v);
        });
        let v = rx
            .recv_timeout(std::time::Duration::from_secs(10))
            .expect("la llamada anidada se ha colgado");
        assert_eq!(v, 42);
    }

    #[test]
    fn renderiza_pagina() {
        let tmp = std::env::temp_dir().join("editor_pdf_test_render.pdf");
        crea_pdf(&["Hola"], &tmp);
        let png_b64 = render_page_b64(tmp.to_string_lossy().into_owned(), 0, 200).expect("render");
        std::fs::remove_file(&tmp).ok();
        let png = base64::engine::general_purpose::STANDARD
            .decode(&png_b64)
            .expect("base64 válido");
        assert!(png.len() > 100, "PNG sospechosamente pequeño");
        assert_eq!(&png[1..4], b"PNG");
    }

    #[test]
    fn tamanos_de_pagina() {
        let tmp = std::env::temp_dir().join("editor_pdf_test_tamanos.pdf");
        crea_pdf(&["Uno", "Dos", "Tres"], &tmp);
        let sizes = get_page_sizes(tmp.to_string_lossy().into_owned()).expect("tamaños");
        std::fs::remove_file(&tmp).ok();
        assert_eq!(sizes.len(), 3);
        for s in &sizes {
            assert!((s.width - 595.28).abs() < 0.1, "ancho A4: {}", s.width);
            assert!((s.height - 841.89).abs() < 0.1, "alto A4: {}", s.height);
        }
    }

    #[test]
    fn firma_digital() {
        use sha2::{Digest, Sha256};

        let dir = std::env::temp_dir();
        let src = dir.join("editor_pdf_test_firma_src.pdf");
        let dest = dir.join("editor_pdf_test_firma_out.pdf");
        crea_pdf(&["Documento importante"], &src);

        let cert_pem = include_str!("../fixtures/test_cert.pem");
        let key_pem = include_str!("../fixtures/test_key.pem");
        let cred = firma::credenciales_pem(cert_pem, key_pem).expect("credenciales PEM");
        firma::sign(
            &src.to_string_lossy(),
            &dest.to_string_lossy(),
            &cred,
            Some("Prueba".into()),
        )
        .expect("firmar");

        let bytes = std::fs::read(&dest).expect("leer PDF firmado");

        // extraer ByteRange [a b c d]
        let txt = String::from_utf8_lossy(&bytes);
        let br_start = txt.find("/ByteRange").expect("ByteRange presente");
        let open = txt[br_start..].find('[').unwrap() + br_start + 1;
        let close = txt[open..].find(']').unwrap() + open;
        let nums: Vec<usize> = txt[open..close]
            .split_whitespace()
            .map(|n| n.parse().expect("número en ByteRange"))
            .collect();
        assert_eq!(nums.len(), 4, "ByteRange: {:?}", nums);
        assert_eq!(nums[0], 0);
        assert_eq!(
            nums[2] + nums[3],
            bytes.len(),
            "el ByteRange debe cubrir hasta el final del fichero"
        );

        // digest sobre los dos rangos
        let mut hasher = Sha256::new();
        hasher.update(&bytes[nums[0]..nums[0] + nums[1]]);
        hasher.update(&bytes[nums[2]..nums[2] + nums[3]]);
        let digest = hasher.finalize();

        // el hueco entre rangos es <hex de la firma>
        let gap = &bytes[nums[0] + nums[1]..nums[2]];
        assert_eq!(gap[0], b'<');
        assert_eq!(gap[gap.len() - 1], b'>');
        let hex: String = String::from_utf8_lossy(&gap[1..gap.len() - 1]).into_owned();
        let der_bytes: Vec<u8> = (0..hex.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).expect("hex válido"))
            .collect();

        // parsear el CMS (el hueco lleva ceros de relleno tras el DER, así
        // que no se puede exigir consumo exacto) y comparar el messageDigest
        let ci: cms::content_info::ContentInfo = {
            use der::Reader;
            let mut reader = der::SliceReader::new(&der_bytes).unwrap();
            reader.decode().expect("ContentInfo DER válido")
        };
        assert_eq!(ci.content_type, const_oid::db::rfc5911::ID_SIGNED_DATA);
        let sd: cms::signed_data::SignedData = ci.content.decode_as().expect("SignedData válido");
        let signer = sd.signer_infos.0.iter().next().expect("un firmante");
        let attrs = signer.signed_attrs.as_ref().expect("atributos firmados");
        let md_attr = attrs
            .iter()
            .find(|a| a.oid == const_oid::db::rfc5911::ID_MESSAGE_DIGEST)
            .expect("atributo messageDigest");
        let md_der = {
            use der::Encode;
            md_attr
                .values
                .iter()
                .next()
                .expect("valor")
                .to_der()
                .unwrap()
        };
        // el valor es un OCTET STRING: 0x04, len, bytes
        assert_eq!(
            &md_der[md_der.len() - 32..],
            digest.as_slice(),
            "el messageDigest firmado debe coincidir con el hash del ByteRange"
        );

        std::fs::remove_file(&src).ok();
        std::fs::remove_file(&dest).ok();
    }

    #[test]
    fn firma_con_p12() {
        let dir = std::env::temp_dir();
        let src = dir.join("editor_pdf_test_p12_src.pdf");
        let dest = dir.join("editor_pdf_test_p12_out.pdf");
        crea_pdf(&["Firmado con p12"], &src);

        let p12 = include_bytes!("../fixtures/test_bundle.p12");
        let cred = firma::credenciales_p12(p12, "test1234").expect("abrir p12");
        firma::sign(&src.to_string_lossy(), &dest.to_string_lossy(), &cred, None)
            .expect("firmar con p12");
        let bytes = std::fs::read(&dest).expect("leer firmado");
        assert!(
            find_in(&bytes, b"/SubFilter/adbe.pkcs7.detached")
                || find_in(&bytes, b"/SubFilter /adbe.pkcs7.detached"),
            "el PDF firmado debe llevar el SubFilter"
        );

        // contraseña incorrecta debe fallar con error, no colgarse ni abrir
        assert!(firma::credenciales_p12(p12, "mala").is_err());

        std::fs::remove_file(&src).ok();
        std::fs::remove_file(&dest).ok();
    }

    fn find_in(haystack: &[u8], needle: &[u8]) -> bool {
        haystack.windows(needle.len()).any(|w| w == needle)
    }

}
