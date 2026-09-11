import { useModal } from "../hooks/useModal";

/** Modal de contraseña para un fichero (documento protegido o .p12). */
export default function DialogoContrasena({
  titulo,
  fichero,
  valor,
  onChange,
  onConfirm,
  onClose,
  etiqueta,
  placeholder = "Contraseña",
  error = null,
}: {
  titulo: string;
  /** Ruta del fichero; se muestra solo el nombre. */
  fichero: string;
  valor: string;
  onChange: (v: string) => void;
  onConfirm: () => void;
  onClose: () => void;
  /** Texto del botón de confirmar. */
  etiqueta: string;
  placeholder?: string;
  /** Lo que ha ido mal con la contraseña anterior, dentro del diálogo:
   *  la banda de la aplicación queda detrás del velo. */
  error?: string | null;
}) {
  const { ref, onKeyDown } = useModal({ onClose, onConfirm });

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div
        className="modal"
        role="dialog"
        aria-modal="true"
        aria-label={titulo}
        onClick={(e) => e.stopPropagation()}
        onKeyDown={onKeyDown}
        ref={ref}
        tabIndex={-1}
      >
        <h3>{titulo}</h3>
        <p className="modal-file">{fichero.split(/[\\/]/).pop()}</p>
        <input
          type="password"
          placeholder={placeholder}
          value={valor}
          onChange={(e) => onChange(e.target.value)}
          aria-invalid={error ? true : undefined}
        />
        {error && (
          <p className="modal-error" role="alert">
            {error}
          </p>
        )}
        <div className="card-actions">
          <button className="btn" onClick={onClose}>
            Cancelar
          </button>
          <button className="btn btn-primary" onClick={onConfirm}>
            {etiqueta}
          </button>
        </div>
      </div>
    </div>
  );
}
