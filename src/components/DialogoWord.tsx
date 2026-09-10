import { useState } from "react";
import { useModal } from "../hooks/useModal";
import { indicesDeRango } from "../tipos";
import RangoPaginas from "./RangoPaginas";

/**
 * «Word (.docx)…»: el aviso de lo que no sale va ANTES de elegir destino,
 * porque es una función que promete mucho y da menos, y descubrirlo con el
 * fichero ya escrito es tarde.
 *
 * Y con el rango de páginas de Acrobat: exportar un contrato de 200 páginas
 * para quedarse con dos era el camino largo a un fichero que no se quería.
 */
export default function DialogoWord({
  pageCount,
  onConfirm,
  onClose,
}: {
  pageCount: number;
  /** Índices de página, o null para el documento entero. */
  onConfirm: (paginas: number[] | null) => void;
  onClose: () => void;
}) {
  const [todas, setTodas] = useState(true);
  const [rango, setRango] = useState("");
  const indices = indicesDeRango(todas, rango, pageCount);
  const puede = todas || (indices?.length ?? 0) > 0;
  const confirmar = () => {
    if (puede) onConfirm(indices);
  };
  const { ref, onKeyDown } = useModal({ onClose, onConfirm: confirmar });

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div
        className="modal"
        role="dialog"
        aria-modal="true"
        aria-label="Exportar a Word"
        onClick={(e) => e.stopPropagation()}
        onKeyDown={onKeyDown}
        ref={ref}
        tabIndex={-1}
      >
        <h3>Exportar a Word</h3>
        <p className="modal-file" style={{ whiteSpace: "normal" }}>
          El texto y las imágenes salen; la maquetación de columnas y tablas,
          no. Para un documento sencillo suele bastar.
        </p>
        <RangoPaginas
          pageCount={pageCount}
          todas={todas}
          setTodas={setTodas}
          rango={rango}
          setRango={setRango}
        />
        <div className="card-actions">
          <button className="btn" onClick={onClose}>
            Cancelar
          </button>
          <button
            className="btn btn-primary"
            disabled={!puede}
            title={puede ? undefined : "Ese rango no deja ninguna página"}
            onClick={confirmar}
          >
            Elegir dónde y exportar…
          </button>
        </div>
      </div>
    </div>
  );
}
