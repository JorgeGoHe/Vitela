import { useState } from "react";
import type { FirmaGuardada } from "../api";
import { open } from "../dialogos";
import { useModal } from "../hooks/useModal";
import type { FirmaDraft } from "../tipos";
import Icon from "./Icon";

/** Nombre de fichero de una ruta, para no enseñar la ruta entera. */
function nombreDe(ruta: string): string {
  return ruta.split(/[\\/]/).pop() ?? ruta;
}

/**
 * Un único diálogo para firmar con certificado: qué fichero hace falta y
 * por qué, el motivo, el nombre que se verá y la firma manuscrita que se
 * dibuja dentro del recuadro. Antes eran tres diálogos del sistema
 * encadenados sin explicación (U-13).
 */
export default function DialogoFirmar({
  inicial,
  pagina,
  firmas,
  onConfirm,
  onClose,
}: {
  inicial: FirmaDraft;
  /** Página (desde 1) donde se ha dibujado el recuadro. */
  pagina: number;
  firmas: FirmaGuardada[];
  onConfirm: (d: FirmaDraft) => void;
  onClose: () => void;
}) {
  const [d, setD] = useState<FirmaDraft>(inicial);
  const esP12 = /\.(p12|pfx)$/i.test(d.certPath);
  const listo = !!d.certPath && (esP12 ? !!d.password : !!d.keyPath);
  const confirmar = () => {
    if (listo) onConfirm(d);
  };
  const { ref, onKeyDown } = useModal({ onClose, onConfirm: confirmar });

  async function elegirCert() {
    const sel = await open({
      filters: [
        {
          name: "Certificado (PKCS#12 o PEM)",
          extensions: ["p12", "pfx", "pem", "crt", "cer"],
        },
      ],
      multiple: false,
      title: "Certificado de firma",
    });
    if (typeof sel === "string") setD((v) => ({ ...v, certPath: sel }));
  }

  async function elegirClave() {
    const sel = await open({
      filters: [{ name: "Clave privada PEM", extensions: ["pem", "key"] }],
      multiple: false,
      title: "Clave privada",
    });
    if (typeof sel === "string") setD((v) => ({ ...v, keyPath: sel }));
  }

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div
        className="modal"
        role="dialog"
        aria-modal="true"
        aria-label="Firmar con certificado"
        onClick={(e) => e.stopPropagation()}
        onKeyDown={onKeyDown}
        ref={ref}
        tabIndex={-1}
      >
        <h3>Firmar con certificado</h3>
        <p className="modal-file" style={{ whiteSpace: "normal" }}>
          La firma se verá en el recuadro que has dibujado en la página{" "}
          <span className="dato">{pagina}</span>. Hace falta tu certificado
          —un fichero .p12 o .pfx del que se saca la clave con su contraseña,
          o un .pem con su clave aparte—: Vitela lo usa aquí, en tu ordenador,
          y no lo guarda.
        </p>

        <span className="card-label">Certificado</span>
        <div className="card-actions" style={{ justifyContent: "flex-start" }}>
          <button className="btn" onClick={elegirCert}>
            <Icon name="doc" size={14} />
            {d.certPath ? "Cambiar…" : "Elegir fichero…"}
          </button>
          <span className="dato">
            {d.certPath ? nombreDe(d.certPath) : "ninguno elegido"}
          </span>
        </div>
        {esP12 && (
          <label className="prop-field">
            <span className="card-label">Contraseña del certificado</span>
            <input
              type="password"
              value={d.password}
              onChange={(e) => setD({ ...d, password: e.target.value })}
            />
          </label>
        )}
        {!!d.certPath && !esP12 && (
          <div className="card-actions" style={{ justifyContent: "flex-start" }}>
            <button className="btn" onClick={elegirClave}>
              <Icon name="doc" size={14} />
              {d.keyPath ? "Cambiar la clave…" : "Elegir la clave privada…"}
            </button>
            <span className="dato">
              {d.keyPath ? nombreDe(d.keyPath) : "ninguna elegida"}
            </span>
          </div>
        )}

        <label className="prop-field">
          <span className="card-label">Nombre que se verá</span>
          <input
            type="text"
            placeholder="El del certificado"
            value={d.signerName}
            onChange={(e) => setD({ ...d, signerName: e.target.value })}
          />
        </label>
        <label className="prop-field">
          <span className="card-label">Motivo</span>
          <input
            type="text"
            placeholder="Conforme, Revisado…"
            value={d.reason}
            onChange={(e) => setD({ ...d, reason: e.target.value })}
          />
        </label>
        {firmas.length > 0 && (
          <label className="prop-field">
            <span className="card-label">Usar mi firma manuscrita</span>
            <select
              className="size-select"
              value={d.firmaId}
              onChange={(e) => setD({ ...d, firmaId: e.target.value })}
            >
              <option value="">Sin dibujo: solo el nombre y la fecha</option>
              {firmas.map((f) => (
                <option key={f.id} value={f.id}>
                  {f.name}
                </option>
              ))}
            </select>
          </label>
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
                ? "Se pedirá dónde guardar el PDF firmado"
                : "Falta el certificado (y su contraseña o su clave)"
            }
            onClick={confirmar}
          >
            Firmar y guardar como…
          </button>
        </div>
      </div>
    </div>
  );
}
