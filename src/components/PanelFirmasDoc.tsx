import type { FirmaInfo } from "../api";
import { estadoDeFirma, fechaLarga, permisosCertificacion } from "../tipos";
import Icon from "./Icon";

/** El certificado no está caducado sino al revés: su periodo de validez
 *  empezaba después de la fecha de la firma. El backend lo marca igual
 *  (`expired`), y decir «caducado» de un certificado que aún no había
 *  entrado en vigor sería contar lo contrario de lo que pasa. */
function aunNoValido(f: FirmaInfo): boolean {
  // lo contesta el backend, que es quien ha leído el certificado y la hora
  // de la firma; aquí no se vuelve a calcular
  return f.not_yet_valid === true;
}

/** Quién responde por el certificado, en una línea. La confianza es del
 *  certificado y la validez es del documento: son dos cosas distintas y por
 *  eso esta línea **no cambia el color de la tarjeta ni el de la banda** —
 *  mezclarlas es lo que hace incomprensible el aviso de Acrobat. Y nunca
 *  dice «válida»: al abrir no se consulta a nadie; si la firma lleva
 *  archivada la prueba de vigencia (LTV), se dice aparte y con su fecha. */
function quienResponde(f: FirmaInfo): string {
  const emisor = f.cert_issuer || "un emisor sin nombre";
  if (f.confianza === "raiz_conocida")
    return `Emitido por ${emisor} · reconocido por el sistema`;
  // sin `confianza` (un backend anterior) manda lo que sí se sabe leer del
  // certificado: si se firmó a sí mismo
  if (f.confianza === "autofirmado" || (!f.confianza && f.self_signed))
    return "Autofirmado, no lo respalda nadie más";
  return `Emitido por ${emisor} · no se ha podido comprobar quién lo emitió`;
}

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
            {/* una firma que certifica avala el documento entero y dice qué
                se puede cambiar detrás sin romperlo: es otra cosa que firmar
                y la tarjeta lo dice con las mismas palabras que la banda */}
            {!!f.certifica && (
              <span className="firma-linea">
                Certifica el documento · {permisosCertificacion(f.certifica)}
              </span>
            )}
            {f.signed_at && (
              <span className="firma-linea dato">{fechaLarga(f.signed_at)}</span>
            )}
            {/* la fecha de una firma SIN sello de tiempo es la del reloj del
                que firmó: cuando hay sello se dice quién da fe de ella */}
            {f.sello_de_tiempo && (
              <span className="firma-linea">
                Hora sellada por {f.sello_de_tiempo.autoridad || "un servidor de tiempo"}
                {f.sello_de_tiempo.fecha
                  ? ` el ${fechaLarga(f.sello_de_tiempo.fecha)}`
                  : ""}
              </span>
            )}
            {f.reason && <span className="firma-linea">{f.reason}</span>}
            <span className="firma-linea">{quienResponde(f)}</span>
            {/* la prueba de vigencia viaja dentro del PDF y se lee de ahí:
                Vitela no llama a nadie al abrir un documento */}
            {f.ltv_archivado && (
              <span className="firma-linea">
                Prueba de vigencia del certificado archivada en el documento
                {f.ltv_fecha ? ` (${fechaLarga(f.ltv_fecha)})` : ""}
              </span>
            )}
            <span className="firma-linea">
              {f.expired
                ? aunNoValido(f)
                  ? "El certificado todavía no era válido cuando se firmó"
                  : "El certificado está caducado"
                : f.not_after
                  ? `Certificado válido hasta ${fechaLarga(f.not_after)}`
                  : "Sin fecha de caducidad"}
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
