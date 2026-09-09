/** Borradores rectangulares: recorte, redacción y fantasma de la firma. */
import type { Mode, Rect } from "../../tipos";
import Icon from "../Icon";
import { clampCardLeft } from "../../hooks/pagina/geometria";
import type { Areas } from "../../hooks/pagina/useAreas";

type Props = {
  mode: Mode;
  areas: Areas;
  scale: number;
  displayWidth: number;
  activeSig: { png: string; ratio: number } | null;
  onModeChange: (m: Mode) => void;
  /** Zonas marcadas para censurar en esta página. */
  marcas: { markIndex: number; rect: Rect }[];
  onQuitarMarca: (markIndex: number) => void;
};

export default function CapaAreas({
  mode,
  areas,
  scale,
  displayWidth,
  activeSig,
  onModeChange,
  marcas,
  onQuitarMarca,
}: Props) {
  const { cropDraft, setCropDraft, redactDraft, sigDraft, certDraft, applyCrop } =
    areas;
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
        <div
          className="redact-rect"
          style={{
            left: redactDraft.x * scale,
            top: redactDraft.y * scale,
            width: redactDraft.w * scale,
            height: redactDraft.h * scale,
          }}
        />
      )}
      {mode === "redact" &&
        marcas.map((m) => (
          <div
            key={m.markIndex}
            className="redact-marca"
            tabIndex={0}
            role="button"
            aria-label={`Zona marcada para censurar; Supr la quita`}
            title="Zona marcada. Se censura al aplicar; Supr la quita"
            style={{
              left: m.rect.x * scale,
              top: m.rect.y * scale,
              width: m.rect.w * scale,
              height: m.rect.h * scale,
            }}
            onMouseDown={(e) => e.stopPropagation()}
            onKeyDown={(e) => {
              if (e.key === "Delete" || e.key === "Backspace") {
                e.preventDefault();
                e.stopPropagation();
                onQuitarMarca(m.markIndex);
              }
            }}
          >
            <button
              className="redact-quitar"
              title="Quitar esta marca"
              aria-label="Quitar esta marca"
              onClick={(e) => {
                e.stopPropagation();
                onQuitarMarca(m.markIndex);
              }}
            >
              <Icon name="close" size={11} />
            </button>
          </div>
        ))}
      {mode === "firma-cert" && certDraft && (
        <div
          className="crop-rect"
          style={{
            left: certDraft.x * scale,
            top: certDraft.y * scale,
            width: certDraft.w * scale,
            height: certDraft.h * scale,
          }}
        />
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
