import type { Adjunto } from "../api";
import { tamanoFichero } from "../tipos";
import Icon from "./Icon";

/**
 * Pestaña «Adjuntos» del sidebar: los ficheros que el PDF lleva dentro
 * (`/EmbeddedFiles`), con su tamaño y su fecha. Hasta ahora Vitela solo
 * sabía borrarlos —sanitizar se los llevaba— sin que el usuario pudiera
 * verlos siquiera.
 */
export default function PanelAdjuntos({
  adjuntos,
  onGuardar,
  onAnadir,
}: {
  adjuntos: Adjunto[];
  onGuardar: (index: number, a: Adjunto) => void;
  onAnadir: () => void;
}) {
  return (
    <div className="com-panel">
      <span className="com-total dato">
        {adjuntos.length === 1 ? "1 adjunto" : `${adjuntos.length} adjuntos`}
      </span>
      {adjuntos.length === 0 && (
        <p className="sign-empty">Este documento no lleva ningún fichero.</p>
      )}
      {adjuntos.map((a, i) => (
        <div className="adj-row" key={`${a.name}-${i}`}>
          <span className="adj-nombre">{a.name}</span>
          <span className="adj-dato dato">
            {tamanoFichero(a.bytes)}
            {a.created ? ` · ${a.created}` : ""}
          </span>
          {a.description && (
            <span className="adj-descripcion">{a.description}</span>
          )}
          <button className="btn" onClick={() => onGuardar(i, a)}>
            <Icon name="save" size={13} />
            Guardar como…
          </button>
        </div>
      ))}
      <button className="btn" onClick={onAnadir}>
        <Icon name="merge" size={13} />
        Añadir…
      </button>
    </div>
  );
}
