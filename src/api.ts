import { invoke } from "@tauri-apps/api/core";

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
}): Promise<void> {
  return invoke("add_watermark", { ...args });
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
