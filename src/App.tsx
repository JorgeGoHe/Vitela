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

  const [pageText, setPageText] = useState<PageText | null>(null);
  const [selection, setSelection] = useState<Selection | null>(null);
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

  /** Guarda un render en el caché de páginas, con tope de entradas. */
  function cachePut(key: string, src: string) {
    const cache = pageCacheRef.current;
    cache.set(key, src);
    if (cache.size > 30) {
      const oldest = cache.keys().next().value;
      if (oldest) cache.delete(oldest);
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

  // Copiar selección con ⌘C / Ctrl+C
  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      if ((e.metaKey || e.ctrlKey) && e.key === "c" && selection && pageText) {
        const text = pageText.chars
          .slice(selection.start, selection.end + 1)
          .map((c) => c.ch)
          .join("");
        copyToClipboard(text);
        e.preventDefault();
      }
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [selection, pageText]);

  /** Tras mutar el documento: refrescar render, miniaturas y limpiar búsqueda. */
  function afterMutation(newCount: number, nextPage?: number) {
    setPageCount(newCount);
    setModified(true);
    setMatches([]);
    setSearched(false);
    setLastQuery("");
    setPageIndex((p) =>
      Math.max(0, Math.min(nextPage ?? p, newCount - 1)),
    );
    setDocVersion((v) => v + 1);
  }

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
    if (mode === "edit") return;
    if (!pageText) return;
    anchorRef.current = charIndexAt(pageText, x, y);
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
    if (mode === "draw" && strokePts.length > 0) finishStroke();
  }

  const selectionRects =
    selection && pageText
      ? mergeLineRects(pageText.chars.slice(selection.start, selection.end + 1))
      : [];

  const fileName = originalPath?.split("/").pop() ?? null;

  return (
    <div className="app">
      <header className="toolbar">
        <button onClick={openFile}>Abrir PDF…</button>
        {fileName && (
          <span className="filename">
            {fileName}
            {modified ? " •" : ""}
          </span>
        )}
        {pageCount > 0 && (
          <>
            <div className="group">
              <button
                disabled={pageIndex === 0}
                onClick={() => setPageIndex((i) => i - 1)}
              >
                ◀
              </button>
              <span>
                {pageIndex + 1} / {pageCount}
              </span>
              <button
                disabled={pageIndex >= pageCount - 1}
                onClick={() => setPageIndex((i) => i + 1)}
              >
                ▶
              </button>
            </div>
            <div className="group">
              <button onClick={() => setZoom((z) => Math.max(0.5, z - 0.25))}>
                −
              </button>
              <span>{Math.round(zoom * 100)}%</span>
              <button onClick={() => setZoom((z) => Math.min(4, z + 0.25))}>
                +
              </button>
            </div>
            <div className="group search">
              <input
                type="text"
                placeholder="Buscar…"
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
                      ? `${matchIdx + 1} / ${matches.length}`
                      : "Sin resultados"}
                  </span>
                  {matches.length > 0 && (
                    <>
                      <button onClick={() => gotoMatch(-1)}>↑</button>
                      <button onClick={() => gotoMatch(1)}>↓</button>
                    </>
                  )}
                </>
              )}
            </div>
            <div className="group">
              {selection && (
                <button className="accent" onClick={highlightSelection}>
                  Resaltar
                </button>
              )}
              <button
                className={mode === "draw" ? "active" : ""}
                title="Dibujar a mano alzada"
                onClick={() => {
                  setMode((m) => (m === "draw" ? "select" : "draw"));
                  setSelection(null);
                  setNoteDraft(null);
                }}
              >
                ✏️ Dibujar
              </button>
              <button
                className={mode === "note" ? "active" : ""}
                title="Añadir una nota (clic en la página)"
                onClick={() => {
                  setMode((m) => (m === "note" ? "select" : "note"));
                  setSelection(null);
                  setStrokePts([]);
                }}
              >
                📝 Nota
              </button>
              <button
                className={mode === "edit" ? "active" : ""}
                title="Editar el texto del documento (clic en un bloque)"
                onClick={() => {
                  setMode((m) => (m === "edit" ? "select" : "edit"));
                  setSelection(null);
                  setStrokePts([]);
                  setNoteDraft(null);
                }}
              >
                Aa Editar
              </button>
              <button
                title="Deshacer la última anotación"
                onClick={undoAnnotation}
              >
                ↩ Deshacer
              </button>
            </div>
            <div className="group">
              <button onClick={addPdf} title="Añadir las páginas de otro PDF al final">
                Añadir PDF…
              </button>
              <button
                onClick={extractCurrentPage}
                title="Guardar la página actual como PDF nuevo"
              >
                Extraer página…
              </button>
            </div>
            <div className="group">
              <button disabled={!modified} onClick={saveFile}>
                Guardar
              </button>
              <button onClick={saveFileAs}>Guardar como…</button>
              <button
                onClick={signPdf}
                title="Firmar digitalmente con certificado y clave PEM"
              >
                🔏 Firmar…
              </button>
            </div>
          </>
        )}
        {loading && <span className="status">Renderizando…</span>}
      </header>

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
            <div className="note-popover-actions">
              <button onClick={() => setP12Draft(null)}>Cancelar</button>
              <button className="accent" onClick={signWithP12}>
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
                    ↑
                  </button>
                  <button
                    title="Bajar"
                    disabled={i === pageCount - 1}
                    onClick={(e) => {
                      e.stopPropagation();
                      movePage(i, i + 1);
                    }}
                  >
                    ↓
                  </button>
                  <button
                    title="Rotar 90°"
                    onClick={(e) => {
                      e.stopPropagation();
                      rotatePage(i);
                    }}
                  >
                    ⟳
                  </button>
                  <button
                    title="Eliminar página"
                    disabled={pageCount <= 1}
                    onClick={(e) => {
                      e.stopPropagation();
                      deletePage(i);
                    }}
                  >
                    ✕
                  </button>
                </div>
              </div>
            ))}
          </aside>
        )}

        <main className="viewer">
          {error && <p className="error">{error}</p>}
          {!workPath && !error && (
            <p className="placeholder">Abre un PDF para empezar</p>
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
                {strokePts.length > 1 && (
                  <svg className="stroke-preview">
                    <polyline
                      points={strokePts
                        .map((p) => `${p[0] * scale},${p[1] * scale}`)
                        .join(" ")}
                    />
                  </svg>
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
                {blockDraft && (
                  <div
                    className="note-editor block-editor"
                    style={{
                      left: blockDraft.block.x * scale,
                      top:
                        (blockDraft.block.y + blockDraft.block.h) * scale + 4,
                      width: Math.max(240, blockDraft.block.w * scale),
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
                    <div className="note-popover-actions">
                      <button onClick={deleteBlock}>Eliminar bloque</button>
                      <button onClick={submitBlockDraft}>Guardar</button>
                      <button onClick={() => setBlockDraft(null)}>
                        Cancelar
                      </button>
                    </div>
                  </div>
                )}
                {mode === "select" &&
                  formFields.map((f) => (
                    <div
                      key={`f${f.annot_index}`}
                      className={`form-field${
                        f.kind === "Checkbox" || f.kind === "RadioButton"
                          ? " form-check"
                          : ""
                      }`}
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
                    className="note-editor"
                    style={{
                      left: fieldDraft.field.x * scale,
                      top: (fieldDraft.field.y + fieldDraft.field.h) * scale + 4,
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
                        setNotePopover((p) =>
                          p?.index === a.index ? null : a,
                        );
                      }}
                    >
                      📝
                    </button>
                  ))}
                {notePopover && (
                  <div
                    className="note-popover"
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
                    <div className="note-popover-actions">
                      <button onClick={() => deleteAnnotation(notePopover)}>
                        Eliminar
                      </button>
                      <button onClick={() => setNotePopover(null)}>
                        Cerrar
                      </button>
                    </div>
                  </div>
                )}
                {noteDraft && (
                  <div
                    className="note-editor"
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
              </div>
            </div>
          )}
        </main>
      </div>
    </div>
  );
}

export default App;
