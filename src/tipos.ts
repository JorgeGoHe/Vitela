/**
 * Tipos y utilidades compartidos entre el visor (App) y las páginas
 * individuales (components/Pagina).
 */
import type { EstadoFirma, Rgba } from "./api";

/** La app corre en macOS: cambia el modificador de los atajos y cómo se
 *  escriben. Un solo sitio para la pregunta, que se hace en varios. */
export const ES_MAC = navigator.platform.startsWith("Mac");
/** Tecla modificadora en los tooltips y los menús («⌘» en Mac, «Ctrl+»). */
export const MOD = ES_MAC ? "⌘" : "Ctrl+";
/** Atajo del panel lateral, escrito como lo escribe cada plataforma. */
export const ATAJO_PANEL = ES_MAC ? "⌥⌘1" : "Ctrl+Alt+1";
/** Atajos de las otras dos pestañas del panel (abren Y llevan el foco). */
export const ATAJO_MARCADORES = ES_MAC ? "⌥⌘2" : "Ctrl+Alt+2";
export const ATAJO_COMENTARIOS = ES_MAC ? "⌥⌘3" : "Ctrl+Alt+3";

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
export type SearchMatch = {
  page_index: number;
  rects: Rect[];
  /** La frase alrededor de la coincidencia (30 caracteres a cada lado), que
   *  solo llega si la búsqueda se pidió con `context`. */
  before?: string;
  after?: string;
  /** `object_index` del bloque de texto en el que cae la coincidencia, tal
   *  como lo sitúa el backend (el más pequeño de los que la contienen).
   *  Llega con `context` y es lo que necesita `replace_text`: la UI no lo
   *  vuelve a calcular, que era el mismo criterio escrito dos veces y con
   *  dos resultados posibles. */
  block_index?: number;
};
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
  /** Índice de la anotación a la que responde (`/IRT`), o `null` si es un
   *  comentario de primer nivel. Es lo que deja anidar el hilo. */
  in_reply_to: number | null;
  /** Estado de revisión (`/State`): "Accepted", "Rejected", "Cancelled",
   *  "Completed" o vacío. */
  state: string;
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
  /** Campo obligatorio (bit 2 de `/Ff`). Los PDFs que no lo declaren y los
   *  motores que no lo lean lo dejan sin definir: entonces no se pinta. */
  required?: boolean;
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
  /** Color del relleno del texto, tal como está en el content stream. Es lo
   *  que deja pintar del color real el swatch «A» («el que ya tenga») en vez
   *  de una letra gris con un tooltip. */
  color: Rgba;
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
/** Lo mismo para un bloque de texto: se coloca y se estira con el gesto que
 *  el usuario ya conoce de las imágenes y los sellos. */
export type TxtAction = {
  kind: "move" | "resize";
  handle?: ResizeHandle;
  startX: number;
  startY: number;
  orig: TextBlock;
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
  | "freetext"
  /** Dibujar el recuadro donde se verá la firma con certificado. */
  | "firma-cert";
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
  Square: "Marca de redacción",
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
  Square: "Marcas de redacción",
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
  Square: "redact",
};

/** «todos» o el plural de un tipo (`KIND_PLURALS`). */
export type FiltroComentarios = string;

/** Los cuatro estados de revisión, con su nombre en español. El valor es el
 *  que se escribe en el PDF; la etiqueta, la que lee el usuario. */
export const ESTADOS_COMENTARIO: [string, string][] = [
  ["Accepted", "Aceptado"],
  ["Rejected", "Rechazado"],
  ["Cancelled", "Cancelado"],
  ["Completed", "Completado"],
];

/** «Aceptado» a partir de «Accepted»; vacío si no hay estado. */
export function nombreEstado(state: string): string {
  return ESTADOS_COMENTARIO.find(([v]) => v === state)?.[1] ?? "";
}

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

/** Los índices que pide un bloque «Páginas: todas / 1-3, 8», o `null` para
 *  «todas», que es lo que el backend entiende por «sin rango». */
export function indicesDeRango(
  todas: boolean,
  rango: string,
  pageCount: number,
): number[] | null {
  return todas ? null : parseRango(rango, pageCount);
}

/** «1 página» / «4 páginas»: la forma correcta, no «4 página(s)». */
export function plural(n: number, singular: string, plural: string): string {
  return `${n} ${n === 1 ? singular : plural}`;
}

/** «834 KB» o «2,41 MB»: por debajo de 1 MB dos decimales de MB no
 *  distinguen nada, y la coma decimal es la que escribe el español. */
export function tamanoFichero(bytes: number): string {
  if (bytes < 1024 * 1024) return `${Math.round(bytes / 1024)} KB`;
  return `${(bytes / 1024 / 1024).toFixed(2).replace(".", ",")} MB`;
}

export function hexToRgba(hex: string, alpha = 255): Rgba {
  const n = parseInt(hex.slice(1), 16);
  return [(n >> 16) & 255, (n >> 8) & 255, n & 255, alpha];
}

/** El camino de vuelta, para pintar en la UI un color que llega del PDF. */
export function rgbaToHex(c: Rgba | null | undefined): string | null {
  if (!c) return null;
  const dos = (n: number) => Math.max(0, Math.min(255, n)).toString(16).padStart(2, "0");
  return `#${dos(c[0])}${dos(c[1])}${dos(c[2])}`;
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

/** Las páginas que salen de las opciones del diálogo, ya filtradas por
 *  pares/impares. Vive aquí y no en `App` porque el diálogo la necesita
 *  para decir «con esto no queda ninguna» **antes** de aceptar: el error de
 *  después llegaba con el diálogo ya cerrado y las opciones perdidas. */
export function paginasImprimibles(
  o: OpcionesImprimir,
  pageCount: number,
  paginaActual: number,
): number[] {
  const base =
    o.ambito === "todas"
      ? Array.from({ length: pageCount }, (_, i) => i)
      : o.ambito === "actual"
        ? [paginaActual]
        : parseRango(o.rango, pageCount);
  if (o.subconjunto === "todas") return base;
  // «pares» e «impares» van por el número que ve el usuario, no por índice
  const quiereImpar = o.subconjunto === "impares";
  return base.filter((i) => (i + 1) % 2 === (quiereImpar ? 1 : 0));
}

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

/** Tema de la app: el del sistema, o el que se elija a mano. */
export type Tema = "automatico" | "claro" | "oscuro";
/** Con qué zoom se abre un documento. «ultimo» conserva el que haya. */
export type ZoomInicial = "pagina" | "ancho" | "100" | "ultimo";
/** El fondo donde flota el documento (DESIGN.md dejó prevista la salida). */
export type Lienzo = "verde" | "gris";

export type Preferencias = {
  autor: string;
  /** Modo nocturno del documento: solo cambia lo que se ve en pantalla. */
  nocturno: boolean;
  tema: Tema;
  zoomInicial: ZoomInicial;
  lienzo: Lienzo;
};

const PREFS_POR_DEFECTO: Preferencias = {
  autor: "",
  nocturno: false,
  tema: "automatico",
  zoomInicial: "ancho",
  lienzo: "verde",
};

const TEMAS: Tema[] = ["automatico", "claro", "oscuro"];
const ZOOMS: ZoomInicial[] = ["pagina", "ancho", "100", "ultimo"];
const LIENZOS: Lienzo[] = ["verde", "gris"];

export function cargaPreferencias(): Preferencias {
  try {
    const g = JSON.parse(localStorage.getItem(CLAVE_PREFS) ?? "{}");
    return {
      autor: typeof g.autor === "string" ? g.autor : "",
      nocturno: !!g.nocturno,
      tema: TEMAS.includes(g.tema) ? g.tema : "automatico",
      zoomInicial: ZOOMS.includes(g.zoomInicial) ? g.zoomInicial : "ancho",
      lienzo: LIENZOS.includes(g.lienzo) ? g.lienzo : "verde",
    };
  } catch {
    return { ...PREFS_POR_DEFECTO };
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

/** «9 sept 2026, 14:30» a partir del `/M` del comentario, o cadena vacía si
 *  no se puede leer. La hora va desde que el `/M` que escribe el backend
 *  lleva su zona horaria: sin ella, dos comentarios del mismo día no se
 *  distinguían. */
export function fechaAnotacion(modified: string): string {
  if (!modified) return "";
  const d = new Date(modified);
  if (Number.isNaN(d.getTime())) return "";
  return d.toLocaleString("es-ES", {
    day: "numeric",
    month: "short",
    year: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
}

/** «Jorge · 9 sept 2026, 14:30» para el popover de un comentario. */
export function firmaAnotacion(author: string, modified: string): string {
  return [author, fechaAnotacion(modified)].filter(Boolean).join(" · ");
}

/* ---- presentación de página (persistida en localStorage) ---- */

const CLAVE_VISTA = "editorPdf.vista";

/** Las cuatro presentaciones de Acrobat. «continuo» es la de siempre. */
export type ModoPagina = "una" | "continuo" | "dos" | "dos-continuo";

export type PrefsVista = {
  modoPagina: ModoPagina;
  /** «Mostrar portada en vista de dos páginas». */
  portadaSola: boolean;
};

const MODOS_PAGINA: ModoPagina[] = ["una", "continuo", "dos", "dos-continuo"];

export function cargaVista(): PrefsVista {
  try {
    const g = JSON.parse(localStorage.getItem(CLAVE_VISTA) ?? "{}");
    return {
      modoPagina: MODOS_PAGINA.includes(g.modoPagina)
        ? g.modoPagina
        : "continuo",
      portadaSola: g.portadaSola !== false,
    };
  } catch {
    return { modoPagina: "continuo", portadaSola: true };
  }
}

export function guardaVista(v: PrefsVista) {
  localStorage.setItem(CLAVE_VISTA, JSON.stringify(v));
}

/** Reparte las páginas en filas de pantalla: una por fila, o de dos en dos
 *  con la portada sola si así se pide (como el «Two Page View» de Acrobat). */
export function filasDePaginas(
  pageCount: number,
  dobles: boolean,
  portadaSola: boolean,
): number[][] {
  if (!dobles) return Array.from({ length: pageCount }, (_, i) => [i]);
  const filas: number[][] = [];
  let i = 0;
  if (portadaSola && pageCount > 0) {
    filas.push([0]);
    i = 1;
  }
  for (; i < pageCount; i += 2) {
    filas.push(i + 1 < pageCount ? [i, i + 1] : [i]);
  }
  return filas;
}

/* ---- firma digital ---- */

/** Todo lo que hace falta para firmar, recogido de una sola vez. */
export type FirmaDraft = {
  /** Certificado: un .p12/.pfx, o un .pem con su clave aparte. */
  certPath: string;
  keyPath: string;
  password: string;
  reason: string;
  signerName: string;
  /** Id de la firma manuscrita guardada que se dibuja en el recuadro. */
  firmaId: string;
};

export const FIRMA_VACIA: FirmaDraft = {
  certPath: "",
  keyPath: "",
  password: "",
  reason: "",
  signerName: "",
  firmaId: "",
};

/** Lo que se ha comprobado de una firma, dicho en llano. Nunca «CMS», ni
 *  «ByteRange», ni «digest»: el usuario quiere saber si el documento es el
 *  que se firmó, no cómo se ha averiguado.
 *
 *  Tres niveles, como Acrobat: bien, mal y **«no se ha podido comprobar»**.
 *  El tercero es el que faltaba: una firma ECDSA o con SHA-512 salía en rojo
 *  acusando de manipulación un documento intacto. Acrobat nunca dice «no
 *  válida» cuando lo que pasa es que no sabe.
 *
 *  **Rojo solo cuando el contenido firmado no cuadra.** Una revisión añadida
 *  detrás de la firma —rellenar un campo, una segunda firma, el DSS de una
 *  firma con LTV— es lo normal en un PDF firmado que sigue vivo, y en
 *  Acrobat tampoco es rojo: la firma sigue siendo válida y lo que hay es
 *  contenido que no avala. Por eso ese caso es `duda`, y va después de
 *  comprobar el digest: si además el digest falla, manda el rojo. */
export function estadoDeFirma(f: {
  estado?: EstadoFirma;
  covers_whole_file: boolean;
  digest_ok: boolean;
}): { nivel: NivelFirma; texto: string } {
  if (f.estado === "desconocido") {
    return {
      nivel: "duda",
      texto:
        "No se ha podido comprobar la firma: usa un tipo de firma que Vitela todavía no sabe leer",
    };
  }
  if (f.estado === "modificado" || !f.digest_ok) {
    return {
      nivel: "mal",
      texto: "El documento ha cambiado después de firmarse",
    };
  }
  if (!f.covers_whole_file) {
    return {
      nivel: "duda",
      texto: "La firma es válida, pero hay cambios posteriores que no avala",
    };
  }
  return { nivel: "ok", texto: "El documento no ha cambiado desde la firma" };
}

/** Cómo se pinta cada estado: verde, rojo o neutro. */
export type NivelFirma = "ok" | "mal" | "duda";

/** «9 de septiembre de 2026» a partir de un ISO 8601; el texto tal cual si
 *  no se puede leer como fecha. */
export function fechaLarga(iso: string): string {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  return d.toLocaleDateString("es-ES", {
    day: "numeric",
    month: "long",
    year: "numeric",
  });
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

/* ---- último zoom usado (persistido en localStorage) ---- */

const CLAVE_ZOOM = "editorPdf.zoom";

/** Los tres zooms de Acrobat: porcentaje fijo, al ancho, o la hoja entera. */
export type Zoom = number | "ajuste" | "pagina";

/** El zoom con el que se cerró la app. La preferencia «zoom al abrir: el
 *  último» no hacía nada entre sesiones porque el zoom no se guardaba. */
export function cargaZoom(): Zoom {
  const g = localStorage.getItem(CLAVE_ZOOM);
  if (g === "pagina" || g === "ajuste") return g;
  const n = Number(g);
  return Number.isFinite(n) && n > 0 ? n : "ajuste";
}

export function guardaZoom(z: Zoom) {
  localStorage.setItem(CLAVE_ZOOM, String(z));
}

/* ---- aviso de pantalla completa (una sola vez, localStorage) ---- */

const CLAVE_AVISO_PANTALLA = "editorPdf.avisoPantallaCompleta";

/** «Pulsa Esc para salir» se enseña una vez y ya está. Vivía en un `useRef`,
 *  así que volvía a salir en cada arranque de la app. */
export function avisoPantallaVisto(): boolean {
  return localStorage.getItem(CLAVE_AVISO_PANTALLA) === "1";
}

export function marcaAvisoPantalla() {
  localStorage.setItem(CLAVE_AVISO_PANTALLA, "1");
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
