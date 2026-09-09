import type { FirmaInfo } from "../api";
import { estadoDeFirma, fechaLarga } from "../tipos";
import Icon from "./Icon";

/**
 * Pestaña «Firmas» del sidebar: una tarjeta por firma con quién firmó,
 * quién emitió su certificado, cuándo, por qué y qué se ha comprobado.
 * Clic lleva a la página del recuadro.
 */
export default function PanelFirmasDoc({
  firmas,
  onGoto,
}: {
  firmas: FirmaInfo[];
  onGoto: (page: number) => void;
}) {
  if (firmas.length === 0) {
    return (
      <div className="com-panel">
        <p className="sign-empty">Este documento no lleva ninguna firma.</p>
      </div>
    );
  }
  return (
    <div className="com-panel">
      <span className="com-total dato">
        {firmas.length === 1 ? "1 firma" : `${firmas.length} firmas`}
      </span>
      {firmas.map((f, i) => {
        const estado = estadoDeFirma(f);
        return (
          <div
            key={i}
            className={`firma-card ${estado.nivel}`}
            role={f.page_index === null ? undefined : "button"}
            tabIndex={f.page_index === null ? undefined : 0}
            title={
              f.page_index === null
                ? "Firma sin recuadro visible"
                : `Ir a la página ${f.page_index + 1}`
            }
            onClick={() => {
              if (f.page_index !== null) onGoto(f.page_index);
            }}
            onKeyDown={(e) => {
              if ((e.key === "Enter" || e.key === " ") && f.page_index !== null) {
                e.preventDefault();
                onGoto(f.page_index);
              }
            }}
          >
            <span className="firma-estado">
              <Icon
                name={
                  estado.nivel === "ok"
                    ? "lock"
                    : estado.nivel === "duda"
                      ? "sliders"
                      : "close"
                }
                size={13}
              />
              {estado.texto}
            </span>
            <span className="firma-quien">
              {f.name || f.cert_subject || "Firmante sin nombre"}
            </span>
            {f.signed_at && (
              <span className="firma-linea dato">{fechaLarga(f.signed_at)}</span>
            )}
            {f.reason && <span className="firma-linea">{f.reason}</span>}
            {f.cert_issuer && (
              <span className="firma-linea">Emitido por {f.cert_issuer}</span>
            )}
            <span className="firma-linea">
              {f.expired
                ? "El certificado está caducado"
                : f.not_after
                  ? `Certificado válido hasta ${fechaLarga(f.not_after)}`
                  : "Sin fecha de caducidad"}
              {f.self_signed ? " · autofirmado, no lo respalda nadie más" : ""}
            </span>
            {/* el algoritmo, en Fragment Mono: es el dato que explica por qué
                una firma sale como «no se ha podido comprobar» */}
            {f.algoritmo && (
              <span className="firma-linea dato">{f.algoritmo}</span>
            )}
            {f.page_index !== null && (
              <span className="firma-linea dato">
                pág. {f.page_index + 1}
              </span>
            )}
          </div>
        );
      })}
    </div>
  );
}
