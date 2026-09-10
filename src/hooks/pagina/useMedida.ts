import { useEffect, useRef, useState } from "react";
import { addTextBlock } from "../../api";
import { invoke } from "../../ipc";
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

/** Un vértice de una polilínea, en el espacio de la vista. */
export type Vertice = { x: number; y: number };

/** Longitud de la diagonal (la distancia entre dos puntos), en puntos. */
export function longitudDe(d: Medida): number {
  return Math.hypot(d.x2 - d.x1, d.y2 - d.y1);
}

/** Perímetro de una polilínea: la suma de sus tramos. Con `cerrada` cuenta
 *  también el tramo que vuelve al primer vértice, que es lo que mide el
 *  contorno de una parcela. */
export function perimetroDe(pts: Vertice[], cerrada = false): number {
  let total = 0;
  for (let i = 1; i < pts.length; i++)
    total += Math.hypot(pts[i].x - pts[i - 1].x, pts[i].y - pts[i - 1].y);
  if (cerrada && pts.length > 2)
    total += Math.hypot(
      pts[0].x - pts[pts.length - 1].x,
      pts[0].y - pts[pts.length - 1].y,
    );
  return total;
}

/** Área de un polígono por la fórmula del cordón de zapato (Gauss): la de
 *  verdad, no la de la caja que lo envuelve. El polígono se cierra solo. */
export function areaPoligono(pts: Vertice[]): number {
  if (pts.length < 3) return 0;
  let suma = 0;
  for (let i = 0; i < pts.length; i++) {
    const a = pts[i];
    const b = pts[(i + 1) % pts.length];
    suma += a.x * b.y - b.x * a.y;
  }
  return Math.abs(suma) / 2;
}

/** Cuántos vértices necesita cada herramienta para tener una medida. */
const MINIMO: Record<string, number> = { perimetro: 2, area: 3 };

/**
 * Medir sobre la página (modo «Medir»), con las tres herramientas de
 * Acrobat: Distancia entre dos puntos —un arrastre—, y Perímetro y Área
 * **por vértices**: clic por punto, doble clic o Enter cierra, Retroceso
 * quita el último y Esc cancela. No toca el documento salvo cuando se pide
 * «dejar la medida puesta», que la escribe como un trazo con su texto al
 * lado.
 *
 * La escala se fija una vez por documento con un arrastre sobre algo de
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
  /** Funde los últimos pasos del historial en uno: dejar una medida son dos
   *  comandos (el trazo y su texto) y un solo gesto del usuario. */
  onAgrupar: (pasos: number) => void;
  onError: (e: unknown) => void;
  onNotice: (texto: string) => void;
}) {
  const {
    workPath,
    index,
    mode,
    size,
    tool,
    onPageMutated,
    onAgrupar,
    onError,
    onNotice,
  } = ctx;
  const [medidaDraft, setMedidaDraft] = useState<Medida | null>(null);
  const medidaStartRef = useRef<{ x: number; y: number } | null>(null);
  const medidaLiveRef = useRef<Medida | null>(null);
  // trazo de calibración a la espera de que se diga cuánto mide de verdad
  const [calibre, setCalibre] = useState<Medida | null>(null);
  // perímetro y área: los vértices puestos hasta ahora, dónde está el ratón
  // (el tramo que se ve moverse) y si la figura ya está cerrada
  const [vertices, setVertices] = useState<Vertice[]>([]);
  const [cursor, setCursor] = useState<Vertice | null>(null);
  const [cerrada, setCerrada] = useState(false);
  const tipo = tool.medidaTipo;
  const porVertices = tipo === "perimetro" || tipo === "area";

  // Al cambiar de modo o de herramienta: fuera borradores
  useEffect(() => {
    setMedidaDraft(null);
    setCalibre(null);
    setVertices([]);
    setCursor(null);
    setCerrada(false);
    medidaStartRef.current = null;
    medidaLiveRef.current = null;
  }, [mode, tipo]);

  /** La etiqueta de un arrastre: la distancia entre sus dos puntos. */
  function etiquetaDe(d: Medida): string {
    return formateaLongitud(longitudDe(d), tool.escalaMm);
  }

  /** La cifra de la figura que se está poniendo: el perímetro suma los
   *  tramos y el área usa la fórmula del polígono. Mientras se coloca, el
   *  vértice de debajo del ratón cuenta, que es lo que hace que la cifra se
   *  actualice en vivo. */
  function etiquetaPoligono(pts: Vertice[]): string {
    return tipo === "area"
      ? formateaArea(areaPoligono(pts), tool.escalaMm)
      : formateaLongitud(perimetroDe(pts, false), tool.escalaMm);
  }

  /** Los vértices que se ven ahora mismo: los puestos más el del ratón
   *  mientras la figura sigue abierta. */
  const enCurso: Vertice[] =
    cerrada || !cursor || vertices.length === 0 ? vertices : [...vertices, cursor];

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

  /** «Dejar la medida puesta»: el trazo con su texto al lado. Son dos pasos
   *  de deshacer, y se dice. */
  async function escribeMedida(pts: Vertice[], texto: string) {
    if (!workPath || pts.length < 2) return;
    const color = hexToRgba(tool.shapeColor);
    // el trazo entero en UNA llamada: `add_stroke` acepta la polilínea
    // completa, así que una medida es una anotación y no una por tramo
    const puntos = pts.map((p) => {
      const q = puntoAPagina(p, size);
      return [q.x, q.y] as [number, number];
    });
    try {
      await invoke("add_stroke", {
        workPath,
        pageIndex: index,
        points: puntos,
        color,
        width: 1,
        author: autorComentarios(),
      });
      // el texto va junto al centro de la figura, apartado del trazo
      const cx = pts.reduce((s, p) => s + p.x, 0) / pts.length;
      const cy = pts.reduce((s, p) => s + p.y, 0) / pts.length;
      const anclaje = puntoAPagina({ x: cx + 4, y: cy - 4 }, size);
      await addTextBlock({
        workPath,
        pageIndex: index,
        x: anclaje.x,
        y: anclaje.y,
        text: texto,
        fontSize: 9,
        color,
      });
      // el trazo y su texto son dos comandos y un solo gesto: se funden en
      // un paso de historial, o «⌘Z la quita» pedía dos ⌘Z (AC-069)
      onAgrupar(2);
      onPageMutated(index);
      onNotice(`Medida puesta: ${texto} · ${MOD}Z la quita`);
    } catch (e) {
      onError(e);
    }
  }

  /** Distancia: el arrastre de siempre, una línea entre dos puntos. */
  function dejaMedida(d: Medida) {
    escribeMedida(
      [
        { x: d.x1, y: d.y1 },
        { x: d.x2, y: d.y2 },
      ],
      etiquetaDe(d),
    );
  }

  /** «Dejarla puesta» **después** de medir: quien mide, ve el número y
   *  luego quiere conservarlo no tiene que marcar una casilla y volver a
   *  medir. Es el mismo trabajo que hace la casilla de la fila contextual,
   *  con la medida que hay en pantalla. */
  function dejaLaDeAhora() {
    if (medidaDraft) {
      dejaMedida(medidaDraft);
      setMedidaDraft(null);
      return;
    }
    if (!cerrada || vertices.length < 2) return;
    const pts = tipo === "area" ? [...vertices, vertices[0]] : vertices;
    escribeMedida(pts, etiquetaPoligono(vertices));
    setVertices([]);
    setCerrada(false);
  }

  /** Clic en la página con una herramienta de vértices: pone uno más, o
   *  empieza una figura nueva si la anterior estaba cerrada. */
  function anadeVertice(p: Vertice) {
    if (cerrada) {
      setCerrada(false);
      setVertices([p]);
      return;
    }
    setVertices((v) => [...v, p]);
  }

  /** Cierra la figura (doble clic o Enter): si llega al mínimo de vértices
   *  se queda en pantalla con su cifra, y solo toca el documento si se ha
   *  pedido dejarla puesta. */
  function cierra() {
    if (cerrada) return;
    const minimo = MINIMO[tipo] ?? 2;
    if (vertices.length < minimo) {
      if (vertices.length > 0)
        onNotice(
          tipo === "area"
            ? "Un área necesita al menos tres puntos"
            : "Un perímetro necesita al menos dos puntos",
        );
      setVertices([]);
      setCursor(null);
      return;
    }
    setCerrada(true);
    setCursor(null);
    if (tool.medidaDejar) {
      const pts = tipo === "area" ? [...vertices, vertices[0]] : vertices;
      escribeMedida(
        pts,
        tipo === "area"
          ? formateaArea(areaPoligono(vertices), tool.escalaMm)
          : formateaLongitud(perimetroDe(vertices, false), tool.escalaMm),
      );
    }
  }

  // el listener del teclado se suscribe una vez por figura, no por render,
  // así que llama al `cierra` de ahora mismo a través de un ref
  const cierraRef = useRef(cierra);
  useEffect(() => {
    cierraRef.current = cierra;
  });

  /** Teclado de la figura en curso: Enter cierra, Retroceso quita el último
   *  vértice y Esc cancela. En fase de captura y parando el evento, porque
   *  Esc es también «salir de la herramienta» y aquí manda lo que se está
   *  poniendo. */
  useEffect(() => {
    if (mode !== "medir" || !porVertices) return;
    if (vertices.length === 0) return;
    function onKey(e: KeyboardEvent) {
      // con el foco en la tarjeta de la escala manda el campo, no la figura
      const tag = (e.target as HTMLElement)?.tagName;
      if (tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT") return;
      if (e.key === "Enter") {
        e.preventDefault();
        e.stopPropagation();
        cierraRef.current();
      } else if (e.key === "Backspace" || e.key === "Delete") {
        e.preventDefault();
        e.stopPropagation();
        setVertices((v) => v.slice(0, -1));
        setCerrada(false);
      } else if (e.key === "Escape") {
        e.preventDefault();
        e.stopPropagation();
        setVertices([]);
        setCursor(null);
        setCerrada(false);
      }
    }
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [mode, porVertices, vertices.length]);

  return {
    medidaDraft,
    setMedidaDraft,
    medidaStartRef,
    medidaLiveRef,
    calibre,
    setCalibre,
    etiquetaDe,
    etiquetaPoligono,
    fijaEscala,
    dejaMedida,
    vertices,
    enCurso,
    cerrada,
    cursor,
    setCursor,
    anadeVertice,
    cierra,
    dejaLaDeAhora,
    /** Hay una medida hecha en pantalla que se puede dejar puesta. */
    hayMedida: medidaDraft !== null || (cerrada && vertices.length >= 2),
  };
}

export type MedidaHook = ReturnType<typeof useMedida>;
