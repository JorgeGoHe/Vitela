import { useState } from "react";
import type { HeaderFooter } from "../api";

/**
 * Diálogo de encabezado y pie: seis zonas con plantillas {n}, {total} y
 * {fecha}. El botón «Solo numerar» rellena el pie centrado con {n} / {total}.
 */
export default function DialogoEncabezado({
  onApply,
  onClose,
}: {
  onApply: (zonas: HeaderFooter, fontSize: number) => void;
  onClose: () => void;
}) {
  const [zonas, setZonas] = useState<HeaderFooter>({});
  const [fontSize, setFontSize] = useState(10);

  function campo(key: keyof HeaderFooter, placeholder: string) {
    return (
      <input
        type="text"
        placeholder={placeholder}
        value={zonas[key] ?? ""}
        onChange={(e) => setZonas({ ...zonas, [key]: e.target.value })}
      />
    );
  }

  const vacio = Object.values(zonas).every((v) => !v?.trim());

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="modal modal-hf" onClick={(e) => e.stopPropagation()}>
        <h3>Encabezado y pie de página</h3>
        <span className="card-label">Encabezado</span>
        <div className="hf-grid">
          {campo("headerLeft", "Izquierda")}
          {campo("headerCenter", "Centro")}
          {campo("headerRight", "Derecha")}
        </div>
        <span className="card-label">Pie</span>
        <div className="hf-grid">
          {campo("footerLeft", "Izquierda")}
          {campo("footerCenter", "Centro")}
          {campo("footerRight", "Derecha")}
        </div>
        <div className="card-row">
          <select
            className="size-select"
            title="Tamaño"
            value={fontSize}
            onChange={(e) => setFontSize(Number(e.target.value))}
          >
            {[8, 9, 10, 11, 12, 14].map((s) => (
              <option key={s} value={s}>
                {s} pt
              </option>
            ))}
          </select>
          <button
            className="btn"
            onClick={() => setZonas({ ...zonas, footerCenter: "{n} / {total}" })}
          >
            Solo numerar
          </button>
        </div>
        <p className="modal-file">
          Plantillas: {"{n}"} = página, {"{total}"} = total, {"{fecha}"} = hoy.
        </p>
        <div className="card-actions">
          <button className="btn" onClick={onClose}>
            Cancelar
          </button>
          <button
            className="btn btn-primary"
            disabled={vacio}
            onClick={() => onApply(zonas, fontSize)}
          >
            Aplicar
          </button>
        </div>
      </div>
    </div>
  );
}
