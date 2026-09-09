import { useEffect, useRef, useState, type MouseEvent } from "react";
import { invoke } from "../../ipc";
import { addMarkup, addShape, addStamp, transformAnnotation } from "../../api";
import {
  autorComentarios,
  hexToRgba,
  mergeLineRects,
  type AnnotationInfo,
  type Mode,
  type Rect,
  type ResizeHandle,
} from "../../tipos";
import type { ToolProps } from "../../components/Pagina";
import type { SeleccionTexto } from "./useSeleccionTexto";
import { pagePoint } from "./geometria";

/**
 * Anotaciones de la página: la lista (iconos de nota, overlays de marcas,
 * popover) y los borradores de nota, trazo (Ink), forma y arrastre de
 * sellos/dibujos en modo selección. Los handlers de ratón viven en `Pagina`.
 */
export function useAnotaciones(ctx: {
  workPath: string;
  index: number;
  visible: boolean;
  docVersion: number;
  annotVersion: number;
  pageVersion: number;
  mode: Mode;
  scale: number;
  tool: ToolProps;
  selOwner: number | null;
  seleccion: Pick<SeleccionTexto, "selection" | "pageText" | "setSelection">;
  onAnnotated: (page: number) => void;
  onError: (e: unknown) => void;
  onModeChange: (m: Mode) => void;
}) {
  const {
    workPath,
    index,
    visible,
    docVersion,
    annotVersion,
    pageVersion,
    mode,
    scale,
    tool,
    selOwner,
    seleccion,
    onAnnotated,
    onError,
    onModeChange,
  } = ctx;
  const [annots, setAnnots] = useState<AnnotationInfo[]>([]);
  const [notePopover, setNotePopover] = useState<AnnotationInfo | null>(null);
  const [noteDraft, setNoteDraft] = useState<{
    x: number;
    y: number;
    text: string;
  } | null>(null);
  const [strokePts, setStrokePts] = useState<[number, number][]>([]);
  const strokeLiveRef = useRef<[number, number][]>([]);

  // arrastre/redimensionado de anotaciones (sellos y dibujos) en modo select
  const [annotDraft, setAnnotDraft] = useState<(Rect & { index: number }) | null>(null);
  const annotActionRef = useRef<{
    kind: "move" | "resize";
    handle?: ResizeHandle;
    startX: number;
    startY: number;
    orig: AnnotationInfo;
    moved: boolean;
  } | null>(null);
  const annotLiveRef = useRef<(Rect & { index: number }) | null>(null);

  const [shapeDraft, setShapeDraft] = useState<{
    x1: number;
    y1: number;
    x2: number;
    y2: number;
  } | null>(null);
  const shapeStartRef = useRef<{ x: number; y: number } | null>(null);
  const shapeLiveRef = useRef<{ x1: number; y1: number; x2: number; y2: number } | null>(null);

  // Al cambiar de modo: fuera borradores y estado transitorio
  useEffect(() => {
    setStrokePts([]);
    strokeLiveRef.current = [];
    setNoteDraft(null);
    setNotePopover(null);
    setShapeDraft(null);
    shapeStartRef.current = null;
    shapeLiveRef.current = null;
    setAnnotDraft(null);
    annotActionRef.current = null;
    annotLiveRef.current = null;
  }, [mode]);

  // Solo una página puede tener la selección viva
  useEffect(() => {
    if (selOwner !== index) {
      setNotePopover(null);
    }
  }, [selOwner, index]);

  // Anotaciones de la página (iconos de nota, overlays y popovers)
  useEffect(() => {
    if (!workPath || !visible) return;
    let cancelled = false;
    setNotePopover(null);
    invoke<AnnotationInfo[]>("get_annotations", { path: workPath, pageIndex: index })
      .then((a) => {
        if (cancelled) return;
        setAnnots(a);
        // el borrador de un arrastre aguanta hasta aquí para no ver saltos
        setAnnotDraft(null);
      })
      .catch(() => {
        if (!cancelled) setAnnots([]);
      });
    return () => {
      cancelled = true;
    };
  }, [workPath, index, visible, docVersion, annotVersion, pageVersion]);

  /** Resalta, subraya o tacha la selección actual. */
  async function markupSelection(kind: "highlight" | "underline" | "strikeout") {
    if (!workPath || !seleccion.selection || !seleccion.pageText) return;
    const rects = mergeLineRects(
      seleccion.pageText.chars.slice(seleccion.selection.start, seleccion.selection.end + 1),
    );
    if (rects.length === 0) return;
    try {
      const accion =
        kind === "highlight"
          ? "resaltar"
          : kind === "underline"
            ? "subrayar"
            : "tachar";
      const colorHex =
        tool.markupPending ?? tool.markupColors[accion];
      await addMarkup({
        workPath,
        pageIndex: index,
        rects,
        kind,
        color: hexToRgba(colorHex, kind === "highlight" ? 140 : 255),
        author: autorComentarios(),
      });
      tool.onMarkupUsed(kind, colorHex);
      seleccion.setSelection(null);
      onAnnotated(index);
    } catch (e) {
      onError(e);
    }
  }

  async function commitShape(d: {
    x1: number;
    y1: number;
    x2: number;
    y2: number;
  }) {
    if (!workPath) return;
    const fillable = tool.shapeKind === "rect" || tool.shapeKind === "ellipse";
    try {
      await addShape({
        workPath,
        pageIndex: index,
        kind: tool.shapeKind,
        x1: d.x1,
        y1: d.y1,
        x2: d.x2,
        y2: d.y2,
        stroke: hexToRgba(tool.shapeColor),
        fill: tool.shapeFill && fillable ? hexToRgba(tool.shapeColor, 70) : null,
        strokeWidth: tool.shapeWidth,
        author: autorComentarios(),
      });
      onAnnotated(index);
    } catch (e) {
      onError(e);
    }
  }

  async function placeStamp(x: number, y: number) {
    const text =
      tool.stampText === "custom" ? tool.stampCustom.trim() : tool.stampText;
    if (!workPath || !text) return;
    try {
      await addStamp({
        workPath,
        pageIndex: index,
        text,
        color: hexToRgba(tool.stampColor),
        x,
        y,
        fontSize: 22,
        author: autorComentarios(),
      });
      onAnnotated(index);
    } catch (e) {
      onError(e);
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
        pageIndex: index,
        x: noteDraft.x,
        y: noteDraft.y,
        text: noteDraft.text,
        author: autorComentarios(),
      });
      setNoteDraft(null);
      onModeChange("select");
      onAnnotated(index);
    } catch (e) {
      onError(e);
    }
  }

  async function finishStroke() {
    const pts = strokeLiveRef.current;
    strokeLiveRef.current = [];
    setStrokePts([]);
    if (!workPath || pts.length < 2) return;
    try {
      await invoke("add_stroke", {
        workPath,
        pageIndex: index,
        points: pts,
        color: hexToRgba(tool.drawColor),
        width: tool.drawWidth,
        author: autorComentarios(),
      });
      onAnnotated(index);
    } catch (e) {
      onError(e);
    }
  }

  async function deleteAnnotation(annot: AnnotationInfo) {
    if (!workPath) return;
    try {
      await invoke("remove_annotation", {
        workPath,
        pageIndex: index,
        annotIndex: annot.index,
      });
      setNotePopover(null);
      onAnnotated(index);
    } catch (e) {
      onError(e);
    }
  }

  /** Clic simple en modo selección: abre el popover de la anotación pulsada. */
  function onClickLayer(e: MouseEvent<HTMLDivElement>) {
    if (mode !== "select" || seleccion.selection) return;
    const { x, y } = pagePoint(e, scale);
    // Ink y Stamp tienen su propio overlay arrastrable con su mousedown
    const CLICKABLE = ["Highlight", "Underline", "Strikeout", "StrikeOut"];
    const hit = annots.find((a) => {
      if (!CLICKABLE.includes(a.kind)) return false;
      const zonas =
        a.rects.length > 0 ? a.rects : [{ x: a.x, y: a.y, w: a.w, h: a.h }];
      return zonas.some(
        (r) => x >= r.x && x <= r.x + r.w && y >= r.y && y <= r.y + r.h,
      );
    });
    if (hit) setNotePopover(hit);
  }

  function startAnnotAction(
    e: MouseEvent<HTMLDivElement>,
    a: AnnotationInfo,
    kind: "move" | "resize",
    handle: ResizeHandle = "se",
  ) {
    e.stopPropagation();
    if (e.button !== 0) return;
    const rect = (
      e.currentTarget.closest(".textlayer") as HTMLElement
    ).getBoundingClientRect();
    annotActionRef.current = {
      kind,
      handle,
      startX: (e.clientX - rect.left) / scale,
      startY: (e.clientY - rect.top) / scale,
      orig: a,
      moved: false,
    };
    setNotePopover(null);
    const d = { index: a.index, x: a.x, y: a.y, w: a.w, h: a.h };
    annotLiveRef.current = d;
    setAnnotDraft(d);
  }

  async function commitAnnot(a: AnnotationInfo, r: Rect) {
    if (!workPath) return;
    try {
      await transformAnnotation({
        workPath,
        pageIndex: index,
        annotIndex: a.index,
        x: r.x,
        y: r.y,
        w: r.w,
        h: r.h,
      });
      onAnnotated(index);
    } catch (e) {
      setAnnotDraft(null);
      onError(e);
    }
  }

  return {
    annots,
    notePopover,
    setNotePopover,
    noteDraft,
    setNoteDraft,
    strokePts,
    setStrokePts,
    strokeLiveRef,
    annotDraft,
    setAnnotDraft,
    annotActionRef,
    annotLiveRef,
    shapeDraft,
    setShapeDraft,
    shapeStartRef,
    shapeLiveRef,
    markupSelection,
    commitShape,
    placeStamp,
    submitNote,
    finishStroke,
    deleteAnnotation,
    onClickLayer,
    startAnnotAction,
    commitAnnot,
  };
}

export type Anotaciones = ReturnType<typeof useAnotaciones>;
