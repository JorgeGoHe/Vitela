import { useRef, useState } from "react";
import { open } from "../dialogos";
import { useModal } from "../hooks/useModal";
import { plural } from "../tipos";
import Icon from "./Icon";

/** Nombre y carpeta de una ruta, para distinguir dos ficheros iguales. */
function partesDe(ruta: string): { nombre: string; dir: string } {
  const trozos = ruta.split(/[\\/]/);
  const nombre = trozos.pop() ?? ruta;
  return { nombre, dir: trozos.join("/") };
}

/**
 * «Combinar ficheros…»: la rejilla de Acrobat. Los PDF se ordenan
 * arrastrando, se quitan por fila y solo se tocan al aceptar; Cancelar deja
 * el documento como estaba.
 */
export default function DialogoCombinar({
  paginaActual,
  onConfirm,
  onClose,
}: {
  /** Página que enseña la píldora, para «después de la actual». */
  paginaActual: number;
  onConfirm: (opts: { rutas: string[]; alFinal: boolean }) => void;
  onClose: () => void;
}) {
  const [rutas, setRutas] = useState<string[]>([]);
  const [alFinal, setAlFinal] = useState(true);
  const arrastreRef = useRef<number | null>(null);
  const [dropIdx, setDropIdx] = useState<number | null>(null);
  const confirmar = () => {
    if (rutas.length > 0) onConfirm({ rutas, alFinal });
  };
  const { ref, onKeyDown } = useModal({ onClose, onConfirm: confirmar });

  async function anadir() {
    const sel = await open({
      filters: [{ name: "PDF", extensions: ["pdf"] }],
      multiple: true,
      title: "PDF que se añaden",
    });
    const nuevas = typeof sel === "string" ? [sel] : Array.isArray(sel) ? sel : [];
    if (nuevas.length > 0) setRutas((v) => [...v, ...nuevas]);
  }

  function mover(desde: number, hasta: number) {
    setRutas((v) => {
      const copia = [...v];
      const [x] = copia.splice(desde, 1);
      copia.splice(hasta > desde ? hasta - 1 : hasta, 0, x);
      return copia;
    });
  }

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div
        className="modal modal-hf"
        role="dialog"
        aria-modal="true"
        aria-label="Combinar ficheros"
        onClick={(e) => e.stopPropagation()}
        onKeyDown={onKeyDown}
        ref={ref}
        tabIndex={-1}
      >
        <h3>Combinar ficheros</h3>
        {rutas.length === 0 && (
          <p className="sign-empty">
            Todavía no has elegido ningún PDF. Se añadirán en el orden de la
            lista, que se cambia arrastrando.
          </p>
        )}
        {rutas.length > 0 && (
          <div className="combinar-lista">
            {rutas.map((r, i) => {
              const { nombre, dir } = partesDe(r);
              return (
                <div
                  key={`${r}-${i}`}
                  className={`combinar-fila${dropIdx === i ? " drop-antes" : ""}${
                    dropIdx === i + 1 ? " drop-despues" : ""
                  }`}
                  draggable
                  onDragStart={(e) => {
                    arrastreRef.current = i;
                    e.dataTransfer.effectAllowed = "move";
                  }}
                  onDragOver={(e) => {
                    if (arrastreRef.current === null) return;
                    e.preventDefault();
                    const caja = e.currentTarget.getBoundingClientRect();
                    setDropIdx(
                      e.clientY < caja.top + caja.height / 2 ? i : i + 1,
                    );
                  }}
                  onDrop={(e) => {
                    e.preventDefault();
                    const desde = arrastreRef.current;
                    const hueco = dropIdx;
                    arrastreRef.current = null;
                    setDropIdx(null);
                    if (desde !== null && hueco !== null) mover(desde, hueco);
                  }}
                  onDragEnd={() => {
                    arrastreRef.current = null;
                    setDropIdx(null);
                  }}
                >
                  <span className="dato combinar-orden">{i + 1}</span>
                  <span className="combinar-nombre">
                    <span>{nombre}</span>
                    <span className="reciente-dir">{dir}</span>
                  </span>
                  <button
                    className="btn"
                    aria-label={`Quitar ${nombre} de la lista`}
                    onClick={() =>
                      setRutas((v) => v.filter((_, j) => j !== i))
                    }
                  >
                    Quitar
                  </button>
                </div>
              );
            })}
          </div>
        )}
        <div className="card-actions" style={{ justifyContent: "flex-start" }}>
          <button className="btn" onClick={anadir}>
            <Icon name="merge" size={14} />
            Añadir PDF…
          </button>
          {rutas.length > 0 && (
            <span className="dato">
              {plural(rutas.length, "fichero", "ficheros")}
            </span>
          )}
        </div>
        <span className="card-label">Dónde se añaden</span>
        <select
          className="size-select"
          aria-label="Dónde se añaden las páginas"
          value={alFinal ? "final" : "aqui"}
          onChange={(e) => setAlFinal(e.target.value === "final")}
        >
          <option value="final">Al final del documento</option>
          <option value="aqui">Después de la página {paginaActual + 1}</option>
        </select>
        <div className="card-actions">
          <button className="btn" onClick={onClose}>
            Cancelar
          </button>
          <button
            className="btn btn-primary"
            disabled={rutas.length === 0}
            title={rutas.length === 0 ? "Añade antes algún PDF" : undefined}
            onClick={confirmar}
          >
            Combinar
          </button>
        </div>
      </div>
    </div>
  );
}
