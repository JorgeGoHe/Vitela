import { useEffect, useRef, useState } from "react";
import { cropPage, redactArea, stampSignature, type RedactReport } from "../../api";
import type { Mode, Rect } from "../../tipos";

/**
 * Herramientas de área rectangular: recorte (modo crop), redacción (modo
 * redact, con informe previo) y estampado de la firma activa (modo firmar).
 * Los handlers de ratón viven en `Pagina`.
 */
export function useAreas(ctx: {
  workPath: string;
  index: number;
  pageCount: number;
  mode: Mode;
  activeSig: { png: string; ratio: number } | null;
  onPageMutated: (page: number) => void;
  onDocMutated: (newCount: number, nextPage?: number) => void;
  onError: (e: unknown) => void;
  onModeChange: (m: Mode) => void;
  onSigStamped: () => void;
}) {
  const {
    workPath,
    index,
    pageCount,
    mode,
    activeSig,
    onPageMutated,
    onDocMutated,
    onError,
    onModeChange,
    onSigStamped,
  } = ctx;
  const [cropDraft, setCropDraft] = useState<Rect | null>(null);
  const cropStartRef = useRef<{ x: number; y: number } | null>(null);
  const [redactDraft, setRedactDraft] = useState<Rect | null>(null);
  const redactStartRef = useRef<{ x: number; y: number } | null>(null);
  const redactLiveRef = useRef<Rect | null>(null);
  const [redactReport, setRedactReport] = useState<RedactReport | null>(null);
  const [sigDraft, setSigDraft] = useState<Rect | null>(null);
  const sigLiveRef = useRef<Rect | null>(null);
  const sigDragRef = useRef<{ x: number; y: number } | null>(null);

  // Al cambiar de modo: fuera borradores y estado transitorio
  useEffect(() => {
    setSigDraft(null);
    sigLiveRef.current = null;
    sigDragRef.current = null;
    setCropDraft(null);
    cropStartRef.current = null;
    setRedactDraft(null);
    setRedactReport(null);
    redactStartRef.current = null;
    redactLiveRef.current = null;
  }, [mode]);

  async function applyCrop(allPages: boolean) {
    if (!workPath || !cropDraft) return;
    try {
      await cropPage(workPath, index, cropDraft, allPages);
      setCropDraft(null);
      onModeChange("select");
      onDocMutated(pageCount);
    } catch (e) {
      onError(e);
    }
  }

  async function previewRedact(r: Rect) {
    if (!workPath) return;
    try {
      setRedactReport(await redactArea(workPath, index, r, true));
    } catch (e) {
      onError(e);
    }
  }

  async function applyRedact() {
    if (!workPath || !redactDraft) return;
    try {
      await redactArea(workPath, index, redactDraft, false);
      setRedactDraft(null);
      setRedactReport(null);
      onModeChange("select");
      onPageMutated(index);
    } catch (e) {
      onError(e);
    }
  }

  async function stampActiveSignature(r: Rect) {
    if (!workPath || !activeSig) return;
    try {
      await stampSignature({
        workPath,
        pageIndex: index,
        pngBase64: activeSig.png,
        x: r.x,
        y: r.y,
        w: r.w,
        h: r.h,
      });
      setSigDraft(null);
      // la firma estampada es una imagen: el modo imagen permite moverla,
      // redimensionarla o borrarla al instante
      onSigStamped();
      onPageMutated(index);
    } catch (e) {
      onError(e);
    }
  }

  return {
    cropDraft,
    setCropDraft,
    cropStartRef,
    redactDraft,
    setRedactDraft,
    redactStartRef,
    redactLiveRef,
    redactReport,
    setRedactReport,
    sigDraft,
    setSigDraft,
    sigLiveRef,
    sigDragRef,
    applyCrop,
    previewRedact,
    applyRedact,
    stampActiveSignature,
  };
}

export type Areas = ReturnType<typeof useAreas>;
