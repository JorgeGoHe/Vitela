import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";
import {
  addBlankPage,
  addHeaderFooter,
  addMarkup,
  addShape,
  addStamp,
  addWatermark,
  cropPage,
  deleteStoredSignature,
  duplicatePage,
  getImageData,
  importSignatureFile,
  insertPdfAt,
  listStoredSignatures,
  saveStoredSignature,
  stampSignature,
  type FirmaGuardada,
  type HeaderFooter,
  type Rgba,
} from "./api";
import {
  compressPdf,
  encryptPdf,
  exportPagesPng,
  exportText,
  flattenPdf,
  getLinks,
  getMetadata,
  getOutline,
  redactArea,
  setMetadata,
  setOutline,
  type LinkInfo,
  type Metadata,
  type OutlineNode,
  type RedactReport,
} from "./api";
import { openUrl } from "@tauri-apps/plugin-opener";
import PanelFirmas from "./components/PanelFirmas";
import DibujarFirma from "./components/DibujarFirma";
import DialogoMarcaAgua from "./components/DialogoMarcaAgua";
import DialogoEncabezado from "./components/DialogoEncabezado";
import PanelMarcadores from "./components/PanelMarcadores";
import DialogoPropiedades from "./components/DialogoPropiedades";
import "./App.css";

const BASE_WIDTH = 900;
const THUMB_WIDTH = 240;

type CharBox = { ch: string; x: number; y: number; w: number; h: number };
type PageText = { width: number; height: number; chars: CharBox[] };
type Rect = { x: number; y: number; w: number; h: number };
type SearchMatch = { page_index: number; rects: Rect[] };
type Selection = { start: number; end: number };
type AnnotationInfo = {
  index: number;
  kind: string;
  x: number;
  y: number;
  w: number;
  h: number;
  contents: string;
  rects: Rect[];
};
type FormFieldInfo = {
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
type TextBlock = {
  object_index: number;
  text: string;
  x: number;
  y: number;
  w: number;
  h: number;
  font_size: number;
  font_family: string;
};
type ImageInfo = {
  object_index: number;
  x: number;
  y: number;
  w: number;
  h: number;
};
type ImgAction = {
  kind: "move" | "resize";
  startX: number;
  startY: number;
  orig: ImageInfo;
  moved: boolean;
};
type Mode =
  | "select"
  | "draw"
  | "note"
  | "edit"
  | "image"
  | "firmar"
  | "shape"
  | "stamp"
  | "crop"
  | "redact";
type ShapeKind = "rect" | "ellipse" | "line" | "arrow";

const SHAPE_COLORS = ["#e23d3d", "#3478f6", "#2ea043", "#f5b400", "#111111"];
const STAMP_PRESETS = [
  "APROBADO",
  "BORRADOR",
  "CONFIDENCIAL",
  "REVISADO",
  "URGENTE",
];

function hexToRgba(hex: string, alpha = 255): Rgba {
  const n = parseInt(hex.slice(1), 16);
  return [(n >> 16) & 255, (n >> 8) & 255, n & 255, alpha];
}

const FONT_CHOICES: { value: string; label: string }[] = [
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
type UndoEntry = { page: number };

const KIND_LABELS: Record<string, string> = {
  Text: "Nota",
  Highlight: "Resaltado",
  Ink: "Dibujo",
  Underline: "Subrayado",
  Strikeout: "Tachado",
  StrikeOut: "Tachado",
  Stamp: "Sello",
};

const ICONS: Record<string, string[]> = {
  open: [
    "M4 20h16a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13c0 1.1.9 2 2 2Z",
  ],
  save: [
    "M19 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h11l5 5v11a2 2 0 0 1-2 2Z",
    "M17 21v-8H7v8",
    "M7 3v5h8",
  ],
  chevLeft: ["m15 18-6-6 6-6"],
  chevRight: ["m9 18 6-6-6-6"],
  minus: ["M5 12h14"],
  plus: ["M12 5v14", "M5 12h14"],
  search: ["M11 3a8 8 0 1 0 0 16 8 8 0 0 0 0-16Z", "m21 21-4.35-4.35"],
  select: ["m3 3 7.07 16.97 2.51-7.39 7.39-2.51L3 3Z"],
  pen: ["M17 3a2.85 2.83 0 1 1 4 4L7.5 20.5 2 22l1.5-5.5L17 3Z"],
  note: [
    "M15.5 3H5a2 2 0 0 0-2 2v14c0 1.1.9 2 2 2h14a2 2 0 0 0 2-2V8.5L15.5 3Z",
    "M15 3v6h6",
  ],
  textedit: ["M4 7V5h16v2", "M9 20h6", "M12 5v15"],
  undo: ["M3 7v6h6", "M21 17a9 9 0 0 0-15-6.7L3 13"],
  highlight: [
    "m9 11-6 6v3h9l3-3",
    "m22 12-4.6 4.6a2 2 0 0 1-2.83 0l-5.17-5.17a2 2 0 0 1 0-2.83L14 4",
  ],
  trash: [
    "M3 6h18",
    "M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6",
    "M8 6V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2",
  ],
  rotate: ["M21 3v5h-5", "M21 8a9 9 0 1 0-2.34 8.66"],
  up: ["m18 15-6-6-6 6"],
  down: ["m6 9 6 6 6-6"],
  copy: [
    "M20 8H10a2 2 0 0 0-2 2v10c0 1.1.9 2 2 2h10a2 2 0 0 0 2-2V10a2 2 0 0 0-2-2Z",
    "M4 16c-1.1 0-2-.9-2-2V4c0-1.1.9-2 2-2h10c1.1 0 2 .9 2 2",
  ],
  close: ["M18 6 6 18", "m6 6 12 12"],
  sign: ["M3 17c3-6 6-6 8 0s5 6 8 0", "M3 21h18"],
  merge: ["M14 2H6a2 2 0 0 0-2 2v16c0 1.1.9 2 2 2h12a2 2 0 0 0 2-2V8l-6-6Z", "M12 11v6", "M9 14h6"],
  extract: [
    "M14 2H6a2 2 0 0 0-2 2v16c0 1.1.9 2 2 2h12a2 2 0 0 0 2-2V8l-6-6Z",
    "M14 2v6h6",
    "M12 18v-6",
    "m9 15 3 3 3-3",
  ],
  doc: [
    "M14 2H6a2 2 0 0 0-2 2v16c0 1.1.9 2 2 2h12a2 2 0 0 0 2-2V8l-6-6Z",
    "M14 2v6h6",
  ],
  image: [
    "M19 3H5a2 2 0 0 0-2 2v14c0 1.1.9 2 2 2h14a2 2 0 0 0 2-2V5a2 2 0 0 0-2-2Z",
    "M9 9.5a1.5 1.5 0 1 0 0-3 1.5 1.5 0 0 0 0 3Z",
    "m21 15-3.09-3.09a2 2 0 0 0-2.82 0L6 21",
  ],
  sticky: ["M12 3v10", "M12 13l-3-3", "M12 13l3-3"],
  shapes: [
    "M8 3H3v5h5V3Z",
    "M17 21a4 4 0 1 0 0-8 4 4 0 0 0 0 8Z",
    "m14 4 6 6",
  ],
  stamp: [
    "M5 22h14",
    "M19.27 13.73A2.5 2.5 0 0 0 17.5 13h-11A2.5 2.5 0 0 0 4 15.5V17a1 1 0 0 0 1 1h14a1 1 0 0 0 1-1v-1.5c0-.66-.26-1.3-.73-1.77Z",
    "M14 13V8.5C14 7 15 7 15 5a3 3 0 0 0-6 0c0 2 1 2 1 3.5V13",
  ],
  underline: ["M6 4v6a6 6 0 0 0 12 0V4", "M4 20h16"],
  strike: ["M16 4H9a3 3 0 0 0-2.83 4", "M14 12a4 4 0 0 1 0 8H6", "M4 12h16"],
  crop: ["M6 2v14a2 2 0 0 0 2 2h14", "M18 22V8a2 2 0 0 0-2-2H2"],
  lock: [
    "M19 11H5a2 2 0 0 0-2 2v7a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-7a2 2 0 0 0-2-2Z",
    "M7 11V7a5 5 0 0 1 10 0v4",
  ],
  flatten: ["M12 3v12", "m8 11 4 4 4-4", "M4 21h16"],
  redact: ["M4 5h16v6H4Z", "M4 15h7", "M4 19h10"],
  printer: [
    "M6 9V3h12v6",
    "M6 18H4a2 2 0 0 1-2-2v-5a2 2 0 0 1 2-2h16a2 2 0 0 1 2 2v5a2 2 0 0 1-2 2h-2",
    "M6 14h12v8H6Z",
  ],
  shrink: ["m15 15 6 6", "m15 9 6-6", "M9 21v-6H3", "M3 9h6V3"],
  water: ["M12 22a7 7 0 0 0 7-7c0-2-1-3.9-3-5.5s-3.5-4-4-6.5c-.5 2.5-2 4.9-4 6.5C6 11.1 5 13 5 15a7 7 0 0 0 7 7Z"],
  hf: ["M3 5h18", "M3 19h18", "M7 12h10"],
};

function Icon({ name, size = 16 }: { name: string; size?: number }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.8}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden
    >
      {ICONS[name]?.map((d, i) => (
        <path key={i} d={d} />
      ))}
    </svg>
  );
}

/** Une cajas de caracteres consecutivos en rectángulos por línea. */
function mergeLineRects(boxes: CharBox[]): Rect[] {
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
function charIndexAt(pt: PageText, x: number, y: number): number | null {
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

function copyToClipboard(text: string) {
  navigator.clipboard?.writeText(text).catch(() => {
    const ta = document.createElement("textarea");
    ta.value = text;
    document.body.appendChild(ta);
    ta.select();
    document.execCommand("copy");
    ta.remove();
  });
}

function App() {
  const [originalPath, setOriginalPath] = useState<string | null>(null);
  const [workPath, setWorkPath] = useState<string | null>(null);
  const [modified, setModified] = useState(false);
  const [docVersion, setDocVersion] = useState(0);
  const [pageCount, setPageCount] = useState(0);
  const [pageIndex, setPageIndex] = useState(0);
  const [zoom, setZoom] = useState(1);
  const [imgSrc, setImgSrc] = useState<string | null>(null);
  const [thumbs, setThumbs] = useState<(string | null)[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [menuOpen, setMenuOpen] = useState(false);

  const [pageText, setPageText] = useState<PageText | null>(null);
  const [selection, setSelection] = useState<Selection | null>(null);
  const [dragging, setDragging] = useState(false);
  const anchorRef = useRef<number | null>(null);
  const pageCacheRef = useRef<Map<string, string>>(new Map());

  const [mode, setMode] = useState<Mode>("select");
  const [annots, setAnnots] = useState<AnnotationInfo[]>([]);
  const [annotVersion, setAnnotVersion] = useState(0);
  const [strokePts, setStrokePts] = useState<[number, number][]>([]);
  const [noteDraft, setNoteDraft] = useState<{
    x: number;
    y: number;
    text: string;
  } | null>(null);
  const [notePopover, setNotePopover] = useState<AnnotationInfo | null>(null);
  const undoStackRef = useRef<UndoEntry[]>([]);
  const [formFields, setFormFields] = useState<FormFieldInfo[]>([]);
  const [fieldDraft, setFieldDraft] = useState<{
    field: FormFieldInfo;
    text: string;
  } | null>(null);
  const [textBlocks, setTextBlocks] = useState<TextBlock[]>([]);
  const [blockDraft, setBlockDraft] = useState<{
    block: TextBlock;
    text: string;
  } | null>(null);
  const [newTextDraft, setNewTextDraft] = useState<{
    x: number;
    y: number;
    text: string;
    size: number;
    font: string;
  } | null>(null);
  const [images, setImages] = useState<ImageInfo[]>([]);
  const [imgPreviews, setImgPreviews] = useState<Record<number, string>>({});
  const [imgDraft, setImgDraft] = useState<ImageInfo | null>(null);
  const [imagePopover, setImagePopover] = useState<ImageInfo | null>(null);
  const imgActionRef = useRef<ImgAction | null>(null);
  const [p12Draft, setP12Draft] = useState<{
    path: string;
    password: string;
  } | null>(null);
  const [shapeKind, setShapeKind] = useState<ShapeKind>("rect");
  const [shapeColor, setShapeColor] = useState(SHAPE_COLORS[0]);
  const [shapeFill, setShapeFill] = useState(false);
  const [shapeWidth, setShapeWidth] = useState(2);
  const [shapeDraft, setShapeDraft] = useState<{
    x1: number;
    y1: number;
    x2: number;
    y2: number;
  } | null>(null);
  const shapeStartRef = useRef<{ x: number; y: number } | null>(null);
  const [stampText, setStampText] = useState(STAMP_PRESETS[0]);
  const [stampCustom, setStampCustom] = useState("");
  const [stampColor, setStampColor] = useState("#c81e1e");
  const [cropDraft, setCropDraft] = useState<Rect | null>(null);
  const cropStartRef = useRef<{ x: number; y: number } | null>(null);
  const [wmOpen, setWmOpen] = useState(false);
  const [hfOpen, setHfOpen] = useState(false);
  const [sidebarTab, setSidebarTab] = useState<"paginas" | "marcadores">(
    "paginas",
  );
  const [pwdDraft, setPwdDraft] = useState<{
    path: string;
    password: string;
  } | null>(null);
  const [protectDraft, setProtectDraft] = useState<{
    user: string;
    owner: string;
  } | null>(null);
  const [flattenAsk, setFlattenAsk] = useState(false);
  const [redactDraft, setRedactDraft] = useState<Rect | null>(null);
  const redactStartRef = useRef<{ x: number; y: number } | null>(null);
  const [redactReport, setRedactReport] = useState<RedactReport | null>(null);
  const [printPages, setPrintPages] = useState<string[] | null>(null);
  const [exportOpen, setExportOpen] = useState(false);
  const [exportFmt, setExportFmt] = useState<"png" | "jpeg">("png");
  const [exportDpi, setExportDpi] = useState(150);
  const [compressOpen, setCompressOpen] = useState(false);
  const [compressQuality, setCompressQuality] = useState(75);
  const [compressDpi, setCompressDpi] = useState(150);
  const [notice, setNotice] = useState<string | null>(null);
  const [outline, setOutlineState] = useState<OutlineNode[]>([]);
  const [links, setLinks] = useState<LinkInfo[]>([]);
  const [propsDraft, setPropsDraft] = useState<Metadata | null>(null);
  const [firmas, setFirmas] = useState<FirmaGuardada[]>([]);
  const [activeSig, setActiveSig] = useState<{
    png: string;
    ratio: number;
  } | null>(null);
  const [sigDraft, setSigDraft] = useState<Rect | null>(null);
  const [drawingSig, setDrawingSig] = useState(false);
  const sigDragRef = useRef<{ x: number; y: number } | null>(null);

  const [query, setQuery] = useState("");
  const [lastQuery, setLastQuery] = useState("");
  const [matches, setMatches] = useState<SearchMatch[]>([]);
  const [matchIdx, setMatchIdx] = useState(0);
  const [searched, setSearched] = useState(false);
  const currentHitRef = useRef<HTMLDivElement | null>(null);

  async function openPath(path: string, password?: string) {
    try {
      setError(null);
      setImgSrc(null);
      setThumbs([]);
      setMatches([]);
      setSearched(false);
      setQuery("");
      setModified(false);
      const info = await invoke<{ page_count: number; work_path: string }>(
        "open_pdf",
        { path, password: password ?? null },
      );
      setPwdDraft(null);
      setOriginalPath(path);
      setWorkPath(info.work_path);
      setPageCount(info.page_count);
      setPageIndex(0);
      setDocVersion((v) => v + 1);
    } catch (e) {
      if (String(e) === "PASSWORD_REQUIRED") {
        setPwdDraft({ path, password: "" });
        if (password !== undefined) setError("Contraseña incorrecta");
      } else {
        setError(String(e));
      }
    }
  }

  async function openFile() {
    const selected = await open({
      filters: [{ name: "PDF", extensions: ["pdf"] }],
      multiple: false,
    });
    if (typeof selected !== "string") return;
    await openPath(selected);
  }

  // Anotaciones de la página actual (para iconos de nota y popovers)
  useEffect(() => {
    if (!workPath) return;
    let cancelled = false;
    setNotePopover(null);
    invoke<AnnotationInfo[]>("get_annotations", { path: workPath, pageIndex })
      .then((a) => {
        if (!cancelled) setAnnots(a);
      })
      .catch(() => {
        if (!cancelled) setAnnots([]);
      });
    return () => {
      cancelled = true;
    };
  }, [workPath, pageIndex, docVersion, annotVersion]);

  // Campos de formulario de la página actual
  useEffect(() => {
    if (!workPath) return;
    let cancelled = false;
    setFieldDraft(null);
    invoke<FormFieldInfo[]>("get_form_fields", { path: workPath, pageIndex })
      .then((f) => {
        if (!cancelled) setFormFields(f);
      })
      .catch(() => {
        if (!cancelled) setFormFields([]);
      });
    return () => {
      cancelled = true;
    };
  }, [workPath, pageIndex, docVersion, annotVersion]);

  // Bloques de texto (solo en modo edición)
  useEffect(() => {
    setNewTextDraft(null);
    if (!workPath || mode !== "edit") {
      setTextBlocks([]);
      setBlockDraft(null);
      return;
    }
    let cancelled = false;
    invoke<TextBlock[]>("get_text_blocks", { path: workPath, pageIndex })
      .then((b) => {
        if (!cancelled) setTextBlocks(b);
      })
      .catch((e) => {
        if (!cancelled) setError(String(e));
      });
    return () => {
      cancelled = true;
    };
  }, [workPath, pageIndex, docVersion, mode]);

  // Imágenes de la página (solo en modo imagen). Se precarga también su
  // contenido para que al arrastrar se mueva la imagen, no solo el recuadro.
  useEffect(() => {
    setImagePopover(null);
    setImgDraft(null);
    imgActionRef.current = null;
    setImgPreviews({});
    if (!workPath || mode !== "image") {
      setImages([]);
      return;
    }
    let cancelled = false;
    invoke<ImageInfo[]>("get_images", { path: workPath, pageIndex })
      .then((list) => {
        if (cancelled) return;
        setImages(list);
        for (const im of list) {
          getImageData(workPath, pageIndex, im.object_index)
            .then((b64) => {
              if (!cancelled)
                setImgPreviews((p) => ({ ...p, [im.object_index]: b64 }));
            })
            .catch(() => {
              // sin vista previa: al arrastrar se verá solo el recuadro
            });
        }
      })
      .catch((e) => {
        if (!cancelled) setError(String(e));
      });
    return () => {
      cancelled = true;
    };
  }, [workPath, pageIndex, docVersion, mode]);

  // Marcadores del documento (pestaña del sidebar)
  useEffect(() => {
    if (!workPath) {
      setOutlineState([]);
      return;
    }
    let cancelled = false;
    getOutline(workPath)
      .then((o) => {
        if (!cancelled) setOutlineState(o);
      })
      .catch(() => {
        if (!cancelled) setOutlineState([]);
      });
    return () => {
      cancelled = true;
    };
  }, [workPath, docVersion]);

  // Enlaces de la página actual (zonas clicables en modo selección)
  useEffect(() => {
    if (!workPath) {
      setLinks([]);
      return;
    }
    let cancelled = false;
    getLinks(workPath, pageIndex)
      .then((l) => {
        if (!cancelled) setLinks(l);
      })
      .catch(() => {
        if (!cancelled) setLinks([]);
      });
    return () => {
      cancelled = true;
    };
  }, [workPath, pageIndex, docVersion]);

  async function persistOutline(nodes: OutlineNode[]) {
    if (!workPath) return;
    const anterior = outline;
    setOutlineState(nodes);
    try {
      await setOutline(workPath, nodes);
      setModified(true);
    } catch (e) {
      setOutlineState(anterior);
      setError(String(e));
    }
  }

  async function openProperties() {
    if (!workPath) return;
    try {
      setPropsDraft(await getMetadata(workPath));
    } catch (e) {
      setError(String(e));
    }
  }

  async function saveProperties(meta: Metadata) {
    if (!workPath) return;
    try {
      await setMetadata(workPath, meta);
      setPropsDraft(null);
      setModified(true);
    } catch (e) {
      setError(String(e));
    }
  }

  function onLinkClick(l: LinkInfo) {
    if (l.uri) {
      openUrl(l.uri).catch((e) => setError(String(e)));
    } else if (l.dest_page !== null) {
      setPageIndex(l.dest_page);
    }
  }

  // Esc sale de los modos de área (recorte y redacción)
  useEffect(() => {
    if (mode !== "crop" && mode !== "redact") return;
    function onKey(e: KeyboardEvent) {
      if (e.key === "Escape") {
        setCropDraft(null);
        setRedactDraft(null);
        setRedactReport(null);
        setMode("select");
      }
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [mode]);

  // Biblioteca de firmas al entrar en modo firma; Esc cancela el estampado
  useEffect(() => {
    if (mode !== "firmar") return;
    listStoredSignatures()
      .then(setFirmas)
      .catch((e) => setError(String(e)));
    function onKey(e: KeyboardEvent) {
      if (e.key === "Escape") {
        setActiveSig(null);
        setSigDraft(null);
        setMode("select");
      }
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [mode]);

  /** Guarda un render en el caché de páginas, con tope de entradas. */
  function cachePut(key: string, src: string) {
    const cache = pageCacheRef.current;
    cache.set(key, src);
    if (cache.size > 30) {
      const oldest = cache.keys().next().value;
      if (oldest) cache.delete(oldest);
    }
  }

  // Render de la página actual (instantáneo si ya está en caché)
  useEffect(() => {
    if (!workPath) return;
    const key = `${docVersion}:${pageIndex}:${zoom}`;
    const cached = pageCacheRef.current.get(key);
    if (cached) {
      setImgSrc(cached);
      setLoading(false);
      return;
    }
    let cancelled = false;
    setLoading(true);
    const width = Math.round(BASE_WIDTH * zoom * window.devicePixelRatio);
    invoke<string>("render_page", { path: workPath, pageIndex, width })
      .then((b64) => {
        const src = `data:image/png;base64,${b64}`;
        cachePut(key, src);
        if (!cancelled) setImgSrc(src);
      })
      .catch((e) => {
        if (!cancelled) setError(String(e));
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [workPath, pageIndex, zoom, docVersion, annotVersion]);

  // Prefetch de las páginas adyacentes cuando la actual ya está lista
  useEffect(() => {
    if (!workPath || pageCount === 0 || loading) return;
    const width = Math.round(BASE_WIDTH * zoom * window.devicePixelRatio);
    for (const p of [pageIndex + 1, pageIndex - 1]) {
      if (p < 0 || p >= pageCount) continue;
      const key = `${docVersion}:${p}:${zoom}`;
      if (pageCacheRef.current.has(key)) continue;
      cachePut(key, ""); // reserva para no pedirla dos veces
      invoke<string>("render_page", { path: workPath, pageIndex: p, width })
        .then((b64) => cachePut(key, `data:image/png;base64,${b64}`))
        .catch(() => pageCacheRef.current.delete(key));
    }
  }, [workPath, pageIndex, zoom, docVersion, annotVersion, pageCount, loading]);

  // Capa de texto de la página actual
  useEffect(() => {
    if (!workPath) return;
    let cancelled = false;
    setPageText(null);
    setSelection(null);
    invoke<PageText>("get_page_text", { path: workPath, pageIndex })
      .then((t) => {
        if (!cancelled) setPageText(t);
      })
      .catch((e) => {
        if (!cancelled) setError(String(e));
      });
    return () => {
      cancelled = true;
    };
  }, [workPath, pageIndex, docVersion]);

  // Miniaturas de la barra lateral (secuencial, en segundo plano)
  useEffect(() => {
    if (!workPath || pageCount === 0) return;
    let cancelled = false;
    setThumbs(Array(pageCount).fill(null));
    (async () => {
      for (let i = 0; i < pageCount; i++) {
        if (cancelled) return;
        try {
          const b64 = await invoke<string>("render_page", {
            path: workPath,
            pageIndex: i,
            width: THUMB_WIDTH,
          });
          if (cancelled) return;
          setThumbs((t) => {
            const next = [...t];
            next[i] = `data:image/png;base64,${b64}`;
            return next;
          });
        } catch {
          // miniatura fallida: se queda el placeholder
        }
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [workPath, pageCount, docVersion]);

  function copySelection() {
    if (!selection || !pageText) return;
    const text = pageText.chars
      .slice(selection.start, selection.end + 1)
      .map((c) => c.ch)
      .join("");
    copyToClipboard(text);
  }

  // Copiar selección con ⌘C / Ctrl+C
  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      if ((e.metaKey || e.ctrlKey) && e.key === "c" && selection && pageText) {
        copySelection();
        e.preventDefault();
      }
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [selection, pageText]);

  /** Refresca solo la miniatura de una página (tras anotar). */
  async function refreshThumb(page: number) {
    if (!workPath) return;
    try {
      const b64 = await invoke<string>("render_page", {
        path: workPath,
        pageIndex: page,
        width: THUMB_WIDTH,
      });
      setThumbs((t) => {
        const next = [...t];
        next[page] = `data:image/png;base64,${b64}`;
        return next;
      });
    } catch {
      // la miniatura vieja sigue siendo razonable
    }
  }

  /** Tras anotar: invalidar el render de esa página sin recargar todo. */
  function afterAnnotate(page: number, pushUndo = true) {
    if (pushUndo) undoStackRef.current.push({ page });
    setModified(true);
    for (const key of [...pageCacheRef.current.keys()]) {
      if (key.split(":")[1] === String(page)) pageCacheRef.current.delete(key);
    }
    setAnnotVersion((v) => v + 1);
    refreshThumb(page);
  }

  /** Resalta, subraya o tacha la selección actual. */
  async function markupSelection(kind: "highlight" | "underline" | "strikeout") {
    if (!workPath || !selection || !pageText) return;
    const rects = mergeLineRects(
      pageText.chars.slice(selection.start, selection.end + 1),
    );
    if (rects.length === 0) return;
    try {
      await addMarkup({ workPath, pageIndex, rects, kind });
      setSelection(null);
      afterAnnotate(pageIndex);
    } catch (e) {
      setError(String(e));
    }
  }

  async function commitShape(d: {
    x1: number;
    y1: number;
    x2: number;
    y2: number;
  }) {
    if (!workPath) return;
    const fillable = shapeKind === "rect" || shapeKind === "ellipse";
    try {
      await addShape({
        workPath,
        pageIndex,
        kind: shapeKind,
        x1: d.x1,
        y1: d.y1,
        x2: d.x2,
        y2: d.y2,
        stroke: hexToRgba(shapeColor),
        fill: shapeFill && fillable ? hexToRgba(shapeColor, 70) : null,
        strokeWidth: shapeWidth,
      });
      afterAnnotate(pageIndex);
    } catch (e) {
      setError(String(e));
    }
  }

  async function placeStamp(x: number, y: number) {
    const text = stampText === "custom" ? stampCustom.trim() : stampText;
    if (!workPath || !text) return;
    try {
      await addStamp({
        workPath,
        pageIndex,
        text,
        color: hexToRgba(stampColor),
        x,
        y,
        fontSize: 22,
      });
      afterAnnotate(pageIndex);
    } catch (e) {
      setError(String(e));
    }
  }

  async function submitNote() {
    if (!workPath || !noteDraft || !noteDraft.text.trim()) {
      setNoteDraft(null);
      return;
    }
    try {
      await invoke("add_note", {
        workPath,
        pageIndex,
        x: noteDraft.x,
        y: noteDraft.y,
        text: noteDraft.text,
      });
      setNoteDraft(null);
      setMode("select");
      afterAnnotate(pageIndex);
    } catch (e) {
      setError(String(e));
    }
  }

  async function finishStroke() {
    const pts = strokePts;
    setStrokePts([]);
    if (!workPath || pts.length < 2) return;
    try {
      await invoke("add_stroke", { workPath, pageIndex, points: pts });
      afterAnnotate(pageIndex);
    } catch (e) {
      setError(String(e));
    }
  }

  async function undoAnnotation() {
    const action = undoStackRef.current.pop();
    if (!action || !workPath) return;
    try {
      const list = await invoke<AnnotationInfo[]>("get_annotations", {
        path: workPath,
        pageIndex: action.page,
      });
      if (list.length === 0) return;
      await invoke("remove_annotation", {
        workPath,
        pageIndex: action.page,
        annotIndex: list[list.length - 1].index,
      });
      afterAnnotate(action.page, false);
    } catch (e) {
      setError(String(e));
    }
  }

  async function deleteAnnotation(annot: AnnotationInfo) {
    if (!workPath) return;
    try {
      await invoke("remove_annotation", {
        workPath,
        pageIndex,
        annotIndex: annot.index,
      });
      setNotePopover(null);
      afterAnnotate(pageIndex, false);
    } catch (e) {
      setError(String(e));
    }
  }

  /** Clic simple en modo selección: abre el popover de la anotación pulsada. */
  function onClickLayer(e: React.MouseEvent<HTMLDivElement>) {
    if (mode !== "select" || selection) return;
    const { x, y } = pagePoint(e);
    const CLICKABLE = [
      "Highlight",
      "Underline",
      "Strikeout",
      "StrikeOut",
      "Ink",
      "Stamp",
    ];
    const hit = annots.find((a) => {
      if (!CLICKABLE.includes(a.kind)) return false;
      const zonas =
        a.rects.length > 0
          ? a.rects
          : [{ x: a.x, y: a.y, w: a.w, h: a.h }];
      return zonas.some(
        (r) => x >= r.x && x <= r.x + r.w && y >= r.y && y <= r.y + r.h,
      );
    });
    if (hit) setNotePopover(hit);
  }

  async function insertImageAt(x: number, y: number) {
    if (!workPath) return;
    const sel = await open({
      filters: [
        {
          name: "Imagen",
          extensions: ["png", "jpg", "jpeg", "webp", "gif", "bmp"],
        },
      ],
      multiple: false,
      title: "Insertar imagen",
    });
    if (typeof sel !== "string") return;
    try {
      await invoke("add_image", { workPath, pageIndex, imagePath: sel, x, y });
      afterMutation(pageCount);
    } catch (e) {
      setError(String(e));
    }
  }

  async function commitImage(objectIndex: number, b: ImageInfo) {
    if (!workPath) return;
    try {
      await invoke("transform_image", {
        workPath,
        pageIndex,
        objectIndex,
        x: b.x,
        y: b.y,
        w: b.w,
        h: b.h,
      });
      afterMutation(pageCount);
    } catch (e) {
      setError(String(e));
    }
  }

  async function replaceImagePick(im: ImageInfo) {
    if (!workPath) return;
    const sel = await open({
      filters: [
        {
          name: "Imagen",
          extensions: ["png", "jpg", "jpeg", "webp", "gif", "bmp"],
        },
      ],
      multiple: false,
      title: "Imagen de reemplazo",
    });
    if (typeof sel !== "string") return;
    try {
      await invoke("replace_image", {
        workPath,
        pageIndex,
        objectIndex: im.object_index,
        imagePath: sel,
      });
      setImagePopover(null);
      afterMutation(pageCount);
    } catch (e) {
      setError(String(e));
    }
  }

  async function deleteImage(im: ImageInfo) {
    if (!workPath) return;
    try {
      await invoke("delete_image", {
        workPath,
        pageIndex,
        objectIndex: im.object_index,
      });
      setImagePopover(null);
      afterMutation(pageCount);
    } catch (e) {
      setError(String(e));
    }
  }

  function startImgAction(
    e: React.MouseEvent<HTMLDivElement>,
    im: ImageInfo,
    kind: ImgAction["kind"],
  ) {
    e.stopPropagation();
    if (e.button !== 0) return;
    const rect = (
      e.currentTarget.closest(".textlayer") as HTMLElement
    ).getBoundingClientRect();
    imgActionRef.current = {
      kind,
      startX: (e.clientX - rect.left) / scale,
      startY: (e.clientY - rect.top) / scale,
      orig: im,
      moved: false,
    };
    setImagePopover(null);
    setImgDraft(im);
  }

  async function submitNewText() {
    if (!workPath || !newTextDraft) return;
    if (!newTextDraft.text.trim()) {
      setNewTextDraft(null);
      return;
    }
    try {
      await invoke("add_text_block", {
        workPath,
        pageIndex,
        x: newTextDraft.x,
        y: newTextDraft.y,
        text: newTextDraft.text,
        fontSize: newTextDraft.size,
        font: newTextDraft.font === "auto" ? null : newTextDraft.font,
      });
      setNewTextDraft(null);
      afterMutation(pageCount);
    } catch (e) {
      setError(String(e));
    }
  }

  async function submitBlockDraft() {
    if (!workPath || !blockDraft) return;
    try {
      await invoke("edit_text_block", {
        workPath,
        pageIndex,
        objectIndex: blockDraft.block.object_index,
        newText: blockDraft.text,
      });
      setBlockDraft(null);
      afterMutation(pageCount);
    } catch (e) {
      setError(String(e));
    }
  }

  async function deleteBlock() {
    if (!workPath || !blockDraft) return;
    try {
      await invoke("delete_text_block", {
        workPath,
        pageIndex,
        objectIndex: blockDraft.block.object_index,
      });
      setBlockDraft(null);
      afterMutation(pageCount);
    } catch (e) {
      setError(String(e));
    }
  }

  async function submitFieldDraft() {
    if (!workPath || !fieldDraft) return;
    try {
      await invoke("set_form_text", {
        workPath,
        pageIndex,
        annotIndex: fieldDraft.field.annot_index,
        value: fieldDraft.text,
      });
      setFieldDraft(null);
      afterAnnotate(pageIndex, false);
    } catch (e) {
      setError(String(e));
    }
  }

  async function toggleFormCheck(field: FormFieldInfo) {
    if (!workPath) return;
    try {
      await invoke("set_form_checked", {
        workPath,
        pageIndex,
        annotIndex: field.annot_index,
        checked: !field.checked,
      });
      afterAnnotate(pageIndex, false);
    } catch (e) {
      setError(String(e));
    }
  }

  function onFieldClick(field: FormFieldInfo) {
    if (field.kind === "Text") {
      setFieldDraft({ field, text: field.value });
    } else if (field.kind === "Checkbox" || field.kind === "RadioButton") {
      toggleFormCheck(field);
    }
  }

  /** Tras mutar el documento: refrescar render, miniaturas y limpiar búsqueda. */
  function afterMutation(newCount: number, nextPage?: number) {
    setPageCount(newCount);
    setModified(true);
    setMatches([]);
    setSearched(false);
    setLastQuery("");
    setPageIndex((p) => Math.max(0, Math.min(nextPage ?? p, newCount - 1)));
    setDocVersion((v) => v + 1);
  }

  async function rotatePage(i: number) {
    if (!workPath) return;
    try {
      await invoke("rotate_page", { workPath, pageIndex: i });
      afterMutation(pageCount);
    } catch (e) {
      setError(String(e));
    }
  }

  async function deletePage(i: number) {
    if (!workPath || pageCount <= 1) return;
    try {
      const count = await invoke<number>("delete_page", {
        workPath,
        pageIndex: i,
      });
      afterMutation(count);
    } catch (e) {
      setError(String(e));
    }
  }

  async function movePage(from: number, to: number) {
    if (!workPath || to < 0 || to >= pageCount) return;
    try {
      await invoke("move_page", { workPath, fromIndex: from, toIndex: to });
      afterMutation(pageCount, from === pageIndex ? to : undefined);
    } catch (e) {
      setError(String(e));
    }
  }

  async function blankPageAfter(i: number) {
    if (!workPath) return;
    try {
      const count = await addBlankPage(workPath, i + 1);
      afterMutation(count);
    } catch (e) {
      setError(String(e));
    }
  }

  async function duplicatePageAt(i: number) {
    if (!workPath) return;
    try {
      const count = await duplicatePage(workPath, i);
      afterMutation(count);
    } catch (e) {
      setError(String(e));
    }
  }

  async function insertPdfHere() {
    if (!workPath) return;
    const selected = await open({
      filters: [{ name: "PDF", extensions: ["pdf"] }],
      multiple: false,
      title: "Insertar PDF después de la página actual",
    });
    if (typeof selected !== "string") return;
    try {
      const count = await insertPdfAt(workPath, selected, pageIndex + 1);
      afterMutation(count);
    } catch (e) {
      setError(String(e));
    }
  }

  async function applyCrop(allPages: boolean) {
    if (!workPath || !cropDraft) return;
    try {
      await cropPage(workPath, pageIndex, cropDraft, allPages);
      setCropDraft(null);
      setMode("select");
      afterMutation(pageCount);
    } catch (e) {
      setError(String(e));
    }
  }

  async function applyWatermark(opts: {
    text: string;
    fontSize: number;
    color: string;
    opacity: number;
    diagonal: boolean;
  }) {
    if (!workPath) return;
    try {
      await addWatermark({
        workPath,
        text: opts.text,
        fontSize: opts.fontSize,
        color: hexToRgba(opts.color, Math.round((opts.opacity / 100) * 255)),
        diagonal: opts.diagonal,
      });
      setWmOpen(false);
      afterMutation(pageCount);
    } catch (e) {
      setError(String(e));
    }
  }

  async function applyHeaderFooter(zonas: HeaderFooter, fontSize: number) {
    if (!workPath) return;
    try {
      await addHeaderFooter(workPath, zonas, fontSize);
      setHfOpen(false);
      afterMutation(pageCount);
    } catch (e) {
      setError(String(e));
    }
  }

  async function applyProtect() {
    if (!workPath || !protectDraft || !protectDraft.user) return;
    const dest = await save({
      filters: [{ name: "PDF", extensions: ["pdf"] }],
      defaultPath: (originalPath ?? "documento.pdf").replace(
        /\.pdf$/i,
        "-protegido.pdf",
      ),
      title: "Guardar PDF protegido",
    });
    if (!dest) return;
    try {
      await encryptPdf({
        workPath,
        destPath: dest,
        userPassword: protectDraft.user,
        ownerPassword: protectDraft.owner || null,
      });
      setProtectDraft(null);
    } catch (e) {
      setError(String(e));
    }
  }

  async function applyFlatten() {
    if (!workPath) return;
    try {
      await flattenPdf(workPath);
      setFlattenAsk(false);
      afterMutation(pageCount);
    } catch (e) {
      setError(String(e));
    }
  }

  async function previewRedact(r: Rect) {
    if (!workPath) return;
    try {
      setRedactReport(await redactArea(workPath, pageIndex, r, true));
    } catch (e) {
      setError(String(e));
    }
  }

  async function applyRedact() {
    if (!workPath || !redactDraft) return;
    try {
      await redactArea(workPath, pageIndex, redactDraft, false);
      setRedactDraft(null);
      setRedactReport(null);
      setMode("select");
      afterMutation(pageCount);
    } catch (e) {
      setError(String(e));
    }
  }

  async function printDocument() {
    if (!workPath) return;
    try {
      setNotice("Preparando la impresión…");
      const pages: string[] = [];
      for (let i = 0; i < pageCount; i++) {
        const width = Math.round(((pageText?.width ?? 595) * 200) / 72);
        const b64 = await invoke<string>("render_page", {
          path: workPath,
          pageIndex: i,
          width,
        });
        pages.push(`data:image/png;base64,${b64}`);
      }
      setNotice(null);
      setPrintPages(pages);
    } catch (e) {
      setNotice(null);
      setError(String(e));
    }
  }

  // cuando las páginas de impresión están montadas, abrir el diálogo
  useEffect(() => {
    if (!printPages) return;
    const t = setTimeout(() => {
      try {
        window.print();
      } catch (e) {
        setError(String(e));
      }
      setPrintPages(null);
    }, 200);
    return () => clearTimeout(t);
  }, [printPages]);

  async function exportImages() {
    if (!workPath) return;
    const dir = await open({
      directory: true,
      multiple: false,
      title: "Carpeta para las imágenes",
    });
    if (typeof dir !== "string") return;
    try {
      setExportOpen(false);
      setNotice("Exportando imágenes…");
      const rutas = await exportPagesPng(workPath, dir, exportDpi, exportFmt);
      setNotice(`${rutas.length} imagen(es) exportadas a ${dir}`);
    } catch (e) {
      setNotice(null);
      setError(String(e));
    }
  }

  async function exportPlainText() {
    if (!workPath) return;
    const dest = await save({
      filters: [{ name: "Texto", extensions: ["txt"] }],
      defaultPath: (originalPath ?? "documento.pdf").replace(
        /\.pdf$/i,
        ".txt",
      ),
      title: "Exportar texto plano",
    });
    if (!dest) return;
    try {
      await exportText(workPath, dest);
      setNotice(`Texto exportado a ${dest}`);
    } catch (e) {
      setError(String(e));
    }
  }

  async function applyCompress() {
    if (!workPath) return;
    try {
      setCompressOpen(false);
      setNotice("Comprimiendo…");
      const r = await compressPdf(workPath, compressQuality, compressDpi);
      const mb = (n: number) => (n / 1024 / 1024).toFixed(2);
      setNotice(
        r.imagenes === 0
          ? "No había imágenes que comprimir."
          : `${r.imagenes} imagen(es) recomprimidas: ${mb(r.antes)} MB → ${mb(r.despues)} MB`,
      );
      afterMutation(pageCount);
    } catch (e) {
      setNotice(null);
      setError(String(e));
    }
  }

  async function addPdf() {
    if (!workPath) return;
    const selected = await open({
      filters: [{ name: "PDF", extensions: ["pdf"] }],
      multiple: false,
      title: "Añadir páginas de otro PDF",
    });
    if (typeof selected !== "string") return;
    try {
      const count = await invoke<number>("merge_pdf", {
        workPath,
        otherPath: selected,
      });
      afterMutation(count);
    } catch (e) {
      setError(String(e));
    }
  }

  async function extractCurrentPage() {
    if (!workPath) return;
    const dest = await save({
      filters: [{ name: "PDF", extensions: ["pdf"] }],
      defaultPath: `pagina-${pageIndex + 1}.pdf`,
      title: "Extraer página a un PDF nuevo",
    });
    if (!dest) return;
    try {
      await invoke("extract_pages", {
        workPath,
        pageIndices: [pageIndex],
        destPath: dest,
      });
    } catch (e) {
      setError(String(e));
    }
  }

  async function saveFile() {
    if (!workPath || !originalPath) return;
    try {
      await invoke("save_pdf", { workPath, destPath: originalPath });
      setModified(false);
    } catch (e) {
      setError(String(e));
    }
  }

  async function saveFileAs() {
    if (!workPath) return;
    const dest = await save({
      filters: [{ name: "PDF", extensions: ["pdf"] }],
      defaultPath: originalPath ?? "documento.pdf",
      title: "Guardar como",
    });
    if (!dest) return;
    try {
      await invoke("save_pdf", { workPath, destPath: dest });
      setOriginalPath(dest);
      setModified(false);
    } catch (e) {
      setError(String(e));
    }
  }

  async function pickSignedDest(): Promise<string | null> {
    return await save({
      filters: [{ name: "PDF", extensions: ["pdf"] }],
      defaultPath: (originalPath ?? "documento.pdf").replace(
        /\.pdf$/i,
        "-firmado.pdf",
      ),
      title: "Guardar PDF firmado",
    });
  }

  async function signPdf() {
    if (!workPath) return;
    const certPath = await open({
      filters: [
        {
          name: "Certificado (PEM o PKCS#12)",
          extensions: ["pem", "crt", "cer", "p12", "pfx"],
        },
      ],
      multiple: false,
      title: "Certificado (PEM) o contenedor .p12/.pfx",
    });
    if (typeof certPath !== "string") return;
    if (/\.(p12|pfx)$/i.test(certPath)) {
      setP12Draft({ path: certPath, password: "" });
      return;
    }
    const keyPath = await open({
      filters: [{ name: "Clave privada PEM", extensions: ["pem", "key"] }],
      multiple: false,
      title: "Clave privada (PEM, RSA sin cifrar)",
    });
    if (typeof keyPath !== "string") return;
    const dest = await pickSignedDest();
    if (!dest) return;
    try {
      await invoke("sign_pdf", {
        workPath,
        destPath: dest,
        certPemPath: certPath,
        keyPemPath: keyPath,
        reason: null,
      });
    } catch (e) {
      setError(String(e));
    }
  }

  async function signWithP12() {
    if (!workPath || !p12Draft) return;
    const dest = await pickSignedDest();
    if (!dest) return;
    try {
      await invoke("sign_pdf_p12", {
        workPath,
        destPath: dest,
        p12Path: p12Draft.path,
        password: p12Draft.password,
        reason: null,
      });
      setP12Draft(null);
    } catch (e) {
      setError(String(e));
    }
  }

  /** Activa una firma para estamparla (guarda su relación de aspecto). */
  function pickSignature(f: FirmaGuardada) {
    const img = new Image();
    img.onload = () =>
      setActiveSig({
        png: f.png_base64,
        ratio: img.height / Math.max(1, img.width),
      });
    img.src = `data:image/png;base64,${f.png_base64}`;
  }

  async function uploadSignature() {
    const sel = await open({
      filters: [
        {
          name: "Imagen",
          extensions: ["png", "jpg", "jpeg", "webp", "gif", "bmp"],
        },
      ],
      multiple: false,
      title: "Imagen de tu firma (PNG con transparencia funciona mejor)",
    });
    if (typeof sel !== "string") return;
    try {
      const f = await importSignatureFile(sel);
      setFirmas((l) => [f, ...l]);
      pickSignature(f);
    } catch (e) {
      setError(String(e));
    }
  }

  async function saveDrawnSignature(name: string, png: string) {
    try {
      const f = await saveStoredSignature(name, png);
      setDrawingSig(false);
      setFirmas((l) => [f, ...l]);
      pickSignature(f);
    } catch (e) {
      setError(String(e));
    }
  }

  async function removeSignature(id: string) {
    try {
      await deleteStoredSignature(id);
      setFirmas((l) => l.filter((f) => f.id !== id));
    } catch (e) {
      setError(String(e));
    }
  }

  async function stampActiveSignature(r: Rect) {
    if (!workPath || !activeSig) return;
    try {
      await stampSignature({
        workPath,
        pageIndex,
        pngBase64: activeSig.png,
        x: r.x,
        y: r.y,
        w: r.w,
        h: r.h,
      });
      setActiveSig(null);
      setSigDraft(null);
      // la firma estampada es una imagen: el modo imagen permite moverla,
      // redimensionarla o borrarla al instante
      setMode("image");
      afterMutation(pageCount);
    } catch (e) {
      setError(String(e));
    }
  }

  async function runSearch() {
    if (!workPath) return;
    if (!query.trim()) {
      setMatches([]);
      setSearched(false);
      setLastQuery("");
      return;
    }
    try {
      const res = await invoke<SearchMatch[]>("search_pdf", {
        path: workPath,
        query,
      });
      setMatches(res);
      setMatchIdx(0);
      setSearched(true);
      setLastQuery(query);
      if (res.length > 0) setPageIndex(res[0].page_index);
    } catch (e) {
      setError(String(e));
    }
  }

  function gotoMatch(delta: number) {
    if (matches.length === 0) return;
    const next = (matchIdx + delta + matches.length) % matches.length;
    setMatchIdx(next);
    setPageIndex(matches[next].page_index);
  }

  // Centrar el visor en la coincidencia actual. Depende también de imgSrc y
  // pageText para re-centrar cuando termina el render de una página nueva y
  // cuando cambia la escala (pageText fija el scale real).
  useEffect(() => {
    if (matches.length === 0) return;
    currentHitRef.current?.scrollIntoView({ block: "center", inline: "center" });
  }, [matchIdx, matches, pageIndex, imgSrc, pageText]);

  const displayWidth = BASE_WIDTH * zoom;
  const scale = pageText ? displayWidth / pageText.width : 1;

  function pagePoint(e: React.MouseEvent<HTMLDivElement>) {
    const rect = e.currentTarget.getBoundingClientRect();
    return {
      x: (e.clientX - rect.left) / scale,
      y: (e.clientY - rect.top) / scale,
    };
  }

  function onMouseDown(e: React.MouseEvent<HTMLDivElement>) {
    if (e.button !== 0) return;
    const { x, y } = pagePoint(e);
    if (mode === "draw") {
      setStrokePts([[x, y]]);
      return;
    }
    if (mode === "note") {
      setNoteDraft({ x, y, text: "" });
      return;
    }
    if (mode === "edit") {
      // clic en zona libre: añadir texto nuevo ahí (los bloques existentes
      // capturan su propio clic con stopPropagation)
      setBlockDraft(null);
      setNewTextDraft({ x, y, text: "", size: 12, font: "auto" });
      return;
    }
    if (mode === "image") {
      // clic en zona libre: insertar imagen ahí (las cajas de imagen
      // capturan su propio mousedown con stopPropagation)
      setImagePopover(null);
      insertImageAt(x, y);
      return;
    }
    if (mode === "firmar") {
      if (!activeSig) return;
      sigDragRef.current = { x, y };
      setSigDraft(null);
      return;
    }
    if (mode === "shape") {
      shapeStartRef.current = { x, y };
      setShapeDraft(null);
      return;
    }
    if (mode === "stamp") {
      placeStamp(x, y);
      return;
    }
    if (mode === "crop") {
      cropStartRef.current = { x, y };
      setCropDraft(null);
      return;
    }
    if (mode === "redact") {
      redactStartRef.current = { x, y };
      setRedactDraft(null);
      setRedactReport(null);
      return;
    }
    if (!pageText) return;
    anchorRef.current = charIndexAt(pageText, x, y);
    setDragging(true);
    setSelection(null);
    setNotePopover(null);
  }

  function onMouseMove(e: React.MouseEvent<HTMLDivElement>) {
    if (!(e.buttons & 1)) return;
    if (mode === "image" && imgActionRef.current) {
      const a = imgActionRef.current;
      const { x, y } = pagePoint(e);
      const dx = x - a.startX;
      const dy = y - a.startY;
      if (Math.abs(dx) + Math.abs(dy) > 1) a.moved = true;
      if (a.kind === "move") {
        setImgDraft({ ...a.orig, x: a.orig.x + dx, y: a.orig.y + dy });
      } else {
        let w = Math.max(8, a.orig.w + dx);
        let h = Math.max(8, a.orig.h + dy);
        if (!e.shiftKey && a.orig.w > 0) {
          h = w * (a.orig.h / a.orig.w);
        }
        setImgDraft({ ...a.orig, w, h });
      }
      return;
    }
    if (mode === "draw") {
      if (strokePts.length === 0) return;
      const { x, y } = pagePoint(e);
      setStrokePts((pts) => [...pts, [x, y]]);
      return;
    }
    if (mode === "firmar") {
      const start = sigDragRef.current;
      if (!activeSig || !start) return;
      const { x, y } = pagePoint(e);
      const w = Math.abs(x - start.x);
      if (w < 4) return;
      const h = w * activeSig.ratio;
      setSigDraft({
        x: Math.min(x, start.x),
        y: y >= start.y ? start.y : start.y - h,
        w,
        h,
      });
      return;
    }
    if (mode === "shape") {
      const start = shapeStartRef.current;
      if (!start) return;
      const { x, y } = pagePoint(e);
      setShapeDraft({ x1: start.x, y1: start.y, x2: x, y2: y });
      return;
    }
    if (mode === "crop") {
      const start = cropStartRef.current;
      if (!start) return;
      const { x, y } = pagePoint(e);
      setCropDraft({
        x: Math.min(x, start.x),
        y: Math.min(y, start.y),
        w: Math.abs(x - start.x),
        h: Math.abs(y - start.y),
      });
      return;
    }
    if (mode === "redact") {
      const start = redactStartRef.current;
      if (!start) return;
      const { x, y } = pagePoint(e);
      setRedactDraft({
        x: Math.min(x, start.x),
        y: Math.min(y, start.y),
        w: Math.abs(x - start.x),
        h: Math.abs(y - start.y),
      });
      return;
    }
    if (mode !== "select" || !pageText || anchorRef.current === null) return;
    const { x, y } = pagePoint(e);
    const idx = charIndexAt(pageText, x, y);
    if (idx === null) return;
    const a = anchorRef.current;
    setSelection({ start: Math.min(a, idx), end: Math.max(a, idx) });
  }

  function onMouseUp() {
    anchorRef.current = null;
    setDragging(false);
    if (mode === "firmar") {
      const start = sigDragRef.current;
      sigDragRef.current = null;
      if (!activeSig || !start || !pageText) return;
      const draft = sigDraft;
      setSigDraft(null);
      let r: Rect;
      if (draft && draft.w > 12) {
        r = draft;
      } else {
        // clic simple: tamaño por defecto centrado en el punto
        const w = Math.min(180, pageText.width * 0.5);
        const h = w * activeSig.ratio;
        r = { x: start.x - w / 2, y: start.y - h / 2, w, h };
      }
      r.x = Math.max(0, Math.min(r.x, pageText.width - r.w));
      r.y = Math.max(0, Math.min(r.y, pageText.height - r.h));
      stampActiveSignature(r);
      return;
    }
    if (mode === "shape") {
      shapeStartRef.current = null;
      const d = shapeDraft;
      setShapeDraft(null);
      if (d && Math.abs(d.x2 - d.x1) + Math.abs(d.y2 - d.y1) > 4) {
        commitShape(d);
      }
      return;
    }
    if (mode === "crop") {
      cropStartRef.current = null;
      // el borrador se queda visible; se confirma con los botones
      return;
    }
    if (mode === "redact") {
      redactStartRef.current = null;
      if (redactDraft && redactDraft.w > 6 && redactDraft.h > 6) {
        previewRedact(redactDraft);
      }
      return;
    }
    if (mode === "image" && imgActionRef.current) {
      const a = imgActionRef.current;
      imgActionRef.current = null;
      const draft = imgDraft;
      setImgDraft(null);
      if (!a.moved) {
        // clic simple: abrir el popover de la imagen
        setImagePopover(a.orig);
      } else if (draft) {
        commitImage(a.orig.object_index, draft);
      }
      return;
    }
    if (mode === "draw" && strokePts.length > 0) finishStroke();
  }

  const selectionRects =
    selection && pageText
      ? mergeLineRects(pageText.chars.slice(selection.start, selection.end + 1))
      : [];
  const lastSelRect =
    selectionRects.length > 0
      ? selectionRects[selectionRects.length - 1]
      : null;

  // separador de ruta multiplataforma (macOS "/" y Windows "\")
  const fileName = originalPath?.split(/[\\/]/).pop() ?? null;

  const MODES: { id: Mode; icon: string; label: string; hint: string }[] = [
    { id: "select", icon: "select", label: "Seleccionar", hint: "Seleccionar texto" },
    { id: "draw", icon: "pen", label: "Dibujar", hint: "Dibujar a mano alzada" },
    { id: "note", icon: "note", label: "Nota", hint: "Añadir una nota (clic en la página)" },
    {
      id: "edit",
      icon: "textedit",
      label: "Editar",
      hint: "Editar un bloque de texto o añadir texto nuevo (clic en zona libre)",
    },
    {
      id: "image",
      icon: "image",
      label: "Imagen",
      hint: "Insertar imágenes (clic en zona libre) o editar las existentes (arrastrar mueve, tirador redimensiona, clic abre opciones)",
    },
    {
      id: "shape",
      icon: "shapes",
      label: "Formas",
      hint: "Dibujar rectángulos, elipses, líneas y flechas (arrastra en la página)",
    },
    {
      id: "stamp",
      icon: "stamp",
      label: "Sello",
      hint: "Estampar un sello (APROBADO, BORRADOR…) con un clic",
    },
    {
      id: "firmar",
      icon: "sign",
      label: "Firma",
      hint: "Estampar tu firma manuscrita: elige o crea una y haz clic (o arrastra) donde quieras colocarla",
    },
  ];

  function selectMode(m: Mode) {
    setMode((cur) => (cur === m ? "select" : m));
    setSelection(null);
    setStrokePts([]);
    setNoteDraft(null);
    setActiveSig(null);
    setSigDraft(null);
    sigDragRef.current = null;
    setShapeDraft(null);
    shapeStartRef.current = null;
    setCropDraft(null);
    cropStartRef.current = null;
    setRedactDraft(null);
    setRedactReport(null);
    redactStartRef.current = null;
  }

  return (
    <div className="app">
      <header className="toolbar">
        <div className="toolbar-left">
          <button className="btn" onClick={openFile}>
            <Icon name="open" />
            Abrir
          </button>
          {fileName && (
            <span className="filename" title={originalPath ?? undefined}>
              {fileName}
              {modified ? " •" : ""}
            </span>
          )}
          {loading && <span className="status">Renderizando…</span>}
        </div>

        {pageCount > 0 && (
          <div className="segmented">
            {MODES.map((m) => (
              <button
                key={m.id}
                className={`btn${mode === m.id ? " on" : ""}`}
                title={m.hint}
                onClick={() => selectMode(m.id)}
              >
                <Icon name={m.icon} size={14} />
                {m.label}
              </button>
            ))}
          </div>
        )}

        <div className="toolbar-right">
          {pageCount > 0 && (
            <>
              <div className="search">
                <Icon name="search" size={13} />
                <input
                  type="text"
                  placeholder="Buscar"
                  value={query}
                  onChange={(e) => setQuery(e.target.value)}
                  onKeyDown={(e) => {
                    if (e.key !== "Enter") return;
                    if (searched && matches.length > 0 && query === lastQuery) {
                      gotoMatch(e.shiftKey ? -1 : 1);
                    } else {
                      runSearch();
                    }
                  }}
                />
                {searched && (
                  <>
                    <span className="match-count">
                      {matches.length > 0
                        ? `${matchIdx + 1}/${matches.length}`
                        : "0"}
                    </span>
                    {matches.length > 0 && (
                      <>
                        <button className="btn btn-icon" onClick={() => gotoMatch(-1)}>
                          <Icon name="up" size={13} />
                        </button>
                        <button className="btn btn-icon" onClick={() => gotoMatch(1)}>
                          <Icon name="down" size={13} />
                        </button>
                      </>
                    )}
                  </>
                )}
              </div>
              <button
                className="btn btn-icon"
                title="Deshacer la última anotación"
                onClick={undoAnnotation}
              >
                <Icon name="undo" />
              </button>
              <button
                className="btn btn-primary"
                disabled={!modified}
                onClick={saveFile}
              >
                <Icon name="save" size={14} />
                Guardar
              </button>
              <div className="menu-wrap">
                <button
                  className="btn btn-icon"
                  title="Más acciones"
                  onClick={() => setMenuOpen((o) => !o)}
                >
                  ⋯
                </button>
                {menuOpen && (
                  <>
                    <div
                      className="menu-backdrop"
                      onClick={() => setMenuOpen(false)}
                    />
                    <div className="menu">
                      <button
                        className="btn"
                        onClick={() => {
                          setMenuOpen(false);
                          saveFileAs();
                        }}
                      >
                        <Icon name="save" size={14} />
                        Guardar como…
                      </button>
                      <button
                        className="btn"
                        onClick={() => {
                          setMenuOpen(false);
                          signPdf();
                        }}
                      >
                        <Icon name="sign" size={14} />
                        Firma digital (certificado)…
                      </button>
                      <button
                        className="btn"
                        onClick={() => {
                          setMenuOpen(false);
                          addPdf();
                        }}
                      >
                        <Icon name="merge" size={14} />
                        Añadir PDF…
                      </button>
                      <button
                        className="btn"
                        onClick={() => {
                          setMenuOpen(false);
                          extractCurrentPage();
                        }}
                      >
                        <Icon name="extract" size={14} />
                        Extraer página…
                      </button>
                      <button
                        className="btn"
                        onClick={() => {
                          setMenuOpen(false);
                          insertPdfHere();
                        }}
                      >
                        <Icon name="merge" size={14} />
                        Insertar PDF aquí…
                      </button>
                      <button
                        className="btn"
                        onClick={() => {
                          setMenuOpen(false);
                          selectMode("select");
                          setMode("crop");
                        }}
                      >
                        <Icon name="crop" size={14} />
                        Recortar página…
                      </button>
                      <button
                        className="btn"
                        onClick={() => {
                          setMenuOpen(false);
                          setWmOpen(true);
                        }}
                      >
                        <Icon name="water" size={14} />
                        Marca de agua…
                      </button>
                      <button
                        className="btn"
                        onClick={() => {
                          setMenuOpen(false);
                          setHfOpen(true);
                        }}
                      >
                        <Icon name="hf" size={14} />
                        Encabezado, pie y numeración…
                      </button>
                      <button
                        className="btn"
                        onClick={() => {
                          setMenuOpen(false);
                          openProperties();
                        }}
                      >
                        <Icon name="doc" size={14} />
                        Propiedades del documento…
                      </button>
                      <button
                        className="btn"
                        onClick={() => {
                          setMenuOpen(false);
                          setProtectDraft({ user: "", owner: "" });
                        }}
                      >
                        <Icon name="lock" size={14} />
                        Proteger con contraseña…
                      </button>
                      <button
                        className="btn"
                        onClick={() => {
                          setMenuOpen(false);
                          setFlattenAsk(true);
                        }}
                      >
                        <Icon name="flatten" size={14} />
                        Aplanar anotaciones…
                      </button>
                      <button
                        className="btn"
                        onClick={() => {
                          setMenuOpen(false);
                          selectMode("select");
                          setMode("redact");
                        }}
                      >
                        <Icon name="redact" size={14} />
                        Redactar (censurar)…
                      </button>
                      <button
                        className="btn"
                        onClick={() => {
                          setMenuOpen(false);
                          printDocument();
                        }}
                      >
                        <Icon name="printer" size={14} />
                        Imprimir…
                      </button>
                      <button
                        className="btn"
                        onClick={() => {
                          setMenuOpen(false);
                          setExportOpen(true);
                        }}
                      >
                        <Icon name="image" size={14} />
                        Exportar como imágenes…
                      </button>
                      <button
                        className="btn"
                        onClick={() => {
                          setMenuOpen(false);
                          exportPlainText();
                        }}
                      >
                        <Icon name="extract" size={14} />
                        Exportar texto…
                      </button>
                      <button
                        className="btn"
                        onClick={() => {
                          setMenuOpen(false);
                          setCompressOpen(true);
                        }}
                      >
                        <Icon name="shrink" size={14} />
                        Reducir tamaño…
                      </button>
                    </div>
                  </>
                )}
              </div>
            </>
          )}
        </div>
      </header>

      {error && (
        <div className="banner-error">
          <p title={error}>{error}</p>
          <button className="btn btn-icon" onClick={() => setError(null)}>
            <Icon name="close" size={13} />
          </button>
        </div>
      )}
      {notice && (
        <div className="banner-notice">
          <p title={notice}>{notice}</p>
          <button className="btn btn-icon" onClick={() => setNotice(null)}>
            <Icon name="close" size={13} />
          </button>
        </div>
      )}

      {p12Draft && (
        <div className="modal-backdrop" onClick={() => setP12Draft(null)}>
          <div className="modal" onClick={(e) => e.stopPropagation()}>
            <h3>Contraseña del .p12</h3>
            <p className="modal-file">{p12Draft.path.split(/[\\/]/).pop()}</p>
            <input
              type="password"
              autoFocus
              placeholder="Contraseña"
              value={p12Draft.password}
              onChange={(e) =>
                setP12Draft({ ...p12Draft, password: e.target.value })
              }
              onKeyDown={(e) => {
                if (e.key === "Enter") signWithP12();
                if (e.key === "Escape") setP12Draft(null);
              }}
            />
            <div className="card-actions">
              <button className="btn" onClick={() => setP12Draft(null)}>
                Cancelar
              </button>
              <button className="btn btn-primary" onClick={signWithP12}>
                Firmar
              </button>
            </div>
          </div>
        </div>
      )}

      {mode === "firmar" && !activeSig && !drawingSig && (
        <PanelFirmas
          firmas={firmas}
          onPick={pickSignature}
          onUpload={uploadSignature}
          onDraw={() => setDrawingSig(true)}
          onDelete={removeSignature}
          onClose={() => selectMode("select")}
        />
      )}
      {drawingSig && (
        <DibujarFirma
          onSave={saveDrawnSignature}
          onClose={() => setDrawingSig(false)}
        />
      )}
      {wmOpen && (
        <DialogoMarcaAgua onApply={applyWatermark} onClose={() => setWmOpen(false)} />
      )}
      {hfOpen && (
        <DialogoEncabezado
          onApply={applyHeaderFooter}
          onClose={() => setHfOpen(false)}
        />
      )}
      {propsDraft && (
        <DialogoPropiedades
          initial={propsDraft}
          onSave={saveProperties}
          onClose={() => setPropsDraft(null)}
        />
      )}
      {pwdDraft && (
        <div className="modal-backdrop" onClick={() => setPwdDraft(null)}>
          <div className="modal" onClick={(e) => e.stopPropagation()}>
            <h3>Documento protegido</h3>
            <p className="modal-file">{pwdDraft.path.split(/[\\/]/).pop()}</p>
            <input
              type="password"
              autoFocus
              placeholder="Contraseña del documento"
              value={pwdDraft.password}
              onChange={(e) =>
                setPwdDraft({ ...pwdDraft, password: e.target.value })
              }
              onKeyDown={(e) => {
                if (e.key === "Enter")
                  openPath(pwdDraft.path, pwdDraft.password);
                if (e.key === "Escape") setPwdDraft(null);
              }}
            />
            <div className="card-actions">
              <button className="btn" onClick={() => setPwdDraft(null)}>
                Cancelar
              </button>
              <button
                className="btn btn-primary"
                onClick={() => openPath(pwdDraft.path, pwdDraft.password)}
              >
                Abrir
              </button>
            </div>
          </div>
        </div>
      )}
      {protectDraft && (
        <div className="modal-backdrop" onClick={() => setProtectDraft(null)}>
          <div className="modal" onClick={(e) => e.stopPropagation()}>
            <h3>Proteger con contraseña</h3>
            <input
              type="password"
              autoFocus
              placeholder="Contraseña (necesaria para abrir)"
              value={protectDraft.user}
              onChange={(e) =>
                setProtectDraft({ ...protectDraft, user: e.target.value })
              }
            />
            <input
              type="password"
              placeholder="Contraseña de propietario (opcional)"
              value={protectDraft.owner}
              onChange={(e) =>
                setProtectDraft({ ...protectDraft, owner: e.target.value })
              }
            />
            <p className="modal-file">
              Cifrado AES-256. Se guarda como una copia protegida; si el
              documento va a llevar firma digital, fírmalo por separado.
            </p>
            <div className="card-actions">
              <button className="btn" onClick={() => setProtectDraft(null)}>
                Cancelar
              </button>
              <button
                className="btn btn-primary"
                disabled={!protectDraft.user}
                onClick={applyProtect}
              >
                Proteger…
              </button>
            </div>
          </div>
        </div>
      )}
      {flattenAsk && (
        <div className="modal-backdrop" onClick={() => setFlattenAsk(false)}>
          <div className="modal" onClick={(e) => e.stopPropagation()}>
            <h3>Aplanar anotaciones y formularios</h3>
            <p className="modal-file" style={{ whiteSpace: "normal" }}>
              Los sellos, formas, trazos y campos rellenados pasan a ser
              contenido fijo de la página (ya no se podrán editar ni borrar).
              Ojo: los resaltados, subrayados y notas creados con esta app se
              perderán al aplanar.
            </p>
            <div className="card-actions">
              <button className="btn" onClick={() => setFlattenAsk(false)}>
                Cancelar
              </button>
              <button className="btn btn-primary" onClick={applyFlatten}>
                Aplanar
              </button>
            </div>
          </div>
        </div>
      )}
      {mode === "redact" && !redactDraft && (
        <div className="sign-hint">
          Arrastra sobre el área a censurar: el contenido se ELIMINA de verdad
          · Esc cancela
        </div>
      )}
      {exportOpen && (
        <div className="modal-backdrop" onClick={() => setExportOpen(false)}>
          <div className="modal" onClick={(e) => e.stopPropagation()}>
            <h3>Exportar como imágenes</h3>
            <div className="card-row">
              <select
                className="size-select"
                value={exportFmt}
                onChange={(e) => setExportFmt(e.target.value as "png" | "jpeg")}
              >
                <option value="png">PNG</option>
                <option value="jpeg">JPEG</option>
              </select>
              <select
                className="size-select"
                value={exportDpi}
                onChange={(e) => setExportDpi(Number(e.target.value))}
              >
                {[96, 150, 200, 300].map((d) => (
                  <option key={d} value={d}>
                    {d} ppp
                  </option>
                ))}
              </select>
            </div>
            <p className="modal-file">Una imagen por página del documento.</p>
            <div className="card-actions">
              <button className="btn" onClick={() => setExportOpen(false)}>
                Cancelar
              </button>
              <button className="btn btn-primary" onClick={exportImages}>
                Elegir carpeta…
              </button>
            </div>
          </div>
        </div>
      )}
      {compressOpen && (
        <div className="modal-backdrop" onClick={() => setCompressOpen(false)}>
          <div className="modal" onClick={(e) => e.stopPropagation()}>
            <h3>Reducir tamaño del PDF</h3>
            <div className="card-row">
              <select
                className="size-select"
                title="Calidad JPEG"
                value={compressQuality}
                onChange={(e) => setCompressQuality(Number(e.target.value))}
              >
                <option value={60}>Calidad baja (más pequeño)</option>
                <option value={75}>Calidad media</option>
                <option value={85}>Calidad alta</option>
              </select>
              <select
                className="size-select"
                title="Resolución máxima"
                value={compressDpi}
                onChange={(e) => setCompressDpi(Number(e.target.value))}
              >
                {[110, 150, 200, 300].map((d) => (
                  <option key={d} value={d}>
                    {d} ppp máx.
                  </option>
                ))}
              </select>
            </div>
            <p className="modal-file" style={{ whiteSpace: "normal" }}>
              Recomprime las imágenes del documento (las que tienen
              transparencia se conservan tal cual). El texto no se toca.
            </p>
            <div className="card-actions">
              <button className="btn" onClick={() => setCompressOpen(false)}>
                Cancelar
              </button>
              <button className="btn btn-primary" onClick={applyCompress}>
                Comprimir
              </button>
            </div>
          </div>
        </div>
      )}
      {printPages && (
        <div className="print-pages">
          {printPages.map((src, i) => (
            <img key={i} src={src} alt={`Página ${i + 1}`} />
          ))}
        </div>
      )}
      {mode === "crop" && !cropDraft && (
        <div className="sign-hint">
          Arrastra para marcar el área que quieres conservar · Esc cancela
        </div>
      )}
      {mode === "firmar" && activeSig && (
        <div className="sign-hint">
          Haz clic donde quieras la firma (o arrastra para elegir el tamaño) ·
          Esc cancela
        </div>
      )}
      {mode === "shape" && (
        <div className="tool-options">
          <div className="segmented">
            {(
              [
                ["rect", "Rectángulo"],
                ["ellipse", "Elipse"],
                ["line", "Línea"],
                ["arrow", "Flecha"],
              ] as [ShapeKind, string][]
            ).map(([k, label]) => (
              <button
                key={k}
                className={`btn${shapeKind === k ? " on" : ""}`}
                onClick={() => setShapeKind(k)}
              >
                {label}
              </button>
            ))}
          </div>
          <div className="swatches">
            {SHAPE_COLORS.map((c) => (
              <button
                key={c}
                className={`swatch${shapeColor === c ? " on" : ""}`}
                style={{ background: c }}
                title={c}
                onClick={() => setShapeColor(c)}
              />
            ))}
          </div>
          <label
            className={`opt-check${
              shapeKind === "line" || shapeKind === "arrow" ? " disabled" : ""
            }`}
          >
            <input
              type="checkbox"
              checked={shapeFill}
              disabled={shapeKind === "line" || shapeKind === "arrow"}
              onChange={(e) => setShapeFill(e.target.checked)}
            />
            Relleno
          </label>
          <select
            className="size-select"
            title="Grosor"
            value={shapeWidth}
            onChange={(e) => setShapeWidth(Number(e.target.value))}
          >
            {[1, 2, 3, 5].map((s) => (
              <option key={s} value={s}>
                {s} pt
              </option>
            ))}
          </select>
        </div>
      )}
      {mode === "stamp" && (
        <div className="tool-options">
          <select
            className="size-select"
            value={stampText}
            onChange={(e) => setStampText(e.target.value)}
          >
            {STAMP_PRESETS.map((s) => (
              <option key={s} value={s}>
                {s}
              </option>
            ))}
            <option value="custom">Personalizado…</option>
          </select>
          {stampText === "custom" && (
            <input
              type="text"
              className="stamp-input"
              placeholder="Texto del sello"
              value={stampCustom}
              onChange={(e) => setStampCustom(e.target.value.toUpperCase())}
            />
          )}
          <div className="swatches">
            {["#c81e1e", "#1a4fd6", "#1d7a34", "#111111"].map((c) => (
              <button
                key={c}
                className={`swatch${stampColor === c ? " on" : ""}`}
                style={{ background: c }}
                title={c}
                onClick={() => setStampColor(c)}
              />
            ))}
          </div>
          <span className="opt-hint">Clic en la página para colocarlo</span>
        </div>
      )}

      <div className="body">
        {pageCount > 0 && (
          <aside className="sidebar">
            <div className="sidebar-tabs">
              <button
                className={`btn${sidebarTab === "paginas" ? " on" : ""}`}
                onClick={() => setSidebarTab("paginas")}
              >
                Páginas
              </button>
              <button
                className={`btn${sidebarTab === "marcadores" ? " on" : ""}`}
                onClick={() => setSidebarTab("marcadores")}
              >
                Marcadores
              </button>
            </div>
            {sidebarTab === "marcadores" && (
              <PanelMarcadores
                outline={outline}
                currentPage={pageIndex}
                onGoto={setPageIndex}
                onChange={persistOutline}
              />
            )}
            {sidebarTab === "paginas" &&
              thumbs.map((src, i) => (
              <div
                key={i}
                className={`thumb${i === pageIndex ? " active" : ""}`}
                onClick={() => setPageIndex(i)}
              >
                {src ? (
                  <img src={src} draggable={false} alt={`Página ${i + 1}`} />
                ) : (
                  <div className="thumb-placeholder" />
                )}
                <span className="thumb-num">{i + 1}</span>
                <div className="thumb-actions">
                  <button
                    title="Subir"
                    disabled={i === 0}
                    onClick={(e) => {
                      e.stopPropagation();
                      movePage(i, i - 1);
                    }}
                  >
                    <Icon name="up" size={13} />
                  </button>
                  <button
                    title="Bajar"
                    disabled={i === pageCount - 1}
                    onClick={(e) => {
                      e.stopPropagation();
                      movePage(i, i + 1);
                    }}
                  >
                    <Icon name="down" size={13} />
                  </button>
                  <button
                    title="Rotar 90°"
                    onClick={(e) => {
                      e.stopPropagation();
                      rotatePage(i);
                    }}
                  >
                    <Icon name="rotate" size={13} />
                  </button>
                  <button
                    title="Duplicar página"
                    onClick={(e) => {
                      e.stopPropagation();
                      duplicatePageAt(i);
                    }}
                  >
                    <Icon name="copy" size={13} />
                  </button>
                  <button
                    title="Página en blanco después"
                    onClick={(e) => {
                      e.stopPropagation();
                      blankPageAfter(i);
                    }}
                  >
                    <Icon name="plus" size={13} />
                  </button>
                  <button
                    title="Eliminar página"
                    disabled={pageCount <= 1}
                    onClick={(e) => {
                      e.stopPropagation();
                      deletePage(i);
                    }}
                  >
                    <Icon name="trash" size={13} />
                  </button>
                </div>
              </div>
            ))}
          </aside>
        )}

        <div className="viewer-wrap">
          <main className="viewer">
            {!workPath && (
              <div className="placeholder">
                <Icon name="doc" size={56} />
                <p>Abre un PDF para empezar</p>
                <button className="btn btn-primary" onClick={openFile}>
                  <Icon name="open" size={14} />
                  Abrir PDF
                </button>
              </div>
            )}
            {imgSrc && (
              <div className="page-wrap" style={{ width: displayWidth }}>
                <img
                  className="page"
                  src={imgSrc}
                  draggable={false}
                  alt={`Página ${pageIndex + 1}`}
                />
                <div
                  className={`textlayer mode-${mode}`}
                  onMouseDown={onMouseDown}
                  onMouseMove={onMouseMove}
                  onMouseUp={onMouseUp}
                  onClick={onClickLayer}
                >
                  {annots
                    .filter((a) => a.kind === "Highlight")
                    .flatMap((a) =>
                      a.rects.map((r, j) => (
                        <div
                          key={`h${a.index}-${j}`}
                          className="annot-highlight"
                          style={{
                            left: r.x * scale,
                            top: r.y * scale,
                            width: r.w * scale,
                            height: r.h * scale,
                          }}
                        />
                      )),
                    )}
                  {annots
                    .filter(
                      (a) =>
                        a.kind === "Underline" ||
                        a.kind === "Strikeout" ||
                        a.kind === "StrikeOut",
                    )
                    .flatMap((a) =>
                      a.rects.map((r, j) => (
                        <div
                          key={`u${a.index}-${j}`}
                          className={
                            a.kind === "Underline"
                              ? "annot-underline"
                              : "annot-strike"
                          }
                          style={{
                            left: r.x * scale,
                            top:
                              a.kind === "Underline"
                                ? (r.y + r.h) * scale - 2
                                : (r.y + r.h * 0.55) * scale - 1,
                            width: r.w * scale,
                          }}
                        />
                      )),
                    )}
                  {mode === "edit" &&
                    textBlocks.map((b) => (
                      <div
                        key={`b${b.object_index}`}
                        className="text-block"
                        style={{
                          left: b.x * scale,
                          top: b.y * scale,
                          width: b.w * scale,
                          height: b.h * scale,
                        }}
                        onMouseDown={(e) => e.stopPropagation()}
                        onClick={(e) => {
                          e.stopPropagation();
                          setBlockDraft({ block: b, text: b.text });
                        }}
                      />
                    ))}
                  {mode === "image" &&
                    images.map((im) => {
                      const isDragging =
                        imgDraft !== null &&
                        imgDraft.object_index === im.object_index;
                      const b = isDragging ? imgDraft : im;
                      const preview = imgPreviews[im.object_index];
                      return (
                        <div
                          key={`im${im.object_index}`}
                          className="image-box"
                          style={{
                            left: b.x * scale,
                            top: b.y * scale,
                            width: b.w * scale,
                            height: b.h * scale,
                          }}
                          onMouseDown={(e) => startImgAction(e, im, "move")}
                        >
                          {isDragging && preview && (
                            <img
                              className="image-preview"
                              src={`data:image/png;base64,${preview}`}
                              draggable={false}
                              alt=""
                            />
                          )}
                          <div
                            className="image-handle"
                            title="Redimensionar (Shift: libre)"
                            onMouseDown={(e) => startImgAction(e, im, "resize")}
                          />
                        </div>
                      );
                    })}
                  {imagePopover && (
                    <div
                      className="card"
                      style={{
                        left: imagePopover.x * scale,
                        top: (imagePopover.y + imagePopover.h) * scale + 6,
                      }}
                      onMouseDown={(e) => e.stopPropagation()}
                    >
                      <div className="card-actions">
                        <button
                          className="btn"
                          onClick={() => replaceImagePick(imagePopover)}
                        >
                          <Icon name="image" size={13} />
                          Reemplazar…
                        </button>
                        <button
                          className="btn btn-danger"
                          onClick={() => deleteImage(imagePopover)}
                        >
                          <Icon name="trash" size={13} />
                          Eliminar
                        </button>
                        <button
                          className="btn"
                          onClick={() => setImagePopover(null)}
                        >
                          Cerrar
                        </button>
                      </div>
                    </div>
                  )}
                  {newTextDraft && (
                    <div
                      className="card"
                      style={{
                        left: newTextDraft.x * scale,
                        top: newTextDraft.y * scale,
                        width: 280,
                      }}
                      onMouseDown={(e) => e.stopPropagation()}
                    >
                      <textarea
                        autoFocus
                        placeholder="Texto nuevo… (Enter añade, Esc cancela)"
                        value={newTextDraft.text}
                        onChange={(e) =>
                          setNewTextDraft({
                            ...newTextDraft,
                            text: e.target.value,
                          })
                        }
                        onKeyDown={(e) => {
                          if (e.key === "Enter" && !e.shiftKey) {
                            e.preventDefault();
                            submitNewText();
                          }
                          if (e.key === "Escape") setNewTextDraft(null);
                        }}
                      />
                      <div className="card-row">
                        <select
                          className="size-select font-select"
                          title="Fuente"
                          value={newTextDraft.font}
                          onChange={(e) =>
                            setNewTextDraft({
                              ...newTextDraft,
                              font: e.target.value,
                            })
                          }
                        >
                          {FONT_CHOICES.map((f) => (
                            <option key={f.value} value={f.value}>
                              {f.label}
                            </option>
                          ))}
                        </select>
                        <select
                          className="size-select"
                          title="Tamaño"
                          value={newTextDraft.size}
                          onChange={(e) =>
                            setNewTextDraft({
                              ...newTextDraft,
                              size: Number(e.target.value),
                            })
                          }
                        >
                          {[8, 10, 12, 14, 18, 24, 32].map((s) => (
                            <option key={s} value={s}>
                              {s} pt
                            </option>
                          ))}
                        </select>
                      </div>
                      <div className="card-actions">
                        <button
                          className="btn"
                          onClick={() => setNewTextDraft(null)}
                        >
                          Cancelar
                        </button>
                        <button className="btn btn-primary" onClick={submitNewText}>
                          Añadir
                        </button>
                      </div>
                    </div>
                  )}
                  {blockDraft && (
                    <div
                      className="card"
                      style={{
                        left: blockDraft.block.x * scale,
                        top: (blockDraft.block.y + blockDraft.block.h) * scale + 6,
                        width: Math.max(260, blockDraft.block.w * scale),
                      }}
                      onMouseDown={(e) => e.stopPropagation()}
                    >
                      <span className="card-label">
                        {blockDraft.block.font_family || "Fuente del documento"}
                        {" · "}
                        {Math.round(blockDraft.block.font_size)} pt
                      </span>
                      <textarea
                        autoFocus
                        value={blockDraft.text}
                        onChange={(e) =>
                          setBlockDraft({ ...blockDraft, text: e.target.value })
                        }
                        onKeyDown={(e) => {
                          if (e.key === "Enter" && !e.shiftKey) {
                            e.preventDefault();
                            submitBlockDraft();
                          }
                          if (e.key === "Escape") setBlockDraft(null);
                        }}
                      />
                      <div className="card-actions">
                        <button className="btn btn-danger" onClick={deleteBlock}>
                          <Icon name="trash" size={13} />
                          Eliminar
                        </button>
                        <button className="btn" onClick={() => setBlockDraft(null)}>
                          Cancelar
                        </button>
                        <button className="btn btn-primary" onClick={submitBlockDraft}>
                          Guardar
                        </button>
                      </div>
                    </div>
                  )}
                  {mode === "select" &&
                    links.map((l, i) => (
                      <div
                        key={`lk${i}`}
                        className="link-zone"
                        title={l.uri ?? `Ir a la página ${(l.dest_page ?? 0) + 1}`}
                        style={{
                          left: l.x * scale,
                          top: l.y * scale,
                          width: l.w * scale,
                          height: l.h * scale,
                        }}
                        onMouseDown={(e) => e.stopPropagation()}
                        onClick={(e) => {
                          e.stopPropagation();
                          onLinkClick(l);
                        }}
                      />
                    ))}
                  {mode === "select" &&
                    formFields.map((f) => (
                      <div
                        key={`f${f.annot_index}`}
                        className="form-field"
                        title={f.name}
                        style={{
                          left: f.x * scale,
                          top: f.y * scale,
                          width: f.w * scale,
                          height: f.h * scale,
                        }}
                        onMouseDown={(e) => e.stopPropagation()}
                        onClick={(e) => {
                          e.stopPropagation();
                          onFieldClick(f);
                        }}
                      />
                    ))}
                  {fieldDraft && (
                    <div
                      className="card"
                      style={{
                        left: fieldDraft.field.x * scale,
                        top: (fieldDraft.field.y + fieldDraft.field.h) * scale + 6,
                      }}
                      onMouseDown={(e) => e.stopPropagation()}
                    >
                      <textarea
                        autoFocus
                        placeholder={`${fieldDraft.field.name}…`}
                        value={fieldDraft.text}
                        onChange={(e) =>
                          setFieldDraft({ ...fieldDraft, text: e.target.value })
                        }
                        onKeyDown={(e) => {
                          if (e.key === "Enter" && !e.shiftKey) {
                            e.preventDefault();
                            submitFieldDraft();
                          }
                          if (e.key === "Escape") setFieldDraft(null);
                        }}
                      />
                      <div className="card-actions">
                        <button className="btn" onClick={() => setFieldDraft(null)}>
                          Cancelar
                        </button>
                        <button className="btn btn-primary" onClick={submitFieldDraft}>
                          Guardar
                        </button>
                      </div>
                    </div>
                  )}
                  {annots
                    .filter((a) => a.kind === "Text")
                    .map((a) => (
                      <button
                        key={`n${a.index}`}
                        className="note-icon"
                        style={{
                          left: a.x * scale,
                          top: a.y * scale,
                          width: Math.max(18, a.w * scale),
                          height: Math.max(18, a.h * scale),
                        }}
                        title={a.contents}
                        onMouseDown={(e) => e.stopPropagation()}
                        onClick={(e) => {
                          e.stopPropagation();
                          setNotePopover((p) => (p?.index === a.index ? null : a));
                        }}
                      >
                        <Icon name="note" size={12} />
                      </button>
                    ))}
                  {notePopover && (
                    <div
                      className="card"
                      style={{
                        left: notePopover.x * scale,
                        top: (notePopover.y + notePopover.h) * scale + 6,
                      }}
                      onMouseDown={(e) => e.stopPropagation()}
                    >
                      <p>
                        {notePopover.contents ||
                          KIND_LABELS[notePopover.kind] ||
                          notePopover.kind}
                      </p>
                      <div className="card-actions">
                        <button
                          className="btn btn-danger"
                          onClick={() => deleteAnnotation(notePopover)}
                        >
                          <Icon name="trash" size={13} />
                          Eliminar
                        </button>
                        <button className="btn" onClick={() => setNotePopover(null)}>
                          Cerrar
                        </button>
                      </div>
                    </div>
                  )}
                  {noteDraft && (
                    <div
                      className="card"
                      style={{
                        left: noteDraft.x * scale,
                        top: noteDraft.y * scale,
                      }}
                      onMouseDown={(e) => e.stopPropagation()}
                    >
                      <textarea
                        autoFocus
                        placeholder="Escribe la nota y pulsa Enter…"
                        value={noteDraft.text}
                        onChange={(e) =>
                          setNoteDraft({ ...noteDraft, text: e.target.value })
                        }
                        onKeyDown={(e) => {
                          if (e.key === "Enter" && !e.shiftKey) {
                            e.preventDefault();
                            submitNote();
                          }
                          if (e.key === "Escape") setNoteDraft(null);
                        }}
                      />
                    </div>
                  )}
                  {mode === "crop" && cropDraft && (
                    <>
                      <div
                        className="crop-rect"
                        style={{
                          left: cropDraft.x * scale,
                          top: cropDraft.y * scale,
                          width: cropDraft.w * scale,
                          height: cropDraft.h * scale,
                        }}
                      />
                      <div
                        className="card crop-actions"
                        style={{
                          left: cropDraft.x * scale,
                          top: (cropDraft.y + cropDraft.h) * scale + 8,
                        }}
                        onMouseDown={(e) => e.stopPropagation()}
                      >
                        <div className="card-actions">
                          <button
                            className="btn btn-primary"
                            onClick={() => applyCrop(false)}
                          >
                            Recortar
                          </button>
                          <button className="btn" onClick={() => applyCrop(true)}>
                            Todas las páginas
                          </button>
                          <button
                            className="btn"
                            onClick={() => {
                              setCropDraft(null);
                              setMode("select");
                            }}
                          >
                            Cancelar
                          </button>
                        </div>
                      </div>
                    </>
                  )}
                  {mode === "redact" && redactDraft && (
                    <>
                      <div
                        className="redact-rect"
                        style={{
                          left: redactDraft.x * scale,
                          top: redactDraft.y * scale,
                          width: redactDraft.w * scale,
                          height: redactDraft.h * scale,
                        }}
                      />
                      <div
                        className="card crop-actions"
                        style={{
                          left: redactDraft.x * scale,
                          top: (redactDraft.y + redactDraft.h) * scale + 8,
                        }}
                        onMouseDown={(e) => e.stopPropagation()}
                      >
                        <p>
                          {redactReport
                            ? `Se eliminarán ${redactReport.textos} bloque(s) de texto y ${redactReport.imagenes} imagen(es).`
                            : "Calculando…"}
                        </p>
                        <div className="card-actions">
                          <button
                            className="btn btn-danger"
                            disabled={!redactReport}
                            onClick={applyRedact}
                          >
                            Redactar
                          </button>
                          <button
                            className="btn"
                            onClick={() => {
                              setRedactDraft(null);
                              setRedactReport(null);
                            }}
                          >
                            Cancelar
                          </button>
                        </div>
                      </div>
                    </>
                  )}
                  {mode === "firmar" && activeSig && sigDraft && (
                    <img
                      className="sign-ghost"
                      src={`data:image/png;base64,${activeSig.png}`}
                      draggable={false}
                      alt="Vista previa de la firma"
                      style={{
                        left: sigDraft.x * scale,
                        top: sigDraft.y * scale,
                        width: sigDraft.w * scale,
                        height: sigDraft.h * scale,
                      }}
                    />
                  )}
                  {strokePts.length > 1 && (
                    <svg className="stroke-preview">
                      <polyline
                        points={strokePts
                          .map((p) => `${p[0] * scale},${p[1] * scale}`)
                          .join(" ")}
                      />
                    </svg>
                  )}
                  {mode === "shape" && shapeDraft && (
                    <svg className="shape-preview">
                      {(() => {
                        const d = shapeDraft;
                        const stroke = shapeColor;
                        const sw = shapeWidth * scale;
                        const fillable =
                          shapeKind === "rect" || shapeKind === "ellipse";
                        const fill =
                          shapeFill && fillable ? `${shapeColor}46` : "none";
                        const x = Math.min(d.x1, d.x2) * scale;
                        const y = Math.min(d.y1, d.y2) * scale;
                        const w = Math.abs(d.x2 - d.x1) * scale;
                        const h = Math.abs(d.y2 - d.y1) * scale;
                        if (shapeKind === "rect")
                          return (
                            <rect
                              x={x}
                              y={y}
                              width={w}
                              height={h}
                              stroke={stroke}
                              strokeWidth={sw}
                              fill={fill}
                            />
                          );
                        if (shapeKind === "ellipse")
                          return (
                            <ellipse
                              cx={x + w / 2}
                              cy={y + h / 2}
                              rx={w / 2}
                              ry={h / 2}
                              stroke={stroke}
                              strokeWidth={sw}
                              fill={fill}
                            />
                          );
                        const pts = {
                          x1: d.x1 * scale,
                          y1: d.y1 * scale,
                          x2: d.x2 * scale,
                          y2: d.y2 * scale,
                        };
                        const head = (12 + shapeWidth * 2) * scale;
                        const ang = Math.atan2(
                          pts.y2 - pts.y1,
                          pts.x2 - pts.x1,
                        );
                        return (
                          <>
                            <line
                              {...pts}
                              stroke={stroke}
                              strokeWidth={sw}
                            />
                            {shapeKind === "arrow" &&
                              [Math.PI / 6, -Math.PI / 6].map((delta, i) => {
                                const a2 = ang + Math.PI - delta;
                                return (
                                  <line
                                    key={i}
                                    x1={pts.x2}
                                    y1={pts.y2}
                                    x2={pts.x2 + head * Math.cos(a2)}
                                    y2={pts.y2 + head * Math.sin(a2)}
                                    stroke={stroke}
                                    strokeWidth={sw}
                                  />
                                );
                              })}
                          </>
                        );
                      })()}
                    </svg>
                  )}
                  {matches.map((m, i) =>
                    m.page_index === pageIndex
                      ? m.rects.map((r, j) => (
                          <div
                            key={`m${i}-${j}`}
                            ref={i === matchIdx && j === 0 ? currentHitRef : null}
                            className={`hit${i === matchIdx ? " current" : ""}`}
                            style={{
                              left: r.x * scale,
                              top: r.y * scale,
                              width: r.w * scale,
                              height: r.h * scale,
                            }}
                          />
                        ))
                      : null,
                  )}
                  {selectionRects.map((r, i) => (
                    <div
                      key={`s${i}`}
                      className="sel"
                      style={{
                        left: r.x * scale,
                        top: r.y * scale,
                        width: r.w * scale,
                        height: r.h * scale,
                      }}
                    />
                  ))}
                  {lastSelRect && !dragging && (
                    <div
                      className="card sel-popover"
                      style={{
                        left: lastSelRect.x * scale,
                        top: (lastSelRect.y + lastSelRect.h) * scale + 6,
                      }}
                      onMouseDown={(e) => e.stopPropagation()}
                    >
                      <button
                        className="btn"
                        onClick={() => markupSelection("highlight")}
                      >
                        <Icon name="highlight" size={13} />
                        Resaltar
                      </button>
                      <button
                        className="btn"
                        onClick={() => markupSelection("underline")}
                      >
                        <Icon name="underline" size={13} />
                        Subrayar
                      </button>
                      <button
                        className="btn"
                        onClick={() => markupSelection("strikeout")}
                      >
                        <Icon name="strike" size={13} />
                        Tachar
                      </button>
                      <button
                        className="btn"
                        onClick={() => {
                          copySelection();
                          setSelection(null);
                        }}
                      >
                        <Icon name="copy" size={13} />
                        Copiar
                      </button>
                    </div>
                  )}
                </div>
              </div>
            )}
          </main>

          {pageCount > 0 && (
            <div className="nav-pill">
              <button
                className="btn btn-icon"
                disabled={pageIndex === 0}
                onClick={() => setPageIndex((i) => i - 1)}
              >
                <Icon name="chevLeft" size={14} />
              </button>
              <span>
                {pageIndex + 1} / {pageCount}
              </span>
              <button
                className="btn btn-icon"
                disabled={pageIndex >= pageCount - 1}
                onClick={() => setPageIndex((i) => i + 1)}
              >
                <Icon name="chevRight" size={14} />
              </button>
              <div className="sep" />
              <button
                className="btn btn-icon"
                onClick={() => setZoom((z) => Math.max(0.5, z - 0.25))}
              >
                <Icon name="minus" size={14} />
              </button>
              <span>{Math.round(zoom * 100)}%</span>
              <button
                className="btn btn-icon"
                onClick={() => setZoom((z) => Math.min(4, z + 0.25))}
              >
                <Icon name="plus" size={14} />
              </button>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

export default App;
