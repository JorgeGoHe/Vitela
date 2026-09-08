/** Borradores rectangulares: recorte, redacción y fantasma de la firma. */
import type { Mode } from "../../tipos";
import { clampCardLeft } from "../../hooks/pagina/geometria";
import type { Areas } from "../../hooks/pagina/useAreas";

type Props = {
  mode: Mode;
  areas: Areas;
  scale: number;
  displayWidth: number;
  activeSig: { png: string; ratio: number } | null;
  onModeChange: (m: Mode) => void;
};

export default function CapaAreas({
  mode,
  areas,
  scale,
  displayWidth,
  activeSig,
  onModeChange,
}: Props) {
  const {
    cropDraft,
    setCropDraft,
    redactDraft,
    setRedactDraft,
    redactReport,
    setRedactReport,
    sigDraft,
    applyCrop,
    applyRedact,
  } = areas;
  return (
    <>
      {mode === "crop" && cropDraft && (
        <>
          <div
            className="crop-rect"
            style={{
              left: cropDraft.x * scale,
              top: cropDraft.y * scale,
              width: cropDraft.w * scale,
              height: cropDraft.h * scale,
            }}
          />
          <div
            className="card crop-actions"
            style={{
              left: clampCardLeft(cropDraft.x * scale, displayWidth, 320),
              top: (cropDraft.y + cropDraft.h) * scale + 8,
            }}
            onMouseDown={(e) => e.stopPropagation()}
          >
            <div className="card-actions">
              <button
                className="btn btn-primary"
                onClick={() => applyCrop(false)}
              >
                Recortar
              </button>
              <button className="btn" onClick={() => applyCrop(true)}>
                Todas las páginas
              </button>
              <button
                className="btn"
                onClick={() => {
                  setCropDraft(null);
                  onModeChange("select");
                }}
              >
                Cancelar
              </button>
            </div>
          </div>
        </>
      )}
      {mode === "redact" && redactDraft && (
        <>
          <div
            className="redact-rect"
            style={{
              left: redactDraft.x * scale,
              top: redactDraft.y * scale,
              width: redactDraft.w * scale,
              height: redactDraft.h * scale,
            }}
          />
          <div
            className="card crop-actions"
            style={{
              left: clampCardLeft(redactDraft.x * scale, displayWidth, 320),
              top: (redactDraft.y + redactDraft.h) * scale + 8,
            }}
            onMouseDown={(e) => e.stopPropagation()}
          >
            <p>
              {redactReport
                ? `Se eliminarán ${redactReport.textos} bloque(s) de texto y ${redactReport.imagenes} imagen(es).`
                : "Calculando…"}
            </p>
            <div className="card-actions">
              <button
                className="btn btn-danger"
                disabled={!redactReport}
                onClick={applyRedact}
              >
                Redactar
              </button>
              <button
                className="btn"
                onClick={() => {
                  setRedactDraft(null);
                  setRedactReport(null);
                }}
              >
                Cancelar
              </button>
            </div>
          </div>
        </>
      )}
      {mode === "firmar" && activeSig && sigDraft && (
        <img
          className="sign-ghost"
          src={`data:image/png;base64,${activeSig.png}`}
          draggable={false}
          alt="Vista previa de la firma"
          style={{
            left: sigDraft.x * scale,
            top: sigDraft.y * scale,
            width: sigDraft.w * scale,
            height: sigDraft.h * scale,
          }}
        />
      )}
    </>
  );
}
