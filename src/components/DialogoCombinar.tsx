import { useEffect, useRef, useState } from "react";
import { pdfInfo, type PdfInfo } from "../api";
import { open } from "../dialogos";
import { useModal } from "../hooks/useModal";
import { plural, tamanoFichero } from "../tipos";
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
  // páginas, tamaño y cifrado de cada fichero: sin esto el usuario ordena a
  // ciegas ficheros que ha elegido por el nombre. `null` = no se ha podido
  // leer (no es un PDF, o ya no está)
  const [infos, setInfos] = useState<Record<string, PdfInfo | null>>({});
  const pedidasRef = useRef(new Set<string>());
  // un PDF cifrado no se puede combinar: mejor decirlo antes de aceptar que
  // después, con un error rojo y la lista perdida
  const protegidos = rutas.filter((r) => infos[r]?.encrypted).length;
  const confirmar = () => {
    if (rutas.length > 0 && protegidos === 0) onConfirm({ rutas, alFinal });
  };
  const { ref, onKeyDown } = useModal({ onClose, onConfirm: confirmar });

  // se pide una vez por ruta, aunque la fila se mueva o se repita
  useEffect(() => {
    for (const r of rutas) {
      if (pedidasRef.current.has(r)) continue;
      pedidasRef.current.add(r);
      pdfInfo(r)
        .then((i) => setInfos((m) => ({ ...m, [r]: i })))
        .catch(() => setInfos((m) => ({ ...m, [r]: null })));
    }
  }, [rutas]);

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

  /** Subir y bajar una fila con el teclado: el arrastre no es la única forma
   *  de ordenar, como en el panel de páginas. */
  function intercambia(i: number, delta: number) {
    const j = i + delta;
    if (j < 0 || j >= rutas.length) return;
    setRutas((v) => {
      const copia = [...v];
      [copia[i], copia[j]] = [copia[j], copia[i]];
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
            lista, que se cambia arrastrando o con los botones ▲▼.
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
                    {r in infos && (
                      <span
                        className={`dato combinar-info${
                          infos[r]?.encrypted ? " mal" : ""
                        }`}
                      >
                        {infos[r] === null
                          ? "no se ha podido leer"
                          : infos[r].encrypted
                            ? "protegido con contraseña"
                            : `${plural(infos[r].page_count, "página", "páginas")} · ${tamanoFichero(infos[r].bytes)}`}
                      </span>
                    )}
                  </span>
                  <button
                    className="btn btn-icon"
                    title="Subir"
                    aria-label={`Subir ${nombre}`}
                    disabled={i === 0}
                    onClick={() => intercambia(i, -1)}
                  >
                    <Icon name="up" size={13} />
                  </button>
                  <button
                    className="btn btn-icon"
                    title="Bajar"
                    aria-label={`Bajar ${nombre}`}
                    disabled={i === rutas.length - 1}
                    onClick={() => intercambia(i, 1)}
                  >
                    <Icon name="down" size={13} />
                  </button>
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
        {protegidos > 0 && (
          <p className="modal-file combinar-aviso" style={{ whiteSpace: "normal" }}>
            {protegidos === 1
              ? "Uno de los ficheros está protegido con contraseña y no se puede combinar: quítasela antes, o quítalo de la lista."
              : `${protegidos} de los ficheros están protegidos con contraseña y no se pueden combinar: quítasela antes, o quítalos de la lista.`}
          </p>
        )}
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
            disabled={rutas.length === 0 || protegidos > 0}
            title={
              rutas.length === 0
                ? "Añade antes algún PDF"
                : protegidos > 0
                  ? "Hay ficheros protegidos con contraseña en la lista"
                  : undefined
            }
            onClick={confirmar}
          >
            Combinar
          </button>
        </div>
      </div>
    </div>
  );
}
