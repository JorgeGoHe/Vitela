/**
 * Geometría pura de la página: sin hooks ni estado. Las coordenadas son
 * puntos PDF (origen arriba-izquierda); `scale` convierte a píxeles CSS.
 */
import type { MouseEvent } from "react";
import type { Rect, ResizeHandle } from "../../tipos";

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

/** Punto del ratón en coordenadas de página (puntos PDF). */
export function pagePoint(e: MouseEvent<HTMLDivElement>, scale: number) {
  const rect = e.currentTarget.getBoundingClientRect();
  return {
    x: (e.clientX - rect.left) / scale,
    y: (e.clientY - rect.top) / scale,
  };
}

/** Evita que una tarjeta flotante se salga del borde de la página. */
export function clampCardLeft(left: number, displayWidth: number, w = 260) {
  return Math.max(0, Math.min(left, displayWidth - w));
}
