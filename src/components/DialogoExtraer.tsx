import { useState } from "react";
import { useModal } from "../hooks/useModal";

/** Diálogo «Extraer páginas…»: rango con la sintaxis de Acrobat (`1-3, 8`),
 *  con la opción de quitarlas del original y la de un fichero por página. */
export default function DialogoExtraer({
  inicial,
  pageCount,
  onConfirm,
  onClose,
}: {
  /** Rango prerrellenado: la selección del panel o la página actual. */
  inicial: string;
  pageCount: number;
  onConfirm: (opts: {
    rango: string;
    borrar: boolean;
    porPagina: boolean;
  }) => void;
  onClose: () => void;
}) {
  const [rango, setRango] = useState(inicial);
  const [borrar, setBorrar] = useState(false);
  const [porPagina, setPorPagina] = useState(false);
  const confirmar = () => onConfirm({ rango, borrar, porPagina });
  const { ref, onKeyDown } = useModal({ onClose, onConfirm: confirmar });

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div
        className="modal"
        role="dialog"
        aria-modal="true"
        aria-label="Extraer páginas"
        onClick={(e) => e.stopPropagation()}
        onKeyDown={onKeyDown}
        ref={ref}
        tabIndex={-1}
      >
        <h3>Extraer páginas</h3>
        <span className="card-label">
          Páginas a extraer (1 a {pageCount})
        </span>
        <input
          type="text"
          placeholder="1-3, 8"
          aria-label={`Páginas a extraer, de 1 a ${pageCount}`}
          value={rango}
          onChange={(e) => setRango(e.target.value)}
        />
        <label className="opt-check">
          <input
            type="checkbox"
            checked={borrar}
            onChange={(e) => setBorrar(e.target.checked)}
          />
          Eliminar las páginas del original
        </label>
        <label className="opt-check">
          <input
            type="checkbox"
            checked={porPagina}
            onChange={(e) => setPorPagina(e.target.checked)}
          />
          Un fichero por página
        </label>
        <div className="card-actions">
          <button className="btn" onClick={onClose}>
            Cancelar
          </button>
          <button className="btn btn-primary" onClick={confirmar}>
            Extraer
          </button>
        </div>
      </div>
    </div>
  );
}
