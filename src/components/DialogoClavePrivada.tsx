import { open } from "../dialogos";
import { useModal } from "../hooks/useModal";
import Icon from "./Icon";

/** Nombre de fichero de una ruta, para no enseñar la ruta entera. */
function nombreDe(ruta: string): string {
  return ruta.split(/[\\/]/).pop() ?? ruta;
}

/** Lo que hace falta para abrir un PDF cifrado por certificado. */
export type ClaveDraft = {
  /** El PDF que se está intentando abrir. */
  path: string;
  /** El certificado con clave privada del usuario (.p12, .pfx o .pem). */
  keyPath: string;
  /** Su contraseña, si es un .p12 o un PEM cifrado. */
  password: string;
};

/**
 * «Este PDF está cifrado para unos destinatarios»: en vez de una contraseña
 * pide **tu certificado con clave privada**, el mismo fichero con el que se
 * firma. Es el hermano de `DialogoContrasena` y se abre por el mismo camino
 * —al fallar la apertura—, no desde un menú: nadie sabe de antemano que un
 * PDF va cifrado así.
 */
export default function DialogoClavePrivada({
  draft,
  error,
  onChange,
  onConfirm,
  onClose,
}: {
  draft: ClaveDraft;
  /** Lo que ha fallado en el último intento («este PDF no está cifrado para
   *  ese certificado»), dentro del diálogo, que sigue abierto. */
  error: string | null;
  onChange: (d: ClaveDraft) => void;
  onConfirm: () => void;
  onClose: () => void;
}) {
  const esP12 = /\.(p12|pfx)$/i.test(draft.keyPath);
  const listo = !!draft.keyPath && (!esP12 || !!draft.password);
  const confirmar = () => {
    if (listo) onConfirm();
  };
  const { ref, onKeyDown } = useModal({ onClose, onConfirm: confirmar });

  async function elegir() {
    const sel = await open({
      filters: [
        {
          name: "Certificado con clave privada (PKCS#12 o PEM)",
          extensions: ["p12", "pfx", "pem", "key"],
        },
      ],
      multiple: false,
      title: "Tu certificado con clave privada",
    });
    if (typeof sel === "string") onChange({ ...draft, keyPath: sel });
  }

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div
        className="modal"
        role="dialog"
        aria-modal="true"
        aria-label="Documento cifrado para unos destinatarios"
        onClick={(e) => e.stopPropagation()}
        onKeyDown={onKeyDown}
        ref={ref}
        tabIndex={-1}
      >
        <h3>Documento cifrado para unos destinatarios</h3>
        <p className="modal-file">{nombreDe(draft.path)}</p>
        <p className="modal-file" style={{ whiteSpace: "normal" }}>
          Este PDF no lleva contraseña: está cifrado para unas personas
          concretas. Para abrirlo hace falta <strong>tu certificado con clave
          privada</strong> —el mismo .p12, .pfx o .pem con el que firmas—.
          Vitela lo usa aquí, en tu ordenador, y no lo guarda.
        </p>
        <span className="card-label">Tu certificado</span>
        <div className="card-actions" style={{ justifyContent: "flex-start" }}>
          <button className="btn" onClick={elegir}>
            <Icon name="doc" size={14} />
            {draft.keyPath ? "Cambiar…" : "Elegir fichero…"}
          </button>
          <span className="dato">
            {draft.keyPath ? nombreDe(draft.keyPath) : "ninguno elegido"}
          </span>
        </div>
        {esP12 && (
          <label className="prop-field">
            <span className="card-label">Contraseña del certificado</span>
            <input
              type="password"
              value={draft.password}
              onChange={(e) => onChange({ ...draft, password: e.target.value })}
            />
          </label>
        )}
        {error && (
          <p className="modal-error" role="alert">
            {error}
          </p>
        )}
        <div className="card-actions">
          <button className="btn" onClick={onClose}>
            Cancelar
          </button>
          <button
            className="btn btn-primary"
            disabled={!listo}
            title={
              listo
                ? "Se probará con cada destinatario hasta dar con el tuyo"
                : "Falta el certificado (y su contraseña, si es un .p12)"
            }
            onClick={confirmar}
          >
            Abrir
          </button>
        </div>
      </div>
    </div>
  );
}
