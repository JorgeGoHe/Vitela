import { useModal } from "../hooks/useModal";

/** Modal para proteger el documento con contraseña (copia cifrada AES-256). */
export default function DialogoProteger({
  valor,
  onChange,
  onConfirm,
  onClose,
}: {
  valor: { user: string; owner: string };
  onChange: (v: { user: string; owner: string }) => void;
  onConfirm: () => void;
  onClose: () => void;
}) {
  const { ref, onKeyDown } = useModal({
    onClose,
    onConfirm: () => {
      if (valor.user) onConfirm();
    },
  });

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div
        className="modal"
        role="dialog"
        aria-modal="true"
        aria-label="Proteger con contraseña"
        onClick={(e) => e.stopPropagation()}
        onKeyDown={onKeyDown}
        ref={ref}
        tabIndex={-1}
      >
        <h3>Proteger con contraseña</h3>
        <input
          type="password"
          placeholder="Contraseña (necesaria para abrir)"
          value={valor.user}
          onChange={(e) => onChange({ ...valor, user: e.target.value })}
        />
        <input
          type="password"
          placeholder="Contraseña de propietario (opcional)"
          value={valor.owner}
          onChange={(e) => onChange({ ...valor, owner: e.target.value })}
        />
        <p className="modal-file">
          Cifrado AES-256. Se guarda como una copia protegida; si el
          documento va a llevar firma digital, fírmalo por separado.
        </p>
        <div className="card-actions">
          <button className="btn" onClick={onClose}>
            Cancelar
          </button>
          <button
            className="btn btn-primary"
            disabled={!valor.user}
            onClick={onConfirm}
          >
            Proteger…
          </button>
        </div>
      </div>
    </div>
  );
}
