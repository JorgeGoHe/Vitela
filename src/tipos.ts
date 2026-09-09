/**
 * Tipos y utilidades compartidos entre el visor (App) y las páginas
 * individuales (components/Pagina).
 */
import type { Rgba } from "./api";

/** Tecla modificadora en los tooltips y los menús («⌘» en Mac, «Ctrl+»). */
export const MOD = navigator.platform.startsWith("Mac") ? "⌘" : "Ctrl+";
/** Atajo del panel lateral, escrito como lo escribe cada plataforma. */
export const ATAJO_PANEL = navigator.platform.startsWith("Mac")
  ? "⌥⌘1"
  : "Ctrl+Alt+1";
/** Atajos de las otras dos pestañas del panel (abren Y llevan el foco). */
export const ATAJO_MARCADORES = navigator.platform.startsWith("Mac")
  ? "⌥⌘2"
  : "Ctrl+Alt+2";
export const ATAJO_COMENTARIOS = navigator.platform.startsWith("Mac")
  ? "⌥⌘3"
  : "Ctrl+Alt+3";

export type CharBox = { ch: string; x: number; y: number; w: number; h: number };
export type PageText = { width: number; height: number; chars: CharBox[] };
export type Rect = { x: number; y: number; w: number; h: number };
export type PageSize = {
  /** Ancho y alto de la página TAL COMO SE VE (con /Rotate aplicado). */
  width: number;
  height: number;
  /** /Rotate de la página: 0, 90, 180 o 270 grados en sentido horario. */
  rotation: number;
};
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
  /** Autor del comentario (`/T`); vacío si el PDF no lo trae. */
  author: string;
  /** Fecha de modificación (`/M`) en ISO 8601, o vacía. */
  modified: string;
  rects: Rect[];
  color: [number, number, number, number] | null;
};
export type FormFieldInfo = {
  annot_index: number;
  name: string;
  /** "Text", "Checkbox", "RadioButton", "ComboBox" o "ListBox". */
  kind: string;
  value: string;
  /** Opciones del desplegable o de la lista; vacío en el resto. */
  options: string[];
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
  | "link-new"
  | "freetext";
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
  Link: "Enlace",
  FreeText: "Cuadro de texto",
};

/** Plurales para el filtro y el resumen del panel de comentarios. */
export const KIND_PLURALS: Record<string, string> = {
  Text: "Notas",
  Highlight: "Resaltados",
  Underline: "Subrayados",
  Strikeout: "Tachados",
  StrikeOut: "Tachados",
  Ink: "Dibujos",
  Stamp: "Sellos",
  FreeText: "Cuadros de texto",
  Link: "Enlaces",
};

/** Icono del set `Icon` que representa cada tipo de comentario. */
export const KIND_ICONS: Record<string, string> = {
  Text: "note",
  Highlight: "highlight",
  Underline: "underline",
  Strikeout: "strike",
  StrikeOut: "strike",
  Ink: "pen",
  Stamp: "stamp",
  FreeText: "textbox",
  Link: "link",
};

/** «todos» o el plural de un tipo (`KIND_PLURALS`). */
export type FiltroComentarios = string;

/** Lee la sintaxis de rango de Acrobat («1-3, 8») y devuelve los índices
 *  desde 0, ordenados y sin repetidos. Lo que se sale del documento se
 *  descarta en silencio. */
export function parseRango(texto: string, pageCount: number): number[] {
  const fuera = new Set<number>();
  for (const trozo of texto.split(",")) {
    const t = trozo.trim();
    if (!t) continue;
    const m = /^(\d+)\s*(?:-\s*(\d+))?$/.exec(t);
    if (!m) continue;
    const a = Number(m[1]);
    const b = m[2] ? Number(m[2]) : a;
    for (let n = Math.min(a, b); n <= Math.max(a, b); n++) {
      if (n >= 1 && n <= pageCount) fuera.add(n - 1);
    }
  }
  return [...fuera].sort((x, y) => x - y);
}

/** El camino de vuelta: «1-3, 8» a partir de unos índices desde 0. */
export function formateaRango(indices: number[]): string {
  const orden = [...indices].sort((a, b) => a - b);
  const trozos: string[] = [];
  let i = 0;
  while (i < orden.length) {
    let j = i;
    while (j + 1 < orden.length && orden[j + 1] === orden[j] + 1) j++;
    trozos.push(i === j ? `${orden[i] + 1}` : `${orden[i] + 1}-${orden[j] + 1}`);
    i = j + 1;
  }
  return trozos.join(", ");
}

/** «1 página» / «4 páginas»: la forma correcta, no «4 página(s)». */
export function plural(n: number, singular: string, plural: string): string {
  return `${n} ${n === 1 ? singular : plural}`;
}

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

/* ---- impresión ---- */

/** Qué se imprime y cómo. Los valores por defecto son los de Acrobat:
 *  todas las páginas, ajustadas al papel y con las marcas. */
export type OpcionesImprimir = {
  ambito: "todas" | "actual" | "rango";
  rango: string;
  subconjunto: "todas" | "pares" | "impares";
  escala: "ajustar" | "real" | "personalizada";
  porcentaje: number;
  /** «Documento y marcas»: con los comentarios y los campos rellenados. */
  conMarcas: boolean;
};

export const IMPRIMIR_POR_DEFECTO: OpcionesImprimir = {
  ambito: "todas",
  rango: "",
  subconjunto: "todas",
  escala: "ajustar",
  porcentaje: 100,
  conMarcas: true,
};

/* ---- ajuste de línea del cuadro de texto (FreeText) ---- */

/** Interlineado del `/AP` del cuadro de texto. */
export const FREETEXT_INTERLINEA = 1.2;
/** Margen interior del `/AP` (2 pt por lado). */
const FREETEXT_MARGEN = 4;

let medidor: CanvasRenderingContext2D | null = null;

/** Ancho en puntos de un texto en Helvetica al tamaño dado. La apariencia
 *  del cuadro de texto se escribe en Helvetica; Arial es métricamente
 *  compatible y hace de reserva donde Helvetica no esté instalada. */
function anchoTexto(texto: string, fontSize: number): number {
  if (!medidor) medidor = document.createElement("canvas").getContext("2d");
  // sin canvas (entornos sin 2d) queda la media de Helvetica: 0,5 em
  if (!medidor) return texto.length * fontSize * 0.5;
  medidor.font = `${fontSize}px Helvetica, Arial, sans-serif`;
  return medidor.measureText(texto).width;
}

/** Reparte el texto en las líneas que caben en una caja de `ancho` puntos,
 *  como hace el cuadro de texto de Acrobat: respeta los saltos escritos,
 *  parte por espacios y, si una palabra no cabe entera, por letras. La UI
 *  manda las líneas ya hechas para que la apariencia guardada en el PDF sea
 *  la que se ve al escribir. */
export function ajustaLineas(
  texto: string,
  ancho: number,
  fontSize: number,
): string[] {
  const util = Math.max(fontSize, ancho - FREETEXT_MARGEN);
  const salida: string[] = [];
  for (const parrafo of texto.split("\n")) {
    let linea = "";
    for (const palabra of parrafo.split(" ")) {
      const tentativa = linea === "" ? palabra : `${linea} ${palabra}`;
      if (anchoTexto(tentativa, fontSize) <= util) {
        linea = tentativa;
        continue;
      }
      if (linea !== "") salida.push(linea);
      let resto = palabra;
      while (resto.length > 1 && anchoTexto(resto, fontSize) > util) {
        let corte = 1;
        while (
          corte < resto.length &&
          anchoTexto(resto.slice(0, corte + 1), fontSize) <= util
        ) {
          corte++;
        }
        salida.push(resto.slice(0, corte));
        resto = resto.slice(corte);
      }
      linea = resto;
    }
    salida.push(linea);
  }
  return salida;
}

/** Alto en puntos que necesita una caja para enseñar `lineas` renglones. */
export function altoCuadro(lineas: number, fontSize: number): number {
  return Math.max(1, lineas) * fontSize * FREETEXT_INTERLINEA + FREETEXT_MARGEN;
}

/* ---- preferencias de la app (persistidas en localStorage) ---- */

const CLAVE_PREFS = "editorPdf.preferencias";

export type Preferencias = { autor: string };

export function cargaPreferencias(): Preferencias {
  try {
    const g = JSON.parse(localStorage.getItem(CLAVE_PREFS) ?? "{}");
    return { autor: typeof g.autor === "string" ? g.autor : "" };
  } catch {
    return { autor: "" };
  }
}

export function guardaPreferencias(p: Preferencias) {
  localStorage.setItem(CLAVE_PREFS, JSON.stringify(p));
}

/** Autor que se escribe en los comentarios nuevos; null = el nombre de
 *  usuario del sistema, que es lo que pone el backend. */
export function autorComentarios(): string | null {
  return cargaPreferencias().autor.trim() || null;
}

/** «Jorge · 9 sept 2026, 14:30» para el popover de un comentario. La hora
 *  va desde que el `/M` que escribe el backend lleva su zona horaria: sin
 *  ella, dos comentarios del mismo día no se distinguían. */
export function firmaAnotacion(author: string, modified: string): string {
  const partes: string[] = [];
  if (author) partes.push(author);
  if (modified) {
    const d = new Date(modified);
    if (!Number.isNaN(d.getTime())) {
      partes.push(
        d.toLocaleString("es-ES", {
          day: "numeric",
          month: "short",
          year: "numeric",
          hour: "2-digit",
          minute: "2-digit",
        }),
      );
    }
  }
  return partes.join(" · ");
}

/* ---- opciones de búsqueda (persistidas en localStorage) ---- */

const CLAVE_BUSQUEDA = "editorPdf.opcionesBusqueda";

export type OpcionesBusqueda = { matchCase: boolean; wholeWord: boolean };

export function cargaOpcionesBusqueda(): OpcionesBusqueda {
  try {
    const g = JSON.parse(localStorage.getItem(CLAVE_BUSQUEDA) ?? "{}");
    return { matchCase: !!g.matchCase, wholeWord: !!g.wholeWord };
  } catch {
    return { matchCase: false, wholeWord: false };
  }
}

export function guardaOpcionesBusqueda(o: OpcionesBusqueda) {
  localStorage.setItem(CLAVE_BUSQUEDA, JSON.stringify(o));
}

/* ---- resaltado de los campos de formulario (localStorage) ---- */

const CLAVE_CAMPOS = "editorPdf.resaltarCampos";

/** «Resaltar campos existentes» de Acrobat: encendido por defecto. */
export function cargaResaltarCampos(): boolean {
  return localStorage.getItem(CLAVE_CAMPOS) !== "0";
}

export function guardaResaltarCampos(v: boolean) {
  localStorage.setItem(CLAVE_CAMPOS, v ? "1" : "0");
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
