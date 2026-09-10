import type { FirmaGuardada, RanuraImagen } from "../api";
import {
  SELLOS_DINAMICOS,
  SELLOS_ESTANDAR,
  type UltimoSello,
} from "../tipos";
import Icon from "./Icon";
import SelectorRanura from "./SelectorRanura";

/** Un sello de texto de la rejilla, con la cara con la que va a quedar. */
function SelloTexto({
  texto,
  dinamico,
  color,
  elegido,
  onPick,
}: {
  texto: string;
  dinamico: boolean;
  color: string;
  elegido: boolean;
  onPick: () => void;
}) {
  return (
    <button
      className={`sello-celda${elegido ? " on" : ""}`}
      title={
        dinamico
          ? `Estampar «${texto}» con tu nombre y la hora`
          : `Estampar «${texto}»`
      }
      aria-pressed={elegido}
      onClick={onPick}
    >
      <span className="sello-preview" style={{ color, borderColor: color }}>
        {texto}
      </span>
      {dinamico && <span className="sello-celda-pie dato">nombre y hora</span>}
    </button>
  );
}

/**
 * Galería de sellos del modo Sello: una rejilla con la vista previa de cada
 * uno, agrupada como la de Acrobat —Estándar, Dinámicos y Mis sellos— en vez
 * del desplegable de una línea que había hasta ahora, donde el sello no se
 * veía hasta después de ponerlo.
 *
 * El último usado va **el primero**: sellar un expediente es poner el mismo
 * sello cincuenta veces.
 */
export default function PanelSellos({
  color,
  ultimo,
  mios,
  onElegirTexto,
  onElegirImagen,
  onSubirImagen,
  onCambiarRanura,
  onBorrarImagen,
  onClose,
}: {
  /** El color de la fila contextual: la previa no puede mentir. */
  color: string;
  /** El último sello de texto que se puso, si lo hay. */
  ultimo: UltimoSello | null;
  /** «Mis sellos»: las imágenes de la biblioteca con la ranura de sello. */
  mios: FirmaGuardada[];
  onElegirTexto: (texto: string, dinamico: boolean) => void;
  onElegirImagen: (sello: FirmaGuardada) => void;
  onSubirImagen: () => void;
  /** Mover la imagen a otra ranura: un sello que en realidad era la firma
   *  se corrige aquí en vez de borrarlo y volver a subirlo. */
  onCambiarRanura: (id: string, ranura: RanuraImagen) => void;
  onBorrarImagen: (firma: FirmaGuardada) => void;
  onClose: () => void;
}) {
  const esElegido = (texto: string, dinamico: boolean) =>
    !!ultimo && ultimo.texto === texto && ultimo.dinamico === dinamico;

  return (
    <>
      <div className="menu-backdrop" onClick={onClose} />
      <div className="sign-panel">
        <h3>Sellos</h3>
        {ultimo && (
          <>
            <span className="card-label">Último usado</span>
            <div className="sello-rejilla">
              <SelloTexto
                texto={ultimo.texto}
                dinamico={ultimo.dinamico}
                color={color}
                elegido
                onPick={() => onElegirTexto(ultimo.texto, ultimo.dinamico)}
              />
            </div>
          </>
        )}
        <span className="card-label">Estándar</span>
        <div className="sello-rejilla">
          {SELLOS_ESTANDAR.map((s) => (
            <SelloTexto
              key={s}
              texto={s}
              dinamico={false}
              color={color}
              elegido={esElegido(s, false)}
              onPick={() => onElegirTexto(s, false)}
            />
          ))}
        </div>
        <span className="card-label">Dinámicos</span>
        <div className="sello-rejilla">
          {SELLOS_DINAMICOS.map((s) => (
            <SelloTexto
              key={s}
              texto={s}
              dinamico
              color={color}
              elegido={esElegido(s, true)}
              onPick={() => onElegirTexto(s, true)}
            />
          ))}
        </div>
        <span className="card-label">Mis sellos</span>
        {mios.length === 0 ? (
          <p className="sign-empty">
            Todavía no tienes ninguno. Añade una imagen —un PNG con fondo
            transparente queda mejor— y se pone con un clic, como los demás.
          </p>
        ) : (
          <div className="sign-list">
            {mios.map((f) => (
              <div key={f.id} className="sign-item">
                <button
                  className="sign-thumb"
                  title={`Estampar «${f.name}»`}
                  onClick={() => onElegirImagen(f)}
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
                    title="Borrar este sello"
                    aria-label={`Borrar «${f.name}»`}
                    onClick={() => onBorrarImagen(f)}
                  >
                    <Icon name="close" size={12} />
                  </button>
                </div>
                <SelectorRanura firma={f} onCambiar={onCambiarRanura} />
              </div>
            ))}
          </div>
        )}
        <div className="card-actions">
          <button className="btn" onClick={onSubirImagen}>
            Añadir sello desde una imagen…
          </button>
          <button className="btn" onClick={onClose}>
            Cerrar
          </button>
        </div>
      </div>
    </>
  );
}
