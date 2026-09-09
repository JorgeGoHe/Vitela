/**
 * Geometría pura de la página: sin hooks ni estado. Las coordenadas son
 * puntos PDF (origen arriba-izquierda); `scale` convierte a píxeles CSS.
 */
import type { MouseEvent } from "react";
import type { PageSize, Rect, ResizeHandle } from "../../tipos";

/**
 * Rotación de la página normalizada a 0/90/180/270. Un documento abierto con
 * un backend antiguo puede no traerla: sin ella, todo se comporta como antes.
 */
function giro(size: PageSize): number {
  const r = Number(size.rotation) || 0;
  return ((Math.round(r / 90) * 90) % 360 + 360) % 360;
}

/**
 * Punto del espacio de la página VISTA (el del render, el ratón y todos los
 * overlays) al espacio propio de la página, que es en el que están escritas
 * las anotaciones y el que esperan los comandos que crean o mueven algo.
 * Con /Rotate 0 es la identidad, así que no cambia nada en la mayoría de los
 * documentos.
 */
export function puntoAPagina(
  p: { x: number; y: number },
  size: PageSize,
): { x: number; y: number } {
  switch (giro(size)) {
    case 90:
      return { x: p.y, y: size.width - p.x };
    case 180:
      return { x: size.width - p.x, y: size.height - p.y };
    case 270:
      return { x: size.height - p.y, y: p.x };
    default:
      return { x: p.x, y: p.y };
  }
}

/** El camino de vuelta: del espacio propio de la página al de la vista. */
export function puntoAVista(
  p: { x: number; y: number },
  size: PageSize,
): { x: number; y: number } {
  switch (giro(size)) {
    case 90:
      return { x: size.width - p.y, y: p.x };
    case 180:
      return { x: size.width - p.x, y: size.height - p.y };
    case 270:
      return { x: p.y, y: size.height - p.x };
    default:
      return { x: p.x, y: p.y };
  }
}

/** Un rectángulo se convierte por sus dos esquinas y se vuelve a normalizar
 *  (girar 90° intercambia ancho y alto). */
function convierteRect(
  r: Rect,
  size: PageSize,
  f: (p: { x: number; y: number }, s: PageSize) => { x: number; y: number },
): Rect {
  const a = f({ x: r.x, y: r.y }, size);
  const b = f({ x: r.x + r.w, y: r.y + r.h }, size);
  return {
    x: Math.min(a.x, b.x),
    y: Math.min(a.y, b.y),
    w: Math.abs(b.x - a.x),
    h: Math.abs(b.y - a.y),
  };
}

export function rectAPagina(r: Rect, size: PageSize): Rect {
  return convierteRect(r, size, puntoAPagina);
}

export function rectAVista(r: Rect, size: PageSize): Rect {
  return convierteRect(r, size, puntoAVista);
}

/** Redimensiona un rect desde un tirador: bordes mueven un solo eje,
 *  esquinas mantienen la proporción (con Shift, libre). */
export function resizeRect(
  o: Rect,
  hd: ResizeHandle,
  dx: number,
  dy: number,
  free: boolean,
): Rect {
  let left = o.x;
  let top = o.y;
  let right = o.x + o.w;
  let bottom = o.y + o.h;
  if (hd.includes("e")) right = Math.max(left + 8, right + dx);
  if (hd.includes("w")) left = Math.min(right - 8, left + dx);
  if (hd.includes("s")) bottom = Math.max(top + 8, bottom + dy);
  if (hd.includes("n")) top = Math.min(bottom - 8, top + dy);
  let w = right - left;
  let h = bottom - top;
  const esquina = hd.length === 2;
  if (esquina && !free && o.w > 0 && o.h > 0) {
    const ratio = o.h / o.w;
    if (Math.abs(w - o.w) >= Math.abs(h - o.h)) h = w * ratio;
    else w = h / ratio;
    // el ancla es la esquina opuesta al tirador
    if (hd.includes("w")) left = right - w;
    if (hd.includes("n")) top = bottom - h;
  }
  return { x: left, y: top, w, h };
}

/** Punto del ratón dentro de una capa, deshaciendo el giro de la vista
 *  (⇧⌘+/⇧⌘−), que es solo una rotación CSS de la hoja: el rect que devuelve
 *  el navegador para un elemento girado es su caja envolvente. */
function puntoEnRect(
  rect: DOMRect,
  clientX: number,
  clientY: number,
  scale: number,
  viewRotation: number,
): { x: number; y: number } {
  const p = { x: (clientX - rect.left) / scale, y: (clientY - rect.top) / scale };
  if (!viewRotation) return p;
  return puntoAPagina(p, {
    width: rect.width / scale,
    height: rect.height / scale,
    rotation: viewRotation,
  });
}

/** Punto del ratón en coordenadas de página (puntos PDF). */
export function pagePoint(
  e: MouseEvent<HTMLDivElement>,
  scale: number,
  viewRotation = 0,
) {
  return puntoEnRect(
    e.currentTarget.getBoundingClientRect(),
    e.clientX,
    e.clientY,
    scale,
    viewRotation,
  );
}

/** Igual, pero desde un overlay: la referencia es la capa de texto. */
export function puntoEnCapa(
  e: MouseEvent<HTMLElement>,
  scale: number,
  viewRotation = 0,
) {
  const capa = e.currentTarget.closest(".textlayer") as HTMLElement;
  return puntoEnRect(
    capa.getBoundingClientRect(),
    e.clientX,
    e.clientY,
    scale,
    viewRotation,
  );
}

/** Evita que una tarjeta flotante se salga del borde de la página. */
export function clampCardLeft(left: number, displayWidth: number, w = 260) {
  return Math.max(0, Math.min(left, displayWidth - w));
}
