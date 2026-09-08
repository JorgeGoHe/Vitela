/** Modal de exportación de las páginas como imágenes (formato y resolución). */
export default function DialogoExportar({
  fmt,
  setFmt,
  dpi,
  setDpi,
  onConfirm,
  onClose,
}: {
  fmt: "png" | "jpeg";
  setFmt: (f: "png" | "jpeg") => void;
  dpi: number;
  setDpi: (d: number) => void;
  onConfirm: () => void;
  onClose: () => void;
}) {
  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div
        className="modal"
        role="dialog"
        aria-modal="true"
        aria-label="Exportar como imágenes"
        onClick={(e) => e.stopPropagation()}
      >
        <h3>Exportar como imágenes</h3>
        <div className="card-row">
          <select
            className="size-select"
            value={fmt}
            onChange={(e) => setFmt(e.target.value as "png" | "jpeg")}
          >
            <option value="png">PNG</option>
            <option value="jpeg">JPEG</option>
          </select>
          <select
            className="size-select"
            value={dpi}
            onChange={(e) => setDpi(Number(e.target.value))}
          >
            {[96, 150, 200, 300].map((d) => (
              <option key={d} value={d}>
                {d} ppp
              </option>
            ))}
          </select>
        </div>
        <p className="modal-file">Una imagen por página del documento.</p>
        <div className="card-actions">
          <button className="btn" onClick={onClose}>
            Cancelar
          </button>
          <button className="btn btn-primary" onClick={onConfirm}>
            Elegir carpeta…
          </button>
        </div>
      </div>
    </div>
  );
}
