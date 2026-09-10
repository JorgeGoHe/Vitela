/**
 * Reglas, guías y cuadrícula: el andamio de quien coloca sellos, campos y
 * cuadros de texto, que hasta ahora se colocaban a ojo. **No toca el
 * fichero**: las guías viven en `localStorage` por ruta, igual que la
 * escala de medida, y ni se imprimen ni se exportan.
 *
 * Va en su propia capa y no coge el ratón salvo en las dos reglas, de las
 * que se arrastra para dejar una guía: los despachadores de la página no
 * se enteran de que existe.
 */
import { useRef, type MouseEvent } from "react";
import type { Guias } from "../../tipos";

/** Grosor de las reglas, en píxeles de pantalla. */
const REGLA = 16;

type Props = {
  reglas: boolean;
  cuadricula: boolean;
  guiasVisibles: boolean;
  guias: Guias;
  /** Milímetros por punto: la escala del documento si se ha fijado. */
  escalaMm: number;
  scale: number;
  displayWidth: number;
  displayHeight: number;
  onGuia: (eje: "v" | "h", valor: number) => void;
  onQuitarGuia: (eje: "v" | "h", valor: number) => void;
};

/** Las marcas de una regla: una cada 10 mm de los de verdad, con su cifra
 *  cada 50. Devuelve la posición en puntos de página. */
function marcas(largoPt: number, escalaMm: number): number[] {
  const paso = 10 / Math.max(0.0001, escalaMm);
  if (paso <= 2) return [];
  const out: number[] = [];
  for (let x = 0; x <= largoPt; x += paso) out.push(x);
  return out;
}

export default function CapaGuias({
  reglas,
  cuadricula,
  guiasVisibles,
  guias,
  escalaMm,
  scale,
  displayWidth,
  displayHeight,
  onGuia,
  onQuitarGuia,
}: Props) {
  const arrastreRef = useRef<"v" | "h" | null>(null);

  if (!reglas && !cuadricula && !(guiasVisibles && (guias.v.length || guias.h.length)))
    return null;

  const anchoPt = displayWidth / scale;
  const altoPt = displayHeight / scale;
  const pasoPt = 10 / Math.max(0.0001, escalaMm);

  /** Arrastrar desde una regla deja una guía donde se suelte. */
  function empiezaGuia(e: MouseEvent<HTMLDivElement>, eje: "v" | "h") {
    e.preventDefault();
    e.stopPropagation();
    arrastreRef.current = eje;
    const caja = e.currentTarget.parentElement?.getBoundingClientRect();
    if (!caja) return;
    function mover(ev: globalThis.MouseEvent) {
      ev.preventDefault();
    }
    function soltar(ev: globalThis.MouseEvent) {
      window.removeEventListener("mousemove", mover);
      window.removeEventListener("mouseup", soltar);
      const ejeArrastre = arrastreRef.current;
      arrastreRef.current = null;
      if (!ejeArrastre || !caja) return;
      const valor =
        ejeArrastre === "v"
          ? (ev.clientX - caja.left) / scale
          : (ev.clientY - caja.top) / scale;
      if (valor < 0 || valor > (ejeArrastre === "v" ? anchoPt : altoPt)) return;
      onGuia(ejeArrastre, Math.round(valor * 10) / 10);
    }
    window.addEventListener("mousemove", mover);
    window.addEventListener("mouseup", soltar);
  }

  return (
    <>
      {cuadricula && (
        <div
          className="capa-cuadricula"
          style={{
            backgroundSize: `${pasoPt * scale}px ${pasoPt * scale}px`,
          }}
        />
      )}
      {guiasVisibles &&
        guias.v.map((x) => (
          <div
            key={`v${x}`}
            className="guia guia-v"
            style={{ left: x * scale }}
            title="Guía · doble clic la quita"
            onMouseDown={(e) => e.stopPropagation()}
            onDoubleClick={(e) => {
              e.stopPropagation();
              onQuitarGuia("v", x);
            }}
          />
        ))}
      {guiasVisibles &&
        guias.h.map((y) => (
          <div
            key={`h${y}`}
            className="guia guia-h"
            style={{ top: y * scale }}
            title="Guía · doble clic la quita"
            onMouseDown={(e) => e.stopPropagation()}
            onDoubleClick={(e) => {
              e.stopPropagation();
              onQuitarGuia("h", y);
            }}
          />
        ))}
      {reglas && (
        <>
          <div
            className="regla regla-h"
            style={{ height: REGLA, left: 0, right: 0, top: -REGLA }}
            title="Arrastra hacia abajo para dejar una guía"
            onMouseDown={(e) => empiezaGuia(e, "h")}
          >
            {marcas(anchoPt, escalaMm).map((x, i) => (
              <span
                key={x}
                className={`regla-marca${i % 5 === 0 ? " mayor" : ""}`}
                style={{ left: x * scale }}
              >
                {i % 5 === 0 && i > 0 && (
                  <b className="dato">{Math.round(x * escalaMm)}</b>
                )}
              </span>
            ))}
          </div>
          <div
            className="regla regla-v"
            style={{ width: REGLA, top: 0, bottom: 0, left: -REGLA }}
            title="Arrastra hacia la derecha para dejar una guía"
            onMouseDown={(e) => empiezaGuia(e, "v")}
          >
            {marcas(altoPt, escalaMm).map((y, i) => (
              <span
                key={y}
                className={`regla-marca${i % 5 === 0 ? " mayor" : ""}`}
                style={{ top: y * scale }}
              />
            ))}
          </div>
        </>
      )}
    </>
  );
}
