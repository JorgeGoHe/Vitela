import { useEffect, useRef, useState } from "react";
import { comparePdf, renderPageSrc, type Diferencia } from "../api";
import { invoke } from "../ipc";
import { MOD, plural, type PageSize } from "../tipos";
import Icon from "./Icon";

/** Ancho de partida mientras no se ha medido el panel. Cada hoja se
 *  rasteriza al ancho de **su** panel: con un ancho fijo, en una ventana
 *  normal la página no cabía y se veía un tercio. */
const ANCHO_INICIAL = 620;

/** El ancho se redondea a este múltiplo para no volver a rasterizar las dos
 *  hojas en cada píxel de un arrastre de ventana. */
const PASO = 20;

/** Lo que ocupa el `padding` del panel a los dos lados. */
const MARGEN = 32;

/** El color de cada tipo de diferencia, con los de la paleta de anotación:
 *  amarillo lo que cambia, verde lo que llega, rojo lo que se va. */
const COLOR: Record<Diferencia["tipo"], string> = {
  igual: "transparent",
  cambiado: "#f5c400",
  añadido: "#2ea043",
  quitado: "#c0392b",
};

const ETIQUETA: Record<Diferencia["tipo"], string> = {
  igual: "Igual",
  cambiado: "Cambiado",
  añadido: "Añadido",
  quitado: "Quitado",
};

/** La página que nombra una diferencia. Lo que se añade se cuenta por la
 *  hoja del documento nuevo y lo que se quita por la del viejo: es la hoja
 *  donde el usuario puede ir a mirarlo. Una página que solo está en un lado
 *  trae el otro en `null` y se usa el que haya. */
function paginaDe(d: Diferencia): number {
  const preferida = d.tipo === "añadido" ? d.pagina_b : d.pagina_a;
  return (preferida ?? d.pagina_b ?? d.pagina_a ?? 0) + 1;
}

/** Una hoja rasterizada de uno de los dos documentos, con sus rectángulos
 *  de diferencia encima. */
function Hoja({
  src,
  size,
  rects,
  color,
  resaltado,
  ancho,
  vacio,
}: {
  src: string | undefined;
  size: PageSize | undefined;
  rects: { x: number; y: number; w: number; h: number }[];
  color: string;
  resaltado: boolean;
  /** Ancho útil del panel: la hoja se dibuja a esa medida. */
  ancho: number;
  /** Qué decir cuando de este lado no hay hoja que enseñar. Un panel en
   *  blanco sin explicación se lee como que la comparación ha fallado. */
  vacio?: string;
}) {
  if (!src || !size)
    return (
      <div className="comparar-hueco">
        {vacio && <span className="sign-empty">{vacio}</span>}
      </div>
    );
  const escala = ancho / size.width;
  return (
    <div
      className="comparar-hoja"
      style={{ width: ancho, height: size.height * escala }}
    >
      <img src={src} alt="" draggable={false} />
      {rects.map((r, i) => (
        <span
          key={i}
          className={`comparar-marca${resaltado ? " on" : ""}`}
          style={{
            left: r.x * escala,
            top: r.y * escala,
            width: r.w * escala,
            height: r.h * escala,
            background: `color-mix(in srgb, ${color} 32%, transparent)`,
            outlineColor: color,
          }}
        />
      ))}
    </div>
  );
}

/**
 * Comparar dos PDF: **dos visores lado a lado con el scroll sincronizado**,
 * las diferencias pintadas encima y una lista a la izquierda para
 * recorrerlas con ⌘G y ⇧⌘G, como la búsqueda. Nada de un informe en PDF: la
 * gracia es verlo.
 *
 * No toca ninguno de los dos ficheros —solo lee— y salir no pregunta nada.
 */
export default function Comparador({
  workPath,
  nombreA,
  otroPath,
  onError,
  onClose,
}: {
  /** La copia de trabajo del documento abierto (el de la izquierda). */
  workPath: string;
  nombreA: string;
  /** El fichero con el que se compara (el de la derecha). */
  otroPath: string;
  onError: (e: unknown) => void;
  onClose: () => void;
}) {
  const [otroWork, setOtroWork] = useState<string | null>(null);
  const [difs, setDifs] = useState<Diferencia[] | null>(null);
  const [actual, setActual] = useState(0);
  const [sizesA, setSizesA] = useState<PageSize[]>([]);
  const [sizesB, setSizesB] = useState<PageSize[]>([]);
  const [hojasA, setHojasA] = useState<Record<number, string>>({});
  const [hojasB, setHojasB] = useState<Record<number, string>>({});
  const izqRef = useRef<HTMLDivElement | null>(null);
  const derRef = useRef<HTMLDivElement | null>(null);
  // la lista se recorre con ↑ y ↓ como el panel de comentarios: un roving
  // tabindex, una sola parada de tabulador y el foco siempre en la fila
  // señalada
  const filasRef = useRef<Map<number, HTMLButtonElement>>(new Map());
  const sincronizando = useRef(false);
  // el ancho al que se rasteriza cada hoja: el de su panel, medido
  const [ancho, setAncho] = useState(ANCHO_INICIAL);
  // el aviso de error se lee por referencia: si entrara en las dependencias
  // del efecto, cada render de App volvería a abrir el otro documento
  const onErrorRef = useRef(onError);
  onErrorRef.current = onError;
  const nombreB = otroPath.split(/[\\/]/).pop() ?? otroPath;

  // Abrir el otro documento, comparar y soltarlo al salir. El documento del
  // usuario no se toca: la comparación solo lee.
  useEffect(() => {
    let vivo = true;
    let copia: string | null = null;
    (async () => {
      try {
        const info = await invoke<{ work_path: string }>("open_pdf", {
          path: otroPath,
          password: null,
        });
        copia = info.work_path;
        if (!vivo) return;
        setOtroWork(info.work_path);
        const [a, b, d] = await Promise.all([
          invoke<PageSize[]>("get_page_sizes", { path: workPath }),
          invoke<PageSize[]>("get_page_sizes", { path: info.work_path }),
          comparePdf(workPath, info.work_path),
        ]);
        if (!vivo) return;
        setSizesA(a);
        setSizesB(b);
        setDifs(d.filter((x) => x.tipo !== "igual"));
      } catch (e) {
        if (vivo) onErrorRef.current(e);
      }
    })();
    return () => {
      vivo = false;
      if (copia)
        invoke("close_document", { workPath: copia }).catch(() => {});
    };
  }, [workPath, otroPath]);

  // El ancho de la hoja es el del panel que la enseña. Se mide con un
  // `ResizeObserver` y se redondea, para no rasterizar en cada píxel del
  // arrastre; al cambiar, las hojas ya pintadas se tiran y se vuelven a
  // pedir a la medida nueva.
  useEffect(() => {
    const el = izqRef.current;
    if (!el) return;
    const ro = new ResizeObserver(() => {
      const util = Math.max(180, el.clientWidth - MARGEN);
      const nuevo = Math.round(util / PASO) * PASO;
      setAncho((v) => (v === nuevo ? v : nuevo));
    });
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  useEffect(() => {
    setHojasA({});
    setHojasB({});
  }, [ancho]);

  const dif = difs?.[actual];

  // Las dos hojas de la diferencia señalada, rasterizadas a demanda: se
  // pintan las que se miran, no el documento entero
  useEffect(() => {
    if (!dif) return;
    let vivo = true;
    (async () => {
      try {
        if (dif.pagina_a !== null && hojasA[dif.pagina_a] === undefined) {
          const src = await renderPageSrc(workPath, dif.pagina_a, ancho);
          if (vivo) setHojasA((v) => ({ ...v, [dif.pagina_a as number]: src }));
        }
        if (
          otroWork &&
          dif.pagina_b !== null &&
          hojasB[dif.pagina_b] === undefined
        ) {
          const src = await renderPageSrc(otroWork, dif.pagina_b, ancho);
          if (vivo) setHojasB((v) => ({ ...v, [dif.pagina_b as number]: src }));
        }
      } catch (e) {
        if (vivo) onErrorRef.current(e);
      }
    })();
    return () => {
      vivo = false;
    };
  }, [dif, workPath, otroWork, hojasA, hojasB, ancho]);

  /** El scroll de un lado arrastra al otro, que es lo que hace que dos
   *  visores lado a lado sirvan para comparar. */
  function sincroniza(desde: "izq" | "der") {
    if (sincronizando.current) return;
    const a = izqRef.current;
    const b = derRef.current;
    if (!a || !b) return;
    sincronizando.current = true;
    if (desde === "izq") b.scrollTop = a.scrollTop;
    else a.scrollTop = b.scrollTop;
    requestAnimationFrame(() => {
      sincronizando.current = false;
    });
  }

  const total = difs?.length ?? 0;
  const irA = (delta: number) => {
    if (total === 0) return;
    setActual((i) => {
      const n = (i + delta + total) % total;
      // si el foco estaba en la lista, se lleva a la fila nueva; si estaba
      // en el botón de ⌘G, no se le quita
      requestAnimationFrame(() => {
        const fila = filasRef.current.get(n);
        if (fila && document.activeElement?.closest(".comparar-lista"))
          fila.focus();
      });
      return n;
    });
  };

  // ⌘G y ⇧⌘G recorren las diferencias, como recorren las coincidencias de
  // la búsqueda; Esc cierra la comparación
  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      if (e.key === "Escape") {
        e.stopPropagation();
        onClose();
        return;
      }
      if ((e.metaKey || e.ctrlKey) && (e.key === "g" || e.key === "G")) {
        e.preventDefault();
        irA(e.shiftKey ? -1 : 1);
      }
    }
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  });

  return (
    <div className="comparar">
      <div className="comparar-barra">
        <span className="comparar-titulo">
          <Icon name="copy" size={13} />
          Comparando <span className="dato">{nombreA}</span> con{" "}
          <span className="dato">{nombreB}</span>
        </span>
        {/* comparar no toca ninguno de los dos ficheros, y la barra lo dice */}
        <span className="opt-hint">No se toca ninguno de los dos ficheros</span>
        <span className="dato">
          {difs === null
            ? "Comparando…"
            : total === 0
              ? /* se compara el texto: decir «sin diferencias» a secas
                   promete haber mirado la página entera */
                "Sin diferencias de texto"
              : `${actual + 1} de ${total}`}
        </span>
        <button
          className="btn btn-icon"
          title={`Diferencia anterior (⇧${MOD}G)`}
          aria-label="Diferencia anterior"
          disabled={total === 0}
          onClick={() => irA(-1)}
        >
          <Icon name="up" size={13} />
        </button>
        <button
          className="btn btn-icon"
          title={`Diferencia siguiente (${MOD}G)`}
          aria-label="Diferencia siguiente"
          disabled={total === 0}
          onClick={() => irA(1)}
        >
          <Icon name="down" size={13} />
        </button>
        <button
          className="btn"
          title="Cerrar la comparación (Esc)"
          onClick={onClose}
        >
          Cerrar la comparación <span className="menu-atajo">Esc</span>
        </button>
      </div>
      <div className="comparar-cuerpo">
        <div className="comparar-lista" role="listbox" aria-label="Diferencias">
          {difs === null && <p className="sign-empty">Comparando…</p>}
          {difs !== null && total === 0 && (
            <p className="sign-empty">
              Los dos documentos dicen lo mismo: no hay ninguna diferencia de
              texto.
            </p>
          )}
          {total > 0 && (
            // qué significa cada color: tres cuadros de colores sin leyenda
            // son un jeroglífico
            <div className="comparar-leyenda">
              {(["cambiado", "añadido", "quitado"] as const).map((t) => (
                <span key={t} className="dato">
                  <span
                    className="comparar-punto"
                    style={{ background: COLOR[t] }}
                  />
                  {ETIQUETA[t]}
                </span>
              ))}
            </div>
          )}
          {(difs ?? []).map((d, i) => (
            <button
              key={i}
              role="option"
              aria-selected={i === actual}
              tabIndex={i === actual ? 0 : -1}
              ref={(el) => {
                if (el) filasRef.current.set(i, el);
                else filasRef.current.delete(i);
              }}
              className={`comparar-fila${i === actual ? " on" : ""}`}
              onKeyDown={(e) => {
                if (e.key === "ArrowDown" || e.key === "ArrowUp") {
                  e.preventDefault();
                  irA(e.key === "ArrowDown" ? 1 : -1);
                }
              }}
              onClick={() => setActual(i)}
            >
              <span
                className="comparar-punto"
                style={{ background: COLOR[d.tipo] }}
              />
              <span className="comparar-fila-texto">
                <span className="dato">
                  {ETIQUETA[d.tipo]} · pág. {paginaDe(d)}
                </span>
                <span className="comparar-frase">
                  {d.texto_b || d.texto_a || "(sin texto)"}
                </span>
              </span>
            </button>
          ))}
          {total > 0 && (
            <p className="opt-hint">
              {plural(total, "diferencia", "diferencias")} · ↑ y ↓ en la lista,
              o {MOD}G y ⇧{MOD}G desde cualquier sitio
            </p>
          )}
        </div>
        <div
          className="comparar-panel"
          ref={izqRef}
          onScroll={() => sincroniza("izq")}
        >
          <span className="card-label">{nombreA}</span>
          <Hoja
            src={dif?.pagina_a !== null && dif ? hojasA[dif.pagina_a] : undefined}
            size={dif?.pagina_a != null ? sizesA[dif.pagina_a] : undefined}
            rects={dif?.rects_a ?? []}
            color={dif ? COLOR[dif.tipo] : "transparent"}
            resaltado
            ancho={ancho}
            vacio={
              dif && dif.pagina_a === null
                ? `Esta página no está en ${nombreA}`
                : undefined
            }
          />
        </div>
        <div
          className="comparar-panel"
          ref={derRef}
          onScroll={() => sincroniza("der")}
        >
          <span className="card-label">{nombreB}</span>
          <Hoja
            src={dif?.pagina_b !== null && dif ? hojasB[dif.pagina_b] : undefined}
            size={dif?.pagina_b != null ? sizesB[dif.pagina_b] : undefined}
            rects={dif?.rects_b ?? []}
            color={dif ? COLOR[dif.tipo] : "transparent"}
            resaltado
            ancho={ancho}
            vacio={
              dif && dif.pagina_b === null
                ? `Esta página no está en ${nombreB}`
                : undefined
            }
          />
        </div>
      </div>
    </div>
  );
}
