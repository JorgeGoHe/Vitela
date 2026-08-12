import { invoke } from "./ipc";

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

export type Rgba = [number, number, number, number];

/** Marca de texto sobre rects: resaltar, subrayar o tachar. */
export function addMarkup(args: {
  workPath: string;
  pageIndex: number;
  rects: { x: number; y: number; w: number; h: number }[];
  kind: "highlight" | "underline" | "strikeout";
  color?: Rgba;
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
}): Promise<void> {
  return invoke("add_stamp", { ...args });
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
};

export function getLinks(
  path: string,
  pageIndex: number,
): Promise<LinkInfo[]> {
  return invoke("get_links", { path, pageIndex });
}

/** Protege con contraseña (AES-256) escribiendo una copia en destPath. */
export function encryptPdf(args: {
  workPath: string;
  destPath: string;
  userPassword: string;
  ownerPassword?: string | null;
}): Promise<void> {
  return invoke("encrypt_pdf", { ownerPassword: null, ...args });
}

/** Aplana anotaciones y formularios a contenido fijo. */
export function flattenPdf(workPath: string): Promise<void> {
  return invoke("flatten_pdf", { workPath });
}

export type RedactReport = { textos: number; imagenes: number };

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
