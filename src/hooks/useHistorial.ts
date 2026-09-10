import { useCallback, useEffect, useRef, useState } from "react";
import {
  historyState,
  redoDocument,
  squashHistory,
  undoDocument,
  type HistoryState,
} from "../api";

/**
 * Deshacer/rehacer general: el backend guarda instantáneas de la copia de
 * trabajo antes de cada mutación; aquí solo se llevan los contadores y se
 * pide el refresco completo de la UI cuando se restaura una instantánea.
 */
export function useHistorial(opts: {
  workPath: string | null;
  /** Tras deshacer o rehacer: refresco general (el recuento puede cambiar).
   *  `pasos` son los que quedan por deshacer, que es lo que dice si el
   *  documento ha vuelto al punto en que se abrió o se guardó. */
  onRestaurado: (pageCount: number, pasos: number) => void;
  onError: (e: unknown) => void;
}) {
  const [estado, setEstado] = useState<Pick<HistoryState, "undo" | "redo">>({
    undo: 0,
    redo: 0,
  });
  const workRef = useRef(opts.workPath);
  workRef.current = opts.workPath;
  const optsRef = useRef(opts);
  optsRef.current = opts;
  // un segundo ⌘Z mientras el primero está en vuelo se ignora (si no, el
  // segundo fallaría con «Nada que deshacer» en el banner de error)
  const enCursoRef = useRef(false);

  useEffect(() => {
    setEstado({ undo: 0, redo: 0 });
    const w = opts.workPath;
    if (!w) return;
    historyState(w)
      .then((e) => {
        if (workRef.current === w) setEstado(e);
      })
      .catch(() => {});
  }, [opts.workPath]);

  /** Vuelve a leer los contadores (tras cualquier mutación). */
  const refrescar = useCallback(() => {
    const w = workRef.current;
    if (!w) return;
    historyState(w)
      .then((e) => {
        if (workRef.current === w) setEstado(e);
      })
      .catch(() => {});
  }, []);

  const mover = useCallback(async (atras: boolean) => {
    const w = workRef.current;
    if (!w || enCursoRef.current) return;
    enCursoRef.current = true;
    try {
      const e = await (atras ? undoDocument(w) : redoDocument(w));
      setEstado(e);
      optsRef.current.onRestaurado(e.page_count, e.undo);
    } catch (e) {
      optsRef.current.onError(e);
    } finally {
      enCursoRef.current = false;
    }
  }, []);

  const deshacer = useCallback(() => mover(true), [mover]);
  const rehacer = useCallback(() => mover(false), [mover]);

  /** Funde los últimos `pasos` en uno (acciones hechas con varios comandos). */
  const agrupar = useCallback(async (pasos: number) => {
    const w = workRef.current;
    if (!w) return;
    try {
      setEstado(await squashHistory(w, pasos));
    } catch (e) {
      optsRef.current.onError(e);
    }
  }, []);

  return {
    /** Pasos que quedan por deshacer: el «reloj» del documento. */
    pasos: estado.undo,
    puedeDeshacer: estado.undo > 0,
    puedeRehacer: estado.redo > 0,
    deshacer,
    rehacer,
    refrescar,
    agrupar,
  };
}
