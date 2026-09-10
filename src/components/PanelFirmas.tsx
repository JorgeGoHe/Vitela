import { useState } from "react";
import type { FirmaGuardada } from "../api";
import Icon from "./Icon";

type Ranura = "firma" | "iniciales";

/**
 * Popover con la biblioteca de firmas, en las **dos ranuras** de Acrobat:
 * la firma entera y las iniciales, que son las que se estampan en cada
 * página de un contrato. Cada una se elige para estamparla, se sube desde
 * una imagen, se dibuja a mano o se borra.
 */
export default function PanelFirmas({
  firmas,
  iniciales,
  onPick,
  onUpload,
  onDraw,
  onDelete,
  onClose,
}: {
  firmas: FirmaGuardada[];
  /** Ids de las entradas que son iniciales. */
  iniciales: string[];
  onPick: (firma: FirmaGuardada) => void;
  onUpload: (ranura: Ranura) => void;
  onDraw: (ranura: Ranura) => void;
  onDelete: (id: string) => void;
  onClose: () => void;
}) {
  const [ranura, setRanura] = useState<Ranura>("firma");
  const esInicial = (f: FirmaGuardada) => iniciales.includes(f.id);
  const lista = firmas.filter((f) =>
    ranura === "iniciales" ? esInicial(f) : !esInicial(f),
  );

  return (
    <>
      <div className="menu-backdrop" onClick={onClose} />
      <div className="sign-panel">
        <h3>{ranura === "iniciales" ? "Tus iniciales" : "Tu firma"}</h3>
        {/* dos ranuras, no dos paneles: es el mismo trabajo con otra imagen */}
        <div className="segmented sign-ranuras" role="tablist">
          <button
            role="tab"
            className={`btn${ranura === "firma" ? " on" : ""}`}
            aria-selected={ranura === "firma"}
            onClick={() => setRanura("firma")}
          >
            Firma
          </button>
          <button
            role="tab"
            className={`btn${ranura === "iniciales" ? " on" : ""}`}
            aria-selected={ranura === "iniciales"}
            onClick={() => setRanura("iniciales")}
          >
            Iniciales
          </button>
        </div>
        {lista.length === 0 ? (
          <p className="sign-empty">
            {ranura === "iniciales"
              ? "Aún no tienes iniciales guardadas. Son las que se ponen en cada página de un contrato: dibújalas una vez y quedan aquí."
              : "Aún no tienes ninguna firma guardada. Sube una imagen (PNG con fondo transparente funciona mejor) o dibújala aquí mismo."}
          </p>
        ) : (
          <div className="sign-list">
            {lista.map((f) => (
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
                    title="Borrar esta imagen"
                    aria-label={`Borrar «${f.name}»`}
                    onClick={() => onDelete(f.id)}
                  >
                    <Icon name="close" size={12} />
                  </button>
                </div>
              </div>
            ))}
          </div>
        )}
        <div className="card-actions">
          <button className="btn" onClick={() => onUpload(ranura)}>
            Subir imagen…
          </button>
          <button className="btn" onClick={() => onDraw(ranura)}>
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
