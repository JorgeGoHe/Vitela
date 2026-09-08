/**
 * Overlays de anotaciones. Van en dos trozos porque el orden del DOM manda
 * en el apilado: las marcas y las cajas de sello/dibujo (`MarcasAnotaciones`)
 * quedan debajo de enlaces y campos; los iconos de nota, popovers y las
 * previsualizaciones de trazo/forma (`CapaAnotaciones`) encima.
 */
import { KIND_LABELS, type Mode } from "../../tipos";
import { clampCardLeft } from "../../hooks/pagina/geometria";
import type { Anotaciones } from "../../hooks/pagina/useAnotaciones";
import Icon from "../Icon";
import type { ToolProps } from "../Pagina";

export function MarcasAnotaciones({
  mode,
  anotaciones,
  scale,
}: {
  mode: Mode;
  anotaciones: Anotaciones;
  scale: number;
}) {
  const { annots, annotDraft, startAnnotAction } = anotaciones;
  return (
    <>
      {annots
        .filter((a) => a.kind === "Highlight")
        .flatMap((a) =>
          a.rects.map((r, j) => (
            <div
              key={`h${a.index}-${j}`}
              className="annot-highlight"
              style={{
                left: r.x * scale,
                top: r.y * scale,
                width: r.w * scale,
                height: r.h * scale,
                background: a.color
                  ? `rgba(${a.color[0]}, ${a.color[1]}, ${a.color[2]}, 0.45)`
                  : undefined,
              }}
            />
          )),
        )}
      {annots
        .filter(
          (a) =>
            a.kind === "Underline" ||
            a.kind === "Strikeout" ||
            a.kind === "StrikeOut",
        )
        .flatMap((a) =>
          a.rects.map((r, j) => (
            <div
              key={`u${a.index}-${j}`}
              className={
                a.kind === "Underline" ? "annot-underline" : "annot-strike"
              }
              style={{
                left: r.x * scale,
                top:
                  a.kind === "Underline"
                    ? (r.y + r.h) * scale - 2
                    : (r.y + r.h * 0.55) * scale - 1,
                width: r.w * scale,
                background: a.color
                  ? `rgba(${a.color[0]}, ${a.color[1]}, ${a.color[2]}, 0.9)`
                  : undefined,
              }}
            />
          )),
        )}
      {mode === "select" &&
        annots
          .filter((a) => a.kind === "Ink" || a.kind === "Stamp")
          .map((a) => {
            const d =
              annotDraft && annotDraft.index === a.index ? annotDraft : a;
            return (
              <div
                key={`an${a.index}`}
                className="annot-hit"
                title="Arrastrar para mover · clic para opciones"
                style={{
                  left: d.x * scale,
                  top: d.y * scale,
                  width: d.w * scale,
                  height: d.h * scale,
                }}
                onMouseDown={(e) => startAnnotAction(e, a, "move")}
              >
                {(
                  ["nw", "n", "ne", "e", "se", "s", "sw", "w"] as const
                ).map((hd) => (
                  <div
                    key={hd}
                    className={`image-handle h-${hd}`}
                    title="Redimensionar (Shift: libre en esquinas)"
                    onMouseDown={(e) => startAnnotAction(e, a, "resize", hd)}
                  />
                ))}
              </div>
            );
          })}
    </>
  );
}

type Props = {
  mode: Mode;
  anotaciones: Anotaciones;
  scale: number;
  displayWidth: number;
  tool: ToolProps;
};

export default function CapaAnotaciones({
  mode,
  anotaciones,
  scale,
  displayWidth,
  tool,
}: Props) {
  const {
    annots,
    notePopover,
    setNotePopover,
    noteDraft,
    setNoteDraft,
    strokePts,
    shapeDraft,
    submitNote,
    deleteAnnotation,
  } = anotaciones;
  return (
    <>
      {annots
        .filter((a) => a.kind === "Text")
        .map((a) => (
          <button
            key={`n${a.index}`}
            className="note-icon"
            style={{
              left: a.x * scale,
              top: a.y * scale,
              width: Math.max(18, a.w * scale),
              height: Math.max(18, a.h * scale),
            }}
            title={a.contents}
            onMouseDown={(e) => e.stopPropagation()}
            onClick={(e) => {
              e.stopPropagation();
              setNotePopover((p) => (p?.index === a.index ? null : a));
            }}
          >
            <Icon name="note" size={12} />
          </button>
        ))}
      {notePopover && (
        <div
          className="card"
          style={{
            left: clampCardLeft(notePopover.x * scale, displayWidth),
            top: (notePopover.y + notePopover.h) * scale + 6,
          }}
          onMouseDown={(e) => e.stopPropagation()}
        >
          <p>
            {notePopover.contents ||
              KIND_LABELS[notePopover.kind] ||
              notePopover.kind}
          </p>
          <div className="card-actions">
            <button
              className="btn btn-danger"
              onClick={() => deleteAnnotation(notePopover)}
            >
              <Icon name="trash" size={13} />
              Eliminar
            </button>
            <button className="btn" onClick={() => setNotePopover(null)}>
              Cerrar
            </button>
          </div>
        </div>
      )}
      {noteDraft && (
        <div
          className="card"
          style={{
            left: clampCardLeft(noteDraft.x * scale, displayWidth),
            top: noteDraft.y * scale,
          }}
          onMouseDown={(e) => e.stopPropagation()}
        >
          <textarea
            autoFocus
            placeholder="Escribe la nota y pulsa Enter…"
            value={noteDraft.text}
            onChange={(e) =>
              setNoteDraft({ ...noteDraft, text: e.target.value })
            }
            onKeyDown={(e) => {
              if (e.key === "Enter" && !e.shiftKey) {
                e.preventDefault();
                submitNote();
              }
              if (e.key === "Escape") setNoteDraft(null);
            }}
          />
        </div>
      )}
      {strokePts.length > 1 && (
        <svg className="stroke-preview">
          <polyline
            points={strokePts
              .map((p) => `${p[0] * scale},${p[1] * scale}`)
              .join(" ")}
            stroke={tool.drawColor}
            strokeWidth={tool.drawWidth * scale}
          />
        </svg>
      )}
      {mode === "shape" && shapeDraft && (
        <svg className="shape-preview">
          {(() => {
            const d = shapeDraft;
            const stroke = tool.shapeColor;
            const sw = tool.shapeWidth * scale;
            const fillable =
              tool.shapeKind === "rect" || tool.shapeKind === "ellipse";
            const fill =
              tool.shapeFill && fillable ? `${tool.shapeColor}46` : "none";
            const x = Math.min(d.x1, d.x2) * scale;
            const y = Math.min(d.y1, d.y2) * scale;
            const w = Math.abs(d.x2 - d.x1) * scale;
            const h = Math.abs(d.y2 - d.y1) * scale;
            if (tool.shapeKind === "rect")
              return (
                <rect
                  x={x}
                  y={y}
                  width={w}
                  height={h}
                  stroke={stroke}
                  strokeWidth={sw}
                  fill={fill}
                />
              );
            if (tool.shapeKind === "ellipse")
              return (
                <ellipse
                  cx={x + w / 2}
                  cy={y + h / 2}
                  rx={w / 2}
                  ry={h / 2}
                  stroke={stroke}
                  strokeWidth={sw}
                  fill={fill}
                />
              );
            const pts = {
              x1: d.x1 * scale,
              y1: d.y1 * scale,
              x2: d.x2 * scale,
              y2: d.y2 * scale,
            };
            const head = (12 + tool.shapeWidth * 2) * scale;
            const ang = Math.atan2(pts.y2 - pts.y1, pts.x2 - pts.x1);
            return (
              <>
                <line {...pts} stroke={stroke} strokeWidth={sw} />
                {tool.shapeKind === "arrow" &&
                  [Math.PI / 6, -Math.PI / 6].map((delta, i) => {
                    const a2 = ang + Math.PI - delta;
                    return (
                      <line
                        key={i}
                        x1={pts.x2}
                        y1={pts.y2}
                        x2={pts.x2 + head * Math.cos(a2)}
                        y2={pts.y2 + head * Math.sin(a2)}
                        stroke={stroke}
                        strokeWidth={sw}
                      />
                    );
                  })}
              </>
            );
          })()}
        </svg>
      )}
    </>
  );
}
