import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";
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
};
type Mode = "select" | "draw" | "note" | "edit";
type UndoEntry = { page: number };

const KIND_LABELS: Record<string, string> = {
  Text: "Nota",
  Highlight: "Resaltado",
  Ink: "Dibujo",
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
  sticky: ["M12 3v10", "M12 13l-3-3", "M12 13l3-3"],
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
  } | null>(null);
  const [p12Draft, setP12Draft] = useState<{
    path: string;
    password: string;
  } | null>(null);

  const [query, setQuery] = useState("");
  const [lastQuery, setLastQuery] = useState("");
  const [matches, setMatches] = useState<SearchMatch[]>([]);
  const [matchIdx, setMatchIdx] = useState(0);
  const [searched, setSearched] = useState(false);
  const currentHitRef = useRef<HTMLDivElement | null>(null);

  async function openFile() {
    const selected = await open({
      filters: [{ name: "PDF", extensions: ["pdf"] }],
      multiple: false,
    });
    if (typeof selected !== "string") return;
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
        { path: selected },
      );
      setOriginalPath(selected);
      setWorkPath(info.work_path);
      setPageCount(info.page_count);
      setPageIndex(0);
      setDocVersion((v) => v + 1);
    } catch (e) {
      setError(String(e));
    }
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

  async function highlightSelection() {
    if (!workPath || !selection || !pageText) return;
    const rects = mergeLineRects(
      pageText.chars.slice(selection.start, selection.end + 1),
    );
    if (rects.length === 0) return;
    try {
      await invoke("add_highlight", { workPath, pageIndex, rects });
      setSelection(null);
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
    const hit = annots.find((a) => {
      if (a.kind === "Highlight") {
        return a.rects.some(
          (r) => x >= r.x && x <= r.x + r.w && y >= r.y && y <= r.y + r.h,
        );
      }
      if (a.kind === "Ink") {
        return x >= a.x && x <= a.x + a.w && y >= a.y && y <= a.y + a.h;
      }
      return false;
    });
    if (hit) setNotePopover(hit);
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
      setNewTextDraft({ x, y, text: "", size: 12 });
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
    if (mode === "draw") {
      if (strokePts.length === 0) return;
      const { x, y } = pagePoint(e);
      setStrokePts((pts) => [...pts, [x, y]]);
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

  const fileName = originalPath?.split("/").pop() ?? null;

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
  ];

  function selectMode(m: Mode) {
    setMode((cur) => (cur === m ? "select" : m));
    setSelection(null);
    setStrokePts([]);
    setNoteDraft(null);
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
                        Firmar…
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

      {p12Draft && (
        <div className="modal-backdrop" onClick={() => setP12Draft(null)}>
          <div className="modal" onClick={(e) => e.stopPropagation()}>
            <h3>Contraseña del .p12</h3>
            <p className="modal-file">{p12Draft.path.split("/").pop()}</p>
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

      <div className="body">
        {pageCount > 0 && (
          <aside className="sidebar">
            {thumbs.map((src, i) => (
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
                      <div className="card-actions">
                        <select
                          className="size-select"
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
                  {strokePts.length > 1 && (
                    <svg className="stroke-preview">
                      <polyline
                        points={strokePts
                          .map((p) => `${p[0] * scale},${p[1] * scale}`)
                          .join(" ")}
                      />
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
                      <button className="btn" onClick={highlightSelection}>
                        <Icon name="highlight" size={13} />
                        Resaltar
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
