import type { Capa } from "../api";
import { MOD } from "../tipos";

/**
 * Pestaña «Capas» del sidebar: una casilla por capa del documento
 * (`/OCProperties`). A diferencia de Acrobat, aquí apagar una capa **cambia
 * el fichero**: PDFium respeta el `/OFF` del documento al renderizar y no
 * hay forma de ocultarla solo en la vista. Se dice en una línea bajo la
 * lista en vez de dejar que se descubra al guardar.
 */
export default function PanelCapas({
  capas,
  onToggle,
}: {
  capas: Capa[];
  onToggle: (index: number, visible: boolean) => void;
}) {
  return (
    <div className="com-panel">
      <span className="com-total dato">
        {capas.length === 1 ? "1 capa" : `${capas.length} capas`}
      </span>
      {capas.length === 0 && (
        <p className="sign-empty">Este documento no tiene capas.</p>
      )}
      {capas.map((c, i) => (
        <label className="opt-check" key={`${c.name}-${i}`}>
          <input
            type="checkbox"
            checked={c.visible}
            onChange={(e) => onToggle(i, e.target.checked)}
          />
          {c.name || `Capa ${i + 1}`}
        </label>
      ))}
      {capas.length > 0 && (
        <p className="opt-hint">
          Apagar una capa cambia el documento; {MOD}Z lo devuelve.
        </p>
      )}
    </div>
  );
}
