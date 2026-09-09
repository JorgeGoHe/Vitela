# Vitela

Visor y editor de PDF de escritorio multiplataforma. Objetivo: edición real de
texto (reescribir el content stream, nunca parches encima como hace Stirling),
además de anotaciones, gestión de páginas (unir/separar/reordenar), formularios
y firma digital.

## Stack

- **Tauri 2** — shell de escritorio (Mac/Windows/Linux)
- **React + TypeScript + Vite** — UI (`src/`), gestor de paquetes `bun`
- **Rust** — core (`src-tauri/`)
- **PDFium** — motor PDF (render, texto, páginas, formularios), vía el crate
  `pdfium-render`. Binario dinámico en `src-tauri/lib/` (descargado de
  `bblanchon/pdfium-binaries`; no es código fuente, no editarlo):
  `libpdfium.dylib` (mac-arm64), `pdfium.dll` (win-x64) o `libpdfium.so`
  (linux-x64). **Versión fija `chromium/8009`** (`PDFIUM_TAG` en los
  workflows y en el README): otros builds cambian detalles como el nombre
  de las fuentes internas. El bundle empaqueta
  el directorio `lib/` entero como resource, así que cada plataforma lleva el
  suyo. Para compilar el instalador de Windows: workflow de GitHub Actions en
  `.github/workflows/build.yml` (descarga PDFium y compila en
  windows-latest), o en una máquina Windows: descargar `pdfium-win-x64.tgz`,
  copiar `bin/pdfium.dll` a `src-tauri/lib/` y `bun run tauri build`.

## Comandos

```bash
bun install              # deps JS
bun run tauri dev        # app en modo desarrollo
bun run tauri build      # binario de producción
cd src-tauri && cargo check   # compilar solo el core Rust
cd src-tauri && cargo test    # tests del core (corren en paralelo)
cd src-tauri && cargo clippy --all-targets -- -D warnings
bun run lint             # ESLint (rules-of-hooks y exhaustive-deps como error)
```

CI: `.github/workflows/tests.yml` corre tests, clippy, lint y build en
Linux en cada push (es donde PDFium se comporta distinto); `build.yml`
compila los instaladores a mano o al etiquetar `v*`.

`cargo` vive en `~/.cargo/bin` (instalado con rustup).

## Arquitectura

- La UI nunca toca el PDF: todo pasa por comandos Tauri (`invoke`) definidos en
  `src-tauri/src/lib.rs`.
- **Copia de trabajo**: `open_pdf(path)` copia el documento a temp y devuelve
  `{ page_count, work_path, had_password }`. Todos los demás comandos operan
  sobre `work_path`; el original solo se toca con `save_pdf(work_path,
  dest_path)` (Guardar / Guardar como). Las mutaciones guardan sobre la copia
  vía `save_and_close(doc, path)` (PDFium) o `cirugia(work_path, f)` (lopdf,
  en `lib.rs`; `cirugia_en_hilo` es su cuerpo sin el paso de historial, para
  los comandos que ya están dentro de una `mutacion` y del hilo de PDFium y
  necesitan rematar con lopdf): escriben a `.tmp`, **cierran el documento y el caché, y solo
  entonces renombran** — PDFium lee el fichero abierto de forma perezosa y
  en Windows no se puede renombrar encima de un fichero abierto. Las copias
  se registran y se borran con `close_document(work_path)` (la UI lo llama
  al abrir otro PDF y desde «Cerrar documento»), al salir de la app
  (`RunEvent::Exit`) y con un barrido al arrancar de los `vitela-*` del
  temp de más de 24 h. Si el original iba cifrado la copia está en claro:
  por eso importa no dejarlas.
- **Deshacer/rehacer** (`historial.rs`): instantáneas de la copia de
  trabajo (`<work>.snapN`, tope 20 pasos, clon en APFS) tomadas por
  `mutacion(work_path, |work_path| …)`, que **envuelve obligatoriamente todo
  comando que escriba `work_path`** (los `dry_run` no dejan paso; una
  mutación fallida retira el suyo). `undo`/`redo`/`history_state`
  devuelven `{ undo, redo, page_count }`; `squash_history(n)` funde pasos
  (encabezado + pie). La UI (`hooks/useHistorial.ts`) refresca todo con
  `afterMutation(page_count)` tras restaurar.
- Comandos: `open_pdf`, `render_page(path, page_index, width)` → PNG base64,
  `get_page_text(path, page_index)` → caracteres con cajas de glifos (puntos
  PDF, origen arriba-izquierda), `search_pdf(path, query)` → coincidencias con
  rectángulos por página, `delete_page`, `rotate_page` (90° CW acumulativo),
  `move_page(from, to)` (reconstruye el doc en el nuevo orden),
  `merge_pdf(work_path, other_path)` (añade al final),
  `extract_pages(work_path, page_indices, dest_path)`, `save_pdf`,
  `add_highlight` (quadpoints en orden spec UL,UR,LL,LR),
  `add_stroke` (anotación Ink con path object como apariencia),
  `add_note` (anotación Text), `get_annotations` (incluye los quads de los
  resaltados como `rects`, más `author` y `modified`), `remove_annotation`,
  `get_form_fields` / `set_form_text` / `set_form_checked` (formularios),
  `get_text_blocks` / `edit_text_block` / `add_text_block` (texto nuevo en un
  punto, una línea por objeto; parámetro `font` opcional — sin él se detecta
  la familia dominante de la página) / `delete_text_block` (edición real;
  `set_text` requiere `page.regenerate_content()` antes de guardar),
  `get_images` / `add_image` (tamaño natural a 72 dpi, limitado a la página) /
  `transform_image` (mover/redimensionar por ratio de bounds) /
  `replace_image` (borra + recrea en los mismos bounds) / `delete_image`,
  `sign_pdf(work, dest, cert_pem, key_pem, reason)` (módulo `firma`, no usa
  PDFium; test con fixtures en `src-tauri/fixtures/`).
- Anotaciones: PDFium **no escribe** el `/AP` de las marcas de texto. Lo
  genera en memoria al cargar el documento (por eso se veían en
  `render_page`), pero al guardar no queda nada: fuera de Vitela el
  resaltado no existía, no se imprimía y desaparecía al aplanar. Por eso
  `add_highlight` y `add_markup` rematan con un segundo pase de lopdf
  (`anotaciones::escribe_apariencia_marca`) que escribe el Form XObject a
  mano, con `/BBox` igual al `/Rect` para dibujar en coordenadas de
  página: rectángulo por quad en `/BM /Multiply` y `/CA 1` para el
  resaltado (lo que hace Acrobat), línea de 1 pt en la base o a media
  altura para subrayado y tachado, y `/F 4` en los tres. Para las notas
  (Text) sigue sin haber apariencia: las pinta la UI como overlay con los
  datos de `get_annotations`. Añadir objetos DENTRO de la anotación
  (`FPDFAnnot_AppendObject`) sigue siendo la vía de los trazos y las
  formas: en pdfium-render 0.8 solo Ink y Stamp exponen `objects_mut`.
- **Autor y fecha**: los seis comandos que crean anotaciones
  (`add_highlight`, `add_stroke`, `add_note`, `add_markup`, `add_shape`,
  `add_stamp`) aceptan `author: Option<String>`; sin él se usa el usuario
  del sistema (`USER`/`USERNAME`/`LOGNAME`). El remate lo hace
  `anotaciones::remata_annot`, el mismo pase de lopdf de la apariencia:
  escribe `/T` y `/M` (`D:YYYYMMDDHHmmSS`). `get_annotations` los
  devuelve como `author` y `modified` (ISO 8601, o vacíos).
  **El color de las anotaciones se lee
  siempre con lopdf (`anotaciones::datos_annots`, documento cacheado en el
  hilo), nunca con `stroke_color()` de pdfium-render 0.8**: cuando la anotación tiene
  `/AP` (formas, sellos, Ink, y todas tras un render) esa función castea el
  handle de anotación a objeto de página y en Linux es un SIGSEGV. Formas y
  sellos escriben `/C` al crearse por eso mismo. Ojo si se quitan objetos de página con
  `remove_object_at_index`: NUNCA soltar el objeto devuelto — su `Drop` llama
  a `FPDFPageObj_Destroy` y PDFium casca (SIGSEGV); usar `std::mem::forget`.
- **Hilo dedicado de PDFium**: PDFium no tolera dos instancias vivas en el
  mismo proceso (la segunda inicialización se cuelga con deadlock, sin error)
  y sus tipos no son `Send`. Todo acceso pasa por `on_pdfium_thread(f)`, que
  envía el trabajo por canal a un único hilo propietario de la instancia
  (única y viva todo el proceso) y de los cachés del documento abierto
  (`with_doc` para PDFium, `with_lopdf` para lopdf; invalidar con
  `invalidate_doc_cache` tras cada mutación). **Es reentrante**: dentro del
  hilo, una llamada anidada se ejecuta en línea. Nunca llamar a
  `pdfium()`/`with_doc` fuera de ese hilo.
- **Concurrencia**: los comandos son `#[tauri::command(async)]` (siguen
  siendo `fn`; Tauri los saca del hilo principal para que la ventana no se
  congele en búsquedas largas), así que dos comandos pueden solaparse. El
  hilo de PDFium es el punto de serialización de la copia de trabajo: toda
  lectura o escritura de `work_path` (también las de lopdf, `save_pdf`,
  `sign_pdf`, `encrypt_pdf`) va dentro de `on_pdfium_thread` con el caché
  invalidado antes. Escribir a un destino distinto (`export_*`, la firma a
  `dest_path`) no necesita exclusión.
- Comandos añadidos tras la hoja de ruta inicial: `add_markup`
  (subrayado/tachado), `add_shape`, `add_stamp` (anotaciones2.rs);
  `add_blank_page`, `duplicate_page`, `insert_pdf_at`, `crop_page`
  (normaliza MediaBox para no desalinear coordenadas), `add_watermark`,
  `add_header_footer` (paginas2.rs); `get/set_outline`, `get/set_metadata`,
  `get_links` (documento.rs); `encrypt_pdf` (AES-256 R6 propio con
  RustCrypto — lopdf 0.34 no escribe cifrado), `flatten_pdf`, `redact_area`
  (seguridad.rs); `export_pages_png`, `export_text`, `compress_pdf`
  (exportar.rs); `stamp_signature` + biblioteca de firmas
  (firmas_visuales.rs; `DIR_DATOS` OnceLock en vez de AppHandle);
  `get_image_data` (imagenes.rs); `create_form_field` y `create_link`
  (formularios2.rs, cirugía lopdf con `NeedAppearances`; `create_link` solo
  admite http/https/mailto y asume https si falta el esquema;
  `delete_form_field` borra el widget y su entrada de /Fields; los enlaces
  se borran con `remove_annotation`); `close_document`, `undo`, `redo`,
  `history_state`, `squash_history` (ver arriba); `list_recent` /
  `touch_recent` / `remove_recent` (recientes.rs) y `confirmar_cierre`
  (lib.rs, ver abajo).
- **Recientes** (`recientes.rs`): lista de ocho en
  `DIR_DATOS/recientes.json`. Solo se guardan ruta y fecha; `name`, `dir`
  y `exists` se recalculan al listar. `open_pdf` NO toca la lista: la
  llama la UI tras abrir con éxito (un PDF protegido cuya contraseña se
  cancela no se ha abierto). Una lista corrupta o un `DIR_DATOS` sin
  fijar devuelven vacío, nunca un error.
- **Eventos hacia la UI** (los dos con `listen`; en el navegador de QA no
  existen, los shims de `ipc.ts` los dejan en nada):
  - `abrir-fichero` con `{ path }` — doble clic en el Finder/Explorador o
    PDF en la línea de órdenes. `bundle.fileAssociations` declara la
    extensión; en Windows y Linux llega por `std::env::args()`, en macOS
    por `RunEvent::Opened` (también con la app ya abierta). Los eventos de
    Tauri no se encolan y el `listen` de la UI se registra por IPC, así
    que en arranque frío el PDF se guarda y se manda tras `on_page_load`.
    Sin test automático: se prueba con `open -a Vitela fichero.pdf`.
  - `cerrar-solicitado` con `{}` — ⌘W, el botón rojo o ⌘Q. El primer
    intento se frena (`prevent_close` en la ventana y `prevent_exit` en la
    salida, porque en macOS ⌘Q no siempre pasa por la ventana); la UI
    resuelve los cambios sin guardar y llama a `confirmar_cierre`, que
    destruye la ventana de verdad. **Si la UI no escucha este evento la
    app no se puede cerrar.**
- **Errores para humanos**: `mensaje_llano` (lib.rs) traduce la jerga de
  las librerías (el `Display` de `PdfiumError` es el `Debug` de Rust; los
  de E/S acaban en `(os error 2)`) a una frase con la causa y la salida,
  conservando el contexto en español que ya escribimos nosotros. Vive en
  los embudos por los que pasa todo comando —`mutacion`, `with_doc`,
  `with_lopdf`, `cirugia_en_hilo`— más los pocos que no pasan por
  ninguno; no hace falta llamarla en cada `map_err`.
- **Módulos del core**: `lib.rs` solo tiene la infraestructura (hilo,
  cachés, copia de trabajo, `open_pdf`, render, firma, `run()`); el resto
  por dominio: `busqueda.rs`, `paginas.rs`/`paginas2.rs`,
  `anotaciones.rs`/`anotaciones2.rs`, `formularios.rs`/`formularios2.rs`,
  `texto.rs`, `imagenes.rs`, `documento.rs`, `seguridad.rs`, `exportar.rs`,
  `firma.rs`, `firmas_visuales.rs`, `historial.rs`, `recientes.rs`,
  `puente_dev.rs`.
  `generate_handler!` y `despachar` referencian los comandos por ruta de
  módulo (con re-exports no funciona el macro).
- **Estructura de la UI**: `App.tsx` conserva el ciclo de apertura, la
  geometría del visor, atajos e impresión; el resto vive en hooks
  (`src/hooks/`: `useHistorial`, `useRenderCache`, `useMiniaturas`,
  `useBusqueda`, `useFirmas`, `useHerramienta`, `useModal`) y componentes
  (`Busqueda`, `OpcionesHerramienta`, `MenuAcciones`, `PanelPaginas`,
  `Dialogo*`). **Todo modal usa `useModal`** (Esc cierra, Enter confirma,
  foco inicial en el primer campo o en la acción principal, trampa de
  foco): un `Dialogo*` nuevo pone su `ref` y su `onKeyDown` en el `.modal`
  y no vuelve a escuchar teclas por su cuenta. `Pagina.tsx` conserva el
  render, el observer y los tres despachadores de ratón (su orden de ramas
  importa, y el doble/triple clic se despacha en el de `mousedown` por
  `e.detail`); cada dominio tiene su
  hook en `src/hooks/pagina/` (`useSeleccionTexto`, `useEnlaces`,
  `useTexto`, `useFormularios`, `useImagenes`, `useAnotaciones`,
  `useAreas`, más `geometria.ts` puro) y su capa en
  `src/components/pagina/`. Los hooks se llaman `use…` (lo exige
  rules-of-hooks) aunque el resto del identificador vaya en español.
- **Eventos de ventana** (`src/ipc.ts`, todos detrás de `hayTauri` y
  no-op en el navegador de QA): `onAbrirFichero` (evento `abrir-fichero`:
  doble clic en el Finder o argumento de arranque), `onArrastreFicheros`
  (`tauri://drag-enter|leave|drop`; el `drop` de HTML5 no trae la ruta del
  fichero, así que el gesto solo existe dentro de Tauri) y
  `onCerrarSolicitado` (evento `cerrar-solicitado`; la UI responde con el
  comando `confirmar_cierre`, y en QA se dispara con
  `window.__vitelaCerrar()`).
- **Preferencias y memoria de la UI** en `localStorage` (`src/tipos.ts`):
  colores por acción, opciones de búsqueda (`Aa` y `|ab|`) y preferencias
  (`autor` de los comentarios, que se manda como `author` en cada comando
  que crea una anotación).
- **Atajos de teclado** (`App.tsx`, un solo `useEffect`; con un modal
  abierto solo pasa Escape): ⌘O abrir · ⌘S guardar · ⇧⌘S guardar como ·
  ⌘P imprimir · ⌘D propiedades · ⌘, preferencias · ⌘F buscar · ⌘G y ⇧⌘G
  coincidencia siguiente/anterior · ⌘Z y ⇧⌘Z deshacer/rehacer · ⌘+ y ⌘−
  zoom (sin Shift: ⇧⌘+/⇧⌘− quedan para girar la vista) · ⌘0 ajustar ·
  ⌘1 al 100 % · ⇧⌘N ir a la página · ←/→ página anterior y siguiente ·
  Esc sale de cualquier herramienta · Supr borra la anotación
  seleccionada · ⌘A todo el texto de la página · ⌘C copiar la selección.
- **QA como usuario real**: `src/ipc.ts` y `src/dialogos.ts` son shims — en
  Tauri delegan en la API oficial; en un navegador normal hablan con el
  puente HTTP de desarrollo (`src-tauri/src/puente_dev.rs`, puerto 1422,
  solo debug). Sesión: `bun run dev` + `bun run qa:puente` (binario
  `puente`, sin ventana) + el navegador de la skill browse contra
  `http://localhost:1420`. Diálogos de fichero: se encolan con
  `POST /qa/dialogo {"value": ...}`; fixtures con `POST /qa/fixture`.
  Al añadir un comando: registrarlo en `generate_handler!` Y en el match de
  `puente_dev::despachar`. Informes en `.gstack/qa-reports/`.
- Tauri convierte los argumentos camelCase de JS a snake_case de Rust
  automáticamente (`pageIndex` → `page_index`).
- **Enlaces y CSP**: los URI del PDF son contenido no confiable. La UI
  (`src/enlaces.ts`) solo abre http/https/mailto y pide confirmación
  mostrando el dominio; el permiso del opener en `capabilities/default.json`
  está acotado a esos esquemas. `tauri.conf.json` lleva una CSP real
  (`csp` y `devCsp`, esta última con Vite, HMR y el puente de QA): no hay
  recursos remotos (fuentes locales en `src/assets/fonts/`).
- PDFium se carga en runtime: primero desde los resources del bundle
  (producción; `bundle.resources` en `tauri.conf.json` + `RESOURCE_LIB_DIR`
  fijado en el setup), después desde `./lib/` relativo al cwd (que en
  `tauri dev` y `cargo test` es `src-tauri/`), y por último la librería del
  sistema.
- Rendimiento: el documento abierto se cachea en el hilo de PDFium (ver
  arriba) y la UI cachea los renders por `(docVersion, página, zoom)` con
  prefetch de las páginas adyacentes.
- Permisos de plugins en `src-tauri/capabilities/default.json`.

## Hoja de ruta

1. ✅ Visor básico (render por página, zoom, navegación)
2. ✅ Selección de texto y búsqueda (cajas de glifos de PDFium)
3. ✅ Páginas: unir, separar, reordenar, rotar, borrar (`FPDF_ImportPages`)
4. ✅ Anotaciones: resaltado, dibujo, notas (`FPDFPage_CreateAnnot`)
5. ✅ Formularios AcroForm (leer/rellenar texto y casillas)
6. ✅ Edición de texto real: reescribir el objeto de texto del content stream
   (por bloque, misma fuente, sin reflujo entre páginas). Multilínea: las
   líneas extra se insertan como objetos nuevos con fuente estándar
   aproximada por familia/estilo, colocados debajo (no se reutiliza el handle
   de `FPDFTextObj_GetFont`: queda ligado a la página y PDFium casca con
   handles colgantes). Fuentes (`fuente_por_nombre`): estándar aproximada →
   TTF de /System/Library/Fonts/Supplemental (best effort; ojo: los TTF
   cargados con `FPDFText_LoadFont` no llevan ToUnicode y su extracción
   pierde los no-ASCII) → Helvetica. "Arial" se mapea a Helvetica (la
   builtin de PDFium se identifica como «Arial» o, desde chromium/8000,
   «Chrom Sans OTF»; `normaliza_familia` devuelve siempre las estándar).
   Imágenes: insertar, mover,
   redimensionar, reemplazar y borrar objetos de imagen
7. ✅ Firma digital: campo de firma + ByteRange + PKCS#7 detached
   (RSA/SHA-256; certificado en PEM o contenedor .p12/.pfx con contraseña —
   `p12-keystore`; PDFium no firma — cirugía con lopdf y criptografía con
   RustCrypto)

## Convenciones

- Commits: mensajes limpios, sin `Co-Authored-By` ni menciones a IA/Claude.
- UI y textos de la app en español.

## Sistema de diseño

Lee SIEMPRE DESIGN.md antes de cualquier decisión visual o de UI. Fuentes,
colores, spacing y dirección estética están definidos ahí. No te desvíes sin
aprobación explícita del usuario. En modo QA, señala cualquier código que no
case con DESIGN.md.
