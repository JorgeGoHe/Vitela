/**
 * Tipos y utilidades compartidos entre el visor (App) y las páginas
 * individuales (components/Pagina).
 */
import type { Rgba } from "./api";

export type CharBox = { ch: string; x: number; y: number; w: number; h: number };
export type PageText = { width: number; height: number; chars: CharBox[] };
export type Rect = { x: number; y: number; w: number; h: number };
export type PageSize = { width: number; height: number };
export type SearchMatch = { page_index: number; rects: Rect[] };
export type Selection = { start: number; end: number };
export type AnnotationInfo = {
  index: number;
  kind: string;
  x: number;
  y: number;
  w: number;
  h: number;
  contents: string;
  rects: Rect[];
  color: [number, number, number, number] | null;
};
export type FormFieldInfo = {
  annot_index: number;
  name: string;
  kind: string;
  value: string;
  checked: boolean;
  x: number;
  y: number;
  w: number;
  h: number;
};
export type TextBlock = {
  object_index: number;
  text: string;
  x: number;
  y: number;
  w: number;
  h: number;
  font_size: number;
  font_family: string;
};
export type ImageInfo = {
  object_index: number;
  x: number;
  y: number;
  w: number;
  h: number;
};
/** Tirador de redimensionado: esquinas y bordes de la caja. */
export type ResizeHandle = "nw" | "n" | "ne" | "e" | "se" | "s" | "sw" | "w";
export type ImgAction = {
  kind: "move" | "resize";
  handle?: ResizeHandle;
  startX: number;
  startY: number;
  orig: ImageInfo;
  moved: boolean;
};
export type Mode =
  | "select"
  | "draw"
  | "note"
  | "edit"
  | "image"
  | "firmar"
  | "shape"
  | "stamp"
  | "crop"
  | "redact"
  | "form-new"
  | "link-new";
export type ShapeKind = "rect" | "ellipse" | "line" | "arrow";

/* Paleta única de anotación (DESIGN.md): la comparten dibujo, formas,
   marcas de texto y sellos. */
export const ANNOT_COLORS = ["#f5c400", "#2ea043", "#2743c0", "#c0392b", "#1d1c18"];
export const NOMBRE_COLOR: Record<string, string> = {
  "#f5c400": "Amarillo",
  "#2ea043": "Verde",
  "#2743c0": "Azul tinta",
  "#c0392b": "Rojo corrector",
  "#1d1c18": "Negro tinta",
};

export const FONT_CHOICES: { value: string; label: string }[] = [
  { value: "auto", label: "Automática (documento)" },
  { value: "Helvetica", label: "Helvetica" },
  { value: "Helvetica Bold", label: "Helvetica Negrita" },
  { value: "Helvetica Oblique", label: "Helvetica Cursiva" },
  { value: "Times", label: "Times" },
  { value: "Times Bold", label: "Times Negrita" },
  { value: "Times Italic", label: "Times Cursiva" },
  { value: "Courier", label: "Courier" },
  { value: "Courier Bold", label: "Courier Negrita" },
];

export const KIND_LABELS: Record<string, string> = {
  Text: "Nota",
  Highlight: "Resaltado",
  Ink: "Dibujo",
  Underline: "Subrayado",
  Strikeout: "Tachado",
  StrikeOut: "Tachado",
  Stamp: "Sello",
};

export function hexToRgba(hex: string, alpha = 255): Rgba {
  const n = parseInt(hex.slice(1), 16);
  return [(n >> 16) & 255, (n >> 8) & 255, n & 255, alpha];
}

/** Une cajas de caracteres consecutivos en rectángulos por línea. */
export function mergeLineRects(boxes: CharBox[]): Rect[] {
  const out: Rect[] = [];
  for (const b of boxes) {
    if (b.w <= 0 || b.h <= 0) continue;
    const last = out[out.length - 1];
    if (last && Math.abs(b.y - last.y) < Math.max(last.h, b.h) * 0.7) {
      const right = Math.max(last.x + last.w, b.x + b.w);
      const bottom = Math.max(last.y + last.h, b.y + b.h);
      last.x = Math.min(last.x, b.x);
      last.y = Math.min(last.y, b.y);
      last.w = right - last.x;
      last.h = bottom - last.y;
    } else {
      out.push({ x: b.x, y: b.y, w: b.w, h: b.h });
    }
  }
  return out;
}

/** Índice del carácter más cercano a un punto (en puntos PDF). */
export function charIndexAt(pt: PageText, x: number, y: number): number | null {
  let best = -1;
  let bestScore = Infinity;
  pt.chars.forEach((c, i) => {
    if (c.w <= 0 || c.h <= 0) return;
    const dyOut = y < c.y ? c.y - y : y > c.y + c.h ? y - (c.y + c.h) : 0;
    const dxOut = x < c.x ? c.x - x : x > c.x + c.w ? x - (c.x + c.w) : 0;
    const score = dyOut * 20 + dxOut;
    if (score < bestScore) {
      bestScore = score;
      best = i;
    }
  });
  return best >= 0 ? best : null;
}

export function copyToClipboard(text: string) {
  navigator.clipboard?.writeText(text).catch(() => {
    const ta = document.createElement("textarea");
    ta.value = text;
    document.body.appendChild(ta);
    ta.select();
    document.execCommand("copy");
    ta.remove();
  });
}

/* ---- memoria de color por acción (persistida en localStorage) ---- */

const CLAVE_COLORES = "editorPdf.coloresAccion";

export function cargaColores(): Record<string, string> {
  try {
    return JSON.parse(localStorage.getItem(CLAVE_COLORES) ?? "{}");
  } catch {
    return {};
  }
}

export function guardaColor(accion: string, color: string) {
  const c = cargaColores();
  c[accion] = color;
  localStorage.setItem(CLAVE_COLORES, JSON.stringify(c));
}
