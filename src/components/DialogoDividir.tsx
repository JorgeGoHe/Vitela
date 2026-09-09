import { useState } from "react";
import { useModal } from "../hooks/useModal";

/**
 * «Dividir documento…»: en trozos de N páginas o por los marcadores de
 * primer nivel. La carpeta se pide después de aceptar, cuando ya se sabe
 * que la división se puede hacer.
 */
export default function DialogoDividir({
  pageCount,
  hayMarcadores,
  onConfirm,
  onClose,
}: {
  pageCount: number;
  /** Sin marcadores la opción se atenúa, no desaparece: quien no sepa que
   *  hace falta un índice no descubriría nunca que la función existe. */
  hayMarcadores: boolean;
  onConfirm: (opts: { modo: "cada" | "marcadores"; cada: number }) => void;
  onClose: () => void;
}) {
  const [modo, setModo] = useState<"cada" | "marcadores">("cada");
  const [cada, setCada] = useState(1);
  const confirmar = () => onConfirm({ modo, cada });
  const { ref, onKeyDown } = useModal({ onClose, onConfirm: confirmar });
  const trozos = Math.ceil(pageCount / Math.max(1, cada));

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div
        className="modal"
        role="dialog"
        aria-modal="true"
        aria-label="Dividir documento"
        onClick={(e) => e.stopPropagation()}
        onKeyDown={onKeyDown}
        ref={ref}
        tabIndex={-1}
      >
        <h3>Dividir documento</h3>
        <label className="opt-check">
          <input
            type="radio"
            name="modo-dividir"
            checked={modo === "cada"}
            onChange={() => setModo("cada")}
          />
          Cada
          <input
            type="number"
            className="stamp-input"
            style={{ width: 64 }}
            min={1}
            max={Math.max(1, pageCount)}
            aria-label="Páginas por fichero"
            value={cada}
            onFocus={() => setModo("cada")}
            onChange={(e) =>
              setCada(
                Math.min(
                  Math.max(1, pageCount),
                  Math.max(1, Number(e.target.value) || 1),
                ),
              )
            }
          />
          páginas
        </label>
        <label className={`opt-check${hayMarcadores ? "" : " disabled"}`}>
          <input
            type="radio"
            name="modo-dividir"
            disabled={!hayMarcadores}
            checked={modo === "marcadores"}
            onChange={() => setModo("marcadores")}
          />
          Por los marcadores de primer nivel
          {!hayMarcadores && " (este documento no tiene)"}
        </label>
        <p className="modal-file" style={{ whiteSpace: "normal" }}>
          {modo === "cada"
            ? `Saldrán ${trozos} ficheros de ${cada === 1 ? "1 página" : `${cada} páginas`}${
                pageCount % cada === 0 ? "" : " (el último puede tener menos)"
              }. El documento abierto no se toca.`
            : "Sale un fichero por cada marcador de primer nivel. El documento abierto no se toca."}
        </p>
        <div className="card-actions">
          <button className="btn" onClick={onClose}>
            Cancelar
          </button>
          <button className="btn btn-primary" onClick={confirmar}>
            Elegir carpeta y dividir…
          </button>
        </div>
      </div>
    </div>
  );
}
