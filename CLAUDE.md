# Editor PDF

Visor y editor de PDF de escritorio multiplataforma. Objetivo: edición real de
texto (reescribir el content stream, nunca parches encima como hace Stirling),
además de anotaciones, gestión de páginas (unir/separar/reordenar), formularios
y firma digital.

## Stack

- **Tauri 2** — shell de escritorio (Mac/Windows/Linux)
- **React + TypeScript + Vite** — UI (`src/`), gestor de paquetes `bun`
- **Rust** — core (`src-tauri/`)
- **PDFium** — motor PDF (render, texto, páginas, formularios), vía el crate
  `pdfium-render`. Binario dinámico en `src-tauri/lib/libpdfium.dylib`
  (mac-arm64, descargado de `bblanchon/pdfium-binaries`; no es código fuente,
  no editarlo)

## Comandos

```bash
bun install              # deps JS
bun run tauri dev        # app en modo desarrollo
bun run tauri build      # binario de producción
cd src-tauri && cargo check   # compilar solo el core Rust
```

`cargo` vive en `~/.cargo/bin` (instalado con rustup).

## Arquitectura

- La UI nunca toca el PDF: todo pasa por comandos Tauri (`invoke`) definidos en
  `src-tauri/src/lib.rs`.
- **Copia de trabajo**: `open_pdf(path)` copia el documento a temp y devuelve
  `{ page_count, work_path }`. Todos los demás comandos operan sobre
  `work_path`; el original solo se toca con `save_pdf(work_path, dest_path)`
  (Guardar / Guardar como). Las mutaciones guardan sobre la copia vía
  `save_over` (escribe a `.tmp` y renombra, porque PDFium lee el fichero
  abierto de forma perezosa).
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
  resaltados como `rects`), `remove_annotation`,
  `get_form_fields` / `set_form_text` / `set_form_checked` (formularios),
  `get_text_blocks` / `edit_text_block` / `add_text_block` (texto nuevo en un
  punto, Helvetica, una línea por objeto) / `delete_text_block` (edición real;
  `set_text` requiere `page.regenerate_content()` antes de guardar),
  `sign_pdf(work, dest, cert_pem, key_pem, reason)` (módulo `firma`, no usa
  PDFium; test con fixtures en `src-tauri/fixtures/`).
- Anotaciones: **el render de PDFium NO genera apariencia** para las
  anotaciones sin `/AP` (comprobado empíricamente con Highlight y Text) — la
  única vía que renderiza es añadir objetos DENTRO de la anotación
  (`FPDFAnnot_AppendObject`), que en pdfium-render 0.8 solo exponen Ink y
  Stamp (`objects_mut`). Por eso: los trazos son Ink con su path dentro
  (visibles en el render), y los resaltados/notas los pinta la UI como
  overlay con los datos de `get_annotations` (en otros visores se ven bien
  porque sí generan apariencia). Ojo si se quitan objetos de página con
  `remove_object_at_index`: NUNCA soltar el objeto devuelto — su `Drop` llama
  a `FPDFPageObj_Destroy` y PDFium casca (SIGSEGV); usar `std::mem::forget`.
- **Hilo dedicado de PDFium**: PDFium no tolera dos instancias vivas en el
  mismo proceso (la segunda inicialización se cuelga con deadlock, sin error)
  y sus tipos no son `Send`. Todo acceso pasa por `on_pdfium_thread(f)`, que
  envía el trabajo por canal a un único hilo propietario de la instancia
  (única y viva todo el proceso) y del caché del documento abierto
  (`with_doc`, invalidar con `invalidate_doc_cache` tras cada mutación).
  Nunca llamar a `pdfium()`/`with_doc` fuera de ese hilo. Gracias a esto los
  tests corren en paralelo sin precauciones y los comandos podrían pasar a
  `async` sin riesgo.
- Tauri convierte los argumentos camelCase de JS a snake_case de Rust
  automáticamente (`pageIndex` → `page_index`).
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
   handles colgantes)
7. ✅ Firma digital: campo de firma + ByteRange + PKCS#7 detached
   (RSA/SHA-256; certificado en PEM o contenedor .p12/.pfx con contraseña —
   `p12-keystore`; PDFium no firma — cirugía con lopdf y criptografía con
   RustCrypto)

## Convenciones

- Commits: mensajes limpios, sin `Co-Authored-By` ni menciones a IA/Claude.
- UI y textos de la app en español.
