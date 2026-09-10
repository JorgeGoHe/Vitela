# Vitela

Visor y editor de PDF de escritorio multiplataforma. Objetivo: edición real de
texto (reescribir el content stream, nunca parches encima como hace Stirling),
además de anotaciones, gestión de páginas (unir/separar/reordenar), formularios
y firma digital.

## Stack

- **Tauri 2** — shell de escritorio (Mac/Windows/Linux)
- **React + TypeScript + Vite** — UI (`src/`), gestor de paquetes `bun`
- **Rust** — core (`src-tauri/`)
- **RustCrypto** — firma y verificación: `rsa`, `sha2`, `cms`, `x509-cert`
  y, desde el ciclo 4, `p256` y `p384` para las firmas ECDSA que llegan de
  fuera. También `aes`/`cbc` para el cifrado AES-256 R6.
- **Cadena de confianza** (ciclo 5, `confianza.rs`), cada una bajo su
  `cfg`: `security-framework` + `core-foundation` en macOS (el llavero
  evalúa) y `rustls-native-certs` en Windows y Linux (las raíces del
  almacén nativo; la cadena la recorre Vitela con el verificador que ya
  tenía). **`webpki` se descartó a propósito**: obliga a `ring` o
  `aws-lc-rs`, que en x86_64-msvc piden NASM en el runner de Windows, y su
  API exige además un EKU de TLS que un certificado de firma de documentos
  no tiene.
- **`quick-xml`** — leer el XFDF de los comentarios (escribirlo se hace a
  mano). Ya venía en el árbol con `docx-rs`, así que no trae nada nuevo al
  build.
- **`docx-rs`** — exportar a Word (`export_docx`), crate puro. El test lo
  vuelve a abrir con `zip` (dev-dependency) y lee su `word/document.xml`.
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
  `add_stroke` (anotación Ink con path object como apariencia),
  `add_note` (anotación Text), `get_annotations` (incluye los quads de los
  resaltados como `rects`, más `author` y `modified`), `remove_annotation`,
  `get_form_fields` / `set_form_text` / `set_form_checked` (formularios),
  `get_text_blocks` / `edit_text_block` / `add_text_block` (texto nuevo en un
  punto, una línea por objeto; parámetro `font` opcional — sin él se detecta
  la familia dominante de la página) / `delete_text_block` (edición real;
  `set_text` requiere `page.regenerate_content()` antes de guardar),
  `get_images` / `add_image` (tamaño natural a 72 dpi, limitado a la página) /
  `transform_image` (mover/redimensionar por ratio de bounds, y girar y
  voltear) / `reorder_image` (al frente / al fondo) /
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
    `rotation` de `get_page_sizes` antes de mandar): `add_markup`, `add_stroke`, `add_note`, `add_shape`, `add_stamp`,
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
  `add_markup` remata con un segundo pase de lopdf
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
- `transform_annotation` mueve y redimensiona cinco tipos, por cuatro
  caminos: **Stamp** e **Ink** transforman los objetos que llevan dentro;
  **FreeText** se mueve y se redimensiona y su `/AP` se vuelve a dibujar
  (se dibuja en local, `/BBox 0 0 w h`, **partiendo las líneas al ancho
  nuevo**); **Square** son las marcas de redacción, que también se mueven y
  se redimensionan rehaciendo su borde rojo; **Text** solo se mueve, porque
  el icono del post-it tiene tamaño fijo también en Acrobat y del rect
  nuevo solo se toma la esquina.
- **WinAnsi** (`anotaciones2::winansi`): el tramo 0x80–0x9F NO es latin-1;
  ahí es donde WinAnsiEncoding guarda la raya «—», el guion «–», los
  puntos suspensivos «…», las comillas tipográficas y el «€». Lo usan el
  `/AP` de los cuadros de texto y el de la firma visible.
- **Autor y fecha**: los seis comandos que crean anotaciones
  (`add_markup`, `add_stroke`, `add_note`, `add_shape`, `add_stamp`,
  `reply_annotation`) aceptan `author: Option<String>`; sin él se usa el usuario
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
  (única y viva todo el proceso) y de los cachés de documentos abiertos
  (`with_doc` para PDFium, `with_lopdf` para lopdf). Desde el ciclo 7 son
  **mapas por copia de trabajo** con un tope (`DOCUMENTOS_EN_CACHE`, cuatro)
  y expulsión del que lleva más tiempo sin tocarse (`coloca`): con el caché
  de uno solo, abrir el segundo documento echaba al primero y trabajar con
  los dos recargaba el otro en cada comando. `invalidate_doc_cache(path)`
  suelta **ese** documento y hay que llamarla tras cada mutación (soltarlo
  no lo cierra: se recarga en milisegundos). **Es reentrante**: dentro del
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
    espacio de la página vista). Desde el ciclo 5 **sí** hay cadena de
    confianza (`confianza`, más abajo) y desde el ciclo 6 el sujeto y el
    emisor salen en llano (`nombre_llano`, con el DN entero en
    `cert_subject_dn` / `cert_issuer_dn`). Aun así la UI **no puede decir
    «válida»**: no se comprueba la revocación, solo «firmado por X, el
    documento no ha cambiado desde la firma» y quién responde por el
    certificado.
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
    annot_index)` y `apply_redactions(work, dry_run)` → `{ zonas, textos,
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
  - Los envoltorios de todo lo anterior están en `src/api.ts` con el mismo
    nombre en camelCase. `render_page` acepta `with_annotations:
    Option<bool>` (sin el campo, el render de siempre; la UI solo lo manda
    a `false` al imprimir con «Solo el documento»). `redact_area` ya no lo
    usa la UI. El índice de `unmark_redaction` es el `annot_index` que
    trae `list_redactions`: la posición dentro de `/Annots` de la página,
    **no** el ordinal de la marca entre las marcas (con un resaltado
    delante los dos números se separan y se borraba otra anotación).
- Comandos del ciclo 4:
  - `pdf_info(path)` → `{ page_count, bytes, encrypted }` (`documento.rs`):
    la ficha de un PDF **sin abrirlo** —sin copia de trabajo, sin paso de
    deshacer y sin PDFium—, para la rejilla de combinar («12 páginas · 1,4
    MB» por fila) y para marcar los recientes protegidos. De un documento
    cifrado dice lo que se sabe sin la contraseña, y nunca la pide.
  - `search_pdf` acepta `context: Option<bool>`: con él cada coincidencia
    trae `before`/`after` (30 caracteres a cada lado, con los espacios
    colapsados) y `block_index`, el bloque de texto en el que cae, que es
    lo que necesita el reemplazo. Sin él, la búsqueda de siempre y sin
    pasadas de más.
  - `replace_text(work_path, matches)` (`texto.rs`) → `{ hechas, saltadas }`:
    «Reemplazar todo» en UNA mutación (un ⌘Z las devuelve todas). Cada
    `Reemplazo` es `{ page_index, block_index, from, to }`; las que caen en
    el mismo bloque se agrupan para tocar cada objeto una sola vez, y una
    coincidencia cuyo bloque ya no dice lo que decía —o que está en una
    fuente que no se deja reescribir— se salta y se cuenta en `saltadas`,
    para poder decir «9 de 12».
  - `unmark_all_redactions(work_path)` → cuántas quita: «Quitar todas las
    marcas» en una sola mutación (la UI también puede ir llamando a
    `unmark_redaction` por `annot_index` descendente).
    `transform_annotation` trata además los `Square`, así que las marcas de
    redacción se mueven y se redimensionan como cualquier comentario y su
    borde rojo se vuelve a dibujar.
  - `add_watermark` acepta `page_indices: Option<Vec<u16>>` (sin él,
    todas), `image_png: Option<String>` (base64: la marca de agua puede ser
    una imagen), `opacity: Option<f32>` (0,3 por defecto, la de Acrobat; el
    color llega opaco y la opacidad va en su parámetro) y `rotation:
    Option<f32>` (sin ella, 45° si `diagonal`). La imagen se incrusta con
    su alfa ya multiplicado por la opacidad, que es lo que deja el `/SMask`
    escrito. `add_header_footer` acepta también `page_indices`, y `{n}` y
    `{total}` siguen siendo los del documento, no los del rango.
  - **Autoguardado y recuperación** (`recuperacion.rs`):
    `autosave_state(work_path, original_path?, modified)` apunta un
    `sesion.json` en `DIR_DATOS` con la copia viva; `borra_sesion(work_path?)`
    lo borra al cerrar bien o al descartar (con `work_path`, solo si el
    apunte es de ese documento); `recover_session()` → `Option<Sesion>` al
    arrancar, y solo si había cambios sin guardar y la copia sigue en el
    disco. El barrido de huérfanos respeta la copia apuntada y sus
    instantáneas. La copia de trabajo ya sobrevivía al cierre bruto: lo que
    faltaba era el apunte de que existía y no se había guardado.
  - `add_text_block` y `edit_text_block` aceptan `color: Option<[u8;4]>` y
    `align: Option<String>` («izq»/«centro»/«der»). En un PDF no hay
    operador de alineación: se coloca el origen del objeto (y al corregir
    un bloque, se conserva su centro o su borde derecho).
  - `transform_image` acepta `rotate: Option<i16>` (múltiplos de 90,
    horarios), `flip_h` y `flip_v`: con giro o volteo la imagen se centra
    en la caja pedida, así que a 90° el ancho y el alto salen
    intercambiados. `reorder_image(work, page_index, object_index,
    al_frente)` la trae al frente o la manda al fondo sacando y volviendo a
    poner objetos (pdfium-render 0.8 no expone insertar por índice), sin
    soltar nunca uno sacado.
  - El `/AP` de los cuadros de texto **parte las líneas al ancho de la
    caja** (`anotaciones2::parte_lineas`, midiendo en Helvetica con los
    anchos del AFM): como la apariencia se rehace al corregir el texto y al
    transformar la anotación, el texto refluye también al redimensionar,
    igual que en Acrobat. Antes solo partía por `\n` y la frase se salía
    por el borde derecho.
  - **Verificación honesta de firmas** (`firma.rs`): el hash del
    `/ByteRange` se calcula con el algoritmo que declara la firma (SHA-256,
    384 o 512); se comprueban RSA PKCS#1 v1.5, RSA-PSS y ECDSA P-256 y
    P-384 (crates `p256` y `p384`); el certificado es el que señala el
    `SignerIdentifier` (emisor + serie, o el identificador de clave del
    sujeto), **no el primero del bolso** —en una firma cualificada suele ir
    la CA delante—. `FirmaInfo` gana `estado` (`"ok"`, `"modificado"`,
    `"desconocido"`) y `algoritmo` («ECDSA P-256 / SHA-384»): lo que no se
    sabe leer sale como «no se ha podido comprobar» y **nunca** en rojo,
    porque acusar en falso a un contrato firmado es peor que no verificar.
  - `sign` se negaba con un aviso llano si el documento ya llevaba firma:
    se reescribía el fichero entero y la anterior quedaba rota. **Desde el
    ciclo 6 la segunda firma va en una actualización incremental** (ver
    abajo) y el aviso desapareció. La apariencia visible añade «Motivo: …»
    debajo del nombre y la fecha, como el sello de Acrobat.
- Comandos del ciclo 5:
  - **El segundo test cruzado**
    (`puente_dev::los_comandos_estan_en_el_handler_en_el_puente_y_en_la_ui`):
    lee `lib.rs`, `puente_dev.rs` y `src/**/*.ts*` y exige que las tres
    listas digan lo mismo. Falla si un comando está registrado y no
    despachado (QA en el navegador se queda sin esa función), si está
    despachado y no registrado (en la app no existe) o si la UI llama a
    algo que no existe. Desde el ciclo 6, un comando que **nadie llama
    falla** (ver abajo). Con él se retiró `add_highlight`, muerto desde que
    `add_markup` hace resaltado, subrayado y tachado.
  - **Cadena de confianza** (`confianza.rs`): `FirmaInfo` gana `confianza`
    con `"raiz_conocida"` / `"autofirmado"` / `"desconocida"`, evaluada en
    el momento de la firma (el atributo `signingTime`), porque un
    certificado caducado hoy era bueno cuando se firmó. **Sin revocación
    (ni CRL ni OCSP)**: por eso la etiqueta honesta es «emitido por una
    autoridad reconocida» y nunca «válida». En macOS lo evalúa el llavero;
    fuera, las raíces del almacén nativo y la cadena a mano, exigiendo en
    cada escalón que el emisor firmara al hijo, que el intermedio lleve
    `basicConstraints` con `cA: true` y que los dos estuvieran en vigor.
    `FirmaInfo` gana además `not_yet_valid`: `expired` es cierto por los
    dos extremos del periodo de validez, y ese booleano dice por cuál.
  - `export_docx(work_path, dest_path, page_indices?)` → `DocxReport
    { parrafos, imagenes, perdido }` (`exportar.rs`, crate `docx-rs`): un
    párrafo por bloque de texto en orden de lectura, con familia, tamaño,
    negrita, cursiva y color; las imágenes en su sitio aproximado; un
    salto de página por página. **No se intenta detectar tablas ni
    columnas** y está escrito en el doc-comment: un PDF no guarda
    párrafos, guarda trozos de texto colocados en un papel. `perdido` es
    la lista en llano de lo que no ha salido, para que la UI la cuente sin
    adornarla; el aviso previo lo da la UI antes de pedir destino.
  - `TextBlock` gana `color`, `negrita` y `cursiva`, y su `font_size` es
    el que **se ve** (con la escala de la matriz). `normaliza_familia`
    dejó de comerse el estilo —convertía «Arial-BoldMT» en «Helvetica»— y
    ahora normaliza por subcadena mientras `estilo_del_nombre` lee el
    estilo. **El nombre manda sobre el peso**: con las fuentes internas de
    chromium/8009, `times_bold()` se identifica como «Times New Roman» con
    peso 0 y sin la bandera de negrita del descriptor.
  - `move_text_block(work, page, object_index, x, y)` y
    `resize_text_block(…, w, h)` (`texto.rs`): el bloque se arrastra y se
    estira como una imagen. Redimensionar **escala el tamaño de la letra**
    (escala uniforme sacada del área pedida), no la deforma, y la esquina
    de la que no se tira se queda donde estaba.
  - `edit_text_block` y `add_text_block` aceptan `line_height`: en un PDF
    no hay párrafos, las líneas son objetos, así que el interlineado es la
    distancia a la que se coloca cada uno (1,2 por defecto). Las líneas
    2..n **heredan el color** del bloque cuando no se pide ninguno.
    El espaciado entre caracteres (`Tc`) se conectó en el ciclo 6 (ver
    abajo): pdfium-render 0.8 no lo expone, así que lo escribe un segundo
    pase con lopdf. El mando de la UI se retiró ese mismo ciclo por un
    desencuentro entre las dos mitades (R24) y vuelve en el 7 (R32); desde
    R32b el operador sobrevive a mover y a estirar el bloque.
  - `crop_image(work, page, object_index, rect)` (`imagenes.rs`): recorta
    el **bitmap**, no la caja, por el camino de `replace_image`, así que
    lo que queda fuera desaparece del fichero en vez de esconderse detrás.
  - **Comentarios con hilo y estado** (`comentarios.rs`):
    `reply_annotation(work, page, annot_index, text, author?)` → índice de
    la respuesta (`/Text` con `/IRT` y `/RT /Reply`);
    `set_annotation_state(…, state, author?)` con los cuatro del spec
    (`Accepted`, `Rejected`, `Cancelled`, `Completed`; `""` o `"None"` lo
    quitan), guardado en una anotación hija con `/RT /StateModel` y
    `/StateModel /Review` **como lo guarda Acrobat**, para que el revisor
    lo vea allí; y `export_comments` (desde el ciclo 6,
    `(work_path, dest_path, document_name?)`) → cuántos, el
    resumen en llano con el tipo y el estado en español (el FDF/XFDF se
    deja para otro ciclo). `get_annotations` y `get_document_annotations`
    devuelven `in_reply_to` y `state`; las anotaciones de estado **no son
    comentarios** y no se listan, igual que el `/Popup` de una nota. Y
    `remove_annotation` se lleva el hilo entero: borrar la pregunta dejaba
    las respuestas colgando de un objeto que ya no está.
  - **Adjuntos y capas** (`adjuntos.rs`): `list_attachments`,
    `save_attachment` (los bytes tal cual) y `add_attachment` sobre el
    árbol `/Names → /EmbeddedFiles`; `list_layers` y `set_layer_visible`
    sobre `/OCProperties`. **Apagar una capa cambia el fichero**: PDFium
    respeta el `/OFF` del documento al renderizar, así que no es una
    vista; pasa por `cirugia`, deja su paso de deshacer y la UI lo dice.
  - `pdf_from_images(image_paths, dest_path, tamano)` (`paginas2.rs`) con
    `"a4"`, `"carta"` o `"imagen"`: una página por imagen, ajustada sin
    deformarla y centrada, con 36 pt de margen.
  - **Los campos `/Sig` no son campos de formulario**: `get_form_fields` y
    `lee_annots` se los saltan. Un PDF que solo llevaba una firma se
    anunciaba como «este documento se puede rellenar».
  - **AC-046 (crítico)**: `FPDF_ImportPages` copia el grafo de las
    anotaciones recursivamente y el par `/Popup` ↔ `/Parent` de una nota es
    un ciclo que le revienta la pila — importar una página con una nota
    mataba el proceso con SIGSEGV. Todos los caminos de importación abren
    la fuente por `anotaciones::fuente_importable` (copia sin ventanas en
    el temporal, que se borra sola) y rematan con `repon_popups_en`, que
    las vuelve a crear del `/Rect` de cada nota. Son nueve comandos:
    `merge_pdf`, `merge_many`, `insert_pdf_at`, `duplicate_page`,
    `replace_pages`, `move_page`, `extract_pages`, `extract_each_page` y
    `split_pdf`.
  - **AC-049**: `extract_pages` y `extract_each_page` comprueban **antes
    de escribir nada** (`revisa_extraccion`) que las páginas existen, que
    llevárselas no deja el documento sin ninguna y que en la carpeta de
    destino se puede escribir de verdad (escribiendo, que es lo único que
    no miente).
  - `fixtures/firmado_ecdsa.pdf`: un PDF firmado con ECDSA P-256 de
    verdad, para probar la verificación de un documento que llega de fuera
    sin fabricar el CMS en el test.
- Comandos del ciclo 6:
  - **El test cruzado, un nivel más abajo** (`puente_dev`,
    `los_argumentos_de_cada_invoke_son_los_del_comando`): compara los
    **nombres de argumento** de cada `invoke("cmd", { … })` de `src/**`
    con los parámetros del comando en Rust, pasados a snake_case como hace
    Tauri. Falla con lo que la UI manda y el comando no conoce (que Tauri
    descarta en silencio: así se coló media función de espaciado en el
    ciclo 5) y con lo obligatorio que la UI no manda; los `Option<T>` no
    son obligatorios y lo que inyecta Tauri (`AppHandle`) no cuenta. Para
    leer las claves sigue el rastro de los `...spread` de `api.ts` (el
    parámetro de la función que envuelve, con sus intersecciones `& Tipo`,
    los `type X = { … }` y los `const X = { … }` del fichero), sobre el
    fuente **sin comentarios**. Lo que no sabe leer va en `SIN_LEER`.
    Y el aviso de «comandos que la UI no llama» pasó de `eprintln!` a
    **fallo**, con `NADIE_LLAMA` para lo que está a medio integrar
    (marcado «pendiente_ui» con su motivo) y `ARGUMENTOS_PENDIENTES` para
    un desencuentro de nombres que se arregla en la otra mitad. **Las dos
    listas tienen que quedar vacías al cerrar un ciclo.**
  - **Espaciado entre caracteres** (`texto.rs`): `edit_text_block` y
    `add_text_block` aceptan `char_spacing`. pdfium-render 0.8 no expone el
    estado de texto del objeto, así que el operador `Tc` lo escribe un
    segundo pase con lopdf dentro de la misma mutación (`escribe_espaciado`):
    `<tc> Tc` detrás del `BT` del bloque y `0 Tc` antes de su `ET`, para que
    no se escape al resto de la página. El recorrido del content stream se
    hace **sobre los bytes** —saltando cadenas, hexadecimales y
    comentarios—, no con `Content::decode`: el analizador de lopdf 0.34 no
    entiende las imágenes en línea. Los bloques se localizan por su ordinal
    entre los objetos de texto de la página, que es el orden en el que
    PDFium escribe los `BT … ET`. **Ojo**: `FPDF_GenerateContent` no vuelve
    a escribir el `Tc`, así que mover o estirar después ese bloque se lo
    lleva por delante.
  - **Reflujo del párrafo** (`texto.rs`): `edit_text_block` gana `reflow`
    y devuelve `{ lineas, se_sale, reflujo }`. **El contrato, fijado en el
    ciclo 7 (AC-061)**: `new_text` es SIEMPRE el texto del **bloque
    tocado** —la línea, que es lo que la UI tiene en su cuadro de
    edición—, nunca el del párrafo entero; con `reflow: true` el backend
    reconoce él el párrafo (`parrafo_de`, sin que la UI tenga que decidir
    si lo hay), sustituye solo esa línea y recoloca el conjunto. **El
    resto del párrafo no se pierde nunca.** El defecto es
    `reflow.unwrap_or(false)`: la bandera cambia lo que significan los
    demás argumentos, y una bandera así no puede traer puesto el
    comportamiento que toca lo que no se le ha pedido. `parrafo_de` reconoce el
    párrafo bajando desde el bloque tocado por las líneas que comparten
    columna (borde izquierdo, centro o borde derecho), cuerpo de letra y
    distancia de interlineado (más de 2,6 cuerpos ya es otro párrafo).
    El texto se mide con los AFM de `parte_lineas`, se reparte al ancho de
    la columna, se reescriben las líneas que había y se crean o **se
    borran** las que sobren (nunca soltando el objeto sacado). Las líneas
    se colocan por su **línea base** (`get_translation`), no por la caja de
    los glifos, que sube y baja con las mayúsculas y los rabos; y el ancho
    de la columna es el de la caja **o el que midan los AFM de las líneas
    que ya hay**, el que sea mayor, para que una línea sin cambios no se
    parta por un error de medida. Sin pedir interlineado se respeta el del
    párrafo. **No cruza bloques ni páginas**, como Acrobat.
  - **Adjuntos** (`adjuntos.rs`): `delete_attachment(work_path, index)`
    saca la entrada del árbol `/EmbeddedFiles` y **poda los objetos** (sin
    eso, los bytes del fichero seguirían dentro del PDF), y
    `open_attachment(path, index)` deja el adjunto en una carpeta
    `vitela-adjunto-…` del temporal —con su nombre y su extensión de
    verdad, limpiados de barras y `..`— y devuelve la ruta para que la UI
    lo abra con el visor del sistema (`opener:allow-open-path` acotado al
    temporal en `capabilities/default.json`). El barrido de huérfanos del
    arranque se lleva esas carpetas enteras. Añadir y borrar comparten
    `reescribe_arbol`.
  - **Capas con `/OCProperties` en línea** (AC-056): un catálogo puede
    llevarlo como diccionario en línea y entonces no hay objeto que
    modificar; `set_layer_visible` lo **promueve a objeto propio** en vez
    de contestar «no se pueden apagar desde aquí».
  - **Firmas en llano** (`firma.rs`): `nombre_llano(dn)` devuelve el `CN`
    y, si no, el `O`, partiendo el DN por sus comas de verdad (en RFC 4514
    una coma dentro de un valor va escapada). `FirmaInfo` gana
    `cert_subject_dn` y `cert_issuer_dn` con el DN completo, y
    `self_signed` pasa a usar `confianza::es_autofirmado` (nombre **y**
    firma), que es la definición buena.
  - **La segunda firma** (`firma.rs`): si el documento ya lleva firma,
    `sign` hace **actualización incremental** con
    `lopdf::IncrementalDocument`: los bytes de antes se quedan exactamente
    donde estaban y el fichero crece por el final con el campo nuevo, el
    `/Annots` de su página, el `/AcroForm` actualizado, una tabla de
    referencias cruzadas con `/Prev` y su `%%EOF`. La primera firma deja de
    cubrir el fichero entero y **sigue en `ok`**: una revisión detrás es lo
    normal, no una manipulación. Por dentro: `Destino` (documento entero o
    revisión nueva; sabe traerse un objeto de atrás antes de cambiarlo),
    `escribe_campo_de_firma` y `cose_la_firma` (el `/ByteRange` y el PKCS#7
    sobre los bytes ya serializados, **buscando desde el principio de la
    revisión nueva**, que delante hay otra firma con su propio hueco). Los
    campos se llaman `Firma1`, `Firma2`… porque dos `/T` iguales son el
    mismo campo para cualquier visor.
  - **Campos de formulario** (`formularios2.rs`): `create_form_field`
    acepta `radio` (con `group` y `export_value`), `combo` y `list` (con
    `options`) y un `props` anidado **en snake_case** (Tauri solo traduce
    el camelCase de los argumentos de primer nivel): `tooltip` (`/TU`),
    `obligatorio` y `solo_lectura` (`/Ff` bits 2 y 1), `valor_defecto`
    (`/DV`) y `orden_tab` (la posición dentro del `/Annots`, con `/Tabs
    /S`). **Un grupo de radios es un solo campo**: `/FT /Btn`, `/Ff` con el
    bit 16 y un `/Kids` por opción, y el segundo radio del mismo grupo se
    engancha al campo que ya existe. `set_form_checked` tiene su rama de
    radio en lopdf (`/V` del campo y `/AS` de cada hijo, los hermanos a
    `/Off`), y `get_form_fields` devuelve `required` (el bit 2 de `/Ff`,
    heredado del padre), leído con lopdf porque pdfium-render no lo expone.
    El marco y la marca van dentro del `/AP`, en sus dos estados, para que
    aplanar tenga qué copiar.
  - **Llamada y goma** (`anotaciones2.rs`): `add_callout(work_path,
    page_index, rect, punta, text, color, author?)` escribe un `/FreeText`
    con `/IT /FreeTextCallout`, `/CL` de la punta a la caja, `/LE
    /OpenArrow` y un `/Rect` que abarca las dos cosas, con `/RD` diciendo
    dónde queda la caja dentro de él; la línea sale por el centro del lado
    que mira a la punta y la flecha la dibuja el `/AP`.
    `transform_annotation` le aplica al `/CL` y al `/RD` la misma
    transformación que al `/Rect`, así que arrastrar el cuadro arrastra la
    punta. `erase_ink(work_path, page_index, annot_index, rect)` quita del
    trazo los tramos que **tocan** el rectángulo (recorte de
    Liang-Barsky) trabajando sobre el `/AP`, que es donde vive el dibujo
    del Ink (PDFium no escribe `/InkList`), y devuelve si ha quedado algo;
    si no queda nada, el comentario se va.
  - **Resumen de comentarios** (`comentarios.rs`):
    `export_comments(work_path, dest_path, document_name?)` —sin `formato`,
    que solo admitía `"txt"`—. La cabecera lleva el nombre que mande la UI
    y, si no manda ninguno, el que lleva dentro la copia de trabajo
    (`vitela-<nombre>-<nanos>.pdf` → `<nombre>.pdf`): **el nombre del
    temporal no se enseña nunca**. Las respuestas no repiten la página ni
    el tipo del comentario al que contestan, y la fecha va en español
    («10/09/2026 00:25»), no en ISO 8601.
  - `pdf_from_images` **salta las imágenes que no se dejan leer** en vez de
    tirar el lote; desde el ciclo 7 devuelve además cuáles y por qué (ver
    abajo). Si no se lee ninguna, sigue siendo un error y dice cuáles.
- Comandos del ciclo 7:
  - **El espaciado sobrevive a mover y a estirar** (`texto.rs`):
    `FPDF_GenerateContent` no vuelve a escribir el `Tc` al regenerar el
    content stream, así que `move_text_block` y `resize_text_block` leían
    el operador del bloque antes de tocarlo (`lee_espaciado`) y lo reponen
    al terminar, dentro de la misma mutación. El recorrido del content
    stream sobre los bytes pasa a `recorre_stream`, que comparten el que
    escribe el `Tc` y el que lo lee.
  - **La goma, de una pasada** (`anotaciones2.rs`):
    `erase_ink_area(work_path, page_index, rect)` busca él los `Ink` cuya
    caja toca la zona, los recorta todos en una cirugía y quita del
    `/Annots` —de mayor a menor índice— los que se quedan sin nada.
    Devuelve `{ tocados, borrados }`. Pasar la goma por donde no hay trazo
    no es un error y **no deja paso de deshacer**
    (`historial::retira_paso`). Sustituye a `erase_ink`, que se retira en
    cuanto la interfaz deje de llamarlo.
  - `pdf_from_images` devuelve `{ paginas, saltadas, motivos }`: `saltadas`
    son las rutas **tal como llegaron** (la UI las compara con su lista para
    marcar esas filas del diálogo) y `motivos` va en paralelo, con la frase
    en llano de por qué se quedó fuera cada una. El `Display` del crate
    `image` va en inglés y en su jerga, y esto lo lee el usuario.
  - **El menú nativo, espejo en las dos direcciones** (`menu.rs`): el test
    cruzado probaba que todo id del menú del sistema estaba enrutado y nada
    probaba lo contrario, y por eso «Leer en voz alta» y «Exportar
    comentarios…» llevaban un ciclo entero fuera de la barra.
    `el_menu_nativo_es_un_espejo_del_menu_de_la_app` exige que cada
    `<Entrada>` del menú «Acciones» tenga su **etiqueta** en `estructura()`
    (las entradas de la app no llevan id), con dos listas de excepciones
    —`NO_VAN_EN_LA_BARRA` y `EQUIVALENTES`— que fallan también cuando
    envejecen. `Entrada::ep` marca `pendiente_ui`.
  - **R37, la tercera forma de desencontrarse** (`puente_dev.rs`): un
    `Option<T>` que ningún `invoke` manda jamás es una función escrita,
    probada y sin camino hasta el usuario (fue el estado de `char_spacing`
    durante un ciclo). Ahora falla, con `PARAMETROS_PENDIENTES` (comando,
    parámetro, motivo) para lo que esté a medio integrar; la lista se queja
    también cuando la UI ya manda el parámetro.
  - **Marcadores con destino fino** (`documento.rs`): `OutlineNode` gana
    `top` y `zoom` (`/XYZ left top zoom`), así que volver a un marcador
    devuelve la vista donde se puso y no el principio de la página. `top`
    va en el espacio propio de la página con el origen arriba-izquierda,
    como todo lo que la UI lee y escribe; sin `top` ni `zoom` se escribe
    `null`, que en el spec es «déjalo como está», y los dos llevan
    `serde(default)`. **Leer el árbol pasa de PDFium a lopdf**:
    pdfium-render 0.8 da la página del destino pero no los parámetros de
    vista. De paso se resuelven los destinos ajenos (`/Fit`, `/FitH`,
    `/FitR`, el `/A` con `/S /GoTo` y los nombres del árbol
    `/Names /Dests` o del `/Dests` viejo), y los recorridos cortan ciclos.
  - **Reconocer campos** (`formularios2.rs`):
    `detect_form_fields(work_path, page_indices?)` →
    `CampoPropuesto { page_index, rect, kind, name, group, confianza }`,
    que **solo propone**. La heurística va sobre los bloques de texto y la
    **caja** de los objetos de camino (pdfium-render 0.8 no expone los
    segmentos): raya larga y fina —o corrida de «_»— es campo de texto
    encima, cuadro pequeño suelto es casilla, cuadros alineados del mismo
    tamaño son las opciones de un grupo (el grupo lo encabeza el texto que
    va delante de la fila). El nombre sale del texto más cercano, sin
    tildes (el `/T` lo leen otros programas) y desempatado con un número;
    donde ya hay un widget no se propone nada. `confianza` baja cuando no
    había texto del que sacar el nombre: una heurística no acierta siempre
    y no puede fingir que sí. Aceptar el lote es
    `create_form_fields(work_path, fields)`, **una sola cirugía** (el
    cuerpo de `create_form_field` pasa a `crea_campo`, que comparten).
  - **El contrato del reflujo** (AC-061, arriba en el ciclo 6): `new_text`
    es siempre la línea, `reflow` por defecto `false`.
  - **El estado de una casilla lo lee lopdf** (`formularios.rs`, AC-063):
    `is_checked()` de pdfium-render 0.8 devuelve `true` para **todas** las
    opciones de un grupo de radios cuando el campo está en `/V /Off`, y con
    eso el formulario no se podía rellenar (la UI conmutaba `!checked`, que
    siempre era `false`). Se lee el `/AS` del widget —el estado del `/AP`
    que el visor pinta— y, sin él, el `/V` heredado contra el valor de
    exportación. El estado llega a veces como cadena en vez de nombre
    (PDFium escribe «/Yes» así), y las dos formas se entienden.
  - **Solo lectura y ayuda** (AC-065 y AC-066): `FormFieldInfo` gana
    `read_only` (bit 1 del `/Ff`, heredado) y `tooltip` (`/TU`, heredado), y
    `set_form_text`, `set_form_checked` y `set_form_choice` se niegan con
    un aviso en llano sobre un campo bloqueado: quien decide qué se escribe
    en el documento es el backend.
  - **El documento intacto** (`firma.rs`, AC-064): `FirmaInfo` gana
    `documento_intacto`, que es **del documento** y vale lo mismo en todas
    —cierto cuando todas están en «ok» y alguna cubre el fichero entero, que
    solo puede ser la última—. Con dos firmas, componer la banda con la
    primera `FirmaInfo` decía «hay cambios posteriores» y el cambio era la
    otra firma.
  - **La medida puesta es un comentario** (`anotaciones2.rs`, AC-069):
    `add_measure(work_path, page_index, points, text, color, closed?,
    author?)` escribe `/Line` (dos puntos), `/PolyLine` (perímetro) o
    `/Polygon` (área) con `/IT` de medida, `/LE` en los extremos y la cifra
    en el `/Contents`; el `/AP` lo dibujamos nosotros. Una sola mutación, y
    ya no ensucia el texto del documento. **No se escribe `/Measure`**: la
    escala la fija el usuario por documento en la interfaz, y uno sin escala
    de verdad diría que el PDF trae una que no trae.
  - **El resumen y el XFDF** (`comentarios2.rs`):
    `export_comments_pdf(work_path, dest_path, orden, document_name?)` con
    `orden` `"pagina"`/`"autor"`/`"fecha"`/`"tipo"` compone con PDFium un
    PDF de una fila por comentario, respuestas sangradas y **siempre pegadas
    a su comentario**. Las páginas enfrentadas de Acrobat quedan fuera y se
    argumenta en el código. `export_comments_xfdf` e
    `import_comments_xfdf` leen y escriben los diccionarios con lopdf, así
    que viaja todo lo escrito (autor, fechas, `/IRT`, estado, quads,
    vértices); el dibujo de un `Ink` sale de su `/AP`, que es donde PDFium
    lo guarda, como el `<inklist>` del formato. Importar **añade**, en una
    sola mutación, engancha las respuestas por `name`/`inreplyto` y
    redibuja la apariencia de las marcas. `quick-xml` ya venía en el árbol
    con `docx-rs`.
  - `borra_sesion(work_path)` exige la copia de trabajo: con varios
    documentos abiertos, cerrar uno no puede llevarse el apunte de otro.
  - `save_image_data(work_path, page_index, object_index, dest_path)`
    (`imagenes.rs`): el mismo bitmap de `get_image_data` escrito
    directamente en un PNG, sin cruzar el canal en base64.
  - **Un test para CLAUDE.md** (AC-070): ninguna línea de más de cuarenta
    caracteres puede aparecer dos veces. El documento lo tocan cada ciclo
    las dos ramas sobre los mismos párrafos, y dos veces seguidas se coló
    la misma línea pegada detrás de su versión nueva.
- **La mitad de la UI del ciclo 5** (según el desarrollador de interfaz):
  - Comandos del ciclo 5 (los envoltorios, en `src/api.ts`, con el mismo
    nombre en camelCase):
    - `unmark_all_redactions(work_path)` → cuántas quita. **Lo llama ya la
      UI** («Quitar todas las marcas»): era el comando escrito y sin usar que
      cazó el test cruzado.
    - `search_pdf(context)` **no tiene defecto en la UI**: `searchPdf` exige
      el parámetro y `useBusqueda` lo pide solo con el cajón de resultados
      desplegado. `SearchMatch` declara `block_index`, que es el que usa
      `useReemplazo` para agrupar: la UI ya no recalcula el bloque con
      `get_text_blocks` (dos criterios distintos para el mismo contrato).
    - `export_docx(work_path, dest_path, page_indices?)` →
      `DocxReport { parrafos, imagenes, perdido }`. La UI da **antes** de
      pedir destino el aviso de que la maquetación no sale, con el patrón de
      `DialogoCombinar`.
    - `move_text_block` / `resize_text_block` (mover y estirar un bloque de
      texto, con los ocho tiradores de siempre) y `crop_image` (recortar,
      desde el popover de la imagen). `TextBlock` trae `color`, que es lo que
      pinta el swatch «A» de la fila contextual; `add_text_block` y
      `edit_text_block` aceptan `line_height` (`char_spacing` se retiró en
      el ciclo 6: no existía en el backend).
    - `reply_annotation` (hilos `/IRT`), `set_annotation_state` (los cuatro
      estados de revisión de Acrobat, con el nombre que se escribe en el PDF:
      `Accepted`, `Rejected`, `Cancelled`, `Completed`) y
      `export_comments`. `AnnotationInfo` gana
      `in_reply_to` (para anidar) y `state`.
    - `list_attachments` / `save_attachment` / `add_attachment` y
      `list_layers` / `set_layer_visible`. (Borrar y abrir adjuntos llegan
      en el ciclo 6.)
    - `verify_signatures` devuelve `confianza` (`raiz_conocida`,
      `autofirmado`, `desconocida`): una línea más en la tarjeta del panel,
      que **no cambia el color de la banda** —la confianza es del
      certificado, la validez es del documento—.
    - `autosave_state` pide **tres** argumentos: sin `modified` todas las
      llamadas se rechazaban con un 400 y la recuperación no tenía nada que
      recuperar (AC-047). El fallo ya no se traga en silencio.
- **La mitad de la UI del ciclo 6** (según el desarrollador de interfaz):
  - Comandos nuevos que llama la UI, con los nombres de argumento en
    camelCase que exige el test cruzado de R25:
    - `pdf_from_images(imagePaths, destPath, tamano)` — «Crear PDF desde
      imágenes…», en el estado vacío y en Archivo del menú «Acciones»
      (`DialogoImagenes`). Devuelve cuántas páginas ha escrito y el PDF se
      abre al terminar. **Falta la entrada del menú nativo**: el id vive en
      `menu::estructura()`, que es de `src-tauri/`, así que la añade quien
      integre (y con ella su entrada en `accionesMenu`, o el test canta).
    - `open_attachment(path, index)` y `delete_attachment(workPath, index)`
      — «Abrir» (acción principal de la fila y doble clic) y «Quitar» con
      confirmación en `PanelAdjuntos`, que además se recorre con ↑↓ y borra
      con Supr. Abrir pasa por el backend porque el permiso del opener está
      acotado a http/https/mailto y el webview no puede abrir un fichero
      del disco. **`open_attachment` no estaba en el contrato del
      analista**: los nombres de argumento se eligieron como los de
      `save_attachment`, que es la otra lectura.
    - `add_callout(workPath, pageIndex, rect, punta, text, color, author)`
      — modo «Llamada»: clic donde señala, arrastre hasta donde va el
      texto. `punta` es la pareja `[x, y]` en el espacio propio de la
      página, como el `rect`.
    - `erase_ink(workPath, pageIndex, annotIndex, rect)` — la goma, un
      conmutador dentro del modo Dibujar. La zona que se pinta al arrastrar
      es exactamente el rectángulo que se manda: lo que se ve es lo que se
      borra. Se llama una vez por trazo `Ink` que toque la zona.
    - `create_form_field` acepta ahora `kind` `"radio"`, `"combo"` y
      `"list"` además de texto y casilla, más `group`, `exportValue`,
      `options` y `props` (`{ tooltip, obligatorio, solo_lectura,
      valor_defecto, orden_tab }`). Ojo: **`props` es una estructura
      anidada**, y Tauri solo pasa a snake_case los argumentos de primer
      nivel del comando, así que sus claves van ya en snake_case.
      `group` y `exportValue` se mandan siempre (cadena vacía cuando el
      tipo no los usa) y `options` siempre como lista.
    - `edit_text_block` gana `reflow`: la UI lo pide cuando el bloque ocupa
      más de una línea (`h > font_size * 1.5`, o hay saltos de línea), que
      es cuando hay párrafo que recolocar. Y `add_text_block` /
      `edit_text_block` **ya no mandan `char_spacing`** (R24).
    - `export_docx` recibe por fin `pageIndices`: el aviso previo pasa a ser
      `DialogoWord`, con el bloque `RangoPaginas` de siempre. No hay
      contador ni Cancelar porque el comando es un solo viaje sin progreso
      ni interrupción, y la banda lo dice en vez de fingirlos.
  - **Modos nuevos**: `callout` (llamada) y `medir`. Cada uno con su capa
    propia —`CapaLlamada`, `CapaMedida`—, como el recorte de imagen: los
    despachadores de ratón de `Pagina.tsx` solo ganan su rama, sin tocar el
    orden de las que ya había.
  - **Medir** (`useMedida`): distancia y área sobre la página, con la
    medida en Fragment Mono. La escala se fija arrastrando sobre algo de
    medida conocida y se guarda en `localStorage` **por ruta**
    (`cargaEscala`/`guardaEscala` en `tipos.ts`, milímetros por punto);
    sin fijarla se mide el papel (`MM_POR_PUNTO`), que es lo que hace
    Acrobat cuando el PDF no trae `/Measure`. Solo toca el documento con
    «Dejar la medida puesta», que la escribe con `add_shape` y
    `add_text_block` (dos pasos de deshacer, y se dice).
  - **Leer en voz alta** (`useLectura`): `speechSynthesis` del webview
    sobre `get_page_text`, **desde la página que se está leyendo**, con
    ⇧⌘Y, la entrada de «Acciones» y los controles en la fila contextual
    (que mientras suena es la de la lectura). Esc calla, y cambiar o cerrar
    documento también. Sin voz en español se dice y se lee con la del
    sistema; sin voz ninguna se dice y no se intenta. Ganchos de QA
    `window.__vitelaLeer(desde, hastaElFinal)` y
    `window.__vitelaPararLectura()`.
  - **Copiar y Seleccionar todo** ya no reenvían la tecla a ciegas: actúan
    sobre el `input`/`textarea` con el foco si lo hay, luego sobre la
    selección del DOM y solo entonces reenvían la tecla al visor.
    `reenviaTecla` **devuelve si alguien la ha atendido** (los listeners que
    actúan llaman a `preventDefault`), y cuando no hay nada que copiar se
    dice en la banda. Atenuar la entrada del menú necesitaría que
    `set_menu_state` supiera de la selección, que es backend.
  - `FormFieldInfo` declara `required?: boolean` (opcional): si
    `get_form_fields` lo trae, el campo obligatorio se pinta con el borde
    del acento; si no, no se pinta nada.
  - Remates del QA del ciclo 5: «Reemplazar todo» manda **una entrada por
    coincidencia** (dos en la misma línea son dos entradas: `replace_text`
    cuenta cuántas le llegan), `tamanoFichero` dice los bytes por debajo de
    1 KB, la fecha del adjunto sale en formato local, la banda de firmas
    añade «no se ha comprobado quién emitió el certificado» cuando la
    confianza no es de raíz conocida (**sin cambiar el color**: la
    confianza es del certificado y la validez del documento) y la banda de
    recuperación se pliega a un botón «Recuperar…» de la barra en cuanto se
    abre otro documento.
- **Menú nativo** (`menu.rs`): Archivo, Editar, Ver, Documento, Ventana y
  Ayuda en la barra del sistema, espejo del menú «Acciones» de la app —
  con esto la búsqueda de menús de macOS encuentra por fin «Marca de
  agua». **No ejecuta nada**: cada entrada emite el evento `menu-accion`
  con `{ id }` y la UI lo enruta a la misma función que su botón, para que
  no haya dos caminos que puedan separarse. La estructura es un dato
  (`menu::estructura()`), con un test que comprueba que no hay ids
  repetidos ni entradas sin etiqueta. `set_menu_state(has_document)`
  vuelve a montar el menú para atenuar lo que no aplica (en Acrobat se
  atenúa, no desaparece): lo llama **la UI** al abrir y al cerrar
  documento, y también `open_pdf` y `close_document` por dentro
  (`menu::refleja_documento`, con el handle que guarda el setup en
  `menu::registra_app`). **Ojo**: las entradas llevan su acelerador, así
  que en macOS el sistema se queda con ⌘S, ⌘Z, ⌘P… antes que el webview;
  si la UI no escucha `menu-accion`, esos atajos dejan de funcionar.
  Lo que resuelve el sistema va como `Elemento::Nativa` y **no tiene id**:
  Salir y Acerca de, que en macOS están en el menú de la aplicación y en
  Windows y Linux se reponen al final de Archivo y de Ayuda. **Copiar y
  Seleccionar todo fueron nativas hasta el ciclo 5 y ya no**: se pusieron
  así porque con un id propio macOS se quedaba con ⌘C y ⌘A antes que el
  webview, pero con `PredefinedMenuItem` AppKit se las queda igual y le
  manda al webview una orden de copiar del DOM — y **la selección del
  visor no es del DOM**: la pinta Vitela sobre las cajas de glifos de
  PDFium. En la app empaquetada no copiaban nada. Ahora emiten
  `menu-accion` como el resto y la UI hace lo mismo que su atajo.
  La marca temporal `Entrada::pendiente_ui` dice que la mitad de la UI de
  ese id llega en otra rama: el test cruzado avisa por stderr en vez de
  fallar, y **se le quita al integrar**. Los ids —**la única lista**, la
  que enruta la UI, cruzada por un test— son:
  - Archivo: `abrir`, `abrir-reciente`, `crear-desde-imagenes`,
    `guardar`, `guardar-como`,
    `cerrar-documento`, `anadir-pdf`, `insertar-pdf`, `combinar-ficheros`,
    `reemplazar-paginas`, `extraer-paginas`, `dividir-documento`,
    `imprimir`.
  - Editar: `deshacer`, `rehacer`, `copiar`, `seleccionar-todo`,
    `buscar`, `buscar-siguiente`, `buscar-anterior`, `preferencias`.
  - Ver: `zoom-mas`, `zoom-menos`, `zoom-pagina`, `zoom-100`,
    `zoom-ancho`, `pagina-una`, `pagina-continua`, `pagina-dos`,
    `pagina-dos-continua`, `girar-vista-derecha`, `girar-vista-izquierda`,
    `vista-atras`, `vista-adelante`, `panel-lateral`, `pantalla-completa`,
    `modo-nocturno`, `leer-en-voz-alta` (⇧⌘Y, donde lo pone Acrobat).
  - Documento: `organizar-paginas`, `recortar-pagina`, `marca-de-agua`,
    `encabezado-pie`, `quitar-marca-de-agua`, `quitar-encabezados`,
    `anadir-campo`, `reconocer-campos`, `anadir-enlace`,
    `adjuntar-fichero`, `firmar`, `proteger`,
    `quitar-proteccion`, `aplanar`, `redactar`, `sanitizar`,
    `propiedades`, `exportar-imagenes`, `exportar-texto`, `exportar-word`,
    `exportar-comentarios`, `importar-comentarios`, `comprimir`.
  - Ayuda: `atajos` (⌘/ y F1) (y Acerca de, nativa).
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
  La UI lo cuenta tal cual: el candado de la barra sale solo cuando el
  fichero en disco está cifrado de verdad (se abrió con contraseña, o ya se
  ha guardado con la protección puesta) y mientras tanto lleva la etiqueta
  «se protegerá al guardar». La pregunta de «¿mantengo la contraseña?» se
  hace **una vez por documento** y se recuerda: guardar es un gesto que se
  repite, y una pregunta que sale en cada ⌘S se contesta sin leerla.
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
  un reciente falla porque ya no está, lo quita con `remove_recent`. En el
  estado vacío cada fila trae además su ficha (`pdf_info`, que no abre el
  documento ni pide contraseña): «12 páginas · 1,4 MB» y el candado si va
  cifrado, para no descubrirlo al pinchar. «Abrir reciente…» del menú
  nativo sin documento lleva el foco a esa lista, o dice que no hay
  ninguna. Una lista corrupta o un `DIR_DATOS` sin
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
- **Errores de transporte** (`src/ipc.ts`): cuando la llamada **no llega**
  al backend —el motor caído, el puente de QA parado, el IPC sin
  responder—, la banda enseñaba «TypeError: Failed to fetch». `invoke`
  distingue ahora «no ha llegado» de «el backend ha dicho que no»: nuestros
  comandos rechazan con un `String` (el mensaje que ya escribe
  `mensaje_llano` en Rust) y el transporte con un `Error`, que se traduce a
  «No se ha podido hablar con el motor de PDF. Cierra y vuelve a abrir
  Vitela; el documento sigue en la copia de trabajo».
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
  `exportar.rs`, `firma.rs`, `confianza.rs`, `firmas_visuales.rs`,
  `comentarios.rs`/`comentarios2.rs`, `adjuntos.rs`, `historial.rs`,
  `recientes.rs`, `recuperacion.rs`, `menu.rs`, `puente_dev.rs`.
  `generate_handler!` y `despachar` referencian los comandos por ruta de
  módulo (con re-exports no funciona el macro).
- **Estructura de la UI**: `App.tsx` conserva el ciclo de apertura, la
  geometría del visor, atajos e impresión; el resto vive en hooks
  (`src/hooks/`: `useHistorial`, `useRenderCache`, `useMiniaturas`,
  `useBusqueda`, `useFirmas`, `useHerramienta`, `useModal`) y componentes
  (`Busqueda` + `CajonBusqueda`, `OpcionesHerramienta`, `MenuAcciones`,
  `PanelPaginas`, `PanelMarcadores`, `PanelComentarios`, `PanelFirmasDoc`,
  `RangoPaginas`, `Dialogo*`; `useReemplazo` lleva «Reemplazar» y
  «Reemplazar todo», y se crea en `App` **después** de `afterMutation`,
  que es un `useCallback` y no se iza). El
  sidebar tiene tres pestañas fijas —Páginas, Marcadores y Comentarios— y
  tres que solo salen cuando el documento tiene qué enseñar: Firmas,
  Adjuntos (`list_attachments`) y Capas (`list_layers`). Por eso mide
  200 px y las deja envolver a dos líneas: seis fijas no cabrían. **Apagar
  una capa cambia el fichero** (PDFium respeta el `/OFF` del documento al
  renderizar y pdfium-render 0.8 no expone el contexto OCG), así que deja
  su paso de deshacer y el panel lo dice en una línea bajo la lista. `PanelFirmas` (sin «Doc») es otra cosa: la
  biblioteca de firmas manuscritas del modo Firma.
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
  `src/components/pagina/`. Las tarjetas flotantes se colocan con
  `cardTop` (`geometria.ts`), que las **vuelca hacia arriba cuando debajo
  no caben**: en la última línea de la página había que hacer scroll para
  llegar al botón de guardar. Los hooks se llaman `use…` (lo exige
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
- **Presentación de página** (`cargaVista`/`guardaVista` en `tipos.ts`):
  las cuatro de Acrobat —`una`, `continuo` (por defecto), `dos` y
  `dos-continuo`— con su segmentado en la píldora de navegación.
  `filasDePaginas(pageCount, dobles, portadaSola)` reparte las páginas en
  filas de pantalla (la portada sola si se marca «Portada»); en las
  presentaciones no continuas solo se monta la fila actual y `gotoPage`
  lleva el visor arriba en vez de hacer `scrollIntoView`. Toda la
  geometría del scroll (`alturasFila`, `onViewerScroll`, el ancla del
  zoom) trabaja por filas, no por páginas, y con dos columnas cada hoja se
  queda con media anchura útil. En las presentaciones de dos, ←, → y los
  botones de la píldora avanzan **el pliego entero** (`paginaVecina`) y el
  contador enseña la página izquierda, como Acrobat. Al cambiar de
  presentación el visor se recoloca en la página que se estaba leyendo: el
  scroll se quedaba donde estaba y nadie recalculaba el contador.
- **Imprimir**: el diálogo propio rasteriza el rango pedido y monta las
  páginas en `.print-pages`, que va **fuera de `.app`** con
  `createPortal(…, document.body)`. El `@media print` lleva
  `@page { size: auto; margin: 0 }` y `.print-pages img { max-height:
  100vh; object-fit: contain }`: sin eso, una A4 a ancho completo no cabía
  en el área imprimible, se empujaba a la hoja siguiente y salía **un folio
  en blanco por página** (AC-048). El diálogo cuenta cuántas hojas van a
  salir y dice dentro —sin cerrarse— cuándo el rango o el filtro de
  pares/impares no dejan ninguna (`paginasImprimibles`, en `tipos.ts`,
  compartida con `App`). El `@media print` esconde `.app`
  entera, y un ancestro en `display:none` saca todo su subárbol de la caja
  de renderizado: dentro, la hoja salía en blanco. Esc cancela la
  preparación (`printCancelRef`) y los blob URLs se liberan en la limpieza
  del efecto, no dentro del temporizador.
- **Pantalla completa** (⌘L): estado de `App` + `ponerPantallaCompleta` de
  `ipc.ts` (`getCurrentWindow().setFullscreen`, no-op en el navegador de
  QA, donde el chrome se esconde igual). La clase `.app.presentacion`
  esconde barra, panel, fila contextual **y la píldora**, que vuelve al
  acercar el ratón al borde inferior (clase `.pildora`, 200 ms); el
  segmentado de modos no se pinta, porque en presentación manda «una sola
  página». La presentación pasa a una hoja cada vez, el clic y las flechas
  avanzan, se fuerza el modo Seleccionar para que Esc sea siempre la salida
  y la primera vez se avisa de cómo salir (una sola vez de verdad:
  `avisoPantallaVisto` en `localStorage`). `onPantallaCompleta` (`ipc.ts`)
  escucha el redimensionado de la ventana y pregunta `isFullscreen()`:
  Tauri no emite un evento propio y salir con el botón verde o ⌃⌘F dejaba
  el chrome escondido. En QA se dispara con
  `window.__vitelaPantallaCompleta(true)`.
- **Modo nocturno del documento** (⇧⌘L y Preferencias): filtro CSS cálido
  sobre `.viewer.nocturno .page` —invertir y devolver el tono de la mesa;
  nunca `invert(1)` a secas, que deja el papel azul-pizarra—. Solo toca el
  render: overlays, exportación y fichero se quedan como están.
- **Historial de vistas** (⌥← / ⌥→): dos pilas en `App` con
  `{ page, scrollTop, zoom }`. `saltarA(page)` es el `gotoPage` de los
  saltos largos —enlaces, marcadores, comentarios del panel, firmas,
  coincidencias de búsqueda, **el clic en una miniatura y «Ir a la
  página»**— y apila de dónde se viene; los dos botones
  solo salen en la píldora cuando hay algo que recorrer. **El punto de
  partida se lee FUERA del updater de `setState`**: React los ejecuta al
  procesar la cola, ya después del `scrollIntoView` síncrono de `gotoPage`,
  así que lo que se apilaba era el destino y ⌥← no volvía a ninguna parte.
  Las dos pilas se vacían al abrir y al cerrar documento (y el panel
  lateral vuelve a «Páginas»): son del documento, no de la app.
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
  `window.__vitelaCerrar()`) y `onMenuAccion` (evento `menu-accion` con
  `{ id }` del menú nativo; en QA se dispara con
  `window.__vitelaMenu("guardar")`).
- **Menú nativo, la mitad de la UI**: `App.tsx` tiene el mapa
  `accionesMenu`, y cada id hace exactamente lo mismo que su botón. **La
  lista de ids es una sola y vive en `menu::estructura()`** (arriba): no se
  repite aquí a propósito, porque el ciclo 3 demostró que dos listas
  paralelas se separan en silencio (el `?.()` de JavaScript se traga el id
  que no existe). El test `los_ids_del_menu_estan_todos_en_la_ui`
  (`menu.rs`) lee `src/**/*.ts*`, saca las claves de `accionesMenu` y exige
  que coincidan con `estructura()` en los dos sentidos. **Un id nuevo se
  añade en `estructura()` y en `accionesMenu`, o el test lo canta.**
  «Ayuda ▸ Atajos de teclado» abre `DialogoAtajos`, la única pantalla donde
  están escritos todos los atajos, ella incluida (⌘/ y F1).
- **Preferencias y memoria de la UI** en `localStorage` (`src/tipos.ts`):
  colores por acción, opciones de búsqueda (`Aa` y `|ab|`), «Resaltar
  campos» de los formularios (encendido por defecto), la presentación de
  página (`cargaVista`: `modoPagina` y `portadaSola`) y las preferencias
  de ⌘, (`cargaPreferencias`): `autor` de los comentarios —que se manda
  como `author` en cada comando que crea una anotación—, `nocturno`,
  `tema` (`automatico`/`claro`/`oscuro`), `zoomInicial`
  (`pagina`/`ancho`/`100`/`ultimo`) y `lienzo` (`verde`/`gris`). También el
  **último zoom** (`cargaZoom`/`guardaZoom`), sin el cual «zoom al abrir: el
  último» solo valía dentro de la misma sesión, y el aviso de pantalla
  completa, que ahora se enseña una vez y no una por arranque.
  `DialogoPreferencias` es una sola columna sin pestañas y **aplica cada
  cambio al instante** (por eso su botón dice «Cerrar»); `App` guarda las
  preferencias vivas en estado y escribe `data-tema` y `data-lienzo` en el
  `<html>`, que es donde el CSS los espera (con `automatico` no pone
  `data-tema` y manda `prefers-color-scheme`).
- **Avisos**: `setNotice(texto, { persistente, dato, accion })`.
  `persistente` es el progreso («Comprimiendo…»), que se queda hasta que
  lo sustituye su resultado; sin la opción, el aviso se va solo a los 6 s.
  `dato` es el contador honesto en Fragment Mono («12 / 200», lo que pide
  DESIGN.md) y `accion` el botón de la propia banda, que es como se
  cancela la preparación de la impresión. Aviso y error se limpian al
  abrir y al cerrar documento —son del documento que los provocó— y **el
  error se limpia también con el primer aviso de éxito** o con cualquier
  mutación que salga bien: la banda roja se quedaba en pantalla a través
  de operaciones correctas. Los recuentos usan `plural(n, singular,
  plural)`, nunca «página(s)».
- **Buscar y reemplazar**: del campo de búsqueda cuelga `CajonBusqueda`
  (plegado por defecto) con «N coincidencias en M páginas», una fila por
  coincidencia con su frase de contexto, y debajo «Reemplazar con…» con sus
  dos botones. ↑/↓ recorren la lista sin salir del campo y los saltos pasan
  por `saltarA`, así que ⌥← sigue funcionando. Reemplazar es una sola
  mutación y **los recuentos van en coincidencias, no en bloques** (que es
  vocabulario del motor y un número que el usuario no puede reconocer):
  «12 coincidencias reemplazadas · 3 en un texto que no se puede
  reescribir». Lo que no se va a poder hacer se dice **antes**, bajo el
  campo, con las coincidencias ya situadas por su `block_index`. La pasada
  de `context` solo se pide con el cajón abierto; si se abre sobre una
  búsqueda hecha sin él, se repite una vez en silencio. Y **tras cualquier
  mutación la búsqueda se rehace en vez de tirarse** (`trasMutacion`): un
  ⌘Z detrás de reemplazar dejaba la lista vacía con el término escrito.
  ⌘G sin coincidencias repite la última búsqueda, como Acrobat.
- **Marca de agua y encabezados**: los dos diálogos comparten el bloque
  `RangoPaginas` («todas» / «1-3, 8», con `indicesDeRango` en `tipos.ts`) y
  enseñan **vista previa en vivo** sobre la miniatura que el panel lateral
  ya tiene renderizada, con la marca dibujada por la propia UI: ni una
  llamada al backend por cada tecla. La imagen de la marca se elige con el
  selector del navegador (`<input type="file">` + `FileReader`) y no con el
  diálogo nativo, porque la previa necesita los bytes de todas formas.
- **Banda de firmas**: al abrir, `verify_signatures` y, si el documento
  lleva firmas, una banda con el resumen en llano —ni «CMS», ni
  «ByteRange», ni «digest»—; el clic abre la pestaña «Firmas». Se cierra y
  no vuelve hasta el documento siguiente. **Tres estados, no dos**
  (`estadoDeFirma` → `nivel`: `ok`, `mal`, `duda`): «no se ha podido
  comprobar» es neutro, con el gris lápiz, y nunca rojo — una firma ECDSA o
  con SHA-512 salía acusando de manipulación un documento intacto. La banda
  toma el peor estado de todas y nombra a quien firmó esa (bajando **solo
  la primera letra** de la frase: `toLowerCase()` entero se llevaba por
  delante la mayúscula de «Vitela»). **Una firma válida con una revisión
  detrás es `duda`, no `mal`**: rojo solo cuando el digest no cuadra —una
  actualización incremental es lo normal en un PDF firmado que sigue vivo,
  y en Acrobat tampoco es roja—. La tarjeta del panel añade quién responde
  por el certificado (`confianza`) y no llama «caducado» a uno cuyo periodo
  de validez empezaba después de la firma.
- **Comentarios con hilo y estado** (`PanelComentarios`): las respuestas
  (`in_reply_to`) se pintan indentadas bajo su comentario y comparten con
  él el mismo `roving tabindex`, así que ↑/↓, Enter y Supr valen para las
  dos. El estado de revisión es un desplegable **en el propio comentario**
  —no un panel de propiedades aparte— y se guarda como lo guarda Acrobat.
  Hay un tercer filtro por estado junto a los de tipo y autor, y Esc los
  quita los tres. «Exportar comentarios…» (`export_comments`) está en el
  bloque Salida, con «Word (.docx)…».
- **Recuperación tras un cierre inesperado**: la UI apunta la sesión
  (`autosaveState`) diez segundos después del último cambio y la borra al
  guardar y al cerrar. Al arrancar, `recoverSession` y, si hay algo, una
  banda de una línea con «Recuperar» y «No guardar». Recuperar abre la copia
  de trabajo pero **conserva el fichero original** (`openPath(path, pwd,
  original)`), para que ⌘S no escriba en el temporal. «No guardar» se llama
  igual que en el diálogo de cierre y **confirma**: es el único borrado
  irreversible de trabajo del usuario que hay en la app. El apunte manda
  `modified`, sin el cual no se apuntaba nada (AC-047).
- **Fila contextual** (`OpcionesHerramienta`): además de trazo, formas,
  sello y cuadro, el modo Editar lleva color («A» = el que ya tenga, que es
  el defecto, y que se pinta del color real del bloque señalado —`TextBlock`
  trae `color`—), alineación e interlineado, y el modo Firma la fila de marcas de rellenar
  (✓, ✗, ●, línea y «Texto», que lleva al cuadro de texto). La fila va fija
  bajo la barra y **baja lo que ocupen las bandas** (`--bandas` en el
  `.app`): antes se pintaba encima de la de firmas.
- **Atajos de teclado** (`App.tsx`, un solo `useEffect`; con un modal o el
  menú «Acciones» abiertos solo pasa Escape): ⌘O abrir · ⌘S guardar ·
  ⇧⌘S guardar como · ⌘P imprimir · ⌘D propiedades · ⌘, preferencias ·
  ⌘F buscar · ⌘G y ⇧⌘G coincidencia siguiente/anterior · ⌘Z y ⇧⌘Z
  deshacer/rehacer (no llaman al backend si no hay historial) · ⌘+ y ⌘−
  zoom · ⇧⌘+ y ⇧⌘− giran la vista · ⌘0 página entera, ⌘1 al 100 % y ⌘2
  al ancho (los tres de Acrobat) · ⌥⌘1 plegar el panel lateral, ⌥⌘2
  Marcadores y ⌥⌘3 Comentarios (abren la pestaña Y le llevan el foco) ·
  ⌘L pantalla completa (Esc sale) · ⇧⌘L modo nocturno del documento ·
  ⌥← y ⌥→ historial de vistas · ⇧⌘Y leer en voz alta desde esta página ·
  ⇧⌘N ir a la página · ⌘/ y F1 abren los atajos (la pantalla se lista a sí
  misma) · ←/→ página anterior y siguiente · Esc para la lectura, quita las
  coincidencias de búsqueda y, si no hay, sale de la herramienta · Supr
  borra la anotación seleccionada · ⌘A todo el texto de la página (dentro
  del panel de páginas, seleccionarlas todas) · ⌘C copiar la selección.
  En los borradores de comentario y de cuadro de texto Enter salta de
  línea y ⌘Enter confirma; en un campo de formulario Tab y ⇧Tab confirman
  y saltan al campo siguiente o anterior. **Están todos escritos en
  `DialogoAtajos`** («Ayuda ▸ Atajos de teclado»), agrupados como el menú;
  un atajo nuevo se añade ahí, o solo lo descubre quien pase el ratón por
  el botón.
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
   (por bloque, misma fuente; **con reflujo del párrafo desde el ciclo 6**,
   pero nunca entre bloques ni entre páginas, como Acrobat). Sin reflujo,
   las líneas extra se insertan como objetos nuevos con fuente estándar
   aproximada por familia/estilo, colocados debajo (no se reutiliza el handle
   de `FPDFTextObj_GetFont`: queda ligado a la página y PDFium casca con
   handles colgantes). Fuentes (`fuente_por_nombre`): estándar aproximada →
   TTF de /System/Library/Fonts/Supplemental (best effort; ojo: los TTF
   cargados con `FPDFText_LoadFont` no llevan ToUnicode y su extracción
   pierde los no-ASCII) → Helvetica. "Arial" se mapea a Helvetica (la
   builtin de PDFium se identifica como «Arial» o, desde chromium/8000,
   «Chrom Sans OTF»; `normaliza_familia` devuelve siempre las estándar).
   Un bloque se **coloca y se estira** con los mismos ocho tiradores que
   los sellos y las imágenes (`move_text_block`, `resize_text_block`;
   estirar escala el cuerpo de la fuente, no deforma los glifos), y la fila
   contextual lleva interlineado y espaciado: el interlineado se resuelve
   **colocando los objetos** (en un PDF no hay `TL` que valga entre objetos
   distintos) y el espaciado sí es el operador `Tc`, escrito con lopdf.
   Imágenes: insertar, mover,
   redimensionar, girar, voltear, ordenar, recortar (`crop_image`, desde el
   popover), reemplazar y borrar objetos de imagen
7. ✅ Firma digital: campo de firma (visible con `rect`, con su `/AP`:
   firma manuscrita, «Firmado por …», fecha y motivo) + ByteRange +
   PKCS#7 detached, y verificación al abrir (`verify_signatures`, con
   cadena de confianza contra el almacén del sistema desde el ciclo 5:
   `confianza` dice «raiz_conocida», «autofirmado» o «desconocida», y
   **nunca** «válida», porque no se comprueba la revocación). Vitela firma en RSA/SHA-256
   (certificado en PEM o contenedor .p12/.pfx con contraseña —
   `p12-keystore`) y **verifica** además RSA-PSS y ECDSA P-256/P-384 con
   SHA-256/384/512, con tres estados («ok», «modificado», «desconocido»).
   **Varias firmas**: sobre un documento ya firmado, la nueva va en una
   actualización incremental que no toca un byte de las anteriores.
   PDFium no firma: cirugía con lopdf y criptografía con RustCrypto
8. ✅ Comentarios completos: llamada (`/FreeText` con `/CL`), comentario
   asociado a un resaltado (doble clic sobre la marca) y goma de borrar
   (`erase_ink`, un conmutador del modo Dibujar). Formularios: radio con
   grupo, desplegable, lista y las propiedades del primer panel de Acrobat.
   Medir distancias y áreas con escala por documento, y leer en voz alta
   con la síntesis del webview. Reflujo del párrafo al corregir texto
   (`edit_text_block` con `reflow`).

9. ✅ Revisión que sale del programa: resumen de comentarios **en PDF** y
   **XFDF** de ida y vuelta (`comentarios2.rs`), marcadores con destino
   fino (zoom y posición), reconocimiento de campos de formulario que
   **propone y no escribe**, medida puesta como comentario y varios
   documentos abiertos a la vez en el caché del hilo de PDFium. Queda
   fuera de esta rama la fila de pestañas y la lista de sesiones de
   recuperación, que son la mitad de interfaz de esa función.

## Convenciones

- Commits: mensajes limpios, sin `Co-Authored-By` ni menciones a IA/Claude.
- UI y textos de la app en español.

## Sistema de diseño

Lee SIEMPRE DESIGN.md antes de cualquier decisión visual o de UI. Fuentes,
colores, spacing y dirección estética están definidos ahí. No te desvíes sin
aprobación explícita del usuario. En modo QA, señala cualquier código que no
case con DESIGN.md.
