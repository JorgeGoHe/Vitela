import { useState } from "react";
import type { DestinatarioCifrado, Permisos } from "../api";
import { TODO_PERMITIDO } from "../api";
import { open } from "../dialogos";
import { useModal } from "../hooks/useModal";
import Icon from "./Icon";

/** Nombre de fichero de una ruta, para no enseñar la ruta entera. */
function nombreDe(ruta: string): string {
  return ruta.split(/[\\/]/).pop() ?? ruta;
}

/** Los tres permisos, con la frase que lee el usuario. */
const PERMISOS: [keyof Permisos, string][] = [
  ["imprimir", "Imprimir"],
  ["copiar", "Copiar texto"],
  ["editar", "Editar y comentar"],
];

/**
 * «Cifrar con certificado…»: en vez de una contraseña, una lista de
 * destinatarios —el certificado público de cada uno— y lo que se le deja
 * hacer a cada cual. Solo quien tenga la clave privada correspondiente podrá
 * abrir el documento.
 *
 * La primera línea dice qué es esto y cuándo se usa, porque «cifrado por
 * certificado» no se lo dice a nadie que no lo sepa ya.
 */
export default function DialogoCifrarCert({
  firmado,
  onConfirm,
  onClose,
}: {
  /** El documento lleva firma: cifrar reescribe el fichero y la rompe. */
  firmado: boolean;
  onConfirm: (destinatarios: DestinatarioCifrado[]) => void;
  onClose: () => void;
}) {
  const [lista, setLista] = useState<DestinatarioCifrado[]>([]);
  const listo = lista.length > 0;
  const confirmar = () => {
    if (listo) onConfirm(lista);
  };
  const { ref, onKeyDown } = useModal({ onClose, onConfirm: confirmar });

  async function anadir() {
    const sel = await open({
      filters: [
        { name: "Certificado", extensions: ["cer", "crt", "pem", "der"] },
      ],
      multiple: true,
      title: "Certificado del destinatario",
    });
    const rutas =
      typeof sel === "string" ? [sel] : Array.isArray(sel) ? sel : [];
    if (rutas.length === 0) return;
    setLista((v) => [
      ...v,
      ...rutas
        .filter((r) => !v.some((d) => d.cert_path === r))
        .map((r) => ({ cert_path: r, permisos: { ...TODO_PERMITIDO } })),
    ]);
  }

  function cambiaPermiso(i: number, clave: keyof Permisos) {
    setLista((v) =>
      v.map((d, j) =>
        j === i
          ? { ...d, permisos: { ...d.permisos, [clave]: !d.permisos[clave] } }
          : d,
      ),
    );
  }

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div
        className="modal modal-hf"
        role="dialog"
        aria-modal="true"
        aria-label="Cifrar con certificado"
        onClick={(e) => e.stopPropagation()}
        onKeyDown={onKeyDown}
        ref={ref}
        tabIndex={-1}
      >
        <h3>Cifrar con certificado</h3>
        <p className="modal-file" style={{ whiteSpace: "normal" }}>
          Solo quien tenga la clave privada de estos certificados podrá abrir
          el documento. Es lo que usan las administraciones; si lo que quieres
          es una contraseña que puedas decir por teléfono, usa «Proteger con
          contraseña…».
        </p>
        {firmado && (
          <p className="modal-error" role="alert">
            Este documento lleva firma digital: cifrarlo reescribe el fichero y
            la firma dejará de valer.
          </p>
        )}
        <span className="card-label">Destinatarios</span>
        {lista.length === 0 ? (
          <p className="sign-empty">
            Todavía no hay ninguno. Añade el certificado público (.cer o .pem)
            de cada persona que tenga que poder abrirlo.
          </p>
        ) : (
          <div className="combinar-lista">
            {lista.map((d, i) => (
              <div className="combinar-fila" key={d.cert_path}>
                <span className="combinar-nombre">
                  <span>{nombreDe(d.cert_path)}</span>
                  <span className="reciente-dir">{d.cert_path}</span>
                </span>
                <div className="card-row">
                  {PERMISOS.map(([clave, etiqueta]) => (
                    <label className="opt-check" key={clave}>
                      <input
                        type="checkbox"
                        checked={d.permisos[clave]}
                        onChange={() => cambiaPermiso(i, clave)}
                      />
                      {etiqueta}
                    </label>
                  ))}
                </div>
                <button
                  className="btn"
                  aria-label={`Quitar ${nombreDe(d.cert_path)} de la lista`}
                  onClick={() =>
                    setLista((v) => v.filter((_, j) => j !== i))
                  }
                >
                  Quitar
                </button>
              </div>
            ))}
          </div>
        )}
        <div className="card-actions" style={{ justifyContent: "flex-start" }}>
          <button className="btn" onClick={anadir}>
            <Icon name="plus" size={14} />
            Añadir destinatario…
          </button>
        </div>
        <div className="card-actions">
          <button className="btn" onClick={onClose}>
            Cancelar
          </button>
          <button
            className="btn btn-primary"
            disabled={!listo}
            title={
              listo
                ? "Se pedirá dónde guardar la copia cifrada"
                : "Añade al menos un destinatario"
            }
            onClick={confirmar}
          >
            Guardar copia cifrada…
          </button>
        </div>
      </div>
    </div>
  );
}
