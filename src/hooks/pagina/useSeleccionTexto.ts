import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "../../ipc";
import {
  copyToClipboard,
  mergeLineRects,
  type Mode,
  type PageText,
  type Selection,
} from "../../tipos";

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
  selOwner: number | null;
  onError: (e: unknown) => void;
}) {
  const { workPath, index, visible, docVersion, pageVersion, mode, selOwner, onError } = ctx;
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
        if (!cancelled) setPageText(t);
      })
      .catch((e) => {
        if (!cancelled) onError(e);
      });
    return () => {
      cancelled = true;
    };
  }, [workPath, index, visible, docVersion, pageVersion, onError]);

  const copySelection = useCallback(() => {
    if (!selection || !pageText) return;
    const text = pageText.chars
      .slice(selection.start, selection.end + 1)
      .map((c) => c.ch)
      .join("");
    copyToClipboard(text);
  }, [selection, pageText]);

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
    copySelection,
    selectionRects,
    lastSelRect,
  };
}

export type SeleccionTexto = ReturnType<typeof useSeleccionTexto>;
