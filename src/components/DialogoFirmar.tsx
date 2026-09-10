import { useState } from "react";
import type { FirmaGuardada } from "../api";
import { open } from "../dialogos";
import { useModal } from "../hooks/useModal";
import type { FirmaInfo } from "../api";
import {
  fechaLarga,
  permisosCertificacion,
  type FirmaDraft,
  type Rect,
} from "../tipos";
import { TSA_CONOCIDAS, type NivelCertificacion } from "../api";
import Icon from "./Icon";

/** Nombre de fichero de una ruta, para no enseñar la ruta entera. */
function nombreDe(ruta: string): string {
  return ruta.split(/[\\/]/).pop() ?? ruta;
}

/** Las tres opciones del `/DocMDP`, con el texto en llano y no el número:
 *  el usuario elige qué se podrá hacer después, no un nivel. La 2 es la que
 *  trae puesta Acrobat. */
const NIVELES: { nivel: NivelCertificacion; texto: string }[] = [
  { nivel: 1, texto: "Nadie puede cambiar nada" },
  { nivel: 2, texto: "Se pueden rellenar los formularios y firmar" },
  { nivel: 3, texto: "Además se puede comentar" },
];

/**
 * Un único diálogo para firmar con certificado: qué fichero hace falta y
 * por qué, el motivo, el nombre que se verá y la firma manuscrita que se
 * dibuja dentro del recuadro. Antes eran tres diálogos del sistema
 * encadenados sin explicación (U-13).
 *
 * Con `certificar` es el mismo diálogo con **un solo control más** —las tres
 * opciones del `/DocMDP`—, porque certificar es firmar diciendo además qué
 * se puede tocar después: dos diálogos distintos para lo mismo obligarían a
 * aprender dos.
 */
export default function DialogoFirmar({
  inicial,
  certificar,
  pagina,
  rect,
  firmas,
  firmasPrevias,
  onConfirm,
  onClose,
}: {
  inicial: FirmaDraft;
  /** Certificar en vez de firmar: añade el nivel y cambia los textos. */
  certificar: boolean;
  /** Página (desde 1) donde se ha dibujado el recuadro. */
  pagina: number;
  /** El recuadro dibujado, para enseñar la previa con su proporción. */
  rect: Rect;
  firmas: FirmaGuardada[];
  /** Firmas que ya lleva el documento: firmar encima ya no es un muro, así
   *  que lo que hace falta es decir qué le pasa a la que ya estaba. */
  firmasPrevias: FirmaInfo[];
  onConfirm: (d: FirmaDraft) => void;
  onClose: () => void;
}) {
  const [d, setD] = useState<FirmaDraft>(inicial);
  // «Avanzado» va plegado: quien firma un albarán no tiene por qué saber
  // qué es un sello de tiempo, y quien lo necesita lo busca
  const [avanzado, setAvanzado] = useState(inicial.tsa || inicial.ltv);
  const dibujo = firmas.find((f) => f.id === d.firmaId);
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
        aria-label={certificar ? "Certificar documento" : "Firmar con certificado"}
        onClick={(e) => e.stopPropagation()}
        onKeyDown={onKeyDown}
        ref={ref}
        tabIndex={-1}
      >
        <h3>{certificar ? "Certificar documento" : "Firmar con certificado"}</h3>
        <p className="modal-file" style={{ whiteSpace: "normal" }}>
          La firma se verá en el recuadro que has dibujado en la página{" "}
          <span className="dato">{pagina}</span>. Hace falta tu certificado
          —un fichero .p12 o .pfx del que se saca la clave con su contraseña,
          o un .pem con su clave aparte—: Vitela lo usa aquí, en tu ordenador,
          y no lo guarda.
        </p>

        {firmasPrevias.length > 0 && (
          <p className="modal-file" style={{ whiteSpace: "normal" }}>
            {(() => {
              const f = firmasPrevias[firmasPrevias.length - 1];
              const quien = f.name || f.cert_subject || "otra persona";
              const cuando = f.signed_at ? ` el ${fechaLarga(f.signed_at)}` : "";
              return `Ya lo ha firmado ${quien}${cuando}; tu firma se añadirá detrás sin tocar la suya: el fichero crece por el final y los bytes de antes se quedan donde estaban.`;
            })()}
          </p>
        )}
        {certificar && (
          <>
            <span className="card-label">Qué se podrá cambiar después</span>
            {NIVELES.map((n) => (
              <label className="opt-check" key={n.nivel}>
                <input
                  type="radio"
                  name="nivel-certificacion"
                  checked={d.nivel === n.nivel}
                  onChange={() => setD({ ...d, nivel: n.nivel })}
                />
                {n.texto}
              </label>
            ))}
            <p className="modal-file" style={{ whiteSpace: "normal" }}>
              Certificar dice que <strong>esta</strong> es la versión buena del
              documento. Cualquier otro cambio romperá el sello.
            </p>
          </>
        )}
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
        <label className="prop-field">
          <span className="card-label">Usar mi firma manuscrita</span>
          {firmas.length > 0 ? (
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
          ) : (
            <span className="dato">
              todavía no hay ninguna guardada: se dibujan en el modo Firma
            </span>
          )}
        </label>

        <button
          className="btn bloque-plegable"
          aria-expanded={avanzado}
          onClick={() => setAvanzado((v) => !v)}
        >
          <Icon name={avanzado ? "down" : "chevRight"} size={12} />
          Avanzado
        </button>
        {avanzado && (
          <>
            <label className="opt-check">
              <input
                type="checkbox"
                checked={d.tsa}
                onChange={(e) =>
                  setD({
                    ...d,
                    tsa: e.target.checked,
                    tsaUrl: d.tsaUrl || TSA_CONOCIDAS[0].url,
                  })
                }
              />
              Sellar la hora con un servidor de tiempo
            </label>
            {d.tsa && (
              <div className="card-row">
                <select
                  className="size-select"
                  aria-label="Servidor de tiempo"
                  value={
                    TSA_CONOCIDAS.some((t) => t.url === d.tsaUrl)
                      ? d.tsaUrl
                      : "otra"
                  }
                  onChange={(e) =>
                    setD({
                      ...d,
                      tsaUrl: e.target.value === "otra" ? "" : e.target.value,
                    })
                  }
                >
                  {TSA_CONOCIDAS.map((t) => (
                    <option key={t.url} value={t.url}>
                      {t.nombre}
                    </option>
                  ))}
                  <option value="otra">Otra…</option>
                </select>
                {!TSA_CONOCIDAS.some((t) => t.url === d.tsaUrl) && (
                  <input
                    type="text"
                    placeholder="https://…"
                    aria-label="Dirección del servidor de tiempo"
                    value={d.tsaUrl}
                    onChange={(e) => setD({ ...d, tsaUrl: e.target.value })}
                  />
                )}
              </div>
            )}
            <label className="opt-check">
              <input
                type="checkbox"
                checked={d.ltv}
                onChange={(e) => setD({ ...d, ltv: e.target.checked })}
              />
              Guardar la prueba de validez (LTV)
            </label>
            <p className="modal-file" style={{ whiteSpace: "normal" }}>
              {/* las dos necesitan red y se dice ANTES, no después de haber
                  elegido dónde guardar */}
              Las dos necesitan conexión: el sello de tiempo da fe de
              <strong> cuándo</strong> se firmó —sin él, la fecha es la del
              reloj de tu ordenador— y la prueba de validez guarda dentro del
              documento lo que hace falta para comprobar la firma dentro de
              diez años. Si el servidor no contesta, se pregunta antes de
              seguir.
            </p>
          </>
        )}

        {/* lo que va a quedar en el papel, con la proporción del recuadro que
            se ha dibujado: firmar deja de ser a ciegas */}
        <span className="card-label">Así quedará</span>
        <div
          className="firma-previa"
          style={{ aspectRatio: `${Math.max(1, rect.w)} / ${Math.max(1, rect.h)}` }}
        >
          {dibujo && (
            <img
              src={`data:image/png;base64,${dibujo.png_base64}`}
              alt="Firma manuscrita"
            />
          )}
          <span className="firma-previa-pie dato">
            {certificar ? "Certificado" : "Firmado"} por{" "}
            {d.signerName.trim() || "(el nombre del certificado)"}
            <br />
            {new Date().toLocaleDateString("es-ES")}
          </span>
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
                ? certificar
                  ? `Se pedirá dónde guardar el PDF certificado · ${permisosCertificacion(d.nivel)}`
                  : "Se pedirá dónde guardar el PDF firmado"
                : "Falta el certificado (y su contraseña o su clave)"
            }
            onClick={confirmar}
          >
            {certificar ? "Certificar y guardar como…" : "Firmar y guardar como…"}
          </button>
        </div>
      </div>
    </div>
  );
}
