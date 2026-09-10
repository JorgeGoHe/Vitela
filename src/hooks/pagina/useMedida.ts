import { useEffect, useRef, useState } from "react";
import { addShape, addTextBlock } from "../../api";
import {
  autorComentarios,
  formateaArea,
  formateaLongitud,
  hexToRgba,
  MOD,
  type Mode,
  type PageSize,
} from "../../tipos";
import type { ToolProps } from "../../components/Pagina";
import { puntoAPagina } from "./geometria";

/** Un trazo de medida sobre la página, en el espacio de la vista. */
export type Medida = { x1: number; y1: number; x2: number; y2: number };

/** Longitud de la diagonal (distancia) y área de la caja, en puntos. */
export function longitudDe(d: Medida): number {
  return Math.hypot(d.x2 - d.x1, d.y2 - d.y1);
}
export function areaDe(d: Medida): number {
  return Math.abs(d.x2 - d.x1) * Math.abs(d.y2 - d.y1);
}

/**
 * Medir distancia y área sobre la página (modo «Medir»). No toca el
 * documento salvo cuando se pide «dejar la medida puesta», que la escribe
 * como una forma con su texto al lado.
 *
 * La escala se fija una vez por documento con dos clics sobre algo de
 * medida conocida y se guarda por ruta; sin ella se mide el papel, que es
 * lo que hace Acrobat cuando el PDF no trae `/Measure`.
 */
export function useMedida(ctx: {
  workPath: string;
  index: number;
  mode: Mode;
  size: PageSize;
  tool: ToolProps;
  onPageMutated: (page: number) => void;
  onError: (e: unknown) => void;
  onNotice: (texto: string) => void;
}) {
  const { workPath, index, mode, size, tool, onPageMutated, onError, onNotice } =
    ctx;
  const [medidaDraft, setMedidaDraft] = useState<Medida | null>(null);
  const medidaStartRef = useRef<{ x: number; y: number } | null>(null);
  const medidaLiveRef = useRef<Medida | null>(null);
  // trazo de calibración a la espera de que se diga cuánto mide de verdad
  const [calibre, setCalibre] = useState<Medida | null>(null);

  // Al cambiar de modo: fuera borradores
  useEffect(() => {
    setMedidaDraft(null);
    setCalibre(null);
    medidaStartRef.current = null;
    medidaLiveRef.current = null;
  }, [mode]);

  /** La etiqueta que se ve mientras se arrastra y la que se deja puesta. */
  function etiquetaDe(d: Medida): string {
    return tool.medidaTipo === "area"
      ? formateaArea(areaDe(d), tool.escalaMm)
      : formateaLongitud(longitudDe(d), tool.escalaMm);
  }

  /** Fija la escala del documento a partir del trazo de calibración y de lo
   *  que el usuario dice que mide en la realidad. */
  function fijaEscala(realMm: number) {
    const d = calibre;
    setCalibre(null);
    if (!d || realMm <= 0) return;
    const pt = longitudDe(d);
    if (pt < 1) {
      onNotice("Ese trazo es demasiado corto para fijar la escala");
      return;
    }
    tool.onEscala(realMm / pt);
    onNotice(`Escala fijada: ese trazo mide ${formateaLongitud(pt, realMm / pt)}`);
  }

  /** «Dejar la medida puesta»: la línea o el recuadro con su texto al lado,
   *  con `add_shape` y `add_text_block`. Son dos pasos de deshacer, y se
   *  dice. */
  async function dejaMedida(d: Medida) {
    if (!workPath) return;
    const color = hexToRgba(tool.shapeColor);
    const p1 = puntoAPagina({ x: d.x1, y: d.y1 }, size);
    const p2 = puntoAPagina({ x: d.x2, y: d.y2 }, size);
    const texto = etiquetaDe(d);
    try {
      await addShape({
        workPath,
        pageIndex: index,
        kind: tool.medidaTipo === "area" ? "rect" : "line",
        x1: p1.x,
        y1: p1.y,
        x2: p2.x,
        y2: p2.y,
        stroke: color,
        fill: null,
        strokeWidth: 1,
        author: autorComentarios(),
      });
      const anclaje = puntoAPagina(
        { x: (d.x1 + d.x2) / 2 + 4, y: (d.y1 + d.y2) / 2 - 4 },
        size,
      );
      await addTextBlock({
        workPath,
        pageIndex: index,
        x: anclaje.x,
        y: anclaje.y,
        text: texto,
        fontSize: 9,
        color,
      });
      onPageMutated(index);
      onNotice(`Medida puesta: ${texto} · ${MOD}Z la quita`);
    } catch (e) {
      onError(e);
    }
  }

  return {
    medidaDraft,
    setMedidaDraft,
    medidaStartRef,
    medidaLiveRef,
    calibre,
    setCalibre,
    etiquetaDe,
    fijaEscala,
    dejaMedida,
  };
}

export type MedidaHook = ReturnType<typeof useMedida>;
