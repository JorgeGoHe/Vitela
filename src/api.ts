import { invoke } from "./ipc";
import type { AnnotationInfo, SearchMatch, TextBlock } from "./tipos";

/** Firma manuscrita guardada en la biblioteca del usuario. */
export type FirmaGuardada = {
  id: string;
  name: string;
  png_base64: string;
};

export function listStoredSignatures(): Promise<FirmaGuardada[]> {
  return invoke("list_stored_signatures");
}

export function saveStoredSignature(
  name: string,
  pngBase64: string,
): Promise<FirmaGuardada> {
  return invoke("save_stored_signature", { name, pngBase64 });
}

export function importSignatureFile(imagePath: string): Promise<FirmaGuardada> {
  return invoke("import_signature_file", { imagePath });
}

export function deleteStoredSignature(id: string): Promise<void> {
  return invoke("delete_stored_signature", { id });
}

/** Busca en todo el documento. `matchCase` y `wholeWord` son las dos
 *  opciones de la búsqueda de Acrobat; por defecto van apagadas. Con
 *  `context` cada coincidencia trae además la frase de alrededor y el
 *  bloque de texto en el que cae, que es lo que hacen falta para la lista
 *  del cajón y para reemplazar. **No tiene defecto a propósito**: esa
 *  pasada extra recorre los objetos de todas las páginas, así que se pide
 *  solo cuando se va a usar. */
export function searchPdf(
  path: string,
  query: string,
  matchCase: boolean,
  wholeWord: boolean,
  context: boolean,
): Promise<SearchMatch[]> {
  return invoke("search_pdf", { path, query, matchCase, wholeWord, context });
}

/** Bloques de texto de una página, tal como están en el content stream. */
export function getTextBlocks(
  path: string,
  pageIndex: number,
): Promise<TextBlock[]> {
  return invoke("get_text_blocks", { path, pageIndex });
}

/** Alineación del texto: se resuelve colocando el origen del objeto, no con
 *  un operador del PDF. */
export type Alineacion = "izq" | "centro" | "der";

/** Texto nuevo en un punto de la página (contenido, no anotación).
 *  `lineHeight` y `charSpacing` son los operadores `TL` y `Tc`; sin ellos,
 *  los de siempre. */
export function addTextBlock(args: {
  workPath: string;
  pageIndex: number;
  x: number;
  y: number;
  text: string;
  fontSize: number;
  font?: string | null;
  color?: Rgba | null;
  align?: Alineacion | null;
  lineHeight?: number | null;
  charSpacing?: number | null;
}): Promise<void> {
  return invoke("add_text_block", {
    font: null,
    color: null,
    align: null,
    lineHeight: null,
    charSpacing: null,
    ...args,
  });
}

/** Reescribe un bloque del content stream. Sin `color` ni `align` se queda
 *  con los que tuviera: editar no debe recolorear sin querer. */
export function editTextBlock(args: {
  workPath: string;
  pageIndex: number;
  objectIndex: number;
  newText: string;
  color?: Rgba | null;
  align?: Alineacion | null;
  lineHeight?: number | null;
  charSpacing?: number | null;
}): Promise<void> {
  return invoke("edit_text_block", {
    color: null,
    align: null,
    lineHeight: null,
    charSpacing: null,
    ...args,
  });
}

/** Coloca un bloque de texto en otro punto de la página (su matriz), como
 *  `transform_image` hace con una imagen. Coordenadas en el espacio propio
 *  de la página. */
export function moveTextBlock(
  workPath: string,
  pageIndex: number,
  objectIndex: number,
  x: number,
  y: number,
): Promise<void> {
  return invoke("move_text_block", { workPath, pageIndex, objectIndex, x, y });
}

/** Estira un bloque de texto: escala el tamaño de fuente, no deforma los
 *  glifos. Coordenadas en el espacio propio de la página. */
export function resizeTextBlock(
  workPath: string,
  pageIndex: number,
  objectIndex: number,
  w: number,
  h: number,
): Promise<void> {
  return invoke("resize_text_block", { workPath, pageIndex, objectIndex, w, h });
}

/** Recorta una imagen al rectángulo pedido (en el espacio propio de la
 *  página), recreando el objeto con el bitmap recortado. */
export function cropImage(
  workPath: string,
  pageIndex: number,
  objectIndex: number,
  rect: { x: number; y: number; w: number; h: number },
): Promise<void> {
  return invoke("crop_image", { workPath, pageIndex, objectIndex, rect });
}

/** Mueve, redimensiona, gira y voltea un objeto de imagen. `rotate` va en
 *  múltiplos de 90 y se suma a lo que ya tuviera. */
export function transformImage(args: {
  workPath: string;
  pageIndex: number;
  objectIndex: number;
  x: number;
  y: number;
  w: number;
  h: number;
  rotate?: number | null;
  flipH?: boolean | null;
  flipV?: boolean | null;
}): Promise<void> {
  return invoke("transform_image", {
    rotate: null,
    flipH: null,
    flipV: null,
    ...args,
  });
}

/** Trae la imagen al frente o la manda al fondo del content stream. */
export function reorderImage(
  workPath: string,
  pageIndex: number,
  imageIndex: number,
  alFrente: boolean,
): Promise<void> {
  return invoke("reorder_image", {
    workPath,
    pageIndex,
    imageIndex,
    alFrente,
  });
}

/** Una sustitución: en qué bloque, qué sale y qué entra. */
export type Reemplazo = {
  page_index: number;
  /** `object_index` del bloque dentro de su página (`get_text_blocks`). */
  block_index: number;
  from: string;
  to: string;
};

/** Reemplaza texto en varios bloques a la vez: el lote entero en UNA sola
 *  mutación, así que un ⌘Z lo devuelve de una vez. Devuelve cuántos bloques
 *  ha cambiado; los que no se pueden reescribir se saltan sin romper el
 *  resto. */
export type ResultadoReemplazo = { hechas: number; saltadas: number };

export function replaceText(
  workPath: string,
  matches: Reemplazo[],
): Promise<ResultadoReemplazo> {
  return invoke("replace_text", { workPath, matches });
}

/** Fichero abierto hace poco (la lista vive en el backend, máx. 8). */
export type Reciente = {
  path: string;
  name: string;
  /** Carpeta que lo contiene, para distinguir dos ficheros con el mismo nombre. */
  dir: string;
  /** Falso si ya no está en esa ruta (movido o borrado). */
  exists: boolean;
  /** Momento de la última apertura (lo escribe el backend). */
  opened_at: string | number;
};

/** Ruta con la que arrancó la app (doble clic en el Finder con Vitela
 *  cerrada, o argumento de la línea de órdenes). La UI la pide al montar,
 *  cuando ya escucha `abrir-fichero`: los eventos de Tauri no se encolan,
 *  así que el arranque en frío no puede depender de ellos. */
export function uiLista(): Promise<string | null> {
  return invoke("ui_lista");
}

export function listRecent(): Promise<Reciente[]> {
  return invoke("list_recent");
}

/** Sube un fichero al principio de la lista (la UI lo llama tras abrir). */
export function touchRecent(path: string): Promise<void> {
  return invoke("touch_recent", { path });
}

export function removeRecent(path: string): Promise<void> {
  return invoke("remove_recent", { path });
}

export type Rgba = [number, number, number, number];

/** Marca de texto sobre rects: resaltar, subrayar o tachar. */
export function addMarkup(args: {
  workPath: string;
  pageIndex: number;
  rects: { x: number; y: number; w: number; h: number }[];
  kind: "highlight" | "underline" | "strikeout";
  color?: Rgba;
  /** Autor del comentario; null = el nombre de usuario del sistema. */
  author?: string | null;
}): Promise<void> {
  return invoke("add_markup", { color: null, ...args });
}

/** Forma geométrica entre dos puntos (coords de página). */
export function addShape(args: {
  workPath: string;
  pageIndex: number;
  kind: "rect" | "ellipse" | "line" | "arrow";
  x1: number;
  y1: number;
  x2: number;
  y2: number;
  stroke: Rgba;
  fill?: Rgba | null;
  strokeWidth: number;
  author?: string | null;
}): Promise<void> {
  return invoke("add_shape", { fill: null, ...args });
}

/** Sello de texto centrado en el punto dado. */
export function addStamp(args: {
  workPath: string;
  pageIndex: number;
  text: string;
  color: Rgba;
  x: number;
  y: number;
  fontSize: number;
  author?: string | null;
}): Promise<void> {
  return invoke("add_stamp", { author: null, ...args });
}

/** Render de página como URL para <img>. En Tauri llega como bytes (IPC
 *  binario, sin base64 ni JSON: los JSON de varios MB congelaban el webview)
 *  → blob URL (el llamante debe revocarla al desecharla); en el navegador
 *  del puente de QA llega como base64 → data URL. */
export async function renderPageSrc(
  path: string,
  pageIndex: number,
  width: number,
  opts?: { background?: boolean; withAnnotations?: boolean },
): Promise<string> {
  const args: Record<string, unknown> = { path, pageIndex, width };
  // sin la opción, el render de siempre (con anotaciones); con `false`,
  // el documento limpio, que es lo que pide «Solo el documento» al imprimir
  if (opts?.withAnnotations !== undefined) {
    args.withAnnotations = opts.withAnnotations;
  }
  const r = await invoke<unknown>("render_page", args, opts);
  if (typeof r === "string") return `data:image/png;base64,${r}`;
  return URL.createObjectURL(
    new Blob([r as ArrayBuffer], { type: "image/png" }),
  );
}

/** Un comentario del documento: lo mismo que devuelve `get_annotations`
 *  más la página en la que está. */
export type AnotacionDoc = AnnotationInfo & { page_index: number };

/** Los cuatro estados de revisión de Acrobat, con el nombre que se escribe
 *  en el PDF (`/State` con `/StateModel /Review`): así los ve también quien
 *  abra el documento en Acrobat, que es toda la gracia. `""` es sin estado. */
export type EstadoComentario =
  | ""
  | "Accepted"
  | "Rejected"
  | "Cancelled"
  | "Completed";

/** Responde a un comentario: crea una anotación con `/IRT` apuntando a la
 *  original, que es como se guarda un hilo. Devuelve el índice de la
 *  respuesta en `/Annots` de su página. */
export function replyAnnotation(
  workPath: string,
  pageIndex: number,
  annotIndex: number,
  text: string,
  author?: string | null,
): Promise<number> {
  return invoke("reply_annotation", {
    workPath,
    pageIndex,
    annotIndex,
    text,
    author: author ?? null,
  });
}

/** Pone (o quita, con `""`) el estado de revisión de un comentario. */
export function setAnnotationState(
  workPath: string,
  pageIndex: number,
  annotIndex: number,
  state: EstadoComentario,
): Promise<void> {
  return invoke("set_annotation_state", {
    workPath,
    pageIndex,
    annotIndex,
    state,
  });
}

/** Saca la lista de comentarios a un fichero para leerla fuera: el
 *  «Resumen de comentarios» de Acrobat. */
export function exportComments(
  path: string,
  destPath: string,
  formato: "txt",
): Promise<void> {
  return invoke("export_comments", { path, destPath, formato });
}

/** Todos los comentarios del documento en una sola pasada (una llamada por
 *  página serían 300 viajes por el canal del hilo de PDFium). */
export function getDocumentAnnotations(path: string): Promise<AnotacionDoc[]> {
  return invoke("get_document_annotations", { path });
}

/** Elige el valor de un desplegable o de una lista del formulario. */
export function setFormChoice(
  workPath: string,
  pageIndex: number,
  fieldIndex: number,
  value: string,
): Promise<void> {
  return invoke("set_form_choice", { workPath, pageIndex, fieldIndex, value });
}

/** Borra varias páginas en una sola mutación (un solo paso de deshacer);
 *  devuelve el total que queda. */
export function deletePages(
  workPath: string,
  pageIndices: number[],
): Promise<number> {
  return invoke("delete_pages", { workPath, pageIndices });
}

/** Gira varias páginas en una sola mutación; `quarterTurns` va con signo
 *  (±1, ±2, ±3) para admitir el giro antihorario. */
export function rotatePages(
  workPath: string,
  pageIndices: number[],
  quarterTurns: number,
): Promise<void> {
  return invoke("rotate_pages", { workPath, pageIndices, quarterTurns });
}

/** Escribe las páginas dadas en un PDF nuevo; con `deleteAfter` las quita
 *  además del documento de trabajo, dentro de la misma mutación. */
export function extractPages(args: {
  workPath: string;
  pageIndices: number[];
  destPath: string;
  deleteAfter: boolean;
}): Promise<void> {
  return invoke("extract_pages", { ...args });
}

/** «Un fichero por página»: escribe `pagina-N.pdf` por cada índice dentro de
 *  `destDir` y, con `deleteAfter`, las quita del documento DENTRO de la misma
 *  mutación. Un fallo a mitad no deja el trabajo hecho a medias ni dos pasos
 *  de deshacer, y son 200 páginas en un viaje y no en doscientos. */
export function extractEachPage(args: {
  workPath: string;
  pageIndices: number[];
  destDir: string;
  deleteAfter?: boolean;
}): Promise<string[]> {
  return invoke("extract_each_page", { deleteAfter: false, ...args });
}

/** Sustituye las páginas `pageIndices` por las de otro PDF (todas, o las
 *  de `otherIndices`), conservando el resto. Devuelve el total. */
export function replacePages(args: {
  workPath: string;
  pageIndices: number[];
  otherPath: string;
  otherIndices?: number[] | null;
}): Promise<number> {
  return invoke("replace_pages", { otherIndices: null, ...args });
}

/** Parte el documento en varios ficheros dentro de `destDir`; devuelve las
 *  rutas escritas. `modo`: "cada" (con `cada` páginas) o "marcadores". */
export function splitPdf(args: {
  workPath: string;
  destDir: string;
  modo: "cada" | "marcadores";
  cada?: number | null;
}): Promise<string[]> {
  return invoke("split_pdf", { cada: null, ...args });
}

/** Añade varios PDF de una vez, en el orden dado; sin `at`, al final.
 *  Una sola mutación: un solo paso de deshacer. */
export function mergeMany(args: {
  workPath: string;
  others: string[];
  at?: number | null;
}): Promise<number> {
  return invoke("merge_many", { at: null, ...args });
}

/** Cuadro de texto de Acrobat: una anotación FreeText que escribe ENCIMA
 *  del documento, sin tocar el contenido de la página. */
export function addFreeText(args: {
  workPath: string;
  pageIndex: number;
  rect: { x: number; y: number; w: number; h: number };
  text: string;
  fontSize: number;
  color: Rgba;
  border: boolean;
  author?: string | null;
}): Promise<void> {
  return invoke("add_free_text", { author: null, ...args });
}

/** Reescribe el texto de un comentario ya creado; el backend refresca `/M`
 *  (y `/T` si llega `author`). */
export function setAnnotationContents(args: {
  workPath: string;
  pageIndex: number;
  annotIndex: number;
  contents: string;
  author?: string | null;
}): Promise<void> {
  return invoke("set_annotation_contents", { author: null, ...args });
}

/** Cambia el color de un comentario; en las marcas de texto el backend
 *  reescribe además la apariencia guardada para que el PDF exportado no
 *  siga con el color viejo. */
export function setAnnotationColor(args: {
  workPath: string;
  pageIndex: number;
  annotIndex: number;
  color: Rgba;
}): Promise<void> {
  return invoke("set_annotation_color", { ...args });
}

/** Mueve/reescala una anotación con apariencia embebida (sello o dibujo). */
export function transformAnnotation(args: {
  workPath: string;
  pageIndex: number;
  annotIndex: number;
  x: number;
  y: number;
  w: number;
  h: number;
}): Promise<void> {
  return invoke("transform_annotation", { ...args });
}

export function addBlankPage(workPath: string, index: number): Promise<number> {
  return invoke("add_blank_page", { workPath, index });
}

export function duplicatePage(
  workPath: string,
  pageIndex: number,
): Promise<number> {
  return invoke("duplicate_page", { workPath, pageIndex });
}

export function insertPdfAt(
  workPath: string,
  otherPath: string,
  index: number,
): Promise<number> {
  return invoke("insert_pdf_at", { workPath, otherPath, index });
}

/** Tamaño de página de «Crear PDF desde imágenes»: A4 y Carta ajustan la
 *  foto con 36 pt de margen y la centran; «imagen» hace la página del
 *  tamaño de la foto a 72 dpi, sin margen. */
export type TamanoImagenes = "a4" | "carta" | "imagen";

/** Crea un PDF nuevo con una imagen por página. Escribe un fichero aparte:
 *  no toca el documento abierto ni deja paso de deshacer. Devuelve cuántas
 *  páginas ha escrito. */
export function pdfFromImages(
  imagePaths: string[],
  destPath: string,
  tamano: TamanoImagenes,
): Promise<number> {
  return invoke("pdf_from_images", { imagePaths, destPath, tamano });
}

export function cropPage(
  workPath: string,
  pageIndex: number,
  rect: { x: number; y: number; w: number; h: number },
  allPages: boolean,
): Promise<void> {
  return invoke("crop_page", { workPath, pageIndex, rect, allPages });
}

export function addWatermark(args: {
  workPath: string;
  text: string;
  fontSize: number;
  color: Rgba;
  diagonal: boolean;
  /** Celda de un grid 3×3 ("nw".."se"); sin ella, centro. */
  position?: string;
  /** Índices de página; sin ellos (null), todas, como antes. */
  pageIndices?: number[] | null;
  /** PNG en base64 cuando la marca es una imagen; sin él, el texto. */
  imagePng?: string | null;
  /** Opacidad de 0 a 1; 0,3 es la de Acrobat. */
  opacity?: number;
  /** Giro en grados; 45 es la diagonal de siempre. */
  rotation?: number;
}): Promise<void> {
  return invoke("add_watermark", {
    pageIndices: null,
    imagePng: null,
    ...args,
  });
}

/** Elimina el texto marginal añadido (marca de agua o encabezados/pies). */
export function removeMarginalText(
  workPath: string,
  zona: "watermark" | "header" | "footer",
  dryRun: boolean,
): Promise<{ textos: number }> {
  return invoke("remove_marginal_text", { workPath, zona, dryRun });
}

/** Borra un campo de formulario por nombre. */
export function deleteFormField(workPath: string, name: string): Promise<void> {
  return invoke("delete_form_field", { workPath, name });
}

export type HeaderFooter = {
  headerLeft?: string;
  headerCenter?: string;
  headerRight?: string;
  footerLeft?: string;
  footerCenter?: string;
  footerRight?: string;
};

export function addHeaderFooter(
  workPath: string,
  zonas: HeaderFooter,
  fontSize: number,
  /** Índices de página; sin ellos (null), todas, como antes. */
  pageIndices: number[] | null = null,
): Promise<void> {
  return invoke("add_header_footer", {
    workPath,
    pageIndices,
    headerLeft: zonas.headerLeft || null,
    headerCenter: zonas.headerCenter || null,
    headerRight: zonas.headerRight || null,
    footerLeft: zonas.footerLeft || null,
    footerCenter: zonas.footerCenter || null,
    footerRight: zonas.footerRight || null,
    fontSize,
  });
}

export type OutlineNode = {
  title: string;
  page_index: number | null;
  children: OutlineNode[];
};

export function getOutline(path: string): Promise<OutlineNode[]> {
  return invoke("get_outline", { path });
}

export function setOutline(
  workPath: string,
  nodes: OutlineNode[],
): Promise<void> {
  return invoke("set_outline", { workPath, nodes });
}

export type Metadata = {
  title: string;
  author: string;
  subject: string;
  keywords: string;
  creator: string;
  producer: string;
};

export function getMetadata(path: string): Promise<Metadata> {
  return invoke("get_metadata", { path });
}

export function setMetadata(workPath: string, meta: Metadata): Promise<void> {
  return invoke("set_metadata", { workPath, meta });
}

export type LinkInfo = {
  x: number;
  y: number;
  w: number;
  h: number;
  uri: string | null;
  dest_page: number | null;
  /** Índice de la anotación en la página (el que entiende `remove_annotation`). */
  annot_index: number;
};

export function getLinks(
  path: string,
  pageIndex: number,
): Promise<LinkInfo[]> {
  return invoke("get_links", { path, pageIndex });
}

/** Qué deja hacer el fichero protegido; los tres van a `true` salvo que se
 *  restrinjan con contraseña de permisos. */
export type Permisos = { imprimir: boolean; copiar: boolean; editar: boolean };

export const TODO_PERMITIDO: Permisos = {
  imprimir: true,
  copiar: true,
  editar: true,
};

/** Protege con contraseña (AES-256). Sin `destPath` NO cifra nada todavía:
 *  anota la protección del documento abierto y la aplica `save_pdf` al
 *  guardar (cifrar la copia de trabajo la dejaría ilegible para el resto de
 *  comandos). Como no es una mutación, ⌘Z no la quita: la quita
 *  `removeEncryption`. Con destino escribe una copia protegida aparte y deja
 *  el documento abierto como está. */
export function encryptPdf(args: {
  workPath: string;
  destPath?: string | null;
  userPassword: string;
  ownerPassword?: string | null;
  permisos?: Permisos;
}): Promise<void> {
  return invoke("encrypt_pdf", {
    destPath: null,
    ownerPassword: null,
    permisos: TODO_PERMITIDO,
    ...args,
  });
}

/** Quita el cifrado del documento de trabajo. */
export function removeEncryption(workPath: string): Promise<void> {
  return invoke("remove_encryption", { workPath });
}

/** Aplana anotaciones y formularios a contenido fijo. */
export function flattenPdf(workPath: string): Promise<void> {
  return invoke("flatten_pdf", { workPath });
}

export type RedactReport = { textos: number; imagenes: number };

/* ---- redacción en dos fases ---- */

/** Una zona marcada para censurar. Es una propuesta, no una censura: vive
 *  en el PDF como anotación y se puede revisar, quitar y guardar. */
export type Redaccion = {
  page_index: number;
  /** Índice de la anotación DENTRO de `/Annots` de su página: el mismo que
   *  entienden `unmark_redaction`, `transform_annotation` y
   *  `remove_annotation`. No es un ordinal entre las marcas: en una página
   *  con un resaltado delante, los dos números dejan de coincidir. */
  annot_index: number;
  rect: { x: number; y: number; w: number; h: number };
};

/** Marca una zona; devuelve el `annot_index` de la marca en su página. */
export function markRedaction(
  workPath: string,
  pageIndex: number,
  rect: { x: number; y: number; w: number; h: number },
): Promise<number> {
  return invoke("mark_redaction", { workPath, pageIndex, rect });
}

export function listRedactions(workPath: string): Promise<Redaccion[]> {
  return invoke("list_redactions", { workPath });
}

/** Quita una marca por su `annot_index` (el que trae `list_redactions`). */
export function unmarkRedaction(
  workPath: string,
  pageIndex: number,
  annotIndex: number,
): Promise<void> {
  return invoke("unmark_redaction", { workPath, pageIndex, annotIndex });
}

/** Quita TODAS las marcas del documento en una sola mutación (un ⌘Z las
 *  devuelve) y responde cuántas ha quitado. */
export function unmarkAllRedactions(workPath: string): Promise<number> {
  return invoke("unmark_all_redactions", { workPath });
}

/** Aplica todas las marcas en una sola mutación; con `dryRun` solo cuenta
 *  lo que se iría. */
export function applyRedactions(
  workPath: string,
  dryRun: boolean,
): Promise<RedactReport> {
  return invoke("apply_redactions", { workPath, dryRun });
}

/** Lo que el documento lleva escondido y se puede quitar. */
export type SanitizeReport = {
  metadatos: number;
  scripts: number;
  adjuntos: number;
  capas: number;
  formularios: number;
};

/** Quita la información oculta; con `dryRun` solo cuenta por categorías. */
export function sanitizePdf(
  workPath: string,
  dryRun: boolean,
): Promise<SanitizeReport> {
  return invoke("sanitize_pdf", { workPath, dryRun });
}

/** Redacción real de un área; con dryRun solo cuenta qué caería. */
export function redactArea(
  workPath: string,
  pageIndex: number,
  rect: { x: number; y: number; w: number; h: number },
  dryRun: boolean,
): Promise<RedactReport> {
  return invoke("redact_area", { workPath, pageIndex, rect, dryRun });
}

/** Exporta todas las páginas como imágenes; devuelve las rutas escritas. */
export function exportPagesPng(
  path: string,
  destDir: string,
  dpi: number,
  format: "png" | "jpeg",
): Promise<string[]> {
  return invoke("export_pages_png", { path, destDir, dpi, format });
}

export function exportText(path: string, destPath: string): Promise<void> {
  return invoke("export_text", { path, destPath });
}

/** Lo que ha salido en el `.docx` y lo que se ha quedado por el camino.
 *  `perdido` viene ya escrito en llano por el backend (una frase por cosa
 *  que no se ha podido llevar): la exportación promete texto e imágenes, no
 *  maquetación, y el recuento lo dice en vez de cantar un éxito redondo. */
export type DocxReport = {
  parrafos: number;
  imagenes: number;
  perdido: string[];
};

/** Exporta a Word (.docx) de forma **aproximada**: un párrafo por bloque de
 *  texto, con su fuente y su color, las imágenes en su posición aproximada y
 *  un salto de página por página. No se intentan tablas ni columnas. Sin
 *  `pageIndices`, el documento entero. */
export function exportDocx(
  workPath: string,
  destPath: string,
  pageIndices?: number[] | null,
): Promise<DocxReport> {
  return invoke("export_docx", {
    workPath,
    destPath,
    pageIndices: pageIndices ?? null,
  });
}

/* ---- adjuntos y capas ---- */

/** Un fichero incrustado en el PDF (`/EmbeddedFiles`). */
export type Adjunto = {
  name: string;
  bytes: number;
  /** Fecha de creación tal como la trae el `/Filespec`, o vacía. */
  created: string;
  description: string;
};

export function listAttachments(path: string): Promise<Adjunto[]> {
  return invoke("list_attachments", { path });
}

/** Saca el adjunto a un fichero del disco, byte a byte. */
export function saveAttachment(
  path: string,
  index: number,
  destPath: string,
): Promise<void> {
  return invoke("save_attachment", { path, index, destPath });
}

export function addAttachment(
  workPath: string,
  filePath: string,
  description: string,
): Promise<void> {
  return invoke("add_attachment", { workPath, filePath, description });
}

/** Una capa del documento (`/OCProperties /OCGs`). */
export type Capa = { name: string; visible: boolean };

export function listLayers(path: string): Promise<Capa[]> {
  return invoke("list_layers", { path });
}

/** Apaga o enciende una capa. **Cambia el fichero**: PDFium respeta el
 *  `/OCProperties /D /OFF` del documento al renderizar, así que ocultar una
 *  capa es escribir en él. Por eso deja su paso de deshacer y la UI lo dice. */
export function setLayerVisible(
  workPath: string,
  index: number,
  visible: boolean,
): Promise<void> {
  return invoke("set_layer_visible", { workPath, index, visible });
}

export type CompressReport = {
  antes: number;
  despues: number;
  imagenes: number;
};

export function compressPdf(
  workPath: string,
  quality: number,
  maxDpi: number,
): Promise<CompressReport> {
  return invoke("compress_pdf", { workPath, quality, maxDpi });
}

/** Crea un campo de formulario (texto o casilla) en la página. */
export function createFormField(args: {
  workPath: string;
  pageIndex: number;
  kind: "text" | "checkbox";
  rect: { x: number; y: number; w: number; h: number };
  name: string;
}): Promise<void> {
  return invoke("create_form_field", { ...args });
}

/** Crea un enlace (a URL externa o a otra página). */
export function createLink(args: {
  workPath: string;
  pageIndex: number;
  rect: { x: number; y: number; w: number; h: number };
  uri?: string | null;
  destPage?: number | null;
}): Promise<void> {
  return invoke("create_link", { uri: null, destPage: null, ...args });
}

/** Contenido de un objeto de imagen como PNG base64 (para previsualizar). */
export function getImageData(
  path: string,
  pageIndex: number,
  objectIndex: number,
): Promise<string> {
  return invoke("get_image_data", { path, pageIndex, objectIndex });
}

export function stampSignature(args: {
  workPath: string;
  pageIndex: number;
  pngBase64: string;
  x: number;
  y: number;
  w: number;
  h: number;
}): Promise<void> {
  return invoke("stamp_signature", { ...args });
}

/* ---- firma digital ---- */

/** Una firma del documento, tal como la lee `verify_signatures`. Los
 *  nombres técnicos se quedan aquí: la UI no los enseña nunca. */
/** Lo que se sabe de una firma. Son TRES estados, no dos: «desconocido» es
 *  cuando Vitela no ha podido comprobarla (un algoritmo que todavía no sabe
 *  leer, un CMS que no entiende), y nunca debe pintarse como «modificado».
 *  Un falso «el documento ha cambiado» sobre un contrato firmado es un error
 *  caro. */
export type EstadoFirma = "ok" | "modificado" | "desconocido";

/** Los tres niveles de confianza del certificado, separados de la validez
 *  criptográfica de la firma. */
export type Confianza = "raiz_conocida" | "autofirmado" | "desconocida";

export type FirmaInfo = {
  /** Nombre del firmante escrito en la firma. */
  name: string;
  reason: string;
  /** Qué se ha podido comprobar; lo decide el backend, no la UI. */
  estado: EstadoFirma;
  /** «RSA-2048 / SHA-256», para la tarjeta del panel. */
  algoritmo: string;
  /** Momento de la firma, en ISO 8601. */
  signed_at: string;
  /** La firma cubre todo el fichero (no solo un trozo). */
  covers_whole_file: boolean;
  /** El contenido firmado sigue siendo el que hay. */
  digest_ok: boolean;
  cert_subject: string;
  cert_issuer: string;
  /** Quién responde por el certificado, evaluado contra el almacén de
   *  certificados del sistema. **No se consulta revocación** (ni CRL ni
   *  OCSP), así que `raiz_conocida` quiere decir «emitido por una autoridad
   *  reconocida», nunca «la firma es válida»: la confianza es del
   *  certificado y la validez es del documento. */
  confianza: Confianza;
  not_before: string;
  not_after: string;
  expired: boolean;
  self_signed: boolean;
  /** Página del recuadro de la firma, si es visible. */
  page_index: number | null;
  rect: { x: number; y: number; w: number; h: number } | null;
};

/** Comprueba las firmas de un PDF. Sin firmas, lista vacía. */
export function verifySignatures(path: string): Promise<FirmaInfo[]> {
  return invoke("verify_signatures", { path });
}

/** Lo que la UI puede añadir a la firma: dónde se dibuja y con qué cara. */
export type AparienciaFirma = {
  /** Recuadro en el espacio propio de la página; sin él, firma invisible. */
  rect?: { x: number; y: number; w: number; h: number } | null;
  pageIndex?: number | null;
  signerName?: string | null;
  /** PNG en base64 de la firma manuscrita guardada. */
  signaturePng?: string | null;
};

const SIN_APARIENCIA: Required<AparienciaFirma> = {
  rect: null,
  pageIndex: null,
  signerName: null,
  signaturePng: null,
};

/** Firma con certificado y clave en PEM. */
export function signPdf(
  args: {
    workPath: string;
    destPath: string;
    certPemPath: string;
    keyPemPath: string;
    reason?: string | null;
  } & AparienciaFirma,
): Promise<void> {
  return invoke("sign_pdf", { reason: null, ...SIN_APARIENCIA, ...args });
}

/** Firma con un contenedor .p12/.pfx protegido con contraseña. */
export function signPdfP12(
  args: {
    workPath: string;
    destPath: string;
    p12Path: string;
    password: string;
    reason?: string | null;
  } & AparienciaFirma,
): Promise<void> {
  return invoke("sign_pdf_p12", { reason: null, ...SIN_APARIENCIA, ...args });
}

/** Pasos de deshacer/rehacer disponibles y páginas del documento. */
export type HistoryState = { undo: number; redo: number; page_count: number };

export function historyState(workPath: string): Promise<HistoryState> {
  return invoke("history_state", { workPath });
}

export function undoDocument(workPath: string): Promise<HistoryState> {
  return invoke("undo", { workPath });
}

export function redoDocument(workPath: string): Promise<HistoryState> {
  return invoke("redo", { workPath });
}

export function squashHistory(
  workPath: string,
  steps: number,
): Promise<HistoryState> {
  return invoke("squash_history", { workPath, steps });
}

/** Lo que se puede saber de un PDF sin abrirlo como copia de trabajo. */
export type PdfInfo = {
  page_count: number;
  bytes: number;
  /** Va cifrado: hará falta la contraseña para hacer nada con él. */
  encrypted: boolean;
};

/** Páginas, tamaño y cifrado de un PDF cualquiera, sin tocarlo: lo que
 *  necesita la rejilla de combinar para no ordenar ficheros a ciegas. */
export function pdfInfo(path: string): Promise<PdfInfo> {
  return invoke("pdf_info", { path });
}

/* ---- recuperación tras un cierre inesperado ---- */

/** El apunte de que había un documento abierto sin guardar. La copia de
 *  trabajo ya vive en temp y ya sobrevive al cierre: lo único que faltaba
 *  era el apunte de que existía y no se guardó. */
export type Sesion = {
  /** El fichero de verdad, o null si el documento no tenía ruta. */
  original_path: string | null;
  work_path: string;
  modificado: boolean;
  /** Cuándo se tomó el apunte, en ISO 8601. */
  cuando: string;
};

/** Apunta la sesión viva. La UI lo llama con un respiro de 10 s tras cada
 *  cambio: es un apunte, no un guardado, y no debe ir en cada tecla.
 *  `modified` no es opcional en el backend: sin él la llamada entera se
 *  rechazaba y no se apuntaba nunca nada. */
export function autosaveState(
  workPath: string,
  originalPath: string | null,
  modified: boolean,
): Promise<void> {
  return invoke("autosave_state", { workPath, originalPath, modified });
}

/** Borra el apunte: se cierra limpiamente, o el usuario descarta. */
export function borraSesion(): Promise<void> {
  return invoke("borra_sesion");
}

/** La sesión que quedó a medias, si la hubo. */
export function recoverSession(): Promise<Sesion | null> {
  return invoke("recover_session");
}

/* ---- menú nativo ---- */

/** Avisa al backend de si hay documento abierto para que atenúe las
 *  entradas del menú nativo que no aplican (en Acrobat se atenúan, no
 *  desaparecen). La UI lo llama al abrir y al cerrar documento: el menú se
 *  monta una sola vez en el arranque y sin esto se queda atenuado siempre. */
export function setMenuState(hasDocument: boolean): Promise<void> {
  return invoke("set_menu_state", { hasDocument });
}
