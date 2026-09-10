/**
 * Tipos y utilidades compartidos entre el visor (App) y las páginas
 * individuales (components/Pagina).
 */
import type {
  EstadoFirma,
  ModoComposicion,
  NivelCertificacion,
  OpcionesComposicion,
  OrdenComentarios,
  RangoEtiquetas,
  Rgba,
} from "./api";
export type { RangoEtiquetas };

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
  /** Campo de solo lectura (bit 1 de `/Ff`): se pinta apagado y no acepta
   *  cambios, como en Acrobat. Opcional por lo mismo que `required`. */
  read_only?: boolean;
  /** Texto de ayuda del campo (`/TU`): es el que sale al pasar el ratón. */
  tooltip?: string;
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
  /** Llamada: clic donde señala, arrastre hasta donde va el texto. */
  | "callout"
  /** Medir: distancia y área sobre la página, sin tocar el documento. */
  | "medir"
  /** Adjuntar un fichero a un punto de la página, como comentario. */
  | "adjunto"
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
  FileAttachment: "Fichero adjunto",
  Squiggly: "Subrayado ondulado",
  Circle: "Elipse",
  Line: "Línea",
  Polygon: "Área",
  PolyLine: "Perímetro",
  Polyline: "Perímetro",
  Caret: "Marca de inserción",
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
  FileAttachment: "Ficheros adjuntos",
  Squiggly: "Subrayados ondulados",
  Circle: "Elipses",
  Line: "Líneas",
  Polygon: "Áreas",
  PolyLine: "Perímetros",
  Polyline: "Perímetros",
  Caret: "Marcas de inserción",
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
  FileAttachment: "clip",
  Squiggly: "underline",
  Circle: "shapes",
  Line: "ruler",
  Polygon: "ruler",
  PolyLine: "ruler",
  Polyline: "ruler",
  Caret: "textedit",
};

/** Los subtipos que el backend cuenta como comentario
 *  (`comentarios2::es_comentario`). Los tres mapas de arriba tienen que
 *  decir algo de **todos**: lo que falte sale en inglés en el filtro del
 *  panel y sin icono, que es como se coló `FileAttachment` (AC-076). El
 *  spec escribe `/StrikeOut` y `/PolyLine` donde PDFium dice `Strikeout` y
 *  `Polyline`, así que las dos grafías están en los mapas. */
export const SUBTIPOS_COMENTARIO = [
  "Text",
  "Highlight",
  "Underline",
  "StrikeOut",
  "Squiggly",
  "FreeText",
  "Ink",
  "Square",
  "Circle",
  "Line",
  "Polygon",
  "PolyLine",
  "Stamp",
  "Caret",
  "FileAttachment",
];

/** Comprobación de la lista de arriba: devuelve los subtipos que se han
 *  quedado sin nombre en español. En desarrollo se grita por consola al
 *  arrancar; en producción no cuesta nada porque nadie la llama. */
export function subtiposSinNombre(): string[] {
  return SUBTIPOS_COMENTARIO.filter(
    (s) => !KIND_LABELS[s] || !KIND_PLURALS[s] || !KIND_ICONS[s],
  );
}

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
/** El nombre del fichero de una ruta. Es lo que se enseña en una banda o en
 *  un diálogo: la ruta entera empuja el resto de la frase fuera de la caja
 *  y no aporta nada que el usuario no sepa. La ruta va en el `title`. */
export function nombreDeFichero(ruta: string): string {
  return ruta.split(/[\\/]/).pop() || ruta;
}

export function plural(n: number, singular: string, plural: string): string {
  return `${n} ${n === 1 ? singular : plural}`;
}

/** «834 KB» o «2,41 MB»: por debajo de 1 MB dos decimales de MB no
 *  distinguen nada, y la coma decimal es la que escribe el español. */
export function tamanoFichero(bytes: number): string {
  // por debajo de 1 KB se dicen los bytes: «0 KB» se lee como «está vacío»
  if (bytes < 1024) return `${bytes} ${bytes === 1 ? "byte" : "bytes"}`;
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
  /** Detrás del documento, el resumen de comentarios (una fila por
   *  comentario), que es la casilla de Acrobat. */
  resumen: boolean;
  /** En qué orden va ese resumen; «por página» es el de Acrobat. */
  ordenResumen: OrdenComentarios;
  /** Cómo se montan las páginas en el papel: una por hoja («ninguna»),
   *  varias por hoja, folleto o póster. */
  composicion: ModoComposicion;
  /** Los ajustes de las tres composiciones, en snake_case porque viajan
   *  como una estructura anidada. */
  comp: Omit<OpcionesComposicion, "page_indices">;
};

/** Cuántas **hojas de papel** salen de una composición. Es el dato que hace
 *  falta para decidir —«8 páginas → 2 hojas»—, y en folleto no coincide con
 *  el número de caras: cada hoja lleva dos páginas por cada lado. */
export function hojasDeComposicion(
  modo: ModoComposicion,
  comp: Omit<OpcionesComposicion, "page_indices">,
  paginas: number,
): number {
  if (paginas === 0) return 0;
  if (modo === "nup")
    return Math.ceil(paginas / Math.max(1, comp.por_hoja));
  if (modo === "folleto") {
    const hojas = Math.ceil(paginas / 4);
    return comp.caras === "ambas" ? hojas : hojas;
  }
  if (modo === "poster") {
    const trozos = Math.max(1, Math.ceil(comp.escala_por_ciento / 100));
    return paginas * trozos * trozos;
  }
  return paginas;
}

/** El orden en el que van las páginas en la **primera hoja** de un folleto,
 *  que es lo que enseña la vista previa: con 8 páginas, 8-1 por delante y
 *  2-7 por detrás. Sin esto, «folleto» es magia negra. */
export function caraDeFolleto(paginas: number): [number, number] {
  const total = Math.ceil(paginas / 4) * 4;
  return [total, 1];
}

export const COMPOSICION_POR_DEFECTO: Omit<
  OpcionesComposicion,
  "page_indices"
> = {
  por_hoja: 2,
  orden: "horizontal",
  borde: false,
  encuadernacion: "izquierda",
  caras: "ambas",
  escala_por_ciento: 200,
  solape_mm: 0,
  marcas: false,
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
  resumen: false,
  ordenResumen: "pagina",
  composicion: "ninguna",
  comp: COMPOSICION_POR_DEFECTO,
};

/** Lo que «Reducir tamaño» puede descartar además de recomprimir las
 *  imágenes, que son las casillas del Optimizer de Acrobat. */
export type OpcionesComprimir = {
  quitarAdjuntos: boolean;
  quitarMetadatos: boolean;
  aplanarFormularios: boolean;
};

export const COMPRIMIR_POR_DEFECTO: OpcionesComprimir = {
  quitarAdjuntos: false,
  quitarMetadatos: false,
  aplanarFormularios: false,
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
  /** Qué se podrá cambiar después al **certificar** (el `/DocMDP`). El 2 es
   *  el de Acrobat y el que trae puesto el diálogo. */
  nivel: NivelCertificacion;
  /** Sellar la hora con un servidor de tiempo (apagado por defecto: hace
   *  falta red) y con cuál. */
  tsa: boolean;
  tsaUrl: string;
  /** Guardar la prueba de validez (LTV) dentro del documento. */
  ltv: boolean;
};

export const FIRMA_VACIA: FirmaDraft = {
  certPath: "",
  keyPath: "",
  password: "",
  reason: "",
  signerName: "",
  firmaId: "",
  nivel: 2,
  tsa: false,
  tsaUrl: "",
  ltv: false,
};

/** Lo que deja hacer un documento certificado, dicho como lo entiende quien
 *  lo abre y no con el número del `/DocMDP`. Lo usan el diálogo, la banda de
 *  apertura y la tarjeta del panel, para que las tres digan lo mismo. */
export function permisosCertificacion(nivel: number): string {
  if (nivel === 1) return "nadie puede cambiar nada";
  if (nivel === 3) return "se pueden rellenar los formularios, firmar y comentar";
  return "se pueden rellenar los formularios y firmar";
}

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

/** «hoy a las 19:40», «ayer a las 19:40» o «el 3 de septiembre a las
 *  19:40», a partir del ISO 8601 del apunte de sesión. No se ofrece
 *  recuperar nada sin decir **cuándo** fue: recuperar algo de hace tres
 *  semanas sin saberlo es peor que no ofrecerlo. */
export function cuandoLlano(iso: string): string {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return "";
  const hora = d.toLocaleTimeString("es-ES", {
    hour: "2-digit",
    minute: "2-digit",
  });
  const dia = new Date(d.getFullYear(), d.getMonth(), d.getDate());
  const hoy = new Date();
  const cero = new Date(hoy.getFullYear(), hoy.getMonth(), hoy.getDate());
  const dias = Math.round((cero.getTime() - dia.getTime()) / 86400000);
  if (dias === 0) return `hoy a las ${hora}`;
  if (dias === 1) return `ayer a las ${hora}`;
  const fecha = d.toLocaleDateString("es-ES", {
    day: "numeric",
    month: "long",
    ...(d.getFullYear() === hoy.getFullYear() ? {} : { year: "numeric" }),
  });
  return `el ${fecha} a las ${hora}`;
}

/* ---- etiquetas de página (`/PageLabels`) ---- */

/** Un número romano en mayúsculas, hasta 3999 (lo que da el spec). */
function romano(n: number): string {
  if (n <= 0) return String(n);
  const tabla: [number, string][] = [
    [1000, "M"], [900, "CM"], [500, "D"], [400, "CD"], [100, "C"],
    [90, "XC"], [50, "L"], [40, "XL"], [10, "X"], [9, "IX"],
    [5, "V"], [4, "IV"], [1, "I"],
  ];
  let resto = n;
  let out = "";
  for (const [valor, letra] of tabla)
    while (resto >= valor) {
      out += letra;
      resto -= valor;
    }
  return out;
}

/** La numeración por letras del spec: A..Z, luego AA, BB, CC… */
function porLetras(n: number): string {
  if (n <= 0) return String(n);
  const letra = String.fromCharCode(65 + ((n - 1) % 26));
  return letra.repeat(Math.floor((n - 1) / 26) + 1);
}

/** El tramo que manda en una página física, o null si no hay ninguno. */
function rangoDe(
  rangos: RangoEtiquetas[],
  i: number,
): RangoEtiquetas | null {
  let elegido: RangoEtiquetas | null = null;
  for (const r of rangos)
    if (r.desde <= i && (!elegido || r.desde > elegido.desde)) elegido = r;
  return elegido;
}

/** Cómo se llama la página `i` (desde 0): «ii», «A-3», «Portada»… Sin
 *  `/PageLabels` —o con un tramo que no dice nada— es su número físico, que
 *  es lo que hace cualquier visor. */
export function etiquetaDePagina(
  rangos: RangoEtiquetas[],
  i: number,
): string {
  const r = rangoDe(rangos, i);
  if (!r) return String(i + 1);
  const n = Math.max(1, r.empieza_en) + (i - r.desde);
  const cuerpo =
    r.estilo === "arabigo"
      ? String(n)
      : r.estilo === "romano"
        ? romano(n)
        : r.estilo === "romano_min"
          ? romano(n).toLowerCase()
          : r.estilo === "letra"
            ? porLetras(n)
            : r.estilo === "letra_min"
              ? porLetras(n).toLowerCase()
              : "";
  const texto = `${r.prefijo}${cuerpo}`;
  return texto || String(i + 1);
}

/** La página que se ha escrito en «Ir a la página». Se busca primero como
 *  **etiqueta** —«xii», «A-3»: es como se llama la hoja dentro del
 *  documento y es lo que la píldora enseña a un centímetro del campo— y, si
 *  lo escrito no es ninguna, se lee como número físico, que es lo que hace
 *  Acrobat. Devuelve el índice de página, o `null` si no es ni una cosa ni
 *  la otra. */
export function paginaEscrita(
  texto: string,
  rangos: RangoEtiquetas[],
  pageCount: number,
): number | null {
  const buscado = texto.trim().toLowerCase();
  if (!buscado) return null;
  for (let i = 0; i < pageCount; i++) {
    if (etiquetaDePagina(rangos, i).trim().toLowerCase() === buscado) return i;
  }
  const n = Number.parseInt(buscado.replace(/[^0-9]/g, ""), 10);
  if (Number.isNaN(n)) return null;
  return Math.min(Math.max(n - 1, 0), pageCount - 1);
}

/** Cómo se enseña una página cuando su etiqueta no es su número físico:
 *  «ii (2)», como Acrobat. Si coinciden, solo el número. */
export function pagineoLlano(rangos: RangoEtiquetas[], i: number): string {
  const etiqueta = etiquetaDePagina(rangos, i);
  return etiqueta === String(i + 1) ? etiqueta : `${etiqueta} (${i + 1})`;
}

/** Aplica un tramo de numeración a las páginas `desde`..`hasta` y devuelve
 *  la lista entera, que es lo que escribe `set_page_labels`. Lo que va
 *  detrás del tramo conserva la numeración que tenía: sin eso, numerar el
 *  prólogo en romanos renumeraba el libro entero. */
export function aplicaRangoEtiquetas(
  rangos: RangoEtiquetas[],
  desde: number,
  hasta: number,
  nuevo: Omit<RangoEtiquetas, "desde">,
  pageCount: number,
): RangoEtiquetas[] {
  // qué numeración tenía la página de después del tramo, para reponerla
  const siguiente = hasta + 1;
  let cola: RangoEtiquetas | null = null;
  if (siguiente < pageCount && !rangos.some((r) => r.desde === siguiente)) {
    const antes = rangoDe(rangos, siguiente);
    cola = antes
      ? {
          ...antes,
          desde: siguiente,
          empieza_en: antes.empieza_en + (siguiente - antes.desde),
        }
      : {
          desde: siguiente,
          estilo: "arabigo",
          prefijo: "",
          empieza_en: siguiente + 1,
        };
  }
  const resto = rangos.filter((r) => r.desde < desde || r.desde > hasta);
  const lista = [...resto, { ...nuevo, desde }, ...(cola ? [cola] : [])];
  return lista.sort((a, b) => a.desde - b.desde);
}

/* ---- opciones de búsqueda (persistidas en localStorage) ---- */

/* ---- sellos: la galería, el último usado y la línea de los dinámicos ---- */

/** Los siete sellos de la biblioteca estándar de Acrobat, con su texto tal
 *  como se estampa. */
export const SELLOS_ESTANDAR = [
  "APROBADO",
  "REVISADO",
  "RECIBIDO",
  "CONFIDENCIAL",
  "BORRADOR",
  "DEFINITIVO",
  "NULO",
];

/** Los dinámicos: los mismos tres de Acrobat, que componen **en el momento**
 *  quién sella y cuándo debajo de la palabra. */
export const SELLOS_DINAMICOS = ["REVISADO", "RECIBIDO", "APROBADO"];

const CLAVE_SELLO = "editorPdf.ultimoSello";

/** El sello que se puso la última vez: la galería lo enseña el primero y
 *  lo trae elegido, porque quien sella un expediente pone el mismo sello
 *  cincuenta veces. La ranura de las imágenes guardadas ya no vive aquí:
 *  es un campo de la biblioteca del backend. */
export type UltimoSello = { texto: string; dinamico: boolean };

export function cargaUltimoSello(): UltimoSello | null {
  try {
    const v = JSON.parse(localStorage.getItem(CLAVE_SELLO) ?? "null");
    if (!v || typeof v.texto !== "string" || !v.texto) return null;
    return { texto: v.texto, dinamico: !!v.dinamico };
  } catch {
    return null;
  }
}

export function guardaUltimoSello(sello: UltimoSello): void {
  localStorage.setItem(CLAVE_SELLO, JSON.stringify(sello));
}

/** Las tres plantillas de sello dinámico que **el backend** sabe componer
 *  él solo, con el usuario del sistema. Sin nombre en Preferencias es lo
 *  que hay que mandarle: la interfaz no conoce el usuario del sistema, así
 *  que componer aquí dejaba el sello sin la mitad que promete. */
export const PLANTILLAS_DINAMICAS: Record<string, string> = {
  REVISADO: "revisado",
  RECIBIDO: "recibido",
  APROBADO: "aprobado",
};

/** La segunda línea de un sello dinámico, compuesta en el momento de
 *  estamparlo: quién y cuándo. El nombre sale del autor de comentarios de
 *  Preferencias, sin preguntar —es el mismo que firma cada comentario—, y
 *  la fecha va como se escribe en español, no en ISO, y con dos dígitos en
 *  el día y el mes, que es como se lee en un sello. */
export function selloDinamico(autor: string): string {
  const ahora = new Date();
  const fecha = ahora.toLocaleDateString("es-ES", {
    day: "2-digit",
    month: "2-digit",
    year: "numeric",
  });
  const hora = ahora.toLocaleTimeString("es-ES", {
    hour: "2-digit",
    minute: "2-digit",
  });
  const quien = autor.trim();
  return `${quien ? `${quien} · ` : ""}${fecha} ${hora}`;
}

const CLAVE_CARPETA = "editorPdf.carpetaBusqueda";

/** La última carpeta en la que se buscó: quien tiene una carpeta de facturas
 *  la busca todos los días y no tiene por qué volver a señalarla. */
export function cargaCarpetaBusqueda(): string | null {
  return localStorage.getItem(CLAVE_CARPETA);
}

export function guardaCarpetaBusqueda(dir: string): void {
  localStorage.setItem(CLAVE_CARPETA, dir);
}

const CLAVE_RECURSIVO = "editorPdf.busquedaRecursiva";

/** Si la última búsqueda en carpeta entró en las subcarpetas. Se recuerda
 *  igual que la carpeta: quien tiene las facturas por años las busca
 *  siempre igual. */
export function cargaRecursivoBusqueda(): boolean {
  return localStorage.getItem(CLAVE_RECURSIVO) === "1";
}

export function guardaRecursivoBusqueda(v: boolean): void {
  localStorage.setItem(CLAVE_RECURSIVO, v ? "1" : "0");
}

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

/* ---- medir (H5): la escala del documento, en milímetros por punto ---- */

/** Un punto PDF es 1/72 de pulgada: la escala del papel, que es la que usa
 *  Acrobat cuando el PDF no trae `/Measure`. */
export const MM_POR_PUNTO = 25.4 / 72;

/* ---- reglas, guías y cuadrícula (andamio, no documento) ---- */

const CLAVE_GUIAS = "editorPdf.guias";

/** Las guías de un documento, en puntos de página: `v` son las verticales
 *  (su x) y `h` las horizontales (su y). **No tocan el fichero**: son el
 *  andamio de quien coloca sellos y campos, así que viven donde vive la
 *  escala de medida, en `localStorage` y por ruta. */
/** Las guías de una página o del documento entero. */
export type Ejes = { v: number[]; h: number[] };

/** Guías de un documento: **las de cada página** —que es como funcionan en
 *  Acrobat, donde una guía es de la hoja en la que se pone— más las que
 *  valen para todas, que se dejan con ⌥ al soltar. */
export type Guias = Ejes & { paginas: Record<number, Ejes> };

export const SIN_GUIAS: Guias = { v: [], h: [], paginas: {} };

/** Las guías que se ven en una página: las suyas y las de todo el
 *  documento, juntas y sin repetir. */
export function guiasDePagina(guias: Guias, pagina: number): Ejes {
  const suyas = guias.paginas[pagina] ?? { v: [], h: [] };
  return {
    v: [...new Set([...guias.v, ...suyas.v])],
    h: [...new Set([...guias.h, ...suyas.h])],
  };
}

export function cargaGuias(path: string | null): Guias {
  if (!path) return SIN_GUIAS;
  try {
    const todas = JSON.parse(localStorage.getItem(CLAVE_GUIAS) ?? "{}");
    const g = todas[path];
    const nums = (x: unknown) =>
      Array.isArray(x) ? x.filter((n) => Number.isFinite(n)) : [];
    const paginas: Record<number, Ejes> = {};
    // el formato de antes no tenía páginas: sus guías eran de todo el
    // documento y ahí se quedan
    for (const [k, v] of Object.entries(g?.paginas ?? {})) {
      const e = v as { v?: unknown; h?: unknown };
      paginas[Number(k)] = { v: nums(e?.v), h: nums(e?.h) };
    }
    return { v: nums(g?.v), h: nums(g?.h), paginas };
  } catch {
    return SIN_GUIAS;
  }
}

export function guardaGuias(path: string | null, guias: Guias) {
  if (!path) return;
  try {
    const todas = JSON.parse(localStorage.getItem(CLAVE_GUIAS) ?? "{}");
    const vacias =
      guias.v.length === 0 &&
      guias.h.length === 0 &&
      Object.values(guias.paginas).every(
        (e) => e.v.length === 0 && e.h.length === 0,
      );
    if (vacias) delete todas[path];
    else todas[path] = guias;
    localStorage.setItem(CLAVE_GUIAS, JSON.stringify(todas));
  } catch {
    /* sin localStorage las guías valen para esta sesión y ya */
  }
}

/** Un punto ajustado a la cuadrícula, para lo que se coloca con ella
 *  encendida (⇧⌘U). El paso es el mismo que pinta `CapaGuias`: 10 mm de los
 *  de verdad, o de papel si el documento no tiene escala. */
export function ajustaACuadricula(
  p: { x: number; y: number },
  pasoPt: number,
): { x: number; y: number } {
  if (!(pasoPt > 0)) return p;
  return {
    x: Math.round(p.x / pasoPt) * pasoPt,
    y: Math.round(p.y / pasoPt) * pasoPt,
  };
}

const CLAVE_ESCALA = "editorPdf.escala";

/** La escala se pide una vez por documento y se guarda por su ruta: un
 *  plano no cambia de escala entre sesiones. */
export function cargaEscala(path: string | null): number {
  if (!path) return MM_POR_PUNTO;
  try {
    const todas = JSON.parse(localStorage.getItem(CLAVE_ESCALA) ?? "{}");
    const v = Number(todas[path]);
    return Number.isFinite(v) && v > 0 ? v : MM_POR_PUNTO;
  } catch {
    return MM_POR_PUNTO;
  }
}

export function guardaEscala(path: string | null, mmPorPunto: number) {
  if (!path) return;
  try {
    const todas = JSON.parse(localStorage.getItem(CLAVE_ESCALA) ?? "{}");
    todas[path] = mmPorPunto;
    localStorage.setItem(CLAVE_ESCALA, JSON.stringify(todas));
  } catch {
    /* sin localStorage la escala vale para esta sesión y ya */
  }
}

/** Un número con coma decimal y sin ceros de más, como se escribe aquí. */
function numero(n: number, decimales: number): string {
  return n.toFixed(decimales).replace(/\.?0+$/, "").replace(".", ",");
}

/** «12,4 cm» a partir de una longitud en puntos de página. */
export function formateaLongitud(puntos: number, mmPorPunto: number): string {
  const mm = puntos * mmPorPunto;
  if (mm >= 1000) return `${numero(mm / 1000, 2)} m`;
  if (mm >= 10) return `${numero(mm / 10, 1)} cm`;
  return `${numero(mm, 1)} mm`;
}

/** «3,2 cm²» a partir de un área en puntos cuadrados de página. */
export function formateaArea(puntos2: number, mmPorPunto: number): string {
  const mm2 = puntos2 * mmPorPunto * mmPorPunto;
  if (mm2 >= 1_000_000) return `${numero(mm2 / 1_000_000, 2)} m²`;
  if (mm2 >= 100) return `${numero(mm2 / 100, 1)} cm²`;
  return `${numero(mm2, 1)} mm²`;
}
