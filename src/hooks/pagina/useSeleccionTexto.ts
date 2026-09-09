import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "../../ipc";
import {
  charIndexAt,
  copyToClipboard,
  mergeLineRects,
  type Mode,
  type PageSize,
  type PageText,
  type Selection,
} from "../../tipos";
import { rectAVista } from "./geometria";

/**
 * Capa de texto de una página: los caracteres con sus cajas de glifos y la
 * selección por arrastre (ancla + posición del mousedown para ignorar el
 * jitter de un clic simple). Los handlers de ratón viven en `Pagina`.
 */
export function useSeleccionTexto(ctx: {
  workPath: string;
  index: number;
  visible: boolean;
  docVersion: number;
  pageVersion: number;
  mode: Mode;
  size: PageSize;
  selOwner: number | null;
  /** Es la página que está leyendo el usuario (la de la píldora). */
  esActual: boolean;
  claimSel: (page: number | null) => void;
  onError: (e: unknown) => void;
  onNotice: (texto: string) => void;
}) {
  const {
    workPath,
    index,
    visible,
    docVersion,
    pageVersion,
    mode,
    size,
    selOwner,
    esActual,
    claimSel,
    onError,
    onNotice,
  } = ctx;
  const [pageText, setPageText] = useState<PageText | null>(null);
  const [selection, setSelection] = useState<Selection | null>(null);
  const [dragging, setDragging] = useState(false);
  const anchorRef = useRef<number | null>(null);
  const downPosRef = useRef<{ x: number; y: number } | null>(null);

  // Al cambiar de modo: fuera la selección
  useEffect(() => {
    setSelection(null);
  }, [mode]);

  // Solo una página puede tener la selección viva
  useEffect(() => {
    if (selOwner !== index) {
      setSelection(null);
    }
  }, [selOwner, index]);

  // Capa de texto de la página
  useEffect(() => {
    if (!workPath || !visible) return;
    let cancelled = false;
    setPageText(null);
    setSelection(null);
    invoke<PageText>("get_page_text", { path: workPath, pageIndex: index })
      .then((t) => {
        if (cancelled) return;
        // las cajas de glifo vienen en el espacio propio de la página: la
        // selección se pinta y se mide en el de la vista
        setPageText({
          ...t,
          chars: t.chars.map((c) => ({ ...c, ...rectAVista(c, size) })),
        });
      })
      .catch((e) => {
        if (!cancelled) onError(e);
      });
    return () => {
      cancelled = true;
    };
  }, [workPath, index, visible, docVersion, pageVersion, size, onError]);

  /** Doble clic selecciona la palabra y triple clic la línea (Acrobat). */
  const seleccionaBloque = useCallback(
    (x: number, y: number, linea: boolean) => {
      if (!pageText) return;
      const i = charIndexAt(pageText, x, y);
      if (i === null) return;
      const chars = pageText.chars;
      const separa = (c: string) =>
        /[\s.,;:!?()[\]{}"'«»¿¡…/\\]/.test(c);
      let start = i;
      let end = i;
      if (linea) {
        const alto = Math.max(1, chars[i].h);
        while (
          start > 0 &&
          Math.abs(chars[start - 1].y - chars[i].y) < alto * 0.7
        )
          start--;
        while (
          end < chars.length - 1 &&
          Math.abs(chars[end + 1].y - chars[i].y) < alto * 0.7
        )
          end++;
      } else {
        if (separa(chars[i].ch)) return;
        while (start > 0 && !separa(chars[start - 1].ch)) start--;
        while (end < chars.length - 1 && !separa(chars[end + 1].ch)) end++;
      }
      setSelection({ start, end });
    },
    [pageText],
  );

  // ⌘A selecciona todo el texto de la página que se está leyendo
  useEffect(() => {
    if (!esActual || mode !== "select" || !pageText) return;
    const chars = pageText.chars.length;
    if (chars === 0) return;
    function onKey(e: KeyboardEvent) {
      if (!(e.metaKey || e.ctrlKey) || (e.key !== "a" && e.key !== "A")) return;
      const tag = (e.target as HTMLElement)?.tagName;
      if (tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT") return;
      if (document.querySelector(".modal-backdrop")) return;
      e.preventDefault();
      claimSel(index);
      setSelection({ start: 0, end: chars - 1 });
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [esActual, mode, pageText, claimSel, index]);

  const copySelection = useCallback(() => {
    if (!selection || !pageText) return;
    const text = pageText.chars
      .slice(selection.start, selection.end + 1)
      .map((c) => c.ch)
      .join("");
    copyToClipboard(text);
    // el portapapeles no se ve: la banda de avisos ya está montada y acusar
    // recibo cuesta una línea
    onNotice("Texto copiado");
  }, [selection, pageText, onNotice]);

  // Copiar selección con ⌘C / Ctrl+C
  useEffect(() => {
    if (!selection) return;
    function onKey(e: KeyboardEvent) {
      if ((e.metaKey || e.ctrlKey) && e.key === "c") {
        copySelection();
        e.preventDefault();
      }
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [selection, copySelection]);

  const selectionRects =
    selection && pageText
      ? mergeLineRects(pageText.chars.slice(selection.start, selection.end + 1))
      : [];
  const lastSelRect =
    selectionRects.length > 0
      ? selectionRects[selectionRects.length - 1]
      : null;

  return {
    pageText,
    selection,
    setSelection,
    dragging,
    setDragging,
    anchorRef,
    downPosRef,
    seleccionaBloque,
    copySelection,
    selectionRects,
    lastSelRect,
  };
}

export type SeleccionTexto = ReturnType<typeof useSeleccionTexto>;
