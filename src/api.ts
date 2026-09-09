import { invoke } from "./ipc";
import type { AnnotationInfo, SearchMatch } from "./tipos";

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
 *  opciones de la búsqueda de Acrobat; por defecto van apagadas. */
export function searchPdf(
  path: string,
  query: string,
  matchCase: boolean,
  wholeWord: boolean,
): Promise<SearchMatch[]> {
  return invoke("search_pdf", { path, query, matchCase, wholeWord });
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
}): Promise<void> {
  return invoke("add_watermark", { ...args });
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
): Promise<void> {
  return invoke("add_header_footer", {
    workPath,
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
  rect: { x: number; y: number; w: number; h: number };
};

/** Marca una zona; devuelve el índice de la marca en su página. */
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

export function unmarkRedaction(
  workPath: string,
  pageIndex: number,
  markIndex: number,
): Promise<void> {
  return invoke("unmark_redaction", { workPath, pageIndex, markIndex });
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
export type FirmaInfo = {
  /** Nombre del firmante escrito en la firma. */
  name: string;
  reason: string;
  /** Momento de la firma, en ISO 8601. */
  signed_at: string;
  /** La firma cubre todo el fichero (no solo un trozo). */
  covers_whole_file: boolean;
  /** El contenido firmado sigue siendo el que hay. */
  digest_ok: boolean;
  cert_subject: string;
  cert_issuer: string;
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

/* ---- menú nativo ---- */

/** Avisa al backend de si hay documento abierto para que atenúe las
 *  entradas del menú nativo que no aplican (en Acrobat se atenúan, no
 *  desaparecen). La UI lo llama al abrir y al cerrar documento: el menú se
 *  monta una sola vez en el arranque y sin esto se queda atenuado siempre. */
export function setMenuState(hasDocument: boolean): Promise<void> {
  return invoke("set_menu_state", { hasDocument });
}
