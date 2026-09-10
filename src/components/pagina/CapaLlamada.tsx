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
    calloutPuntos,
    commitCallout,
    gomaRect,
    gomaPos,
  } = anotaciones;

  return (
    <>
      {/* los puntos ya puestos a clics: la punta y, si se ha puesto, el
          codo. Sin esto el segundo clic no se ve en ninguna parte */}
      {mode === "callout" && !calloutDraft && calloutPuntos.length > 0 && (
        <svg className="callout-preview">
          {calloutPuntos.length > 1 && (
            <line
              x1={calloutPuntos[0].x * scale}
              y1={calloutPuntos[0].y * scale}
              x2={calloutPuntos[1].x * scale}
              y2={calloutPuntos[1].y * scale}
              stroke={tool.freeTextColor}
              strokeWidth={1.5}
            />
          )}
          {calloutPuntos.map((p, i) => (
            <circle
              key={i}
              cx={p.x * scale}
              cy={p.y * scale}
              r={3.5}
              fill={tool.freeTextColor}
            />
          ))}
        </svg>
      )}
      {mode === "callout" && calloutDraft && (
        <>
          {/* la línea y la punta: lo que señala la llamada se ve desde el
              primer arrastre, no al guardar */}
          <svg className="callout-preview">
            {/* con codo la línea va en dos tramos, como la `/CL` de tres
                puntos que se guarda */}
            <polyline
              points={[
                [calloutDraft.punta.x, calloutDraft.punta.y],
                ...(calloutDraft.codo
                  ? [[calloutDraft.codo.x, calloutDraft.codo.y]]
                  : []),
                [
                  calloutDraft.x + calloutDraft.w / 2,
                  calloutDraft.y + calloutDraft.h / 2,
                ],
              ]
                .map(([px, py]) => `${px * scale},${py * scale}`)
                .join(" ")}
              fill="none"
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
              que se le manda a `erase_ink_area` */}
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
