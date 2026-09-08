import type { ReactNode } from "react";

/** Modal genérico de confirmación: título, cuerpo libre y botón de acción
 *  (rojo si la acción es destructiva). */
export default function DialogoConfirmar({
  titulo,
  cuerpo,
  textoConfirmar,
  peligro = false,
  onConfirm,
  onClose,
}: {
  titulo: string;
  cuerpo: ReactNode;
  textoConfirmar: string;
  peligro?: boolean;
  onConfirm: () => void;
  onClose: () => void;
}) {
  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div
        className="modal"
        role="dialog"
        aria-modal="true"
        aria-label={titulo}
        onClick={(e) => e.stopPropagation()}
      >
        <h3>{titulo}</h3>
        {cuerpo}
        <div className="card-actions">
          <button className="btn" onClick={onClose}>
            Cancelar
          </button>
          <button
            className={peligro ? "btn btn-danger" : "btn btn-primary"}
            onClick={onConfirm}
          >
            {textoConfirmar}
          </button>
        </div>
      </div>
    </div>
  );
}
