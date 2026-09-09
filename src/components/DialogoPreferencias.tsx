import { MOD, type Preferencias } from "../tipos";
import { useModal } from "../hooks/useModal";

/** Una fila de la columna: etiqueta, control y, si hace falta, su ayuda. */
function Fila({
  etiqueta,
  ayuda,
  children,
}: {
  etiqueta: string;
  ayuda?: string;
  children: React.ReactNode;
}) {
  return (
    <label className="prop-field">
      <span className="card-label">{etiqueta}</span>
      {children}
      {ayuda && (
        <span className="opt-hint" style={{ whiteSpace: "normal" }}>
          {ayuda}
        </span>
      )}
    </label>
  );
}

/**
 * Preferencias de la app (⌘,): una sola columna, sin pestañas, y cada
 * cambio se aplica al instante —por eso el botón dice «Cerrar» y no
 * «Guardar»—. Se guardan en `localStorage`.
 */
export default function DialogoPreferencias({
  prefs,
  onCambio,
  onClose,
}: {
  prefs: Preferencias;
  onCambio: (p: Preferencias) => void;
  onClose: () => void;
}) {
  const { ref, onKeyDown } = useModal({ onClose, onConfirm: onClose });
  const cambia = (parte: Partial<Preferencias>) =>
    onCambio({ ...prefs, ...parte });

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div
        className="modal"
        role="dialog"
        aria-modal="true"
        aria-label="Preferencias"
        onClick={(e) => e.stopPropagation()}
        onKeyDown={onKeyDown}
        ref={ref}
        tabIndex={-1}
      >
        <h3>Preferencias</h3>

        <Fila etiqueta="Tema">
          <select
            className="size-select"
            aria-label="Tema de la aplicación"
            value={prefs.tema}
            onChange={(e) =>
              cambia({ tema: e.target.value as Preferencias["tema"] })
            }
          >
            <option value="automatico">El del sistema</option>
            <option value="claro">Claro</option>
            <option value="oscuro">Oscuro</option>
          </select>
        </Fila>

        <Fila etiqueta="Zoom al abrir">
          <select
            className="size-select"
            aria-label="Zoom con el que se abre un documento"
            value={prefs.zoomInicial}
            onChange={(e) =>
              cambia({
                zoomInicial: e.target.value as Preferencias["zoomInicial"],
              })
            }
          >
            <option value="pagina">Ajustar a la página</option>
            <option value="ancho">Ajustar al ancho</option>
            <option value="100">100 %</option>
            <option value="ultimo">El último que hubiera</option>
          </select>
        </Fila>

        <Fila
          etiqueta="Color del lienzo"
          ayuda="El fondo sobre el que se apoya la hoja."
        >
          <select
            className="size-select"
            aria-label="Color del fondo del visor"
            value={prefs.lienzo}
            onChange={(e) =>
              cambia({ lienzo: e.target.value as Preferencias["lienzo"] })
            }
          >
            <option value="verde">Verde alfombrilla</option>
            <option value="gris">Gris cálido</option>
          </select>
        </Fila>

        <Fila
          etiqueta="Modo nocturno del documento"
          ayuda="Oscurece el papel y aclara la tinta. Solo cambia lo que ves: el fichero no se toca y las imágenes que exportes siguen en blanco."
        >
          <span className="opt-check">
            <input
              type="checkbox"
              checked={prefs.nocturno}
              onChange={(e) => cambia({ nocturno: e.target.checked })}
            />
            Encendido (⇧{MOD}L)
          </span>
        </Fila>

        <Fila
          etiqueta="Autor de los comentarios"
          ayuda="Es el nombre que se guarda en cada resaltado, nota, forma o sello que crees a partir de ahora. En blanco se usa el del sistema."
        >
          <input
            type="text"
            placeholder="El nombre de usuario del sistema"
            value={prefs.autor}
            onChange={(e) => cambia({ autor: e.target.value })}
          />
        </Fila>

        <div className="card-actions">
          <button className="btn btn-primary" onClick={onClose}>
            Cerrar
          </button>
        </div>
      </div>
    </div>
  );
}
