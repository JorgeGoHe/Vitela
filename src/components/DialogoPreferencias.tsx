import { useState } from "react";
import { MOD, type Preferencias } from "../tipos";
import { useModal } from "../hooks/useModal";

/** Preferencias de la app (⌘,): el autor con el que se firman los
 *  comentarios y el modo nocturno del documento. */
export default function DialogoPreferencias({
  initial,
  onGuardar,
  onClose,
}: {
  initial: Preferencias;
  onGuardar: (p: Preferencias) => void;
  onClose: () => void;
}) {
  const [prefs, setPrefs] = useState<Preferencias>(initial);

  function guardar() {
    onGuardar({ ...prefs, autor: prefs.autor.trim() });
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
            onChange={(e) => setPrefs({ ...prefs, autor: e.target.value })}
          />
        </label>
        <p className="modal-file" style={{ whiteSpace: "normal" }}>
          Es el nombre que se guarda en cada resaltado, nota, forma o sello
          que crees a partir de ahora. En blanco se usa el del sistema.
        </p>
        <span className="card-label">Documento</span>
        <label className="opt-check">
          <input
            type="checkbox"
            checked={prefs.nocturno}
            onChange={(e) => setPrefs({ ...prefs, nocturno: e.target.checked })}
          />
          Modo nocturno (⇧{MOD}L)
        </label>
        <p className="modal-file" style={{ whiteSpace: "normal" }}>
          Oscurece el papel y aclara la tinta. Solo cambia lo que ves: el
          fichero no se toca y las imágenes que exportes siguen en blanco.
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
