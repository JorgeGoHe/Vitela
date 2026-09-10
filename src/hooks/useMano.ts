import { useCallback, useEffect, useState, type MouseEvent, type RefObject } from "react";

/**
 * Herramienta Mano, como en Acrobat: **la barra espaciadora mantenida**
 * convierte el cursor en mano y arrastrar desplaza el documento; al soltarla
 * vuelve el modo que hubiera. No es un modo más de la fila —ya son once— ni
 * toca el documento: es un gesto, y está escrito en `DialogoAtajos`.
 */
export function useMano(
  viewerRef: RefObject<HTMLElement | null>,
  /** Sin documento no hay nada que desplazar. */
  habilitada: boolean,
) {
  const [activa, setActiva] = useState(false);
  const [arrastrando, setArrastrando] = useState(false);

  useEffect(() => {
    if (!habilitada) {
      setActiva(false);
      setArrastrando(false);
      return;
    }
    const esEspacio = (e: KeyboardEvent) => e.code === "Space" || e.key === " ";
    function onDown(e: KeyboardEvent) {
      if (!esEspacio(e)) return;
      // la barra espaciadora es de quien esté escribiendo, y de un botón con
      // el foco (que se pulsa con ella): la mano solo coge la que sobra
      const el = e.target as HTMLElement | null;
      const tag = el?.tagName;
      if (
        tag === "INPUT" ||
        tag === "TEXTAREA" ||
        tag === "SELECT" ||
        tag === "BUTTON" ||
        tag === "A" ||
        el?.isContentEditable
      )
        return;
      if (document.querySelector(".modal-backdrop, .menu-backdrop")) return;
      // sin esto la página baja una pantalla, que es lo que hace el navegador
      e.preventDefault();
      setActiva(true);
    }
    function onUp(e: KeyboardEvent) {
      if (!esEspacio(e)) return;
      setActiva(false);
      setArrastrando(false);
    }
    // la tecla se puede soltar con la ventana ya sin foco (⌘Tab): sin esto,
    // la mano se quedaba puesta al volver
    function onBlur() {
      setActiva(false);
      setArrastrando(false);
    }
    window.addEventListener("keydown", onDown);
    window.addEventListener("keyup", onUp);
    window.addEventListener("blur", onBlur);
    return () => {
      window.removeEventListener("keydown", onDown);
      window.removeEventListener("keyup", onUp);
      window.removeEventListener("blur", onBlur);
    };
  }, [habilitada]);

  /** Empieza el arrastre. Va en **fase de captura** sobre el visor: así la
   *  página no llega a ver el gesto y la herramienta que estuviera activa no
   *  dibuja nada mientras se desplaza. */
  const empieza = useCallback(
    (e: MouseEvent<HTMLElement>) => {
      const visor = viewerRef.current;
      if (!visor || e.button !== 0) return;
      e.preventDefault();
      e.stopPropagation();
      setArrastrando(true);
      const x0 = e.clientX;
      const y0 = e.clientY;
      const izq = visor.scrollLeft;
      const arriba = visor.scrollTop;
      function mueve(ev: globalThis.MouseEvent) {
        visor!.scrollLeft = izq - (ev.clientX - x0);
        visor!.scrollTop = arriba - (ev.clientY - y0);
      }
      function suelta() {
        setArrastrando(false);
        window.removeEventListener("mousemove", mueve);
        window.removeEventListener("mouseup", suelta);
      }
      window.addEventListener("mousemove", mueve);
      window.addEventListener("mouseup", suelta);
    },
    [viewerRef],
  );

  return { activa, arrastrando, empieza };
}
