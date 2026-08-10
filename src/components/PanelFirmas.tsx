import type { FirmaGuardada } from "../api";

/**
 * Popover con la biblioteca de firmas: elegir una para estamparla, subir
 * una imagen nueva, dibujar una a mano o borrar las guardadas.
 */
export default function PanelFirmas({
  firmas,
  onPick,
  onUpload,
  onDraw,
  onDelete,
  onClose,
}: {
  firmas: FirmaGuardada[];
  onPick: (firma: FirmaGuardada) => void;
  onUpload: () => void;
  onDraw: () => void;
  onDelete: (id: string) => void;
  onClose: () => void;
}) {
  return (
    <>
      <div className="menu-backdrop" onClick={onClose} />
      <div className="sign-panel">
        <h3>Tu firma</h3>
        {firmas.length === 0 ? (
          <p className="sign-empty">
            Aún no tienes ninguna firma guardada. Sube una imagen (PNG con
            fondo transparente funciona mejor) o dibújala aquí mismo.
          </p>
        ) : (
          <div className="sign-list">
            {firmas.map((f) => (
              <div key={f.id} className="sign-item">
                <button
                  className="sign-thumb"
                  title={`Estampar «${f.name}»`}
                  onClick={() => onPick(f)}
                >
                  <img
                    src={`data:image/png;base64,${f.png_base64}`}
                    alt={f.name}
                    draggable={false}
                  />
                </button>
                <div className="sign-item-row">
                  <span className="sign-name" title={f.name}>
                    {f.name}
                  </span>
                  <button
                    className="btn btn-icon sign-delete"
                    title="Borrar esta firma"
                    onClick={() => onDelete(f.id)}
                  >
                    ✕
                  </button>
                </div>
              </div>
            ))}
          </div>
        )}
        <div className="card-actions">
          <button className="btn" onClick={onUpload}>
            Subir imagen…
          </button>
          <button className="btn" onClick={onDraw}>
            Dibujar…
          </button>
          <button className="btn" onClick={onClose}>
            Cerrar
          </button>
        </div>
      </div>
    </>
  );
}
