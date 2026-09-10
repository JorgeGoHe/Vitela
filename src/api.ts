import { invoke } from "./ipc";
import type { AnnotationInfo, SearchMatch, TextBlock } from "./tipos";

/** Para qué sirve una imagen guardada: la firma entera, las iniciales que
 *  se ponen en cada página o un sello propio. **Vive en el backend**, que
 *  es donde vive la biblioteca: en `localStorage` se perdía al cambiar de
 *  máquina y no la conocía nadie más. */
export type RanuraImagen = "firma" | "iniciales" | "sello";

/** Imagen guardada en la biblioteca del usuario (firma, iniciales o sello). */
export type FirmaGuardada = {
  id: string;
  name: string;
  png_base64: string;
  ranura: RanuraImagen;
};

export function listStoredSignatures(): Promise<FirmaGuardada[]> {
  return invoke("list_stored_signatures");
}

export function saveStoredSignature(
  name: string,
  pngBase64: string,
  ranura: RanuraImagen,
): Promise<FirmaGuardada> {
  return invoke("save_stored_signature", { name, pngBase64, ranura });
}

export function importSignatureFile(
  imagePath: string,
  ranura: RanuraImagen,
): Promise<FirmaGuardada> {
  return invoke("import_signature_file", { imagePath, ranura });
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

/** Lo que sale de buscar en una carpeta: un grupo por fichero, con sus
 *  coincidencias. Un PDF que no se ha podido abrir —cifrado, roto— **no
 *  rompe la búsqueda**: viene con `error` y sin coincidencias, y la interfaz
 *  los cuenta aparte para decirlo al final. */
export type GrupoCarpeta = {
  path: string;
  /** Nombre del fichero, ya sin la carpeta: es lo que se enseña. */
  nombre: string;
  coincidencias: SearchMatch[];
  /** Por qué no se ha podido abrir; vacío o ausente si se abrió bien. */
  error?: string;
};

/** Busca en todos los PDF de una carpeta. `recursivo` baja a las
 *  subcarpetas y va apagado por defecto, como en Acrobat. Va emitiendo el
 *  progreso por el evento `buscando-carpeta` y se puede parar con
 *  `cancel_search`, que devuelve lo encontrado hasta ese momento. */
export function searchFolder(
  dir: string,
  query: string,
  matchCase: boolean,
  wholeWord: boolean,
  context: boolean,
  recursivo: boolean,
): Promise<GrupoCarpeta[]> {
  return invoke("search_folder", {
    dir,
    query,
    matchCase,
    wholeWord,
    context,
    recursivo,
  });
}

/** Para la búsqueda en carpeta que esté en marcha. Lo encontrado hasta ahí
 *  se queda en pantalla: cancelar no es tirar el trabajo. */
export function cancelSearch(): Promise<void> {
  return invoke("cancel_search");
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
 *  `lineHeight` es la distancia a la que se coloca cada línea: en un PDF no
 *  hay párrafos, las líneas son objetos, así que el interlineado no es el
 *  operador `TL` sino dónde se pone el objeto siguiente. Sin él, el de
 *  siempre.
 *
 *  `charSpacing` sí es un operador, el `Tc`, y lo escribe un segundo pase
 *  con lopdf dentro de la misma mutación: separa las letras entre sí, en
 *  puntos, y 0 es lo normal. */
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

/** Lo que el backend sabe de la edición que acaba de hacer: en cuántas
 *  líneas ha quedado el párrafo, si se sale del papel y si ha refluido de
 *  verdad. `se_sale` lo decide **después** del reflujo real y con la columna
 *  que ha usado, así que la UI no vuelve a medirlo por su cuenta. */
export type InformeEdicion = {
  lineas: number;
  se_sale: boolean;
  reflujo: boolean;
};

/** Reescribe un bloque del content stream. Sin `color` ni `align` se queda
 *  con los que tuviera: editar no debe recolorear sin querer.
 *
 *  Con `reflow` el párrafo entero se reparte de nuevo al ancho que tenía y
 *  las líneas de abajo suben o bajan, en vez de dejar la primera larga y
 *  meter las sobras como objetos nuevos debajo. La UI lo pide cuando el
 *  bloque ocupa más de una línea, que es cuando hay algo que recolocar. */
export function editTextBlock(args: {
  workPath: string;
  pageIndex: number;
  objectIndex: number;
  newText: string;
  color?: Rgba | null;
  align?: Alineacion | null;
  lineHeight?: number | null;
  charSpacing?: number | null;
  reflow?: boolean | null;
}): Promise<InformeEdicion> {
  return invoke("edit_text_block", {
    color: null,
    align: null,
    lineHeight: null,
    charSpacing: null,
    reflow: null,
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
    objectIndex: imageIndex,
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

/** Sello de texto centrado en el punto dado.
 *
 *  `dinamico` es la segunda línea de un **sello dinámico** —quién y cuándo,
 *  «Jorge Gómez · 10/09/2026 19:40»— con la plantilla ya resuelta en el
 *  momento de estampar; el backend la compone dentro del `/AP` debajo de la
 *  palabra del sello. Sin él, el sello de siempre. */
export function addStamp(args: {
  workPath: string;
  pageIndex: number;
  text: string;
  color: Rgba;
  x: number;
  y: number;
  fontSize: number;
  author?: string | null;
  dinamico?: string | null;
}): Promise<void> {
  return invoke("add_stamp", { author: null, dinamico: null, ...args });
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
  author?: string,
): Promise<void> {
  return invoke("set_annotation_state", {
    workPath,
    pageIndex,
    annotIndex,
    state,
    author,
  });
}

/** Saca la lista de comentarios a un fichero para leerla fuera: el
 *  «Resumen de comentarios» de Acrobat. */
export function exportComments(
  workPath: string,
  destPath: string,
  documentName?: string,
): Promise<void> {
  return invoke("export_comments", { workPath, destPath, documentName });
}

/** Por qué se ordena el resumen en PDF; «por página» es el de Acrobat. */
export type OrdenComentarios = "pagina" | "autor" | "fecha" | "tipo";

/** El «Crear resumen de comentarios» de Acrobat: un PDF nuevo con una fila
 *  por comentario —número, tipo, autor, fecha, estado y texto, con las
 *  respuestas sangradas—, imprimible y ordenable. */
export function exportCommentsPdf(
  workPath: string,
  destPath: string,
  orden: OrdenComentarios,
  documentName?: string,
): Promise<void> {
  return invoke("export_comments_pdf", {
    workPath,
    destPath,
    orden,
    documentName,
  });
}

/** XFDF: el formato con el que un revisor devuelve su revisión sobre **su**
 *  copia del documento. Es XML plano, y las anotaciones que Vitela escribe
 *  ya llevan autor, fechas, hilo y estado, que es todo lo que necesita. */
export function exportCommentsXfdf(
  workPath: string,
  destPath: string,
): Promise<void> {
  return invoke("export_comments_xfdf", { workPath, destPath });
}

/** Importar **añade**, no sustituye, y en una sola mutación: un ⌘Z quita
 *  todos los comentarios que hayan entrado. Devuelve cuántos son. */
export function importCommentsXfdf(
  workPath: string,
  srcPath: string,
): Promise<number> {
  return invoke("import_comments_xfdf", { workPath, srcPath });
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

/** Llamada (callout) de Acrobat: un `/FreeText` con la línea `/CL` y su
 *  punta de flecha señalando un punto de la página. El `rect` es la caja del
 *  texto y `punta` el punto al que apunta, los dos en el espacio propio de
 *  la página. */
export function addCallout(args: {
  workPath: string;
  pageIndex: number;
  rect: { x: number; y: number; w: number; h: number };
  punta: [number, number];
  text: string;
  color: Rgba;
  /** Punto intermedio: con él la `/CL` es de tres puntos y la línea sale
   *  acodada, como la dibuja Acrobat. Sin él, la recta de siempre. */
  codo?: [number, number] | null;
  author?: string | null;
}): Promise<void> {
  return invoke("add_callout", { codo: null, author: null, ...args });
}

/** Lo que se ha llevado un pase de goma: cuántos trazos ha tocado y de
 *  cuántos no ha quedado nada (esos desaparecen del `/Annots`). */
export type BorradoGoma = { tocados: number; borrados: number };

/** Goma de borrar del dibujo: quita de **cada** trazo (`Ink`) que toque el
 *  rectángulo los segmentos que caen dentro y vuelve a dibujar su
 *  apariencia, en vez de llevarse el trazo entero. Busca los trazos el
 *  backend y hace el lote en una sola mutación, así que un pase de goma es
 *  un paso de deshacer. Coordenadas en el espacio propio de la página. */
export function eraseInkArea(
  workPath: string,
  pageIndex: number,
  rect: { x: number; y: number; w: number; h: number },
): Promise<BorradoGoma> {
  return invoke("erase_ink_area", { workPath, pageIndex, rect });
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

/** Mete otro PDF en la posición `index`. `pageIndices` es el rango del
 *  **documento que entra** (null = entero): Acrobat lo pregunta desde
 *  siempre y sin él había que insertar el fichero completo y borrar
 *  después lo que sobraba. */
export function insertPdfAt(
  workPath: string,
  otherPath: string,
  index: number,
  pageIndices: number[] | null,
): Promise<number> {
  return invoke("insert_pdf_at", { workPath, otherPath, index, pageIndices });
}

/** Tamaño de página de «Crear PDF desde imágenes»: A4 y Carta ajustan la
 *  foto con 36 pt de margen y la centran; «imagen» hace la página del
 *  tamaño de la foto a 72 dpi, sin margen. */
export type TamanoImagenes = "a4" | "carta" | "imagen";

/** Crea un PDF nuevo con una imagen por página. Escribe un fichero aparte:
 *  no toca el documento abierto ni deja paso de deshacer. Devuelve cuántas
 *  páginas ha escrito. */
/** Lo que ha salido de un lote de imágenes: cuántas páginas se han escrito
 *  y las rutas de las que no se han dejado leer, para poder decir «19 de
 *  20» con el nombre de la que falta en vez de tirar el lote entero. */
export type InformeImagenes = { paginas: number; saltadas: string[] };

export function pdfFromImages(
  imagePaths: string[],
  destPath: string,
  tamano: TamanoImagenes,
): Promise<InformeImagenes> {
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
  /** Debajo del contenido de la página (el objeto entra en la posición 0),
   *  que es lo que Acrobat llama «Fondo». Sin él, encima, como siempre. */
  detras?: boolean;
}): Promise<void> {
  return invoke("add_watermark", {
    pageIndices: null,
    imagePng: null,
    detras: false,
    ...args,
  });
}

/** El **fondo** de Acrobat: un color sólido a sangre o una imagen debajo
 *  del contenido de la página, con su opacidad. Va aparte de la marca de
 *  agua porque no se coloca ni se gira: cubre la página entera, que es el
 *  caso por defecto del diálogo de Acrobat. Se manda `color` **o**
 *  `imagePng`, nunca los dos. Lo que pone Vitela queda marcado, y por eso
 *  `remove_background` sabe quitarlo. */
export function addBackground(args: {
  workPath: string;
  /** Color sólido del fondo, opaco (la opacidad va en su parámetro). */
  color?: Rgba | null;
  /** PNG en base64 cuando el fondo es una imagen. */
  imagePng?: string | null;
  opacity: number;
  /** Índices de página, o null para todas. */
  pageIndices?: number[] | null;
}): Promise<void> {
  return invoke("add_background", {
    color: null,
    imagePng: null,
    pageIndices: null,
    ...args,
  });
}

/** Cuenta (o quita) lo que se puso como fondo o como marca de agua: los
 *  objetos marcados —el rectángulo de color y la imagen— además del texto.
 *  `remove_marginal_text` solo sabía del texto, y por eso un fondo de imagen
 *  se quedaba dentro para siempre. */
export type FondoQuitado = { objetos: number; textos: number };

export function removeBackground(
  workPath: string,
  dryRun: boolean,
): Promise<FondoQuitado> {
  return invoke("remove_background", { workPath, dryRun });
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

/** Numeración Bates: el sello legal que lleva cada página de un expediente
 *  —prefijo, número corrido con ceros delante y sufijo— en una esquina.
 *  Devuelve cuántas páginas ha numerado. */
export function addBates(args: {
  workPath: string;
  prefijo: string;
  sufijo: string;
  /** Cuántos dígitos ocupa el número, rellenando con ceros. Acrobat: 6. */
  digitos: number;
  /** Número de la primera página numerada. Acrobat: 1. */
  empiezaEn: number;
  /** Celda de un grid 3×3 ("nw".."se"); Acrobat: abajo a la derecha. */
  position?: string;
  fontSize?: number;
  /** Índices de página; sin ellos (null), todas. */
  pageIndices?: number[] | null;
}): Promise<number> {
  return invoke("add_bates", {
    position: "se",
    fontSize: 9,
    pageIndices: null,
    ...args,
  });
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

/** Un marcador del árbol. `top` y `zoom` son el destino fino (`/XYZ` del
 *  spec): a qué altura de la página y con qué aumento se estaba mirando
 *  cuando se creó, para que volver a él devuelva la vista exacta y no el
 *  principio de la página. `top` va en puntos desde el borde de arriba, en
 *  el espacio de la vista, como el resto de la UI; el backend lo pasa al de
 *  la página. Un marcador ajeno con `/Fit` llega sin los dos. */
export type OutlineNode = {
  title: string;
  page_index: number | null;
  top: number | null;
  zoom: number | null;
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

/** Lo que ha entrado al importar datos de formulario: cuántos campos se
 *  han rellenado y cuántos del fichero no existen en este documento —que es
 *  la pregunta real de quien recibe las respuestas de un formulario—. */
export type InformeXfdf = { rellenados: number; sin_campo: number };

/** Escribe los valores de los campos en un XFDF, el formato que entienden
 *  Acrobat y los gestores de formularios. */
export function exportFormDataXfdf(
  workPath: string,
  destPath: string,
): Promise<number> {
  return invoke("export_form_data_xfdf", { workPath, destPath });
}

/** Rellena los campos con los valores de un XFDF. **No crea campos**: los
 *  que no existan se cuentan y se dicen. Una sola mutación. */
export function importFormDataXfdf(
  workPath: string,
  srcPath: string,
): Promise<InformeXfdf> {
  return invoke("import_form_data_xfdf", { workPath, srcPath });
}

/** Adjunta un fichero **a un punto de la página**, como comentario
 *  (`/FileAttachment` con el fichero embebido y una chincheta por
 *  apariencia). Es distinto del adjunto del documento (`add_attachment`),
 *  que va en `/EmbeddedFiles` y no tiene sitio en ninguna página. El punto
 *  va en el espacio propio de la página. */
export function addFileAttachmentAnnotation(args: {
  workPath: string;
  pageIndex: number;
  punto: [number, number];
  srcPath: string;
  author?: string | null;
}): Promise<number> {
  return invoke("add_file_attachment_annotation", { author: null, ...args });
}

/** Una fuente del documento, como la enseña Acrobat en su pestaña
 *  «Fuentes»: cómo se llama, de qué tipo es y si va dentro del fichero. */
export type FuenteInfo = {
  nombre: string;
  /** «TrueType», «Type1», «Type0 (CID)»… tal como lo dice el PDF. */
  tipo: string;
  incrustada: boolean;
};

/** Lo que Acrobat enseña en las cuatro pestañas de «Propiedades» y no se
 *  puede escribir: es la ficha del fichero, no sus metadatos. */
export type DocumentInfo = {
  bytes: number;
  page_count: number;
  /** Versión del PDF, «1.7». */
  version: string;
  /** Tamaño de la primera página, en puntos. */
  page_width: number;
  page_height: number;
  /** Si tiene AcroForm con campos. */
  formulario: boolean;
  cifrado: boolean;
  /** Qué deja hacer el `/P` del documento. */
  permisos: Permisos;
  fuentes: FuenteInfo[];
  /** Cuándo se creó y cuándo se tocó por última vez (el `/CreationDate` y
   *  el `/ModDate` del `/Info`, o el XMP), en ISO 8601. Opcionales: un
   *  backend que no los traiga deja esas dos líneas fuera del diálogo en
   *  vez de enseñar un hueco. */
  creado?: string;
  modificado?: string;
};

/** La ficha completa del documento abierto (solo lectura). Contesta a «¿por
 *  qué este PDF pesa 40 MB?», que es la pregunta que la gente le hace a
 *  Acrobat. */
export function getDocumentInfo(path: string): Promise<DocumentInfo> {
  return invoke("get_document_info", { path });
}

export function setMetadata(workPath: string, meta: Metadata): Promise<void> {
  return invoke("set_metadata", { workPath, meta });
}

/** Los estilos de numeración del spec (`/S` de `/PageLabels`), en el
 *  vocabulario de la interfaz. `ninguno` es la página que solo lleva
 *  prefijo (la portada, «Anexo»). */
export type EstiloEtiqueta =
  | "arabigo"
  | "romano"
  | "romano_min"
  | "letra"
  | "letra_min"
  | "ninguno";

/** Un tramo de la numeración: desde qué página física empieza, con qué
 *  estilo, con qué prefijo y en qué número. Las claves van en snake_case:
 *  es una estructura anidada dentro de la lista. */
export type RangoEtiquetas = {
  /** Página física en la que empieza el tramo (desde 0). */
  desde: number;
  estilo: EstiloEtiqueta;
  prefijo: string;
  empieza_en: number;
};

/** Las etiquetas de página del documento (`/PageLabels`). Un PDF que no las
 *  trae devuelve la lista vacía, y entonces la página se llama por su
 *  número físico. */
/** Rangos de numeración del documento y la etiqueta ya compuesta de cada
 *  página (vacía si el PDF no lleva `/PageLabels`). */
export type EtiquetasPaginas = { rangos: RangoEtiquetas[]; etiquetas: string[] };

export function getPageLabels(path: string): Promise<EtiquetasPaginas> {
  return invoke("get_page_labels", { path });
}

/** Reescribe la numeración entera. Con la lista vacía se quita el
 *  `/PageLabels` y el documento vuelve a numerarse 1..N. */
export function setPageLabels(
  workPath: string,
  rangos: RangoEtiquetas[],
): Promise<void> {
  return invoke("set_page_labels", { workPath, rangos });
}

/* ---- composición de impresión: folleto, N-up y póster ---- */

/** Las tres composiciones de Acrobat, más «ninguna», que es imprimir una
 *  página por hoja como hasta ahora. */
export type ModoComposicion = "ninguna" | "nup" | "folleto" | "poster";

/** Lo que necesita cada composición. Va como **una sola estructura
 *  anidada** con todos los campos, así que sus claves van en snake_case:
 *  Tauri solo traduce el camelCase de los argumentos de primer nivel. */
export type OpcionesComposicion = {
  /** Páginas que entran, en orden (las del rango del diálogo). */
  page_indices: number[];
  /** N-up: cuántas por hoja (2, 4, 6, 9 o 16) y en qué orden se recorren. */
  por_hoja: number;
  orden: "horizontal" | "vertical";
  /** El borde de cada página, que es la casilla de Acrobat. */
  borde: boolean;
  /** Folleto: por dónde se grapa y qué caras salen. */
  encuadernacion: "izquierda" | "derecha";
  caras: "ambas" | "anverso" | "reverso";
  /** Póster: a cuánto se amplía, cuánto se solapan las hojas y si se
   *  imprimen las marcas de corte. */
  escala: number;
  solape_mm: number;
  marcas: boolean;
};

/** Compone las páginas nuevas —folleto, varias por hoja o póster— en un PDF
 *  temporal y devuelve su ruta. **No toca el documento**: lo que se compone
 *  es lo que se va a imprimir, y se rasteriza por el camino de siempre. */
export function composePrint(
  workPath: string,
  modo: ModoComposicion,
  opciones: OpcionesComposicion,
): Promise<string> {
  return invoke("compose_print", { workPath, modo, opciones });
}

/* ---- vista inicial (`/OpenAction` y `/PageLayout`) ---- */

/** Con qué cara se abre el documento: la única parte de las propiedades que
 *  además se escribe. `"defecto"` en el zoom y en la disposición es «lo que
 *  diga el visor de quien lo abra», que es lo que trae un PDF normal y lo
 *  que Acrobat llama «Predeterminado».
 *
 *  **Las claves van en snake_case**: es una estructura anidada y Tauri solo
 *  traduce el camelCase de los argumentos de primer nivel, como el `props`
 *  de `create_form_field` y los rangos de `set_page_labels`. */
export type VistaInicial = {
  /** Página de arranque, desde 0. */
  page_index: number;
  /** "defecto", "pagina" (la página entera), "ancho" o "100". */
  zoom: string;
  /** "defecto", "una", "continuo", "dos" o "dos-continuo". */
  disposicion: string;
  /** El panel de marcadores abierto al abrir el documento. */
  marcadores: boolean;
};

export function getOpenAction(path: string): Promise<VistaInicial> {
  return invoke("get_open_action", { path });
}

export function setOpenAction(
  workPath: string,
  vista: VistaInicial,
): Promise<void> {
  return invoke("set_open_action", { workPath, vista });
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
/** Un destinatario del cifrado por certificado: su certificado público y
 *  lo que se le deja hacer. Van dentro de una lista de estructuras, así que
 *  las claves se escriben ya en snake_case. */
export type DestinatarioCifrado = {
  cert_path: string;
  permisos: Permisos;
};

/** Cifra el PDF **para unos destinatarios** en vez de con una contraseña:
 *  un `/Filter /Adobe.PubSec` con un recipiente por certificado que envuelve
 *  la clave del documento. Es lo que usan las administraciones. Escribe
 *  siempre una copia: el documento abierto se queda como está, porque quien
 *  lo cifra no tiene por qué poder volver a abrirlo. */
export function encryptPdfCert(
  workPath: string,
  destPath: string,
  destinatarios: DestinatarioCifrado[],
): Promise<void> {
  return invoke("encrypt_pdf_cert", { workPath, destPath, destinatarios });
}

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
/** Un `.html` por documento: una `<div class="pagina">` por página con cada
 *  bloque de texto en su sitio, las imágenes en una carpeta al lado y los
 *  enlaces como `<a>`. Sin JavaScript y sin dependencias, que es lo que
 *  hace que se pueda abrir dentro de diez años. `rango` son los índices de
 *  página, o null para el documento entero. */
export function exportHtml(
  workPath: string,
  destPath: string,
  rango: number[] | null,
): Promise<void> {
  return invoke("export_html", { workPath, destPath, rango });
}

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

/** Saca el adjunto a un temporal y lo abre con el visor del sistema. El
 *  paso por el backend no es un rodeo: el permiso del opener está acotado a
 *  http/https/mailto, así que abrir un fichero del disco no se puede hacer
 *  desde el webview. */
export function openAttachment(path: string, index: number): Promise<string> {
  return invoke("open_attachment", { path, index });
}

/** Saca a un temporal el fichero que lleva dentro la **chincheta de una
 *  página** (`/FileAttachment`) y devuelve su ruta, para abrirlo con el
 *  visor del sistema. Es el hermano de `open_attachment`, que es el del
 *  documento: este vive en una página y es un comentario. */
export function openPageAttachment(
  path: string,
  pageIndex: number,
  annotIndex: number,
): Promise<string> {
  return invoke("open_page_attachment", { path, pageIndex, annotIndex });
}

/** Escribe en el disco el fichero de una chincheta, con su nombre y su
 *  extensión: hasta ahora se podía adjuntar y no se podía sacar. */
export function savePageAttachment(
  path: string,
  pageIndex: number,
  annotIndex: number,
  destPath: string,
): Promise<void> {
  return invoke("save_page_attachment", {
    path,
    pageIndex,
    annotIndex,
    destPath,
  });
}

export function addAttachment(
  workPath: string,
  filePath: string,
  description: string,
): Promise<void> {
  return invoke("add_attachment", { workPath, filePath, description });
}

/** Quita un adjunto del documento. Pasa por `cirugia`, así que deja su paso
 *  de deshacer: es trabajo del usuario y ⌘Z lo devuelve. */
export function deleteAttachment(
  workPath: string,
  index: number,
): Promise<void> {
  return invoke("delete_attachment", { workPath, index });
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

/** Una categoría de la auditoría de espacio: cuánto pesa y qué parte del
 *  fichero es. Son las de Acrobat —imágenes, fuentes incrustadas, contenido
 *  de página, anotaciones, adjuntos, marcadores y enlaces, metadatos,
 *  estructura y «lo demás»— y la suma cuadra con el tamaño del fichero. */
export type CategoriaPeso = {
  categoria: string;
  bytes: number;
  porcentaje: number;
};

/** De qué está hecho el PDF. Es lo que contesta «¿por qué pesa 40 MB?», que
 *  es la pregunta que la gente le hace a Acrobat y la que hace que reducir
 *  el tamaño se entienda en vez de ser una ruleta. */
export function auditPdf(path: string): Promise<CategoriaPeso[]> {
  return invoke("audit_pdf", { path });
}

/** Reduce el tamaño: recomprime las imágenes y, si se pide, descarta lo que
 *  no hace falta. Nunca dice que ha reducido si el fichero ha crecido. */
export function compressPdf(
  workPath: string,
  quality: number,
  maxDpi: number,
  quitarAdjuntos: boolean,
  quitarMetadatos: boolean,
  aplanarFormularios: boolean,
): Promise<CompressReport> {
  return invoke("compress_pdf", {
    workPath,
    quality,
    maxDpi,
    quitarAdjuntos,
    quitarMetadatos,
    aplanarFormularios,
  });
}

/** Los cinco tipos que crea «Preparar formulario» de Acrobat. */
export type TipoCampo = "text" | "checkbox" | "radio" | "combo" | "list";

/** Las propiedades del primer panel de Acrobat. Van dentro de una
 *  estructura anidada, y Tauri solo pasa a snake_case los argumentos de
 *  primer nivel del comando: por eso estas claves se escriben ya como las
 *  espera el backend. */
export type PropsCampo = {
  /** Texto de ayuda al pasar el ratón (`/TU`). */
  tooltip: string | null;
  /** Bit 2 de `/Ff`. */
  obligatorio: boolean;
  /** Bit 1 de `/Ff`. */
  solo_lectura: boolean;
  /** `/DV`. */
  valor_defecto: string | null;
  /** Posición en el orden de tabulación de la página. */
  orden_tab: number | null;
};

/** Un campo que la detección **propone**: nada de esto está escrito en el
 *  PDF hasta que el usuario dice que sí. `rect` va en el espacio propio de
 *  la página, como el de `create_form_field`, y `confianza` (0 a 1) es lo
 *  seguro que está el backend de esa propuesta: una heurística no acierta
 *  siempre y no puede fingir que sí. */
export type CampoPropuesto = {
  page_index: number;
  rect: { x: number; y: number; w: number; h: number };
  kind: TipoCampo;
  name: string;
  confianza: number;
};

/** «Reconocer campos…»: propone los campos de un formulario impreso a
 *  partir de las líneas de subrayado y las cajas de la página. **No
 *  escribe nada**: devuelve la lista para que la UI la enseñe y el usuario
 *  la corrija antes de crear nada. */
export function detectFormFields(
  workPath: string,
  pageIndices: number[] | null,
): Promise<CampoPropuesto[]> {
  return invoke("detect_form_fields", { workPath, pageIndices });
}

/** Crea un campo de formulario en la página.
 *
 *  En un botón de radio, los widgets del mismo `group` comparten **una sola
 *  entrada en `/Fields`** con un `/Kids` por opción, que es lo que hace que
 *  desmarcar los hermanos sea automático y no una casualidad; `exportValue`
 *  es lo que queda escrito en `/V` cuando se marca esa opción. En un
 *  desplegable o una lista, `options` son las opciones. Los tres campos van
 *  vacíos en los tipos que no los usan. */
export function createFormField(args: {
  workPath: string;
  pageIndex: number;
  kind: TipoCampo;
  rect: { x: number; y: number; w: number; h: number };
  name: string;
  group: string;
  exportValue: string;
  options: string[];
  props: PropsCampo;
}): Promise<void> {
  return invoke("create_form_field", { ...args });
}

/** Un campo del lote de `createFormFields`. Las claves van en snake_case:
 *  es una estructura anidada y Tauri solo traduce los argumentos de primer
 *  nivel del comando. */
export type CampoNuevo = {
  page_index: number;
  kind: TipoCampo;
  rect: { x: number; y: number; w: number; h: number };
  name: string;
  group?: string | null;
  export_value?: string | null;
  options?: string[] | null;
  props?: PropsCampo | null;
};

/** Crea un lote de campos en **una sola cirugía**: si uno falla no se
 *  escribe ninguno y un ⌘Z devuelve el formulario entero. Es la vía de
 *  «Reconocer campos…»: aceptar ocho propuestas es un gesto, no ocho, y un
 *  formulario a medias es peor que uno que no se creó. Devuelve cuántos. */
export function createFormFields(
  workPath: string,
  fields: CampoNuevo[],
): Promise<number> {
  return invoke("create_form_fields", { workPath, fields });
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

/** La medida que se deja puesta, como comentario y no como dibujo:
 *  `/Line` con dos puntos, `/PolyLine` con más (perímetro) y `/Polygon`
 *  con `closed` (área), con la cifra en el `/Contents`. Los puntos van en
 *  el espacio propio de la página. Una sola mutación: un ⌘Z la quita, sale
 *  en el panel de comentarios y se borra desde ahí. */
export function addMeasure(args: {
  workPath: string;
  pageIndex: number;
  points: [number, number][];
  text: string;
  color: Rgba;
  closed?: boolean;
  author?: string | null;
}): Promise<number> {
  return invoke("add_measure", { closed: false, author: null, ...args });
}

/** Escribe el bitmap de un objeto de imagen en un PNG del disco. Es el
 *  mismo mapa de bits de `getImageData`, pero sin cruzar el canal en
 *  base64: lo escribe el backend, que es el único que puede. */
export function saveImageData(
  workPath: string,
  pageIndex: number,
  objectIndex: number,
  destPath: string,
): Promise<void> {
  return invoke("save_image_data", {
    workPath,
    pageIndex,
    objectIndex,
    destPath,
  });
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
  /** **Del documento, no de esta firma**: no hay ningún cambio detrás de la
   *  última firma. Una revisión que solo añade otra firma no es un cambio
   *  que haya que denunciar, y sin este dato la banda acusaba de
   *  manipulación a un documento firmado por dos personas. */
  documento_intacto: boolean;
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
  /** El sello de tiempo (RFC 3161) que acompaña a la firma, si lo lleva.
   *  Sin él, **la fecha de la firma es la del reloj de quien firmó**, y eso
   *  cambia lo que la tarjeta puede prometer. Opcional: un backend anterior
   *  no lo trae. */
  sello_de_tiempo?: { fecha: string; autoridad: string } | null;
  /** Nivel del `/DocMDP` cuando esta firma **certifica** el documento (1, 2
   *  o 3); sin él, es una firma normal. Certificar dice «esta es la versión
   *  buena» y qué se puede cambiar después sin romper el sello, así que la
   *  banda y la tarjeta lo cuentan aparte: sin eso la función es invisible
   *  justo después de usarla. Opcional porque un backend anterior no lo
   *  trae, como `required` en los campos de formulario. */
  certifica?: NivelCertificacion | null;
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

/** Lo «Avanzado» de firmar, que es lo que convierte una firma que funciona
 *  en una firma que vale en un expediente. Las dos necesitan red. */
export type FirmaAvanzada = {
  /** Servidor RFC 3161 que sella la hora; sin él, la fecha es la del reloj
   *  del que firma. */
  tsaUrl?: string | null;
  /** Empotrar en `/DSS` la cadena y las respuestas OCSP/CRL del momento,
   *  que es lo que deja comprobar la firma dentro de diez años. */
  ltv?: boolean | null;
};

const SIN_AVANZADO: Required<FirmaAvanzada> = { tsaUrl: null, ltv: false };

/** Los servidores de tiempo públicos que se ofrecen en el desplegable. Se
 *  puede escribir otro: la lista es un atajo, no una jaula. */
export const TSA_CONOCIDAS: { url: string; nombre: string }[] = [
  { url: "https://freetsa.org/tsr", nombre: "FreeTSA" },
  { url: "http://timestamp.digicert.com", nombre: "DigiCert" },
  { url: "http://timestamp.sectigo.com", nombre: "Sectigo" },
  { url: "http://tsa.izenpe.com", nombre: "Izenpe (España)" },
];

/** Firma con certificado y clave en PEM. */
export function signPdf(
  args: {
    workPath: string;
    destPath: string;
    certPemPath: string;
    keyPemPath: string;
    reason?: string | null;
  } & AparienciaFirma &
    FirmaAvanzada,
): Promise<void> {
  return invoke("sign_pdf", {
    reason: null,
    ...SIN_APARIENCIA,
    ...SIN_AVANZADO,
    ...args,
  });
}

/** Los tres niveles del `/DocMDP`, con el número que escribe el PDF. La
 *  interfaz **nunca** enseña el número: enseña qué se puede hacer después. */
export type NivelCertificacion = 1 | 2 | 3;

/** Certificar: la firma más el `/DocMDP` que dice qué se puede cambiar
 *  después sin romperla. Acepta los dos caminos del certificado (el .p12 con
 *  su contraseña o el .pem con su clave aparte) en un solo comando, y la
 *  misma apariencia visible que firmar. **Solo la primera firma puede
 *  certificar**: el sello avala el documento entero. */
export function certifyPdf(
  args: {
    workPath: string;
    destPath: string;
    nivel: NivelCertificacion;
    certPemPath?: string | null;
    keyPemPath?: string | null;
    p12Path?: string | null;
    password?: string | null;
    reason?: string | null;
  } & AparienciaFirma &
    FirmaAvanzada,
): Promise<void> {
  return invoke("certify_pdf", {
    certPemPath: null,
    keyPemPath: null,
    p12Path: null,
    password: null,
    reason: null,
    ...SIN_APARIENCIA,
    ...SIN_AVANZADO,
    ...args,
  });
}

/** Firma con un contenedor .p12/.pfx protegido con contraseña. */
export function signPdfP12(
  args: {
    workPath: string;
    destPath: string;
    p12Path: string;
    password: string;
    reason?: string | null;
  } & AparienciaFirma &
    FirmaAvanzada,
): Promise<void> {
  return invoke("sign_pdf_p12", {
    reason: null,
    ...SIN_APARIENCIA,
    ...SIN_AVANZADO,
    ...args,
  });
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

/** Borra el apunte de **ese** documento: se cierra limpiamente, o el
 *  usuario descarta. `workPath` es obligatorio también aquí —lo es en el
 *  comando— porque con varios abiertos cerrar uno no puede llevarse por
 *  delante la sesión sin guardar de otro; declararlo opcional y mandar
 *  `null` hacía que el comando rechazara la llamada y el apunte se
 *  quedara puesto. */
export function borraSesion(workPath: string): Promise<void> {
  return invoke("borra_sesion", { workPath });
}

/** Las sesiones que quedaron a medias: **una por documento** que tenía
 *  cambios y cuya copia de trabajo sigue en el disco. La lista puede venir
 *  vacía, que es el caso normal. Acrobat ofrece recuperar todos los
 *  documentos que estaban abiertos, no solo el último que se tocó. */
export function recoverSession(): Promise<Sesion[]> {
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
