import { useModal } from "../hooks/useModal";

/** Modal para reducir el tamaño del PDF recomprimiendo sus imágenes. */
export default function DialogoComprimir({
  quality,
  setQuality,
  dpi,
  setDpi,
  onConfirm,
  onClose,
}: {
  quality: number;
  setQuality: (q: number) => void;
  dpi: number;
  setDpi: (d: number) => void;
  onConfirm: () => void;
  onClose: () => void;
}) {
  const { ref, onKeyDown } = useModal({ onClose, onConfirm });

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div
        className="modal"
        role="dialog"
        aria-modal="true"
        aria-label="Reducir tamaño del PDF"
        onClick={(e) => e.stopPropagation()}
        onKeyDown={onKeyDown}
        ref={ref}
        tabIndex={-1}
      >
        <h3>Reducir tamaño del PDF</h3>
        <div className="card-row">
          <select
            className="size-select"
            title="Calidad JPEG"
            value={quality}
            onChange={(e) => setQuality(Number(e.target.value))}
          >
            <option value={60}>Calidad baja (más pequeño)</option>
            <option value={75}>Calidad media</option>
            <option value={85}>Calidad alta</option>
          </select>
          <select
            className="size-select"
            title="Resolución máxima"
            value={dpi}
            onChange={(e) => setDpi(Number(e.target.value))}
          >
            {[110, 150, 200, 300].map((d) => (
              <option key={d} value={d}>
                {d} ppp máx.
              </option>
            ))}
          </select>
        </div>
        <p className="modal-file" style={{ whiteSpace: "normal" }}>
          Recomprime las imágenes del documento (las que tienen
          transparencia se conservan tal cual). El texto no se toca.
        </p>
        <div className="card-actions">
          <button className="btn" onClick={onClose}>
            Cancelar
          </button>
          <button className="btn btn-primary" onClick={onConfirm}>
            Comprimir
          </button>
        </div>
      </div>
    </div>
  );
}
