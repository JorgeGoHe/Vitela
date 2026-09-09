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
- **Guardar**: `save_pdf` copia la copia de trabajo al destino dejando
  `/Creator (Vitela)` en su `/Info`. Un PDF **firmado** (lleva `/ByteRange`)
  se copia byte a byte, sin pasar por lopdf: reescribirlo movería el
  `/ByteRange` e invalidaría la firma. Si hay protección puesta (ver
  «Protección»), en vez de copiar cifra al destino.
- **Documento sin ruta**: unir varios PDF soltados produce un documento
  nuevo con `originalPath = null` (la barra lo llama «Documento
  combinado»); ⌘S y el botón Guardar caen entonces en Guardar como, para
  no escribir encima de ninguno de los originales.
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
- Comandos: `open_pdf`, `render_page(path, page_index, width, with_annotations?)`
  → PNG (`with_annotations` por defecto `true`; con `false` salen el
  documento y los campos de formulario rellenados pero no los
  comentarios, que es «Solo el documento» del diálogo de impresión de
  Acrobat),
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
  `sign_pdf(work, dest, cert_pem, key_pem, reason, rect?, page_index?,
  signer_name?, signature_png?)` y `sign_pdf_p12` (módulo `firma`, no usa
  PDFium; test con fixtures en `src-tauri/fixtures/`), `verify_signatures`.
- **Coordenadas y páginas rotadas** (`Geo`, en `lib.rs`): hay dos espacios y
  cada comando usa uno, a propósito.
  - **Espacio de la página vista**: el del render, con el `/Rotate` ya
    aplicado, origen arriba a la izquierda. Es donde la UI dibuja.
  - **Espacio propio de la página**: el de la caja de la página sin rotar,
    también con origen arriba a la izquierda para la UI. Es donde viven de
    verdad los `/Rect` de las anotaciones y las cajas de los objetos.
  - **Leen en el espacio de la página vista** (la UI los pinta tal cual):
    `get_annotations`, `get_document_annotations`, `get_links`,
    `get_form_fields`.
  - **Escriben en el espacio propio de la página** (la UI convierte con la
    `rotation` de `get_page_sizes` antes de mandar): `add_highlight`,
    `add_markup`, `add_stroke`, `add_note`, `add_shape`, `add_stamp`,
    `add_free_text`, `transform_annotation`, `create_link`,
    `create_form_field`, `redact_area`, `crop_page`, `add_image`,
    `transform_image`, `add_text_block`, `stamp_signature`.
  - **Leen en el espacio propio de la página** (la UI convierte al leer):
    `get_page_text`, `get_text_blocks`, `get_images`, `search_pdf`. Los
    cuatro voltean la `y` con `Geo::de_pagina(&page).propia()`, nunca con
    `page.height()`: su respuesta no depende del `/Rotate` (AC-014,
    AC-034).
  - **Los objetos nuevos se giran al revés que la página**: `add_text_block`
    y `add_image` colocan el ancla con la altura propia y giran el objeto
    `−/Rotate` (`Geo::ejes()` da hacia dónde va «a la derecha» y «hacia
    abajo» de la vista, en coordenadas del papel) para que el texto y las
    imágenes se lean derechos, igual que `add_stamp` (AC-035).
  - `get_page_sizes` devuelve `width`/`height` **ya rotados** (el tamaño tal
    como se ve) más `rotation` en grados horarios (0/90/180/270).
  - Ojo con `page.height()` de pdfium-render: devuelve la altura **ya
    rotada** mientras que `annotation.bounds()` sigue sin rotar. Usarla para
    voltear la `y` desplazaba las anotaciones 246 pt en una A4 girada
    (AC-014). `Geo::de_pagina` da el espacio de la vista y `.propia()` el de
    la página; nunca volver a mezclar.
  - Excepción visible: `add_stamp` cruza su caja y gira su texto al revés
    que la página para que se lea derecho en pantalla, como Acrobat. El
    punto de anclaje sigue siendo el que manda la UI, en el espacio propio.
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
  Las notas llevan `/Name /Comment` y una ventana `/Popup` con `/Open
  false`, que es el post-it que enseñan Acrobat y Vista Previa. **El popup
  no es un comentario**: `get_annotations` se lo salta (por `kind`) y
  `remove_annotation` lo borra junto a su nota para no dejarlo huérfano en
  `/Annots`.
- `transform_annotation` mueve y redimensiona cuatro tipos, por tres
  caminos: **Stamp** e **Ink** transforman los objetos que llevan dentro;
  **FreeText** se mueve y se redimensiona y su `/AP` se vuelve a dibujar
  (se dibuja en local, `/BBox 0 0 w h`); **Text** solo se mueve, porque el
  icono del post-it tiene tamaño fijo también en Acrobat y del rect nuevo
  solo se toma la esquina.
- **WinAnsi** (`anotaciones2::winansi`): el tramo 0x80–0x9F NO es latin-1;
  ahí es donde WinAnsiEncoding guarda la raya «—», el guion «–», los
  puntos suspensivos «…», las comillas tipográficas y el «€». Lo usan el
  `/AP` de los cuadros de texto y el de la firma visible.
- **Autor y fecha**: los seis comandos que crean anotaciones
  (`add_highlight`, `add_stroke`, `add_note`, `add_markup`, `add_shape`,
  `add_stamp`) aceptan `author: Option<String>`; sin él se usa el usuario
  del sistema (`USER`/`USERNAME`/`LOGNAME`). El remate lo hace
  `anotaciones::remata_annot`, el mismo pase de lopdf de la apariencia:
  escribe `/T` y `/M` (`D:YYYYMMDDHHmmSS` **más el desfase horario**:
  `+HH'mm'` o `Z`; sin él, dos comentarios de husos distintos se ordenan mal
  en cualquier revisor). `get_annotations` los devuelve como `author` y
  `modified` (ISO 8601 con su zona, o vacíos). Al crear se escribe también
  `/CreationDate` con esa misma hora: PDFium pone una suya en UTC y en el
  mismo objeto quedaban dos horas distintas. `remata_annot_en` hace lo
  mismo sobre una anotación concreta y conserva el `/T` que hubiera: es lo
  que usa `transform_annotation` para refrescar `/M` al mover un
  comentario (PDFium escribe una fecha suya, en UTC, que se reescribe).
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
  `get_links` (documento.rs); `set_annotation_contents`,
  `set_annotation_color`, `get_document_annotations`, `add_free_text`
  (comentarios editables, panel de comentarios y cuadro de texto);
  `delete_pages`, `rotate_pages` (lote, un solo paso de deshacer) y
  `extract_pages` con `delete_after`; `set_form_choice` (desplegables y
  listas, con `options` en `get_form_fields`); `remove_encryption`;
  `encrypt_pdf` (AES-256 R6 propio con RustCrypto — lopdf 0.34 no escribe
  cifrado; `dest_path` opcional: sin él NO cifra la copia de trabajo, anota
  la protección y la aplica `save_pdf` al guardar; acepta `permisos`; ver
  «Protección» abajo), `flatten_pdf`, `redact_area`
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
- Comandos del ciclo 2:
  - `set_annotation_contents(work_path, page_index, annot_index, contents,
    author?)` y `set_annotation_color(work_path, page_index, annot_index,
    color)` (anotaciones.rs): corregir y recolorear un comentario ya creado,
    al instante y sin «Aceptar», como las propiedades de Acrobat. El color
    va en `/C` (y `/CA` si no es opaco) y además recolorea lo que la
    anotación lleve dibujado dentro (Ink y Stamp); las marcas de texto
    regeneran su `/AP` y los cuadros de texto su `/DA` y su `/AP`.
  - `get_document_annotations(path)` → `AnnotationInfo` + `page_index` (JSON
    plano) de **todo** el documento en una pasada, para el panel de
    comentarios: pedirlas página a página serían N viajes por el canal del
    hilo de PDFium.
  - `add_free_text(work_path, page_index, rect, text, font_size, color,
    border, author?)` (anotaciones2.rs): el «Cuadro de texto» de Acrobat,
    comentario `FreeText` encima del documento (no toca el content stream,
    a diferencia de `add_text_block`). Se construye entero con lopdf, con
    `/AP` propio en Helvetica WinAnsi; corregir su texto o su color rehace
    esa apariencia (`regenera_freetext`), que si no cambiaría el dato y no
    lo que se ve.
  - `delete_pages(work_path, page_indices)` → nuevo total y
    `rotate_pages(work_path, page_indices, quarter_turns)` con signo (±1,
    ±2, ±3) (paginas.rs): el lote entero en una sola mutación, un solo ⌘Z.
    Borrar va de mayor a menor y se niega a dejar el documento sin páginas.
    `extract_pages` acepta `deleteAfter` para llevarse las páginas del
    original dentro de la misma operación.
  - `set_form_choice(work_path, page_index, field_index, value)`
    (formularios.rs): elige una opción de un desplegable o una lista.
    `field_index` es el `annot_index` de `get_form_fields`, que ahora
    devuelve también `options` y trata `ComboBox` y `ListBox`. Va con lopdf
    (`/V`, `/I` en las listas, `NeedAppearances`) porque pdfium-render 0.8
    lee las opciones pero no deja escribir el valor.
  - `remove_encryption(work_path)` y `encrypt_pdf` con `dest_path`
    opcional y `permisos` (seguridad.rs, ver «Protección»).
  - Los flags opcionales se declaran `Option<T>` en la firma del comando y
    no solo en el puente: `search_pdf(match_case, whole_word)`,
    `extract_pages(delete_after)`, `encrypt_pdf(dest_path, permisos)`. Un
    `invoke` que no mande el campo tiene que funcionar.
- Comandos del ciclo 3:
  - `verify_signatures(path)` → un `FirmaInfo` por cada campo `/Sig`
    (`firma.rs`): comprueba que el `/ByteRange` cubre el fichero entero
    salvo el hueco de su `/Contents` (`covers_whole_file`), que el SHA-256
    de esos rangos es el `messageDigest` firmado del PKCS#7 y que la firma
    RSA de los atributos firmados la hizo la clave del certificado
    embebido (las dos cosas juntas son `digest_ok`), y saca del
    certificado `cert_subject`, `cert_issuer`, `not_before`, `not_after`,
    `expired` y `self_signed`, más `page_index` y `rect` del widget (en el
    espacio de la página vista). **No hay cadena de confianza**: no se
    consulta el llavero del sistema, así que la UI no puede decir
    «válida», solo «firmado por X, el documento no ha cambiado desde la
    firma» y, si no hay raíz, «certificado no verificado».
  - `sign_pdf` / `sign_pdf_p12` con `rect`, `page_index`, `signer_name` y
    `signature_png` (base64): con `rect` el widget deja de ser `[0 0 0 0]`
    y lleva su `/AP` —un Form XObject con el PNG de la firma manuscrita
    (incrustado con su alfa en `/SMask`) y debajo «Firmado por …» y la
    fecha en Helvetica— más `/DA` y `/F 132`. Sin `rect`, invisible, como
    antes. El diccionario de firma gana `/Name`.
  - **Redacción en dos fases** (`seguridad2.rs`): `mark_redaction(work,
    page_index, rect)` → índice de la marca en `/Annots`,
    `list_redactions(work)` → `{ page_index, annot_index, rect }` (rect en
    el espacio de la página vista), `unmark_redaction(work, page_index,
    mark_index)` y `apply_redactions(work, dry_run)` → `{ zonas, textos,
    imagenes }`. Las marcas son anotaciones `/Square` con `/C [1 0 0]`,
    `/IC [0 0 0]`, su `/AP` de borde rojo y la clave propia
    `/Vitela /Redact`: sobreviven a guardar, se ven en cualquier visor
    como lo que son (una propuesta, no una censura) y la UI las mueve y
    las borra con `transform_annotation` y `remove_annotation`, como
    cualquier comentario. Aplicar es UNA mutación para todo el lote.
  - `sanitize_pdf(work, dry_run)` → `{ metadatos, scripts, adjuntos,
    capas, formularios }` (`seguridad2.rs`): quita `/Info` y el XMP, los
    `/JavaScript` del `/Names`, `/OpenAction` y `/AA`, los
    `/EmbeddedFiles` y las anotaciones `/FileAttachment`, `/OCProperties`
    y el `/AcroForm`, y **poda los objetos** (`prune_objects`): quitar la
    referencia dejaría el script y el adjunto dentro del fichero, sin
    nadie que apuntara a ellos. Limitación conocida: lo que va dentro de
    un content stream marcado con BDC de una capa apagada no se toca.
  - `replace_pages(work, page_indices, other_path, other_indices?)`,
    `split_pdf(work, dest_dir, modo, cada?)` (modo `"cada"` o
    `"marcadores"`, ficheros `parte-N.pdf`; escribe fuera, así que no muta
    ni deja paso de deshacer) y `merge_many(work, others, at?)`
    (`paginas2.rs`), cada uno en una sola mutación.
  - `extract_each_page(work, page_indices, dest_dir, delete_after?)`
    (`paginas.rs`): «un fichero por página» (`pagina-N.pdf`) entero en una
    operación — escribe todos los ficheros primero y borra dentro de la
    misma mutación, así que un fallo a mitad no deja el trabajo hecho a
    medias ni dos pasos de deshacer.
  - `set_menu_state(has_document)` (`menu.rs`, ver «Menú nativo»).
- **Menú nativo** (`menu.rs`): Archivo, Editar, Ver, Documento, Ventana y
  Ayuda en la barra del sistema, espejo del menú «Acciones» de la app —
  con esto la búsqueda de menús de macOS encuentra por fin «Marca de
  agua». **No ejecuta nada**: cada entrada emite el evento `menu-accion`
  con `{ id }` y la UI lo enruta a la misma función que su botón, para que
  no haya dos caminos que puedan separarse. La estructura es un dato
  (`menu::estructura()`), con un test que comprueba que no hay ids
  repetidos ni entradas sin etiqueta. `set_menu_state(has_document)`
  vuelve a montar el menú para atenuar lo que no aplica (en Acrobat se
  atenúa, no desaparece). **Ojo**: las entradas llevan su acelerador, así
  que en macOS el sistema se queda con ⌘S, ⌘Z, ⌘P… antes que el webview;
  si la UI no escucha `menu-accion`, esos atajos dejan de funcionar. Los
  ids, que son el contrato con la UI:
  - Archivo: `abrir`, `abrir-reciente`, `guardar`, `guardar-como`,
    `cerrar-documento`, `anadir-pdf`, `insertar-pdf`, `combinar-ficheros`,
    `reemplazar-paginas`, `extraer-paginas`, `dividir-documento`,
    `imprimir`.
  - Editar: `deshacer`, `rehacer`, `copiar`, `seleccionar-todo`, `buscar`,
    `buscar-siguiente`, `buscar-anterior`, `preferencias`.
  - Ver: `zoom-mas`, `zoom-menos`, `zoom-pagina`, `zoom-100`,
    `zoom-ancho`, `pagina-una`, `pagina-continua`, `pagina-dos`,
    `pagina-dos-continua`, `girar-vista-derecha`, `girar-vista-izquierda`,
    `panel-lateral`, `pantalla-completa`, `modo-nocturno`.
  - Documento: `organizar-paginas`, `recortar-pagina`, `marca-de-agua`,
    `encabezado-pie`, `quitar-marca-de-agua`, `quitar-encabezados`,
    `anadir-campo`, `anadir-enlace`, `firmar`, `proteger`,
    `quitar-proteccion`, `aplanar`, `redactar`, `sanitizar`,
    `propiedades`, `exportar-imagenes`, `exportar-texto`, `comprimir`.
  - Ayuda: `atajos`.
- **Protección** (`seguridad.rs`): `encrypt_pdf` compone la máscara `/P`
  del spec a partir de `permisos { imprimir, copiar, editar }` (los tres a
  `true` por defecto): bit 3 imprimir —y con él el 12, alta calidad—, bit 5
  copiar, bits 4, 6, 9 y 11 editar/comentar/rellenar/montar; el bit 10
  (accesibilidad) se queda siempre puesto, como en Acrobat. Con `dest_path`
  escribe una copia protegida; **sin él la protección se anota para el
  documento abierto y la aplica `save_pdf`**. No se cifra la copia de
  trabajo en el sitio a propósito: quedaría ilegible para el resto de
  comandos (PDFium pediría la contraseña en cada render) y el documento en
  pantalla dejaría de funcionar. La anotación vive en un mapa por
  `work_path` (`proteccion_de` / `olvida_proteccion`, que llama
  `borra_copia` al cerrar) y por eso **no entra en el historial**: ⌘Z no la
  quita, la quita `remove_encryption` (que sí pasa por `mutacion`, así que
  deja su paso). **Un PDF firmado no se cifra**: cifrar reescribe el
  documento y movería el `/ByteRange`, así que `encrypt_pdf` (con
  `dest_path` y sin él) y `save_pdf` con protección anotada se niegan con
  `firma::AVISO_FIRMADO` en vez de romper la firma.
- **Aplanar y casillas**: el marco de un widget lo pinta el entorno de
  formularios de PDFium desde `/MK` y `/BS` al vuelo, así que
  `FPDFPage_Flatten` no tiene nada que copiar y la casilla sin marcar
  desaparecía al aplanar. El marco va dentro de la apariencia:
  `create_form_field` lo dibuja en sus dos estados y `prepara_para_aplanar`
  genera el `/AP /N` de los widgets `Btn` que no lo lleven (los PDFs de
  fuera) desde `/MK` (`/BG`, `/BC`) y `/BS` (`/W`).
- **Recientes** (`recientes.rs`): lista de ocho en
  `DIR_DATOS/recientes.json`. Solo se guardan ruta y fecha; `name`, `dir`
  y `exists` se recalculan al listar. `open_pdf` NO toca la lista: la
  llama la UI tras abrir con éxito (un PDF protegido cuya contraseña se
  cancela no se ha abierto). La UI **revalida la lista cada vez que la
  enseña** (al desplegar el menú y al volver al estado vacío) y, si abrir
  un reciente falla porque ya no está, lo quita con `remove_recent`. Una lista corrupta o un `DIR_DATOS` sin
  fijar devuelven vacío, nunca un error.
- **Eventos hacia la UI** (los dos con `listen`; en el navegador de QA no
  existen, los shims de `ipc.ts` los dejan en nada):
  - **Arranque con fichero**: al montar, la UI llama al comando `ui_lista()`
    (sin argumentos, devuelve `string | null`) y abre la ruta que traiga.
    Los eventos de Tauri no se encolan, así que el arranque en frío (doble
    clic en el Finder con la app cerrada) no puede depender del evento; el
    evento sirve para los ficheros que llegan con la app ya abierta.
  - `abrir-fichero` con `{ path }` — doble clic en el Finder/Explorador o
    PDF en la línea de órdenes. `bundle.fileAssociations` declara la
    extensión; en Windows y Linux llega por `std::env::args_os()` (con
    `args()` un nombre que no sea UTF-8 hace cascar la app al arrancar), en
    macOS por `RunEvent::Opened` (también con la app ya abierta). **Los
    eventos de Tauri no se encolan** y el `listen` de la UI se registra por
    IPC, así que en arranque en frío el PDF se guarda en `ABRIR_PENDIENTE`
    y se lo lleva la UI con el comando `ui_lista()`, que llama al montarse
    y que devuelve `Option<String>` con el fichero que esperaba. Ese
    comando es también el que enciende la bandera «la UI ya escucha»: a
    partir de ahí los ficheros van por el evento. Ojo con
    `webview_windows().is_empty()` como guardia de arranque en frío: la
    ventana se crea en `.build()`, antes de `RunEvent::Opened`, así que
    nunca es cierta. Sin test automático de extremo a extremo: se prueba
    con `open -a Vitela fichero.pdf`, con la app cerrada y con ella
    abierta.
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
  ninguno; no hace falta llamarla en cada `map_err`. **Las rutas de ensayo
  previo (`dry_run`) y las copias de solo lectura no pasan por `mutacion`**:
  ahí sí hay que llamarla a mano (`get_image_data`, y `redact_area` y
  `remove_marginal_text` con `dry_run`), y son justo las que el usuario ve
  antes de decidir. El test `los_errores_que_ve_el_usuario_no_llevan_jerga`
  las cubre.
- **Módulos del core**: `lib.rs` solo tiene la infraestructura (hilo,
  cachés, copia de trabajo, `open_pdf`, render, firma, `run()`); el resto
  por dominio: `busqueda.rs`, `paginas.rs`/`paginas2.rs`,
  `anotaciones.rs`/`anotaciones2.rs`, `formularios.rs`/`formularios2.rs`,
  `texto.rs`, `imagenes.rs`, `documento.rs`, `seguridad.rs`/`seguridad2.rs`,
  `exportar.rs`, `firma.rs`, `firmas_visuales.rs`, `historial.rs`,
  `recientes.rs`, `menu.rs`, `puente_dev.rs`.
  `generate_handler!` y `despachar` referencian los comandos por ruta de
  módulo (con re-exports no funciona el macro).
- **Estructura de la UI**: `App.tsx` conserva el ciclo de apertura, la
  geometría del visor, atajos e impresión; el resto vive en hooks
  (`src/hooks/`: `useHistorial`, `useRenderCache`, `useMiniaturas`,
  `useBusqueda`, `useFirmas`, `useHerramienta`, `useModal`) y componentes
  (`Busqueda`, `OpcionesHerramienta`, `MenuAcciones`, `PanelPaginas`,
  `PanelMarcadores`, `PanelComentarios`, `Dialogo*`). El sidebar tiene tres
  pestañas —Páginas, Marcadores y Comentarios— y por eso mide 200 px.
  **Todo modal usa `useModal`** (Esc cierra —también con el foco fuera del
  diálogo, gracias a un listener en fase de captura—, Enter confirma, foco
  inicial en el primer campo o en la acción principal, trampa de foco): un
  `Dialogo*` nuevo pone su `ref` y su `onKeyDown` en el `.modal` y no vuelve
  a escuchar teclas por su cuenta. `Pagina.tsx` conserva el
  render, el observer y los tres despachadores de ratón (su orden de ramas
  importa, y el doble/triple clic se despacha en el de `mousedown` por
  `e.detail`); cada dominio tiene su
  hook en `src/hooks/pagina/` (`useSeleccionTexto`, `useEnlaces`,
  `useTexto`, `useFormularios`, `useImagenes`, `useAnotaciones`,
  `useAreas`, más `geometria.ts` puro) y su capa en
  `src/components/pagina/`. Los hooks se llaman `use…` (lo exige
  rules-of-hooks) aunque el resto del identificador vaya en español.
- **Dos sistemas de coordenadas** (`hooks/pagina/geometria.ts`): el
  **espacio de la vista** —el del render, el ratón y todos los overlays— y
  el **espacio propio de la página**, que es en el que están escritas las
  anotaciones. Se diferencian solo si la página lleva `/Rotate`
  (`get_page_sizes` devuelve `rotation` por página y `width`/`height` ya
  girados). `puntoAPagina`/`rectAPagina` convierten los gestos del ratón
  antes de mandarlos a cualquier comando que escriba;
  `puntoAVista`/`rectAVista` traen al overlay lo que llega en espacio de
  página (`get_page_text`, `get_text_blocks`, `get_images`).
  `get_annotations`, `get_links` y `get_form_fields` ya llegan en espacio
  de vista y no se tocan. Con `/Rotate 0` todo es la identidad.
- **Giro de la vista** (`viewRotation`, estado de `App`): ⇧⌘+ y ⇧⌘− giran
  la hoja con un `transform` y la caja exterior intercambia alto y ancho;
  no toca el fichero, no marca el documento como modificado y se pierde al
  cerrar. `pagePoint`/`puntoEnCapa` lo deshacen antes de traducir un gesto.
- **Eventos de ventana** (`src/ipc.ts`, todos detrás de `hayTauri` y
  no-op en el navegador de QA): `onAbrirFichero` (evento `abrir-fichero`:
  doble clic en el Finder o argumento de arranque), `onArrastreFicheros`
  (`tauri://drag-enter|leave|drop`; el `drop` de HTML5 no trae la ruta del
  fichero, así que el gesto solo existe dentro de Tauri) y
  `onCerrarSolicitado` (evento `cerrar-solicitado`; la UI responde con el
  comando `confirmar_cierre`, y en QA se dispara con
  `window.__vitelaCerrar()`).
- **Preferencias y memoria de la UI** en `localStorage` (`src/tipos.ts`):
  colores por acción, opciones de búsqueda (`Aa` y `|ab|`), «Resaltar
  campos» de los formularios (encendido por defecto) y preferencias
  (`autor` de los comentarios, que se manda como `author` en cada comando
  que crea una anotación).
- **Avisos**: `setNotice(texto, { persistente: true })` para el progreso
  («Comprimiendo…»), que se queda hasta que lo sustituye su resultado; sin
  la opción, el aviso se va solo a los 6 s. Aviso y error se limpian al
  abrir y al cerrar documento: son del documento que los provocó.
- **Atajos de teclado** (`App.tsx`, un solo `useEffect`; con un modal o el
  menú «Acciones» abiertos solo pasa Escape): ⌘O abrir · ⌘S guardar ·
  ⇧⌘S guardar como · ⌘P imprimir · ⌘D propiedades · ⌘, preferencias ·
  ⌘F buscar · ⌘G y ⇧⌘G coincidencia siguiente/anterior · ⌘Z y ⇧⌘Z
  deshacer/rehacer (no llaman al backend si no hay historial) · ⌘+ y ⌘−
  zoom · ⇧⌘+ y ⇧⌘− giran la vista · ⌘0 página entera, ⌘1 al 100 % y ⌘2
  al ancho (los tres de Acrobat) · ⌥⌘1 plegar el panel lateral ·
  ⇧⌘N ir a la página · ←/→ página anterior y siguiente · Esc quita las
  coincidencias de búsqueda y, si no hay, sale de la herramienta · Supr
  borra la anotación seleccionada · ⌘A todo el texto de la página (dentro
  del panel de páginas, seleccionarlas todas) · ⌘C copiar la selección.
  En los borradores de comentario y de cuadro de texto Enter salta de
  línea y ⌘Enter confirma; en un campo de formulario Tab y ⇧Tab confirman
  y saltan al campo siguiente o anterior.
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
- **Compilar para las otras plataformas**: en macOS no se ve si Windows o
  Linux compilan, y un `#[cfg(target_os = "macos")]` de más ya dejó el
  instalador de Windows sin salir (AC-032). El código dependiente de
  plataforma se reduce al mínimo: en `lib.rs` solo `RunEvent::Opened` y
  `ruta_de_url` (la URL `file://` que manda macOS); `pide_abrir`,
  `UI_LISTA`, `ABRIR_PENDIENTE` y `ui_lista` son de las tres. Para
  comprobarlo sin una máquina Windows: cambiar `target_os = "macos"` por
  `"windows"` en `lib.rs` y `menu.rs` y correr
  `cargo clippy --all-targets -- -D warnings`; eso es exactamente el
  código que ven Windows y Linux (incluidos los avisos de código muerto,
  que con `-D warnings` son errores en CI).
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
7. ✅ Firma digital: campo de firma (visible con `rect`, con su `/AP`) +
   ByteRange + PKCS#7 detached, y verificación al abrir
   (`verify_signatures`, sin cadena de confianza del sistema)
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
