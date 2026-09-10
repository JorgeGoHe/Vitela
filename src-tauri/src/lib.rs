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
    static DOC_CACHE: RefCell<Vec<(String, PdfDocument<'static>)>> = const { RefCell::new(Vec::new()) };
    static LOPDF_CACHE: RefCell<Vec<(String, lopdf::Document)>> = const { RefCell::new(Vec::new()) };
}

/// Cuántos documentos se quedan abiertos en el caché del hilo de PDFium.
///
/// Hasta el ciclo 6 era **uno**: abrir el segundo cerraba el primero, así
/// que trabajar con dos documentos a la vez recargaba el otro en cada
/// comando. Con un tope pequeño se atiende el uso de verdad —un documento y
/// el que se está mirando al lado— sin comerse la memoria de un PDF de
/// 400 MB por pestaña: pasado el tope se suelta el que lleva más tiempo sin
/// tocarse, y recargarlo cuesta milisegundos.
const DOCUMENTOS_EN_CACHE: usize = 4;

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
                .map_err(|e| {
                    mensaje_llano(format!("No se ha podido cargar libpdfium: {e}"))
                })?,
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
        let i = coloca(&mut cache, path, || {
            pdfium()?
                .load_pdf_from_file(path, None)
                .map_err(mensaje_llano)
        })?;
        f(&cache[i].1).map_err(mensaje_llano)
    })
}

/// Deja el documento de `path` cargado en el caché y devuelve dónde está.
/// El que se usa se va al final, así que el que se suelta al llegar al tope
/// es siempre el que lleva más tiempo sin tocarse.
fn coloca<T>(
    cache: &mut Vec<(String, T)>,
    path: &str,
    carga: impl FnOnce() -> Result<T, String>,
) -> Result<usize, String> {
    if let Some(i) = cache.iter().position(|(p, _)| p == path) {
        let entrada = cache.remove(i);
        cache.push(entrada);
        return Ok(cache.len() - 1);
    }
    let doc = carga()?;
    // se suelta ANTES de meter el nuevo: así nunca hay más documentos
    // abiertos de los que dice el tope
    while cache.len() >= DOCUMENTOS_EN_CACHE {
        cache.remove(0);
    }
    cache.push((path.to_string(), doc));
    Ok(cache.len() - 1)
}

/// Igual que `with_doc` pero con el documento parseado por lopdf (para lo
/// que PDFium no expone o no lee bien). Solo desde el hilo de PDFium.
pub(crate) fn with_lopdf<R>(
    path: &str,
    f: impl FnOnce(&lopdf::Document) -> Result<R, String>,
) -> Result<R, String> {
    LOPDF_CACHE.with(|cell| {
        let mut cache = cell.borrow_mut();
        let i = coloca(&mut cache, path, || {
            lopdf::Document::load(path)
                .map_err(|e| mensaje_llano(format!("No se ha podido leer el PDF: {e}")))
        })?;
        f(&cache[i].1).map_err(mensaje_llano)
    })
}

/// Suelta **ese** documento del caché. Llamar tras cualquier mutación en
/// disco: PDFium lee el fichero de forma perezosa mientras lo tiene
/// abierto, y en Windows no se puede renombrar encima de un fichero abierto.
///
/// Es por documento desde el ciclo 7: con el caché de uno solo, tocar un
/// documento obligaba a recargar el otro, y con varios abiertos eso es
/// recargar un PDF entero en cada comando.
pub(crate) fn invalidate_doc_cache(path: &str) {
    DOC_CACHE.with(|cell| cell.borrow_mut().retain(|(p, _)| p != path));
    LOPDF_CACHE.with(|cell| cell.borrow_mut().retain(|(p, _)| p != path));
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
    invalidate_doc_cache(work_path);
    seguridad::olvida_proteccion(work_path);
    historial::limpia(work_path);
    let _ = std::fs::remove_file(work_path);
    copias_abiertas().remove(work_path);
}

/// Cierra un documento: borra su copia de trabajo e instantáneas.
#[tauri::command(async)]
fn close_document(work_path: String) -> Result<(), String> {
    on_pdfium_thread(move || {
        borra_copia(&work_path);
        // sin documento, el menú vuelve a atenuar lo que no aplica
        menu::refleja_documento(false);
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
    // las copias que hay apuntadas para recuperar NO son huérfanas: son
    // justamente el trabajo que se salvó de un cierre bruto. Son varias
    // desde el ciclo 8 (una por documento abierto): proteger la primera y
    // barrer las otras dos es peor que no barrer nada
    let nombres_apuntados: Vec<String> = recuperacion::copias_apuntadas()
        .iter()
        .filter_map(|ruta| {
            std::path::Path::new(ruta)
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
        })
        .filter(|n| !n.is_empty())
        .collect();
    let salvada = move |n: &str| nombres_apuntados.iter().any(|a| n.starts_with(a.as_str()));
    let mut borrados = barre_ficheros(dir, edad_minima, |n| {
        n.starts_with("vitela-") && (n.ends_with(".pdf") || n.ends_with(".pdf.tmp")) && !salvada(n)
    });
    borrados += barre_ficheros(&dir.join("vitela-historial"), edad_minima, |n| {
        n.contains(".snap") && !salvada(n)
    });
    // y las carpetas de los adjuntos que se abrieron con el visor del
    // sistema (`open_attachment`): una por adjunto, con su nombre dentro
    borrados += barre_carpetas(dir, edad_minima, |n| n.starts_with("vitela-adjunto-"));
    borrados
}

/// Como [`barre_ficheros`] pero con carpetas enteras.
fn barre_carpetas(
    dir: &std::path::Path,
    edad_minima: std::time::Duration,
    es_nuestra: impl Fn(&str) -> bool,
) -> usize {
    let Ok(entradas) = std::fs::read_dir(dir) else { return 0 };
    let ahora = std::time::SystemTime::now();
    let mut borradas = 0;
    for e in entradas.flatten() {
        let nombre = e.file_name().to_string_lossy().to_string();
        if !es_nuestra(&nombre) || !e.path().is_dir() {
            continue;
        }
        let vieja = e
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|m| ahora.duration_since(m).ok())
            .map(|d| d >= edad_minima)
            .unwrap_or(false);
        if vieja && std::fs::remove_dir_all(e.path()).is_ok() {
            borradas += 1;
        }
    }
    borradas
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
    doc.save_to_file(&tmp).map_err(mensaje_llano)?;
    drop(doc);
    invalidate_doc_cache(path);
    std::fs::rename(&tmp, path).map_err(mensaje_llano)
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
    historial::mutacion(work_path.to_string(), |work_path| {
        on_pdfium_thread(move || cirugia_en_hilo(&work_path, f))
    })
}

/// El cuerpo de [`cirugia`] sin el paso de historial y sin saltar de hilo:
/// para comandos que YA están dentro de una `mutacion` y del hilo de PDFium
/// y necesitan un segundo pase con lopdf (crear la anotación con PDFium y
/// escribirle después su `/AP`, por ejemplo). Nunca llamarlo suelto: sin
/// `mutacion` no habría paso de deshacer.
pub(crate) fn cirugia_en_hilo(
    work_path: &str,
    f: impl FnOnce(&mut lopdf::Document) -> Result<(), String>,
) -> Result<(), String> {
    invalidate_doc_cache(work_path);
    let mut doc = lopdf::Document::load(work_path)
        .map_err(|e| mensaje_llano(format!("No se ha podido leer el PDF: {e}")))?;
    if doc.is_encrypted() {
        return Err("El documento está cifrado: quita la contraseña antes".into());
    }
    f(&mut doc)?;
    let tmp = format!("{work_path}.tmp");
    doc.save(&tmp)
        .map_err(|e| mensaje_llano(format!("No se ha podido guardar el documento: {e}")))?;
    std::fs::rename(&tmp, work_path).map_err(mensaje_llano)
}

/// Abre un PDF creando una copia de trabajo en temp. Todas las mutaciones
/// operan sobre la copia; el original solo se toca al guardar. Si el PDF
/// está cifrado hace falta `password`: la copia de trabajo se guarda ya
/// descifrada para que el resto de comandos no tengan que saber nada. El
/// error "PASSWORD_REQUIRED" indica a la UI que pida contraseña.
/// Traduce a lenguaje llano el error que va a ver el usuario.
///
/// Los errores de las librerías salen en jerga y en inglés: el `Display`
/// de `PdfiumError` es el `Debug` de Rust (`PdfiumLibraryInternalError(\n
/// FormatError,\n)`) y el de `std::io::Error` acaba en `(os error 2)`.
/// Nada de eso le dice a nadie qué ha pasado ni qué hacer. Esta función es
/// el único sitio donde se traduce, y está en los embudos por los que pasa
/// todo comando (`mutacion`, `with_doc`, `with_lopdf`) más los pocos que
/// no pasan por ninguno.
///
/// Los mensajes que ya escribimos nosotros se conservan tal cual; si
/// llevan pegada la causa en inglés, se les cambia solo esa cola.
pub(crate) fn mensaje_llano(e: impl std::fmt::Display) -> String {
    let bruto = e.to_string();
    // el Debug multilínea de PdfiumError en una sola línea
    let plano = bruto.split_whitespace().collect::<Vec<_>>().join(" ");
    let Some(causa) = causa_llana(&plano) else {
        return plano;
    };
    match contexto_de(&plano) {
        Some(ctx) => format!("{ctx}: {causa}"),
        None => {
            let mut c = causa.chars();
            match c.next() {
                Some(primera) => primera.to_uppercase().collect::<String>() + c.as_str(),
                None => plano,
            }
        }
    }
}

/// La parte en español que ya habíamos escrito nosotros, antes de la causa
/// («No se ha podido escribir /tmp/x.png: No such file…»). Se descarta si trae
/// pinta de jerga o no empieza como una frase.
fn contexto_de(s: &str) -> Option<&str> {
    let (ctx, _) = s.split_once(": ")?;
    if ctx.is_empty() || ctx.contains(['(', '{', '"', ',']) || !ctx.starts_with(char::is_uppercase)
    {
        return None;
    }
    Some(ctx)
}

/// Causa reconocida, en minúscula y con la salida para el usuario. `None`
/// si el mensaje no contiene jerga y se puede dejar como está.
fn causa_llana(s: &str) -> Option<&'static str> {
    let tiene = |aguja: &str| s.contains(aguja);
    if tiene("ObjectIndexOutOfBounds") || tiene("ObjectIndexOutOfRange") {
        return Some("ese elemento ya no está en la página; vuelve a abrir el documento");
    }
    if tiene("PageIndexOutOfBounds") || tiene("PageIndexOutOfRange") {
        return Some("esa página ya no está en el documento; ciérralo y vuelve a abrirlo");
    }
    if tiene("PasswordError") {
        return Some("la contraseña no es correcta");
    }
    if tiene("SecurityError") {
        return Some("el PDF no permite abrirse con esta contraseña");
    }
    if tiene("FormatError") {
        return Some("el PDF parece dañado; prueba con otra copia del documento");
    }
    if tiene("NotFound") || tiene("No such file or directory") {
        return Some("no se encuentra el fichero; puede que se haya movido o borrado");
    }
    if tiene("PermissionDenied") || tiene("Permission denied") {
        return Some("no hay permiso para escribir ahí; elige otra carpeta");
    }
    if tiene("No space left") || tiene("os error 28") {
        return Some("no queda espacio en el disco");
    }
    if tiene("FileError") {
        return Some("no se ha podido abrir el fichero; comprueba que sigue donde estaba");
    }
    if tiene("os error") || tiene("IoError") {
        return Some("el sistema no ha dejado terminar la operación; inténtalo de nuevo");
    }
    if tiene("image not found") || tiene("cannot open shared object") || tiene("dlopen") {
        return Some("falta el motor PDF (libpdfium); reinstala la aplicación");
    }
    if tiene("PdfiumLibraryInternalError") || tiene("PdfiumError") {
        return Some("el PDF no ha admitido este cambio; guárdalo, ciérralo y vuelve a abrirlo");
    }
    None
}

/// Traduce el error de PDFium al abrir (su `Display` es el `Debug` de Rust,
/// que no le sirve de nada al usuario).
fn mensaje_apertura(e: &PdfiumError, path: &str) -> String {
    let nombre = std::path::Path::new(path)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string());
    match e {
        // con el nombre: al abrir un reciente que ya no está, «No se
        // encuentra el fichero» a secas no dice cuál ni qué hacer
        PdfiumError::IoError(io) if io.kind() == std::io::ErrorKind::NotFound => {
            format!("No se encuentra «{nombre}»: puede que se haya movido, cambiado de nombre o borrado")
        }
        PdfiumError::IoError(io) => format!("No se ha podido leer el fichero: {io}"),
        PdfiumError::PdfiumLibraryInternalError(PdfiumInternalError::FormatError) => {
            "El fichero no es un PDF válido o está dañado".into()
        }
        PdfiumError::PdfiumLibraryInternalError(PdfiumInternalError::FileError) => {
            "No se ha podido abrir el fichero".into()
        }
        PdfiumError::PdfiumLibraryInternalError(PdfiumInternalError::SecurityError) => {
            "El PDF no permite abrirse con esta contraseña".into()
        }
        otro => format!("No se ha podido abrir el PDF: {otro}"),
    }
}

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
            Err(e) => return Err(mensaje_apertura(&e, &path)),
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
            std::fs::copy(&path, &work_path).map_err(|e| {
                mensaje_llano(format!("No se ha podido preparar el documento: {e}"))
            })?;
        }
        copias_abiertas().insert(work_path.clone());
        // el menú deja de estar atenuado en cuanto hay documento (la UI
        // llama además a `set_menu_state`, que es el contrato con ella)
        menu::refleja_documento(true);
        Ok(DocumentInfo {
            page_count,
            work_path,
            had_password,
        })
    })
}

/// Renderiza una página a PNG (bytes) con el ancho pedido en píxeles.
///
/// `con_anotaciones` es lo que Acrobat llama «Comentarios y formularios» en
/// su diálogo de impresión: con `false` salen el documento y los campos de
/// formulario rellenados, pero no los resaltados, las notas ni los trazos.
pub(crate) fn render_page_png(
    path: String,
    page_index: u16,
    width: i32,
    con_anotaciones: bool,
) -> Result<Vec<u8>, String> {
    on_pdfium_thread(move || {
        with_doc(&path, |doc| {
            let page = doc.pages().get(page_index).map_err(|e| e.to_string())?;
            let bitmap = page
                .render_with_config(
                    &PdfRenderConfig::new()
                        .set_target_width(width)
                        .render_form_data(true)
                        .render_annotations(con_anotaciones),
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
    with_annotations: Option<bool>,
) -> Result<String, String> {
    render_page_png(path, page_index, width, with_annotations.unwrap_or(true))
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
    with_annotations: Option<bool>,
) -> Result<tauri::ipc::Response, String> {
    render_page_png(path, page_index, width, with_annotations.unwrap_or(true))
        .map(tauri::ipc::Response::new)
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
    /// `/Rotate` de la página en grados horarios (0, 90, 180 o 270). El
    /// `width`/`height` de arriba ya la lleva aplicada (es el tamaño que se
    /// ve), pero la UI la necesita para girar sus overlays.
    rotation: u16,
}

/// Tamaño de todas las páginas en puntos PDF (para el layout del scroll
/// continuo sin renderizar nada) y su rotación.
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
                    rotation: Geo::de_pagina(&p).rot,
                })
                .collect())
        })
    })
}

/// Geometría de una página para convertir entre las coordenadas de la UI
/// (las del render: origen arriba-izquierda, con la rotación ya aplicada) y
/// las del PDF (origen abajo-izquierda y SIN rotar, que es donde viven los
/// `/Rect` de las anotaciones y las cajas de los objetos de página).
///
/// Por qué hace falta: `page.height()` de PDFium devuelve la altura YA
/// rotada, mientras que `annotation.bounds()` sigue en el espacio sin
/// rotar. Voltear la `y` con esa altura descoloca todo en cuanto la página
/// lleva `/Rotate` — el sello caía a 246 pt del clic en una A4 girada.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Geo {
    /// Esquina inferior izquierda de la caja de la página, sin rotar.
    x0: f32,
    y0: f32,
    /// Tamaño de la página sin rotar.
    w: f32,
    h: f32,
    /// `/Rotate` en grados horarios: 0, 90, 180 o 270.
    pub(crate) rot: u16,
}

impl Geo {
    /// A partir de la caja de la página (`[x0, y0, x1, y1]`, sin rotar) y su
    /// `/Rotate`: para el código que trabaja con lopdf y no tiene PdfPage.
    pub(crate) fn nueva(caja: &[f32; 4], rot: u16) -> Self {
        Geo {
            x0: caja[0],
            y0: caja[1],
            w: caja[2] - caja[0],
            h: caja[3] - caja[1],
            rot: rot % 360,
        }
    }

    pub(crate) fn de_pagina(page: &PdfPage) -> Self {
        let rot = page
            .rotation()
            .map(|r| r.as_degrees() as u16)
            .unwrap_or(0)
            % 360;
        // el tamaño visible lo da PDFium ya rotado: deshacemos la rotación
        // en vez de leer la caja, para no separarnos nunca del render
        let (vw, vh) = (page.width().value, page.height().value);
        let (w, h) = if rot == 90 || rot == 270 { (vh, vw) } else { (vw, vh) };
        // el origen sí sale de la caja de la página (casi siempre 0,0)
        let caja = page
            .boundaries()
            .crop()
            .or_else(|_| page.boundaries().media())
            .ok();
        let (x0, y0) = caja
            .map(|c| (c.bounds.left().value, c.bounds.bottom().value))
            .unwrap_or((0.0, 0.0));
        Geo { x0, y0, w, h, rot }
    }

    /// La misma caja pero SIN rotación: es el «espacio propio» de la
    /// página, donde trabajan los comandos que escriben. La UI convierte
    /// con la `rotation` de `get_page_sizes` antes de mandar; los que leen
    /// devuelven el espacio de la página vista.
    pub(crate) fn propia(&self) -> Geo {
        Geo { rot: 0, ..*self }
    }

    /// Los ejes de la página VISTA expresados en coordenadas PDF: hacia
    /// dónde va «a la derecha» y hacia dónde «hacia abajo» de lo que el
    /// usuario ve. Con `/Rotate 90`, por ejemplo, la derecha de la pantalla
    /// es el `+y` del papel.
    ///
    /// Lo necesitan los comandos que crean objetos de página (texto e
    /// imágenes): además de colocarlos donde se pulsa, hay que girarlos al
    /// revés que la página para que se lean derechos, igual que hace
    /// `add_stamp`.
    pub(crate) fn ejes(&self) -> ((f32, f32), (f32, f32)) {
        let o = self.ui_a_pdf(0.0, 0.0);
        let d = self.ui_a_pdf(1.0, 0.0);
        let a = self.ui_a_pdf(0.0, 1.0);
        ((d.0 - o.0, d.1 - o.1), (a.0 - o.0, a.1 - o.1))
    }

    /// Tamaño de la página SIN rotar (el del espacio propio).
    pub(crate) fn ancho(&self) -> f32 {
        self.w
    }

    pub(crate) fn alto(&self) -> f32 {
        self.h
    }

    /// Un punto de la UI a coordenadas PDF.
    pub(crate) fn ui_a_pdf(&self, x: f32, y: f32) -> (f32, f32) {
        let (ax, ay) = match self.rot {
            90 => (y, x),
            180 => (self.w - x, y),
            270 => (self.w - y, self.h - x),
            _ => (x, self.h - y),
        };
        (ax + self.x0, ay + self.y0)
    }

    /// Un punto en coordenadas PDF a las de la UI.
    pub(crate) fn pdf_a_ui(&self, px: f32, py: f32) -> (f32, f32) {
        let (ax, ay) = (px - self.x0, py - self.y0);
        match self.rot {
            90 => (ay, ax),
            180 => (self.w - ax, ay),
            270 => (self.h - ay, self.w - ax),
            _ => (ax, self.h - ay),
        }
    }

    /// Un rect de la UI a `PdfRect` (los giros son múltiplos de 90°, así que
    /// el rect sigue siendo paralelo a los ejes: basta con las esquinas).
    pub(crate) fn ui_rect_a_pdf(&self, r: &Rect) -> PdfRect {
        let (ax, ay) = self.ui_a_pdf(r.x, r.y);
        let (bx, by) = self.ui_a_pdf(r.x + r.w, r.y + r.h);
        PdfRect::new(
            PdfPoints::new(ay.min(by)),
            PdfPoints::new(ax.min(bx)),
            PdfPoints::new(ay.max(by)),
            PdfPoints::new(ax.max(bx)),
        )
    }

    /// Un `PdfRect` a rect de la UI.
    pub(crate) fn pdf_rect_a_ui(&self, r: &PdfRect) -> Rect {
        let (ax, ay) = self.pdf_a_ui(r.left().value, r.top().value);
        let (bx, by) = self.pdf_a_ui(r.right().value, r.bottom().value);
        Rect {
            x: ax.min(bx),
            y: ay.min(by),
            w: (bx - ax).abs(),
            h: (by - ay).abs(),
        }
    }
}

mod adjuntos;
mod anotaciones;
mod anotaciones2;
mod busqueda;
mod documento;
mod exportar;
mod comentarios;
mod comentarios2;
mod confianza;
mod firma;
mod firmas_visuales;
mod formularios;
mod formularios2;
mod historial;
mod menu;
mod imagenes;
mod paginas;
mod paginas2;
mod recientes;
mod recuperacion;
#[cfg(debug_assertions)]
pub mod puente_dev;
mod seguridad;
mod seguridad2;
mod texto;

/// Firma digitalmente la copia de trabajo y escribe el PDF firmado en
/// `dest_path`. Certificado y clave privada en PEM (RSA sin cifrar).
///
/// Con `rect` la firma se ve: el widget deja de ser `[0 0 0 0]` y lleva su
/// apariencia (la firma manuscrita en PNG si llega, y debajo «Firmado
/// por …» y la fecha). Sin `rect`, invisible, como hasta ahora.
// la firma es el contrato con la UI: un argumento por propiedad
#[allow(clippy::too_many_arguments)]
#[tauri::command(async)]
fn sign_pdf(
    work_path: String,
    dest_path: String,
    cert_pem_path: String,
    key_pem_path: String,
    reason: Option<String>,
    rect: Option<Rect>,
    page_index: Option<u16>,
    signer_name: Option<String>,
    signature_png: Option<String>,
) -> Result<(), String> {
    let cert_pem = std::fs::read_to_string(&cert_pem_path)
        .map_err(|e| mensaje_llano(format!("No se ha podido leer el certificado: {e}")))?;
    let key_pem = std::fs::read_to_string(&key_pem_path)
        .map_err(|e| mensaje_llano(format!("No se ha podido leer la clave: {e}")))?;
    let cred = firma::credenciales_pem(&cert_pem, &key_pem)?;
    firmar_en_hilo(
        work_path,
        dest_path,
        cred,
        reason,
        firma::Apariencia { rect, page_index, signer_name, signature_png },
    )
}

/// La firma lee la copia de trabajo: se hace en el hilo de PDFium, con el
/// caché invalidado, para no competir con una mutación concurrente.
fn firmar_en_hilo(
    work_path: String,
    dest_path: String,
    cred: firma::Credenciales,
    reason: Option<String>,
    apariencia: firma::Apariencia,
) -> Result<(), String> {
    on_pdfium_thread(move || {
        invalidate_doc_cache(&work_path);
        firma::sign(&work_path, &dest_path, &cred, reason, &apariencia).map_err(mensaje_llano)
    })
}

/// Igual que `sign_pdf` pero con un contenedor PKCS#12 (.p12/.pfx).
#[allow(clippy::too_many_arguments)]
#[tauri::command(async)]
fn sign_pdf_p12(
    work_path: String,
    dest_path: String,
    p12_path: String,
    password: String,
    reason: Option<String>,
    rect: Option<Rect>,
    page_index: Option<u16>,
    signer_name: Option<String>,
    signature_png: Option<String>,
) -> Result<(), String> {
    let bytes = std::fs::read(&p12_path)
        .map_err(|e| mensaje_llano(format!("No se ha podido leer el .p12: {e}")))?;
    let cred = firma::credenciales_p12(&bytes, &password)?;
    firmar_en_hilo(
        work_path,
        dest_path,
        cred,
        reason,
        firma::Apariencia { rect, page_index, signer_name, signature_png },
    )
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
    // «Proteger» se aplica al documento abierto y viaja con Guardar, que es
    // lo que hace Acrobat: si hay protección puesta, el fichero sale cifrado
    if let Some(p) = seguridad::proteccion_de(&work_path) {
        // la protección se anota antes de firmar y `copia_firmando` no llega
        // a mirar: sin esta comprobación, guardar cifraría un documento
        // firmado y lo dejaría con la firma rota
        if firma::esta_firmado(&work_path) {
            return Err(firma::AVISO_FIRMADO.into());
        }
        return seguridad::cifra_a(
            &work_path,
            &dest_path,
            &p.user,
            p.owner.as_deref(),
            p.permisos,
        );
    }
    // en el hilo de PDFium: nadie puede estar renombrando la copia a la vez
    on_pdfium_thread(move || {
        invalidate_doc_cache(&work_path);
        copia_firmando(&work_path, &dest_path)
    })
}

/// Copia la copia de trabajo al destino dejando `/Creator (Vitela)` en su
/// `/Info`, que es lo que hace cualquier editor con el documento que
/// escribe. Si el PDF va firmado se copia tal cual, sin tocar un byte:
/// reescribirlo con lopdf desplazaría el `/ByteRange` e invalidaría la
/// firma. Si la reescritura falla por lo que sea, también se copia tal cual:
/// guardar nunca puede depender de una marca cosmética.
fn copia_firmando(work_path: &str, dest_path: &str) -> Result<(), String> {
    let bytes = std::fs::read(work_path)
        .map_err(|e| mensaje_llano(format!("No se ha podido leer el documento: {e}")))?;
    let firmado = bytes.windows(10).any(|v| v == b"/ByteRange");
    if !firmado {
        if let Ok(mut doc) = lopdf::Document::load_mem(&bytes) {
            documento::marca_creador(&mut doc);
            let tmp = format!("{dest_path}.vitela.tmp");
            if doc.save(&tmp).is_ok() {
                if std::fs::rename(&tmp, dest_path).is_ok() {
                    return Ok(());
                }
                let _ = std::fs::remove_file(&tmp);
            } else {
                let _ = std::fs::remove_file(&tmp);
            }
        }
    }
    std::fs::write(dest_path, &bytes)
        .map_err(|e| mensaje_llano(format!("No se ha podido guardar en {dest_path}: {e}")))
}

/// Nombre del evento con el que el backend le pide a la UI que abra un
/// documento: doble clic en el Finder/Explorador, `open -a Vitela x.pdf`,
/// o el PDF pasado como argumento al arrancar. Carga `{ "path": "…" }`.
const EVENTO_ABRIR: &str = "abrir-fichero";

/// PDF que llegó antes de que la UI estuviera escuchando (arranque en frío).
static ABRIR_PENDIENTE: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

/// Primer `.pdf` existente de los argumentos de arranque. En Windows y
/// Linux el doble clic llega así; en macOS llega por `RunEvent::Opened`.
/// Se ignora el argv[0] (el propio binario) y cualquier opción.
/// Va con `OsString` (`args_os`), no con `String`: `std::env::args()` entra
/// en pánico al iterar si algún argumento no es UTF-8 válido, y un PDF con
/// el nombre en latin-1 haría cascar la app al arrancar en vez de abrirse.
fn pdf_de_argv<I: IntoIterator<Item = std::ffi::OsString>>(args: I) -> Option<String> {
    args.into_iter()
        .skip(1)
        .map(|a| a.to_string_lossy().into_owned())
        .filter(|a| !a.starts_with('-'))
        .find(|a| a.to_lowercase().ends_with(".pdf") && std::path::Path::new(a).is_file())
}

/// Ruta local de una URL de `RunEvent::Opened` (macOS manda `file://…`).
#[cfg(target_os = "macos")]
fn ruta_de_url(url: &tauri::Url) -> Option<String> {
    if url.scheme() == "file" {
        url.to_file_path()
            .ok()
            .map(|p| p.to_string_lossy().into_owned())
    } else {
        None
    }
}

/// La UI ya ha registrado su `listen` y los eventos le llegan. Lo pone
/// `ui_lista`, no `on_page_load`: la página cargada no significa que el JS
/// esté escuchando, y los eventos de Tauri no se encolan.
///
/// Va SIN `cfg`, como `ABRIR_PENDIENTE` y el comando `ui_lista`: el
/// arranque con un PDF en `argv` es de las tres plataformas. Solo
/// `RunEvent::Opened` —y con él `pide_abrir` y `ruta_de_url`— es de macOS.
static UI_LISTA: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Pide a la UI que abra `path`. Si todavía no está escuchando, se guarda
/// y lo recoge `ui_lista`: los eventos de Tauri no se encolan, y el
/// `listen` de la UI tarda un instante en registrarse.
///
/// Es el único camino, en las tres plataformas: el PDF de `argv` (arranque
/// en frío en Windows y Linux) y el de `RunEvent::Opened` (macOS, también
/// con la app ya abierta) pasan los dos por aquí. Sin `cfg`: cuanto menos
/// código dependa de la plataforma, menos se rompe la compilación de las
/// otras dos, que aquí solo se ven en CI.
fn pide_abrir(app: &tauri::AppHandle, path: String) {
    use tauri::Emitter;
    if !UI_LISTA.load(std::sync::atomic::Ordering::SeqCst) {
        // arranque en frío: la ventana existe desde `.build()`, pero el
        // `listen` de la UI todavía no. Se guarda y lo recoge `ui_lista`.
        *ABRIR_PENDIENTE.lock().unwrap_or_else(|e| e.into_inner()) = Some(path);
        return;
    }
    let _ = app.emit(EVENTO_ABRIR, serde_json::json!({ "path": path }));
}

/// La UI avisa de que ya está montada y escuchando, y se lleva de vuelta el
/// PDF que estuviera esperando (doble clic en el Finder con la app cerrada,
/// o ruta en la línea de órdenes). A partir de aquí, los ficheros que
/// lleguen con la app ya abierta van por el evento `abrir-fichero`.
#[tauri::command(async)]
fn ui_lista() -> Result<Option<String>, String> {
    UI_LISTA.store(true, std::sync::atomic::Ordering::SeqCst);
    Ok(ABRIR_PENDIENTE
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .take())
}

/// Nombre del evento con el que el backend le pregunta a la UI si se puede
/// cerrar (⌘W, el botón rojo o ⌘Q). Carga `{}`.
const EVENTO_CERRAR: &str = "cerrar-solicitado";

/// La UI ya ha dicho que se puede cerrar: el siguiente intento no se frena.
static CIERRE_CONFIRMADO: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// ¿Hay que frenar este cierre y preguntar a la UI? Solo la primera vez:
/// una vez confirmado, cerrar de verdad.
fn frenar_cierre() -> bool {
    !CIERRE_CONFIRMADO.load(std::sync::atomic::Ordering::SeqCst)
}

/// Cierra la ventana principal de verdad. Lo llama la UI cuando ya ha
/// resuelto los cambios sin guardar (los haya guardado o los descarte).
///
/// Sin esto, cerrar la ventana o salir de la app tiraba a la basura todo
/// lo hecho desde la última vez que se guardó, sin preguntar: la copia de
/// trabajo se borra en `RunEvent::Exit` y el original nunca se tocó.
#[tauri::command(async)]
fn confirmar_cierre(app: tauri::AppHandle) -> Result<(), String> {
    use tauri::Manager;
    CIERRE_CONFIRMADO.store(true, std::sync::atomic::Ordering::SeqCst);
    match app.get_webview_window("main") {
        Some(w) => w.destroy().map_err(mensaje_llano),
        None => {
            app.exit(0);
            Ok(())
        }
    }
}

/// Pide a la UI que decida sobre el cierre.
fn pregunta_por_el_cierre<E: tauri::Emitter<tauri::Wry>>(emisor: &E) {
    let _ = emisor.emit(EVENTO_CERRAR, serde_json::json!({}));
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
            // menú nativo: espejo del menú «Acciones», sin documento abierto
            menu::registra_app(app.handle());
            if let Err(e) = menu::instala(app.handle(), false) {
                eprintln!("no se ha podido montar el menú del sistema: {e}");
            }
            // PDF pasado como argumento (doble clic en Windows y Linux)
            if let Some(path) = pdf_de_argv(std::env::args_os()) {
                pide_abrir(app.handle(), path);
            }
            Ok(())
        })
        // el menú nativo no ejecuta nada: manda el id y la UI lo enruta a la
        // misma función que su botón, para que no haya dos caminos
        .on_menu_event(|app, event| menu::reenvia(app, event.id().as_ref()))
        // ⌘W y el botón rojo: no se cierra sin que la UI lo confirme
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if frenar_cierre() {
                    api.prevent_close();
                    pregunta_por_el_cierre(window);
                }
            }
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
            paginas::extract_each_page,
            paginas::delete_pages,
            paginas::rotate_pages,
            save_pdf,
            anotaciones::add_stroke,
            anotaciones::add_note,
            anotaciones::get_annotations,
            anotaciones::remove_annotation,
            anotaciones::set_annotation_contents,
            anotaciones::set_annotation_color,
            anotaciones::get_document_annotations,
            comentarios::reply_annotation,
            comentarios::set_annotation_state,
            comentarios::export_comments,
            comentarios2::export_comments_pdf,
            comentarios2::export_comments_xfdf,
            comentarios2::import_comments_xfdf,
            formularios::get_form_fields,
            formularios::set_form_text,
            formularios::set_form_checked,
            formularios::set_form_choice,
            texto::get_text_blocks,
            texto::edit_text_block,
            texto::replace_text,
            texto::add_text_block,
            texto::delete_text_block,
            texto::move_text_block,
            texto::resize_text_block,
            imagenes::get_images,
            imagenes::add_image,
            imagenes::transform_image,
            imagenes::reorder_image,
            imagenes::replace_image,
            imagenes::crop_image,
            imagenes::delete_image,
            sign_pdf,
            firma::verify_signatures,
            sign_pdf_p12,
            firmas_visuales::stamp_signature,
            firmas_visuales::import_signature_file,
            firmas_visuales::save_stored_signature,
            firmas_visuales::list_stored_signatures,
            firmas_visuales::delete_stored_signature,
            imagenes::get_image_data,
            imagenes::save_image_data,
            anotaciones2::add_markup,
            anotaciones2::add_shape,
            anotaciones2::add_stamp,
            anotaciones2::add_free_text,
            anotaciones2::add_callout,
            anotaciones2::add_measure,
            anotaciones2::erase_ink_area,
            anotaciones2::transform_annotation,
            paginas2::add_blank_page,
            paginas2::pdf_from_images,
            paginas2::duplicate_page,
            paginas2::insert_pdf_at,
            paginas2::replace_pages,
            paginas2::split_pdf,
            paginas2::merge_many,
            paginas2::crop_page,
            paginas2::add_watermark,
            paginas2::add_header_footer,
            paginas2::add_bates,
            paginas2::remove_marginal_text,
            documento::get_outline,
            documento::set_outline,
            documento::pdf_info,
            documento::get_metadata,
            documento::get_document_info,
            documento::get_page_labels,
            documento::set_page_labels,
            documento::set_metadata,
            documento::get_links,
            seguridad::encrypt_pdf,
            seguridad::remove_encryption,
            seguridad::flatten_pdf,
            seguridad::redact_area,
            seguridad2::mark_redaction,
            seguridad2::list_redactions,
            seguridad2::unmark_redaction,
            seguridad2::unmark_all_redactions,
            seguridad2::apply_redactions,
            seguridad2::sanitize_pdf,
            adjuntos::list_attachments,
            adjuntos::save_attachment,
            adjuntos::add_attachment,
            adjuntos::add_file_attachment_annotation,
            adjuntos::add_file_attachment_annotation,
            adjuntos::delete_attachment,
            adjuntos::open_attachment,
            adjuntos::list_layers,
            adjuntos::set_layer_visible,
            exportar::export_pages_png,
            exportar::export_text,
            exportar::export_docx,
            exportar::compress_pdf,
            formularios2::create_form_field,
            formularios2::create_form_fields,
            formularios2::detect_form_fields,
            formularios2::create_link,
            formularios2::delete_form_field,
            historial::undo,
            historial::redo,
            historial::history_state,
            historial::squash_history,
            recuperacion::autosave_state,
            recuperacion::borra_sesion,
            recuperacion::recover_session,
            recientes::list_recent,
            recientes::touch_recent,
            recientes::remove_recent,
            confirmar_cierre,
            ui_lista,
            menu::set_menu_state,
            close_document
        ])
        .build(tauri::generate_context!())
        .expect("error while running tauri application")
        .run(|_app, event| match event {
            tauri::RunEvent::Exit => borra_copias_abiertas(),
            // ⌘Q en macOS no siempre pasa por la ventana: si el intento de
            // salida llega antes, se frena igual y se pregunta
            tauri::RunEvent::ExitRequested { api, .. } if frenar_cierre() => {
                api.prevent_exit();
                pregunta_por_el_cierre(_app);
            }
            // macOS: doble clic en el Finder o `open -a Vitela x.pdf`,
            // tanto con la app cerrada como ya abierta
            #[cfg(target_os = "macos")]
            tauri::RunEvent::Opened { urls } => {
                if let Some(path) = urls.iter().find_map(ruta_de_url) {
                    pide_abrir(_app, path);
                }
            }
            _ => {}
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
    fn mensaje_llano_traduce_la_jerga_y_respeta_lo_nuestro() {
        // el Display de PdfiumError es el Debug de Rust, multilínea
        let e = mensaje_llano("PdfiumLibraryInternalError(\n    FormatError,\n)");
        assert_eq!(e, "El PDF parece dañado; prueba con otra copia del documento");
        assert_eq!(
            mensaje_llano("PageIndexOutOfBounds"),
            "Esa página ya no está en el documento; ciérralo y vuelve a abrirlo"
        );
        // el contexto que escribimos nosotros se conserva; la cola, no
        assert_eq!(
            mensaje_llano("No se ha podido escribir /tmp/x.png: No such file or directory (os error 2)"),
            "No se ha podido escribir /tmp/x.png: no se encuentra el fichero; puede que se haya movido o borrado"
        );
        // un mensaje ya llano pasa igual (idempotente)
        let llano = "El área de recorte es demasiado pequeña";
        assert_eq!(mensaje_llano(llano), llano);
        assert_eq!(mensaje_llano(mensaje_llano(llano)), llano);
    }

    /// **H6.** El hilo de PDFium deja de dar por hecho que hay **un**
    /// documento abierto: su caché es un mapa por copia de trabajo con un
    /// tope pequeño. Hasta el ciclo 6, abrir el segundo echaba al primero,
    /// así que trabajar con dos a la vez recargaba el otro en cada comando.
    ///
    /// Lo que este test defiende no es la velocidad, es que **no se pisan**:
    /// mutar uno no toca al otro, ni a su historial, ni a su copia.
    #[test]
    fn dos_documentos_abiertos_a_la_vez_no_se_pisan() {
        let dir = std::env::temp_dir();
        let uno = dir.join("h6-uno.pdf");
        let otro = dir.join("h6-otro.pdf");
        crea_pdf(&["Uno A", "Uno B", "Uno C"], &uno);
        crea_pdf(&["Otro A", "Otro B"], &otro);

        let a = open_pdf(uno.to_string_lossy().into_owned(), None).expect("abrir el primero");
        let b = open_pdf(otro.to_string_lossy().into_owned(), None).expect("abrir el segundo");
        assert_eq!(a.page_count, 3);
        assert_eq!(b.page_count, 2);

        // los dos se leen, alternando, sin echar al otro
        for _ in 0..3 {
            assert_eq!(get_page_sizes(a.work_path.clone()).expect("tamaños").len(), 3);
            assert_eq!(get_page_sizes(b.work_path.clone()).expect("tamaños").len(), 2);
        }

        // mutar el primero no toca al segundo
        paginas::delete_page(a.work_path.clone(), 0).expect("borrar una página del primero");
        assert_eq!(get_page_sizes(a.work_path.clone()).expect("tamaños").len(), 2);
        assert_eq!(
            get_page_sizes(b.work_path.clone()).expect("tamaños").len(),
            2,
            "el segundo se queda como estaba"
        );
        assert_eq!(
            historial::history_state(b.work_path.clone()).expect("historial").undo,
            0,
            "y sin un paso de deshacer que no ha pedido nadie"
        );

        // ⌘Z en el primero tampoco
        historial::undo(a.work_path.clone()).expect("deshacer en el primero");
        assert_eq!(get_page_sizes(a.work_path.clone()).expect("tamaños").len(), 3);
        assert_eq!(get_page_sizes(b.work_path.clone()).expect("tamaños").len(), 2);
        assert!(
            crate::tests::textos_de(std::path::Path::new(&b.work_path))[0].contains("Otro A"),
            "el segundo sigue diciendo lo suyo"
        );

        // cerrar el primero se lleva su copia y deja la del segundo
        close_document(a.work_path.clone()).expect("cerrar el primero");
        assert!(!std::path::Path::new(&a.work_path).exists(), "su copia se va");
        assert!(
            std::path::Path::new(&b.work_path).exists(),
            "la del segundo no se toca"
        );
        assert_eq!(
            get_page_sizes(b.work_path.clone()).expect("tamaños").len(),
            2,
            "y el segundo sigue funcionando con el primero cerrado"
        );

        close_document(b.work_path.clone()).expect("cerrar el segundo");
        std::fs::remove_file(&uno).ok();
        std::fs::remove_file(&otro).ok();
    }

    /// **H6.** El caché tiene tope: pasado él se suelta el documento que
    /// lleva más tiempo **sin tocarse**, no el primero que se abrió. Un PDF
    /// de 400 MB por pestaña no cabe en la memoria de nadie, y recargarlo
    /// cuesta milisegundos.
    ///
    /// Se prueba sobre `coloca`, que es donde está la decisión: el caché de
    /// verdad vive en el hilo de PDFium, que **lo comparten todos los tests**
    /// (corren en paralelo), así que mirarlo por dentro sería una carrera.
    #[test]
    fn el_cache_suelta_el_documento_que_lleva_mas_tiempo_sin_tocarse() {
        let mut cache: Vec<(String, u32)> = Vec::new();
        let mut n = 0u32;
        let mut abre = |cache: &mut Vec<(String, u32)>, nombre: &str| {
            n += 1;
            let orden = n;
            coloca(cache, nombre, || Ok(orden)).expect("colocar")
        };
        for i in 0..DOCUMENTOS_EN_CACHE {
            abre(&mut cache, &format!("doc{i}"));
        }
        assert_eq!(cache.len(), DOCUMENTOS_EN_CACHE);
        // volver a usar el primero lo pasa al final, que es el sitio del
        // más reciente
        let i = abre(&mut cache, "doc0");
        assert_eq!(i, cache.len() - 1);
        assert_eq!(cache.len(), DOCUMENTOS_EN_CACHE, "no se ha vuelto a cargar");
        assert_eq!(cache[i].1, 1, "y es el mismo documento, no otro cargado de nuevo");

        // uno más: se suelta el que llevaba más tiempo sin tocarse, que ya
        // no es el primero
        abre(&mut cache, "otro");
        assert_eq!(cache.len(), DOCUMENTOS_EN_CACHE, "el tope se respeta");
        let nombres: Vec<&str> = cache.iter().map(|(p, _)| p.as_str()).collect();
        assert!(nombres.contains(&"doc0"), "el que se volvió a usar sigue: {nombres:?}");
        assert!(!nombres.contains(&"doc1"), "el más viejo se ha soltado: {nombres:?}");
        assert!(nombres.contains(&"otro"), "y el nuevo ha entrado: {nombres:?}");
    }

    /// **R44b (AC-070) y R50b (AC-073).** CLAUDE.md se escribe a dos manos
    /// —cada ciclo lo tocan la rama de backend y la de interfaz sobre los
    /// mismos párrafos— y tres veces seguidas se ha colado el resto de una
    /// versión anterior pegado detrás de la nueva (AC-059, AC-070 y
    /// AC-073). Un párrafo que se contradice se lee peor que uno que falta,
    /// y esto no lo sufre el usuario: lo sufre el ciclo siguiente, que lee
    /// el contrato y encuentra dos.
    ///
    /// Igual que hay test cruzado para los comandos y para el menú, aquí
    /// hay uno para el documento, con tres cribas:
    ///
    /// 1. **Ninguna línea larga repetida.** Cuarenta caracteres es el
    ///    corte: por debajo son títulos, cierres de bloque y viñetas que se
    ///    repiten con razón. Es la de AC-070 y se queda porque señala la
    ///    línea exacta.
    /// 2. **Ninguna decena de palabras seguidas repetida**, con el texto
    ///    normalizado (minúsculas, sin tildes y sin puntuación) y los
    ///    bloques de código fuera. AC-073 se coló porque el duplicado
    ///    estaba **parafraseado** y empezaba a mitad de frase, así que ni
    ///    la línea ni el párrafo coincidían; diez palabras seguidas iguales
    ///    no son una coincidencia, son un párrafo copiado. Lo que de verdad
    ///    haya que decir dos veces se dice una y se referencia.
    /// 3. **La lista de ids del menú dice lo que dice
    ///    [`menu::estructura`]**, sin repetir ninguno. Esa lista **es el
    ///    contrato con la interfaz** —CLAUDE.md se declara «la única
    ///    lista»— y en el ciclo 7 acabó con cinco ids escritos dos veces.
    #[test]
    fn claude_md_no_arrastra_lineas_repetidas() {
        let ruta = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../CLAUDE.md");
        let texto = std::fs::read_to_string(&ruta)
            .unwrap_or_else(|e| panic!("no se ha podido leer {ruta:?}: {e}"));

        // 1) la línea larga repetida, que es la que se puede señalar
        let mut vistas: std::collections::HashMap<&str, Vec<usize>> = Default::default();
        for (n, linea) in texto.lines().enumerate() {
            let l = linea.trim();
            if l.chars().count() <= 40 {
                continue;
            }
            vistas.entry(l).or_default().push(n + 1);
        }
        let mut repetidas: Vec<String> = vistas
            .into_iter()
            .filter(|(_, donde)| donde.len() > 1)
            .map(|(l, donde)| format!("líneas {donde:?}: {l}"))
            .collect();
        repetidas.sort();
        assert!(
            repetidas.is_empty(),
            "CLAUDE.md arrastra líneas repetidas —casi siempre un párrafo \
             de una versión anterior pegado detrás de la nueva—:\n  {}",
            repetidas.join("\n  ")
        );

        // 2) el párrafo parafraseado: doce palabras seguidas iguales
        let palabras = palabras_normalizadas(&texto);
        const SEGUIDAS: usize = 10;
        let mut donde: std::collections::HashMap<&[String], usize> = Default::default();
        let mut calcadas: Vec<String> = Vec::new();
        if palabras.len() >= SEGUIDAS {
            for i in 0..=palabras.len() - SEGUIDAS {
                let trozo = &palabras[i..i + SEGUIDAS];
                match donde.get(trozo) {
                    Some(_) => calcadas.push(trozo.join(" ")),
                    None => {
                        donde.insert(trozo, i);
                    }
                }
            }
        }
        // los solapes de un mismo duplicado dicen todos lo mismo
        calcadas.dedup_by(|a, b| a.split(' ').skip(1).eq(b.split(' ').take(SEGUIDAS - 1)));
        assert!(
            calcadas.is_empty(),
            "CLAUDE.md repite estas decenas de palabras, así que hay un \
             párrafo dicho dos veces (lo que haya que decir dos veces se \
             dice una y se referencia):\n  {}",
            calcadas.join("\n  ")
        );

        // 3) la lista de ids del menú, que es el contrato con la interfaz
        let mut escritos: Vec<String> = Vec::new();
        let mut en_lista = false;
        for linea in texto.lines() {
            let l = linea.trim_start();
            let grupo = ["Archivo:", "Editar:", "Ver:", "Documento:", "Ayuda:"]
                .iter()
                .find_map(|g| l.strip_prefix("- ").and_then(|r| r.strip_prefix(*g)));
            let mut resto = match grupo {
                Some(r) => {
                    en_lista = true;
                    r
                }
                // la lista sigue mientras la viñeta no cambie: sin esto,
                // cualquier `codigo` del resto del documento entraría
                None if en_lista && !l.starts_with('-') && !l.is_empty() => l,
                None => {
                    en_lista = false;
                    continue;
                }
            };
            while let Some((_, tras)) = resto.split_once('`') {
                let Some((id, mas)) = tras.split_once('`') else { break };
                if !id.is_empty() && id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
                {
                    escritos.push(id.to_string());
                }
                resto = mas;
            }
        }
        assert!(escritos.len() > 40, "la lista de ids se ha leído a medias: {escritos:?}");
        let mut repes: Vec<&String> = Vec::new();
        for (i, id) in escritos.iter().enumerate() {
            if escritos[..i].contains(id) {
                repes.push(id);
            }
        }
        repes.sort();
        repes.dedup();
        assert!(
            repes.is_empty(),
            "CLAUDE.md escribe estos ids del menú dos veces, y esa lista es \
             el contrato con la interfaz: {repes:?}"
        );
        let reales: Vec<&str> = menu::estructura()
            .iter()
            .flat_map(|g| g.entradas.iter())
            .filter_map(|e| match e {
                menu::Elemento::Accion(a) => Some(a.id),
                _ => None,
            })
            .collect();
        let inventados: Vec<&String> =
            escritos.iter().filter(|id| !reales.contains(&id.as_str())).collect();
        assert!(
            inventados.is_empty(),
            "CLAUDE.md nombra ids del menú que no existen en menu::estructura(): {inventados:?}"
        );
        let sin_escribir: Vec<&&str> =
            reales.iter().filter(|id| !escritos.iter().any(|e| e == *id)).collect();
        assert!(
            sin_escribir.is_empty(),
            "estos ids del menú no están en la lista de CLAUDE.md, que es \
             donde la interfaz los busca: {sin_escribir:?}"
        );
    }

    /// El texto de CLAUDE.md en palabras comparables: sin los bloques de
    /// código (que sí repiten líneas con razón), en minúsculas, sin tildes
    /// y sin puntuación. Dos párrafos que dicen lo mismo con otra
    /// puntuación tienen que salir iguales.
    fn palabras_normalizadas(texto: &str) -> Vec<String> {
        let mut out = Vec::new();
        let mut en_codigo = false;
        for linea in texto.lines() {
            if linea.trim_start().starts_with("```") {
                en_codigo = !en_codigo;
                continue;
            }
            if en_codigo {
                continue;
            }
            for palabra in linea.split_whitespace() {
                let limpia: String = palabra
                    .chars()
                    .filter_map(|c| match c {
                        'á' | 'à' | 'ä' | 'â' => Some('a'),
                        'é' | 'è' | 'ë' | 'ê' => Some('e'),
                        'í' | 'ì' | 'ï' | 'î' => Some('i'),
                        'ó' | 'ò' | 'ö' | 'ô' => Some('o'),
                        'ú' | 'ù' | 'ü' | 'û' => Some('u'),
                        'ñ' => Some('n'),
                        c if c.is_ascii_alphanumeric() => Some(c.to_ascii_lowercase()),
                        '_' | '-' => Some(c),
                        _ => None,
                    })
                    .collect();
                if !limpia.is_empty() {
                    out.push(limpia);
                }
            }
        }
        out
    }

    /// Ningún error que llegue a la UI puede llevar jerga de Rust, de
    /// PDFium ni del sistema: quien lo lee no sabe qué es un content stream.
    #[test]
    fn los_errores_que_ve_el_usuario_no_llevan_jerga() {
        let danado = std::env::temp_dir().join("editor_pdf_test_errores_danado.pdf");
        std::fs::write(&danado, b"esto no es un PDF").expect("escribir");
        let d = danado.to_string_lossy().into_owned();
        let bueno = std::env::temp_dir().join("editor_pdf_test_errores_ok.pdf");
        crea_pdf(&["Hola"], &bueno);
        let b = bueno.to_string_lossy().into_owned();

        let casos: Vec<(&str, String)> = vec![
            ("abrir un fichero dañado", open_pdf(d.clone(), None).unwrap_err()),
            ("renderizar un fichero dañado", render_page_b64(d.clone(), 0, 100, None).unwrap_err()),
            (
                "listar anotaciones de un fichero dañado",
                anotaciones::get_annotations(d.clone(), 0).unwrap_err(),
            ),
            ("borrar una página que no existe", paginas::delete_page(b.clone(), 9).unwrap_err()),
            ("girar una página que no existe", paginas::rotate_page(b.clone(), 9).unwrap_err()),
            ("renderizar una página que no existe", render_page_b64(b.clone(), 9, 100, None).unwrap_err()),
            (
                "extraer a una carpeta que no existe",
                paginas::extract_pages(b.clone(), vec![0], "/nope/x.pdf".into(), None).unwrap_err(),
            ),
            (
                "unir con un PDF que no está",
                paginas::merge_pdf(b.clone(), "/tmp/no-existe-jamas.pdf".into()).unwrap_err(),
            ),
            (
                "guardar en una carpeta que no existe",
                save_pdf(b.clone(), "/nope/x.pdf".into()).unwrap_err(),
            ),
            (
                "exportar texto a una carpeta que no existe",
                exportar::export_text(b.clone(), "/nope/x.txt".into()).unwrap_err(),
            ),
            // las rutas de ensayo previo (dry_run) y la copia de solo
            // lectura no pasan por `mutacion`, así que necesitan traducir
            // ellas mismas: son justo las que el usuario ve antes de decidir
            (
                "pedir una imagen que no está en la página",
                imagenes::get_image_data(b.clone(), 0, 99).unwrap_err(),
            ),
            (
                "ensayar la redacción sobre un fichero dañado",
                seguridad::redact_area(
                    d.clone(),
                    0,
                    Rect { x: 0.0, y: 0.0, w: 10.0, h: 10.0 },
                    true,
                )
                .unwrap_err(),
            ),
            (
                "ensayar el borrado de encabezados sobre un fichero dañado",
                paginas2::remove_marginal_text(d.clone(), "header".into(), true).unwrap_err(),
            ),
            (
                "no poder tomar la instantánea de deshacer",
                historial::mutacion("/nope/ni/existe.pdf".to_string(), |_| Ok(())).unwrap_err(),
            ),
            (
                "listar las firmas de una carpeta que no está",
                firmas_visuales::listar_firmas_en(std::path::Path::new("/nope/firmas"))
                    .unwrap_err(),
            ),
            (
                "firmar con un certificado que no está",
                sign_pdf(b.clone(), "/tmp/f.pdf".into(), "/tmp/nope.pem".into(), "/tmp/nope.pem".into(), None, None, None, None, None)
                    .unwrap_err(),
            ),
        ];

        let jerga = [
            "PdfiumLibraryInternalError",
            "PdfiumError",
            "PageIndexOutOfBounds",
            "OutOfBounds",
            "OutOfRange",
            "IoError",
            "os error",
            "No such file",
            "Permission denied",
            "Os {",
        ];
        for (que, e) in &casos {
            for j in jerga {
                assert!(!e.contains(j), "al {que} sale jerga ({j}): {e}");
            }
            assert!(!e.contains('\n'), "al {que} el mensaje va en varias líneas: {e}");
            assert!(
                e.starts_with(char::is_uppercase),
                "al {que} el mensaje no empieza como una frase: {e}"
            );
            assert!(e.len() > 20, "al {que} el mensaje no explica nada: {e}");
        }

        std::fs::remove_file(&danado).ok();
        std::fs::remove_file(&bueno).ok();
    }

    #[test]
    fn el_cierre_se_frena_hasta_que_la_ui_lo_confirma() {
        use std::sync::atomic::Ordering;
        let antes = CIERRE_CONFIRMADO.swap(false, Ordering::SeqCst);
        assert!(
            frenar_cierre(),
            "el primer intento de cierre se para para preguntar por los cambios"
        );
        CIERRE_CONFIRMADO.store(true, Ordering::SeqCst);
        assert!(
            !frenar_cierre(),
            "una vez que la UI ha confirmado, el cierre sigue adelante"
        );
        CIERRE_CONFIRMADO.store(antes, Ordering::SeqCst);
    }

    /// Arranque en frío: el PDF del doble clic llega antes de que la UI
    /// escuche, así que espera guardado y se lo lleva `ui_lista` cuando la
    /// UI avisa. Los eventos de Tauri no se encolan: emitirlo antes era
    /// perderlo, y la ventana salía vacía.
    #[test]
    fn el_pdf_pendiente_se_lo_lleva_la_ui_al_montarse() {
        *ABRIR_PENDIENTE.lock().unwrap_or_else(|e| e.into_inner()) =
            Some("/tmp/doble-clic.pdf".into());
        assert_eq!(ui_lista().unwrap().as_deref(), Some("/tmp/doble-clic.pdf"));
        // solo una vez: el segundo montaje no reabre nada
        assert_eq!(ui_lista().unwrap(), None);
        assert!(UI_LISTA.load(std::sync::atomic::Ordering::SeqCst));
    }

    #[test]
    fn pdf_de_argv_ignora_el_binario_y_las_opciones() {
        let pdf = std::env::temp_dir().join("editor_pdf_test_argv.pdf");
        crea_pdf(&["Hola"], &pdf);
        let ruta = pdf.to_string_lossy().into_owned();

        let args = |v: &[&str]| {
            v.iter()
                .map(|s| std::ffi::OsString::from(*s))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            pdf_de_argv(args(&["/Applications/Vitela.app/…/Vitela", &ruta])),
            Some(ruta.clone())
        );
        // el argv[0] nunca cuenta, aunque acabe en .pdf
        assert_eq!(pdf_de_argv(args(&[&ruta])), None);
        // ni las opciones ni los ficheros que no están
        assert_eq!(
            pdf_de_argv(args(&["Vitela", "--flag", "/tmp/no-existe.pdf"])),
            None
        );
        assert_eq!(pdf_de_argv(args(&["Vitela"])), None);
        // un argumento que no es UTF-8 no puede hacer cascar la app
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStringExt;
            let raro = std::ffi::OsString::from_vec(vec![0xFF, 0xFE, b'.', b'p', b'd', b'f']);
            assert_eq!(
                pdf_de_argv(vec![std::ffi::OsString::from("Vitela"), raro]),
                None
            );
        }
        std::fs::remove_file(&pdf).ok();
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
        // una carpeta de adjunto abierto (open_attachment) y otra ajena
        std::fs::create_dir_all(dir.join("vitela-adjunto-123")).unwrap();
        std::fs::write(dir.join("vitela-adjunto-123").join("factura.xml"), b"x").unwrap();
        std::fs::create_dir_all(dir.join("carpeta-ajena")).unwrap();
        // con edad mínima cero se borran los nuestros; los ajenos se quedan
        assert_eq!(barre_huerfanos(&dir, std::time::Duration::ZERO), 4);
        assert!(dir.join("otro.pdf").exists());
        assert!(dir.join("vitela-notas.txt").exists());
        assert!(dir.join("carpeta-ajena").exists());
        assert!(
            !dir.join("vitela-adjunto-123").exists(),
            "la carpeta del adjunto abierto se va con su fichero dentro"
        );
        std::fs::write(dir.join("vitela-c-3.pdf"), b"x").unwrap();
        // recién creado: con 24 h de margen no se toca
        assert_eq!(barre_huerfanos(&dir, std::time::Duration::from_secs(24 * 3600)), 0);
        assert!(dir.join("vitela-c-3.pdf").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// El barrido de huérfanos no puede llevarse la copia que hay apuntada
    /// para recuperar: es justo el trabajo que se salvó del cierre bruto.
    #[test]
    fn el_barrido_respeta_la_copia_que_hay_que_recuperar() {
        let datos = std::env::temp_dir().join("vitela-test-datos-recuperacion");
        std::fs::create_dir_all(&datos).unwrap();
        let _ = firmas_visuales::DIR_DATOS.set(datos);
        // otro test puede haber fijado ya el directorio: se usa el que valga
        let datos = firmas_visuales::DIR_DATOS.get().unwrap().clone();
        std::fs::create_dir_all(&datos).unwrap();

        let dir = std::env::temp_dir().join("editor_pdf_test_barrido_sesion");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("vitela-historial")).unwrap();
        let salvada = dir.join("vitela-salvada-1.pdf");
        std::fs::write(&salvada, b"x").unwrap();
        std::fs::write(dir.join("vitela-huerfana-2.pdf"), b"x").unwrap();
        std::fs::write(
            dir.join("vitela-historial").join("vitela-salvada-1.pdf.snap1"),
            b"x",
        )
        .unwrap();
        // **H6b**: dos documentos apuntados, no uno. Con pestañas, un
        // cierre bruto deja varias copias que recuperar, y proteger la
        // primera mientras el barrido se lleva la segunda es peor que no
        // barrer nada.
        let salvada2 = dir.join("vitela-salvada-2.pdf");
        std::fs::write(&salvada2, b"x").unwrap();
        std::fs::write(
            dir.join("vitela-historial").join("vitela-salvada-2.pdf.snap1"),
            b"x",
        )
        .unwrap();
        let sesion = datos.join("sesion.json");
        let _ = std::fs::remove_file(&sesion);
        recuperacion::apunta_en(&sesion, &salvada.to_string_lossy(), "/tmp/factura.pdf", true)
            .expect("apuntar");
        recuperacion::apunta_en(&sesion, &salvada2.to_string_lossy(), "/tmp/albaran.pdf", true)
            .expect("apuntar el segundo");

        assert_eq!(
            barre_huerfanos(&dir, std::time::Duration::ZERO),
            1,
            "solo se va la huérfana de verdad"
        );
        assert!(salvada.exists(), "la copia apuntada tiene que sobrevivir");
        assert!(salvada2.exists(), "y la del segundo documento también");
        for snap in ["vitela-salvada-1.pdf.snap1", "vitela-salvada-2.pdf.snap1"] {
            assert!(
                dir.join("vitela-historial").join(snap).exists(),
                "y sus instantáneas con ellas: {snap}"
            );
        }
        let _ = std::fs::remove_file(&sesion);
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

    /// Guardar deja `/Creator (Vitela)` en el documento, como cualquier
    /// editor con lo que escribe.
    #[test]
    fn guardar_firma_el_documento_como_vitela() {
        let dir = std::env::temp_dir();
        let origen = dir.join("editor_pdf_test_creator.pdf");
        let destino = dir.join("editor_pdf_test_creator_dest.pdf");
        crea_pdf(&["Hola"], &origen);
        save_pdf(
            origen.to_string_lossy().into_owned(),
            destino.to_string_lossy().into_owned(),
        )
        .expect("guardar");

        let doc = lopdf::Document::load(&destino).expect("cargar guardado");
        let info = match doc.trailer.get(b"Info").expect("/Info") {
            lopdf::Object::Reference(rid) => doc.get_object(*rid).unwrap().as_dict().unwrap().clone(),
            lopdf::Object::Dictionary(d) => d.clone(),
            otro => panic!("/Info inesperado: {otro:?}"),
        };
        let creator = match info.get(b"Creator").expect("/Creator") {
            lopdf::Object::String(b, _) => b.iter().map(|c| *c as char).collect::<String>(),
            otro => panic!("/Creator inesperado: {otro:?}"),
        };
        assert_eq!(creator, "Vitela");
        // y el documento sigue abriéndose y con su texto
        let info = open_pdf(destino.to_string_lossy().into_owned(), None).expect("reabrir");
        assert_eq!(info.page_count, 1);
        close_document(info.work_path).expect("cerrar");

        for f in [&origen, &destino] {
            std::fs::remove_file(f).ok();
        }
    }

    #[test]
    fn errores_de_apertura_en_castellano() {
        let e = open_pdf("/no/existe/de-verdad.pdf".into(), None).unwrap_err();
        assert_eq!(
            e,
            "No se encuentra «de-verdad.pdf»: puede que se haya movido, cambiado de nombre o borrado"
        );
        let txt = std::env::temp_dir().join("vitela-no-soy-un-pdf.txt");
        std::fs::write(&txt, b"esto no es un PDF").expect("escribir txt");
        let e = open_pdf(txt.to_string_lossy().to_string(), None).unwrap_err();
        assert_eq!(e, "El fichero no es un PDF válido o está dañado");
        std::fs::remove_file(&txt).ok();
    }

    #[test]
    fn renderiza_pagina() {
        let tmp = std::env::temp_dir().join("editor_pdf_test_render.pdf");
        crea_pdf(&["Hola"], &tmp);
        let png_b64 = render_page_b64(tmp.to_string_lossy().into_owned(), 0, 200, None).expect("render");
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
            &firma::Apariencia::default(),
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
        firma::sign(&src.to_string_lossy(), &dest.to_string_lossy(), &cred, None, &firma::Apariencia::default())
            .expect("firmar con p12");
        let bytes = std::fs::read(&dest).expect("leer firmado");
        assert!(
            find_in(&bytes, b"/SubFilter/adbe.pkcs7.detached")
                || find_in(&bytes, b"/SubFilter /adbe.pkcs7.detached"),
            "el PDF firmado debe llevar el SubFilter"
        );

        // contraseña incorrecta debe fallar con un error para el usuario, sin
        // detalle técnico ni colgarse
        let e = match firma::credenciales_p12(p12, "mala") {
            Err(e) => e,
            Ok(_) => panic!("no debería abrirse con la contraseña mala"),
        };
        assert_eq!(e, "Contraseña del .p12 incorrecta");

        std::fs::remove_file(&src).ok();
        std::fs::remove_file(&dest).ok();
    }

    fn find_in(haystack: &[u8], needle: &[u8]) -> bool {
        haystack.windows(needle.len()).any(|w| w == needle)
    }


    /// Cifrar reescribe el documento entero, y eso mueve los
    /// desplazamientos que fija el /ByteRange de la firma: el PDF firmado
    /// quedaba con la firma rota y sin un aviso. `save_pdf` ya tenía el
    /// cuidado de copiar byte a byte un documento firmado, pero la rama de
    /// protección salía antes de esa comprobación.
    #[test]
    fn proteger_un_pdf_firmado_avisa_en_vez_de_romper_la_firma() {
        let dir = std::env::temp_dir();
        let src = dir.join("editor_pdf_test_firmado_proteger_src.pdf");
        let firmado = dir.join("editor_pdf_test_firmado_proteger.pdf");
        crea_pdf(&["Contrato"], &src);
        let cred = firma::credenciales_pem(
            include_str!("../fixtures/test_cert.pem"),
            include_str!("../fixtures/test_key.pem"),
        )
        .expect("credenciales");
        firma::sign(&src.to_string_lossy(), &firmado.to_string_lossy(), &cred, None, &firma::Apariencia::default())
            .expect("firmar");
        let work = firmado.to_string_lossy().into_owned();
        let antes = std::fs::read(&firmado).expect("leer firmado");

        // proteger el documento abierto
        let e = seguridad::encrypt_pdf(work.clone(), None, "secreta".into(), None, None)
            .expect_err("proteger un PDF firmado tiene que avisar");
        assert!(e.contains("firmado"), "el aviso debe decir por qué: {e}");
        assert!(!e.contains("ByteRange"), "nada de jerga: {e}");
        // …y escribir una copia protegida
        let copia = dir.join("editor_pdf_test_firmado_proteger_copia.pdf");
        seguridad::encrypt_pdf(
            work.clone(),
            Some(copia.to_string_lossy().into_owned()),
            "secreta".into(),
            None,
            None,
        )
        .expect_err("una copia protegida también rompería la firma");
        assert!(!copia.exists(), "no debe quedar un fichero a medias");

        // la puerta de atrás: protección ya anotada y después Guardar
        seguridad::anota_proteccion(
            &work,
            "secreta".into(),
            None,
            seguridad::Permisos::default(),
        );
        let dest = dir.join("editor_pdf_test_firmado_proteger_dest.pdf");
        let e = save_pdf(work.clone(), dest.to_string_lossy().into_owned())
            .expect_err("guardar cifrando rompería la firma");
        assert!(e.contains("firmado"), "el aviso al guardar: {e}");
        seguridad::olvida_proteccion(&work);

        // y el fichero firmado sigue byte a byte como estaba
        assert_eq!(
            std::fs::read(&firmado).expect("releer"),
            antes,
            "el documento firmado no se ha tocado"
        );

        std::fs::remove_file(&src).ok();
        std::fs::remove_file(&firmado).ok();
        std::fs::remove_file(&dest).ok();
    }

}
