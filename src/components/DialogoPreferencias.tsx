import { useState } from "react";
import { guardaPreferencias, type Preferencias } from "../tipos";
import { useModal } from "../hooks/useModal";

/** Preferencias de la app (⌘,). De momento solo el autor con el que se
 *  firman los comentarios; lo demás llegará cuando haga falta. */
export default function DialogoPreferencias({
  initial,
  onClose,
}: {
  initial: Preferencias;
  onClose: () => void;
}) {
  const [prefs, setPrefs] = useState<Preferencias>(initial);

  function guardar() {
    guardaPreferencias({ autor: prefs.autor.trim() });
    onClose();
  }

  const { ref, onKeyDown } = useModal({ onClose, onConfirm: guardar });

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div
        className="modal"
        role="dialog"
        aria-modal="true"
        aria-label="Preferencias"
        onClick={(e) => e.stopPropagation()}
        onKeyDown={onKeyDown}
        ref={ref}
        tabIndex={-1}
      >
        <h3>Preferencias</h3>
        <label className="prop-field">
          <span className="card-label">Autor de los comentarios</span>
          <input
            type="text"
            placeholder="El nombre de usuario del sistema"
            value={prefs.autor}
            onChange={(e) => setPrefs({ autor: e.target.value })}
          />
        </label>
        <p className="modal-file" style={{ whiteSpace: "normal" }}>
          Es el nombre que se guarda en cada resaltado, nota, forma o sello
          que crees a partir de ahora. En blanco se usa el del sistema.
        </p>
        <div className="card-actions">
          <button className="btn" onClick={onClose}>
            Cancelar
          </button>
          <button className="btn btn-primary" onClick={guardar}>
            Guardar
          </button>
        </div>
      </div>
    </div>
  );
}
