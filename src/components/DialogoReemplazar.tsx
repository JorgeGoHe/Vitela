import { useState } from "react";
import { open } from "../dialogos";
import { useModal } from "../hooks/useModal";
import Icon from "./Icon";

/** Nombre de fichero de una ruta, sin la carpeta. */
function nombreDe(ruta: string): string {
  return ruta.split(/[\\/]/).pop() ?? ruta;
}

/**
 * «Reemplazar páginas…» de Acrobat: se eligen las páginas del destino y
 * las del origen, y el resto del documento se queda como está.
 */
export default function DialogoReemplazar({
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
    otherPath: string;
    rangoOrigen: string;
  }) => void;
  onClose: () => void;
}) {
  const [rango, setRango] = useState(inicial);
  const [otherPath, setOtherPath] = useState("");
  const [rangoOrigen, setRangoOrigen] = useState("");
  const confirmar = () => {
    if (otherPath) onConfirm({ rango, otherPath, rangoOrigen });
  };
  const { ref, onKeyDown } = useModal({ onClose, onConfirm: confirmar });

  async function elegirPdf() {
    const sel = await open({
      filters: [{ name: "PDF", extensions: ["pdf"] }],
      multiple: false,
      title: "PDF con las páginas nuevas",
    });
    if (typeof sel === "string") setOtherPath(sel);
  }

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div
        className="modal"
        role="dialog"
        aria-modal="true"
        aria-label="Reemplazar páginas"
        onClick={(e) => e.stopPropagation()}
        onKeyDown={onKeyDown}
        ref={ref}
        tabIndex={-1}
      >
        <h3>Reemplazar páginas</h3>
        <span className="card-label">
          Páginas que se sustituyen (1 a {pageCount})
        </span>
        <input
          type="text"
          placeholder="1-3, 8"
          aria-label={`Páginas que se sustituyen, de 1 a ${pageCount}`}
          value={rango}
          onChange={(e) => setRango(e.target.value)}
        />
        <span className="card-label">Con las de este PDF</span>
        <div className="card-actions" style={{ justifyContent: "flex-start" }}>
          <button className="btn" onClick={elegirPdf}>
            <Icon name="doc" size={14} />
            {otherPath ? "Cambiar…" : "Elegir PDF…"}
          </button>
          <span className="dato">
            {otherPath ? nombreDe(otherPath) : "ninguno elegido"}
          </span>
        </div>
        <span className="card-label">Páginas del otro PDF</span>
        <input
          type="text"
          placeholder="Todas"
          aria-label="Páginas del otro PDF; en blanco, todas"
          value={rangoOrigen}
          onChange={(e) => setRangoOrigen(e.target.value)}
        />
        <p className="modal-file" style={{ whiteSpace: "normal" }}>
          El total del documento cambia solo si los dos rangos no tienen el
          mismo número de páginas. {"⌘"}Z lo devuelve.
        </p>
        <div className="card-actions">
          <button className="btn" onClick={onClose}>
            Cancelar
          </button>
          <button
            className="btn btn-primary"
            disabled={!otherPath}
            title={otherPath ? undefined : "Elige antes el PDF de origen"}
            onClick={confirmar}
          >
            Reemplazar
          </button>
        </div>
      </div>
    </div>
  );
}
