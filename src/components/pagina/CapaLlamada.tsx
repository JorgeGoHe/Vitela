/**
 * Capa propia de la llamada (callout) y de la goma de borrar. Va aparte,
 * como el recorte de imagen: son dos gestos que se dibujan encima de todo
 * lo demás y no comparten nada con los overlays de anotación.
 */
import { ajustaLineas, altoCuadro, MOD, type Mode } from "../../tipos";
import type { Anotaciones } from "../../hooks/pagina/useAnotaciones";
import type { ToolProps } from "../Pagina";

export default function CapaLlamada({
  mode,
  anotaciones,
  scale,
  tool,
}: {
  mode: Mode;
  anotaciones: Anotaciones;
  scale: number;
  tool: ToolProps;
}) {
  const {
    calloutDraft,
    setCalloutDraft,
    commitCallout,
    gomaRect,
    gomaPos,
  } = anotaciones;

  return (
    <>
      {mode === "callout" && calloutDraft && (
        <>
          {/* la línea y la punta: lo que señala la llamada se ve desde el
              primer arrastre, no al guardar */}
          <svg className="callout-preview">
            <line
              x1={calloutDraft.punta.x * scale}
              y1={calloutDraft.punta.y * scale}
              x2={(calloutDraft.x + calloutDraft.w / 2) * scale}
              y2={(calloutDraft.y + calloutDraft.h / 2) * scale}
              stroke={tool.freeTextColor}
              strokeWidth={1.5}
            />
            <circle
              cx={calloutDraft.punta.x * scale}
              cy={calloutDraft.punta.y * scale}
              r={3.5}
              fill={tool.freeTextColor}
            />
          </svg>
          <div
            className="freetext-draft"
            style={{
              left: calloutDraft.x * scale,
              top: calloutDraft.y * scale,
              width: Math.max(40, calloutDraft.w * scale),
              height:
                Math.max(
                  24 / scale,
                  calloutDraft.h,
                  altoCuadro(
                    ajustaLineas(
                      calloutDraft.text,
                      calloutDraft.w,
                      tool.freeTextSize,
                    ).length,
                    tool.freeTextSize,
                  ),
                ) * scale,
              borderColor: tool.freeTextColor,
            }}
            onMouseDown={(e) => e.stopPropagation()}
          >
            <textarea
              autoFocus
              aria-label="Texto de la llamada"
              placeholder={`Escribe aquí · ${MOD}Enter la añade`}
              style={{
                color: tool.freeTextColor,
                fontSize: tool.freeTextSize * scale,
              }}
              value={calloutDraft.text}
              onChange={(e) =>
                setCalloutDraft({ ...calloutDraft, text: e.target.value })
              }
              onKeyDown={(e) => {
                if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
                  e.preventDefault();
                  commitCallout();
                } else if (e.key === "Escape") {
                  e.stopPropagation();
                  setCalloutDraft(null);
                }
              }}
            />
          </div>
        </>
      )}
      {mode === "draw" && tool.goma && (
        <>
          {/* lo que se ve es lo que se borra: la zona pintada es la misma
              que se le manda a `erase_ink` */}
          {gomaRect && (
            <div
              className="goma-zona"
              style={{
                left: gomaRect.x * scale,
                top: gomaRect.y * scale,
                width: gomaRect.w * scale,
                height: gomaRect.h * scale,
              }}
            />
          )}
          {gomaPos && (
            <div
              className="goma-cursor"
              style={{
                left: (gomaPos.x - tool.gomaAncho / 2) * scale,
                top: (gomaPos.y - tool.gomaAncho / 2) * scale,
                width: tool.gomaAncho * scale,
                height: tool.gomaAncho * scale,
              }}
            />
          )}
        </>
      )}
    </>
  );
}
