import { useEffect, useRef, useState } from "react";
import { cropPage, markRedaction, stampSignature } from "../../api";
import type { Mode, PageSize, Rect } from "../../tipos";
import { rectAPagina } from "./geometria";

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
  size: PageSize;
  activeSig: { png: string; ratio: number } | null;
  onPageMutated: (page: number) => void;
  onDocMutated: (newCount: number, nextPage?: number) => void;
  onError: (e: unknown) => void;
  onModeChange: (m: Mode) => void;
  onSigStamped: () => void;
  /** La lista de marcas del documento ha cambiado: que App la relea. */
  onMarcasCambian: () => void;
}) {
  const {
    workPath,
    index,
    pageCount,
    mode,
    size,
    activeSig,
    onPageMutated,
    onDocMutated,
    onError,
    onModeChange,
    onSigStamped,
    onMarcasCambian,
  } = ctx;
  const [cropDraft, setCropDraft] = useState<Rect | null>(null);
  const cropStartRef = useRef<{ x: number; y: number } | null>(null);
  const [redactDraft, setRedactDraft] = useState<Rect | null>(null);
  const redactStartRef = useRef<{ x: number; y: number } | null>(null);
  const redactLiveRef = useRef<Rect | null>(null);
  // recuadro de la firma con certificado: el rectángulo se dibuja aquí y lo
  // resuelve App, que es quien tiene el diálogo y el destino
  const [certDraft, setCertDraft] = useState<Rect | null>(null);
  const certStartRef = useRef<{ x: number; y: number } | null>(null);
  const certLiveRef = useRef<Rect | null>(null);
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
    redactStartRef.current = null;
    redactLiveRef.current = null;
    setCertDraft(null);
    certStartRef.current = null;
    certLiveRef.current = null;
  }, [mode]);

  async function applyCrop(allPages: boolean) {
    if (!workPath || !cropDraft) return;
    try {
      await cropPage(workPath, index, rectAPagina(cropDraft, size), allPages);
      setCropDraft(null);
      onModeChange("select");
      onDocMutated(pageCount);
    } catch (e) {
      onError(e);
    }
  }

  /** Marcar NO borra: deja una zona roja revisable, como en Acrobat. Lo
   *  destructivo es «Aplicar redacción», y va aparte. */
  async function marcarRedaccion(r: Rect) {
    if (!workPath) return;
    try {
      await markRedaction(workPath, index, rectAPagina(r, size));
      setRedactDraft(null);
      onMarcasCambian();
      onPageMutated(index);
    } catch (e) {
      onError(e);
    }
  }

  async function stampActiveSignature(r: Rect) {
    if (!workPath || !activeSig) return;
    const pr = rectAPagina(r, size);
    try {
      await stampSignature({
        workPath,
        pageIndex: index,
        pngBase64: activeSig.png,
        x: pr.x,
        y: pr.y,
        w: pr.w,
        h: pr.h,
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
    certDraft,
    setCertDraft,
    certStartRef,
    certLiveRef,
    cropDraft,
    setCropDraft,
    cropStartRef,
    redactDraft,
    setRedactDraft,
    redactStartRef,
    redactLiveRef,
    sigDraft,
    setSigDraft,
    sigLiveRef,
    sigDragRef,
    applyCrop,
    marcarRedaccion,
    stampActiveSignature,
  };
}

export type Areas = ReturnType<typeof useAreas>;
