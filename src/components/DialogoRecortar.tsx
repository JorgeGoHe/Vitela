import { useState } from "react";
import type { MargenesRecorte } from "../api";
import { useModal } from "../hooks/useModal";
import type { PageSize } from "../tipos";

/** Un punto PDF es 1/72 de pulgada; en milímetros se reconoce el A4. */
const MM = 72 / 25.4;

/** Las dos unidades en las que se piden márgenes. La de fábrica es el
 *  milímetro: nadie mide el margen de un folio en puntos. */
const UNIDADES: [string, string, number][] = [
  ["mm", "mm", MM],
  ["pt", "pt", 1],
];

/** Un campo de margen, con su nombre encima. */
function Campo({
  etiqueta,
  valor,
  onChange,
}: {
  etiqueta: string;
  valor: number;
  onChange: (v: number) => void;
}) {
  return (
    <label className="prop-field">
      <span className="card-label">{etiqueta}</span>
      <input
        type="number"
        className="stamp-input"
        style={{ width: 76 }}
        min={0}
        step={1}
        value={valor}
        onChange={(e) => onChange(Math.max(0, Number(e.target.value) || 0))}
      />
    </label>
  );
}

/**
 * «Recortar por márgenes»: la otra mitad de recortar, la exacta. Arrastrar
 * vale para una página; para dejar 20 mm en todas hace falta escribirlo, y
 * es lo que pide Acrobat en su diálogo de recorte.
 *
 * La medida se manda **en puntos** —que es lo que entiende el PDF— y aquí
 * se escribe en la unidad que elija el usuario. El tamaño que queda se
 * enseña mientras se escribe: es lo que dice si el recorte se ha pasado.
 */
export default function DialogoRecortar({
  size,
  pageCount,
  paginaActual,
  onConfirm,
  onClose,
}: {
  /** Tamaño de la página que se está mirando, en puntos. */
  size: PageSize | undefined;
  pageCount: number;
  /** Página que se ve, desde 1. */
  paginaActual: number;
  onConfirm: (margenes: MargenesRecorte, todas: boolean) => void;
  onClose: () => void;
}) {
  const [unidad, setUnidad] = useState("mm");
  const [arriba, setArriba] = useState(0);
  const [abajo, setAbajo] = useState(0);
  const [izq, setIzq] = useState(0);
  const [der, setDer] = useState(0);
  const [todas, setTodas] = useState(false);

  const factor = UNIDADES.find(([u]) => u === unidad)?.[2] ?? MM;
  const enPuntos: MargenesRecorte = {
    arriba: arriba * factor,
    abajo: abajo * factor,
    izq: izq * factor,
    der: der * factor,
  };
  const anchoQueda = (size?.width ?? 0) - enPuntos.izq - enPuntos.der;
  const altoQueda = (size?.height ?? 0) - enPuntos.arriba - enPuntos.abajo;
  const algo = arriba > 0 || abajo > 0 || izq > 0 || der > 0;
  // margen mínimo de página que deja Acrobat: por debajo de 1 pt no queda
  // documento, queda una raya
  const cabe = anchoQueda > 1 && altoQueda > 1;
  const listo = algo && cabe;
  const confirmar = () => {
    if (listo) onConfirm(enPuntos, todas);
  };
  const { ref, onKeyDown } = useModal({ onClose, onConfirm: confirmar });

  const enUnidad = (pt: number) =>
    unidad === "mm" ? Math.round(pt / MM) : Math.round(pt);

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div
        className="modal"
        role="dialog"
        aria-modal="true"
        aria-label="Recortar por márgenes"
        onClick={(e) => e.stopPropagation()}
        onKeyDown={onKeyDown}
        ref={ref}
        tabIndex={-1}
      >
        <h3>Recortar por márgenes</h3>
        <p className="modal-file" style={{ whiteSpace: "normal" }}>
          Cuánto se quita desde cada borde. Es la medida exacta: para una sola
          página también se puede arrastrar el área sobre el documento.
        </p>
        <div className="card-row">
          <Campo etiqueta="Arriba" valor={arriba} onChange={setArriba} />
          <Campo etiqueta="Abajo" valor={abajo} onChange={setAbajo} />
          <Campo etiqueta="Izquierda" valor={izq} onChange={setIzq} />
          <Campo etiqueta="Derecha" valor={der} onChange={setDer} />
          <label className="prop-field">
            <span className="card-label">Unidad</span>
            <select
              className="size-select"
              value={unidad}
              onChange={(e) => setUnidad(e.target.value)}
            >
              {UNIDADES.map(([u, etiqueta]) => (
                <option key={u} value={u}>
                  {etiqueta}
                </option>
              ))}
            </select>
          </label>
        </div>
        {size && (
          <span className="dato">
            Página {paginaActual}: {enUnidad(size.width)} ×{" "}
            {enUnidad(size.height)} {unidad} →{" "}
            {cabe
              ? `${enUnidad(anchoQueda)} × ${enUnidad(altoQueda)} ${unidad}`
              : "no queda página"}
          </span>
        )}
        {pageCount > 1 && (
          <label className="opt-check">
            <input
              type="checkbox"
              checked={todas}
              onChange={(e) => setTodas(e.target.checked)}
            />
            Aplicarlo a todas las páginas
          </label>
        )}
        <p className="modal-file" style={{ whiteSpace: "normal" }}>
          Recortar esconde lo que queda fuera, no lo borra del fichero. Se
          deshace con ⌘Z.
        </p>
        <div className="card-actions">
          <button className="btn" onClick={onClose}>
            Cancelar
          </button>
          <button
            className="btn btn-primary"
            disabled={!listo}
            title={
              !algo
                ? "Escribe al menos un margen"
                : !cabe
                  ? "Con esos márgenes no queda página"
                  : undefined
            }
            onClick={confirmar}
          >
            Recortar
          </button>
        </div>
      </div>
    </div>
  );
}
