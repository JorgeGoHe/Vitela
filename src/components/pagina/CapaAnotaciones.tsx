/**
 * Overlays de anotaciones. Van en dos trozos porque el orden del DOM manda
 * en el apilado: las marcas y las cajas de sello/dibujo (`MarcasAnotaciones`)
 * quedan debajo de enlaces y campos; los iconos de nota, popovers y las
 * previsualizaciones de trazo/forma (`CapaAnotaciones`) encima.
 */
import {
  ajustaLineas,
  altoCuadro,
  ANNOT_COLORS,
  firmaAnotacion,
  KIND_LABELS,
  MOD,
  NOMBRE_COLOR,
  type AnnotationInfo,
  type Mode,
} from "../../tipos";
import { clampCardLeft } from "../../hooks/pagina/geometria";
import type { Anotaciones } from "../../hooks/pagina/useAnotaciones";
import Icon from "../Icon";
import type { ToolProps } from "../Pagina";

/** Color de la anotación en hexadecimal, para marcar el chip activo. */
function hexDeAnotacion(a: AnnotationInfo): string | null {
  if (!a.color) return null;
  return `#${a.color
    .slice(0, 3)
    .map((n) => n.toString(16).padStart(2, "0"))
    .join("")}`;
}

/** Marcas de texto: las que se pueden seleccionar con un clic. */
const MARCAS = ["Highlight", "Underline", "Strikeout", "StrikeOut"];

export function MarcasAnotaciones({
  mode,
  anotaciones,
  scale,
}: {
  mode: Mode;
  anotaciones: Anotaciones;
  scale: number;
}) {
  const { annots, annotDraft, startAnnotAction, setNotePopover } = anotaciones;
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
      {/* los overlays de arriba son decorativos (pointer-events: none): la
          zona clicable de cada marca va aquí, como en Acrobat, donde un
          clic la selecciona y Supr la borra */}
      {mode === "select" &&
        annots
          .filter((a) => MARCAS.includes(a.kind))
          .flatMap((a) =>
            (a.rects.length > 0
              ? a.rects
              : [{ x: a.x, y: a.y, w: a.w, h: a.h }]
            ).map((r, j) => (
              <div
                key={`hm${a.index}-${j}`}
                className="annot-hit-markup"
                title={`${KIND_LABELS[a.kind] ?? a.kind} · clic para opciones`}
                style={{
                  left: r.x * scale,
                  top: r.y * scale,
                  width: r.w * scale,
                  height: r.h * scale,
                }}
                onClick={(e) => {
                  e.stopPropagation();
                  setNotePopover((p) => (p?.index === a.index ? null : a));
                }}
              />
            )),
          )}
      {/* Las marcas de redacción (`Square`) entran aquí con todo lo demás: el
          contrato decía «se selecciona, se mueve y se borra como cualquier
          otra anotación» y en modo Seleccionar eran inertes. */}
      {mode === "select" &&
        annots
          .filter(
            (a) =>
              a.kind === "Ink" ||
              a.kind === "Stamp" ||
              a.kind === "FreeText" ||
              a.kind === "Square",
          )
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
  /** Quitar una marca de redacción por su `annot_index`: va por
   *  `unmark_redaction`, que comprueba que la anotación es una marca y
   *  refresca la lista del documento. */
  onQuitarMarca: (annotIndex: number) => void;
};

export default function CapaAnotaciones({
  mode,
  anotaciones,
  scale,
  displayWidth,
  tool,
  onQuitarMarca,
}: Props) {
  const {
    annots,
    annotDraft,
    notePopover,
    setNotePopover,
    noteEdit,
    setNoteEdit,
    guardarContenido,
    cambiarColor,
    cerrarPopover,
    noteDraft,
    setNoteDraft,
    strokePts,
    shapeDraft,
    submitNote,
    deleteAnnotation,
    startAnnotAction,
    freeTextDraft,
    setFreeTextDraft,
    commitFreeText,
  } = anotaciones;
  const editando =
    notePopover && noteEdit?.index === notePopover.index ? noteEdit : null;
  return (
    <>
      {annots
        .filter((a) => a.kind === "Text")
        .map((a) => {
          // el icono es su propia zona de arrastre (el mismo gesto de sellos
          // y dibujos, sin tiradores: una nota no se redimensiona en Acrobat)
          const d = annotDraft?.index === a.index ? annotDraft : a;
          return (
            <button
              key={`n${a.index}`}
              className="note-icon"
              style={{
                left: d.x * scale,
                top: d.y * scale,
                width: Math.max(18, d.w * scale),
                height: Math.max(18, d.h * scale),
              }}
              title={`${a.contents}\n\nArrastrar para mover · doble clic para editar`}
              onMouseDown={(e) => startAnnotAction(e, a, "move")}
              onDoubleClick={(e) => {
                e.stopPropagation();
                setNotePopover(a);
                setNoteEdit({ index: a.index, text: a.contents });
              }}
            >
              <Icon name="note" size={12} />
            </button>
          );
        })}
      {/* Una marca de redacción no es un comentario: ni color, ni texto que
          corregir. Lo único que se puede hacer con ella es quitarla, y
          conviene recordar que todavía no ha borrado nada. */}
      {notePopover && notePopover.kind === "Square" && (
        <div
          className="card"
          style={{
            left: clampCardLeft(notePopover.x * scale, displayWidth),
            top: (notePopover.y + notePopover.h) * scale + 6,
          }}
          onMouseDown={(e) => e.stopPropagation()}
        >
          <p>
            Zona marcada para censurar. Arrástrala para ajustarla; no se borra
            nada hasta que pulses «Aplicar redacción».
          </p>
          {firmaAnotacion(notePopover.author, notePopover.modified) && (
            <p className="annot-firma dato">
              {firmaAnotacion(notePopover.author, notePopover.modified)}
            </p>
          )}
          <div className="card-actions">
            <button
              className="btn btn-danger"
              onClick={() => {
                setNotePopover(null);
                onQuitarMarca(notePopover.index);
              }}
            >
              <Icon name="trash" size={13} />
              Quitar la marca
            </button>
            <button className="btn" onClick={cerrarPopover}>
              Cerrar
            </button>
          </div>
        </div>
      )}
      {notePopover && notePopover.kind !== "Square" && (
        <div
          className="card"
          style={{
            left: clampCardLeft(notePopover.x * scale, displayWidth),
            top: (notePopover.y + notePopover.h) * scale + 6,
          }}
          onMouseDown={(e) => e.stopPropagation()}
        >
          {editando ? (
            <textarea
              autoFocus
              aria-label="Texto del comentario"
              placeholder={`Escribe el comentario · Enter salta de línea, ${MOD}Enter guarda`}
              value={editando.text}
              onFocus={(e) => {
                // cursor al final, como al abrir un post-it en Acrobat
                const n = e.currentTarget.value.length;
                e.currentTarget.setSelectionRange(n, n);
              }}
              onChange={(e) =>
                setNoteEdit({ index: editando.index, text: e.target.value })
              }
              onKeyDown={(e) => {
                if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
                  e.preventDefault();
                  guardarContenido(notePopover, editando.text);
                } else if (e.key === "Escape") {
                  // cancela y deja el texto que hubiera antes
                  e.stopPropagation();
                  setNoteEdit(null);
                }
              }}
            />
          ) : (
            <p
              title="Doble clic para editar"
              onDoubleClick={() =>
                setNoteEdit({
                  index: notePopover.index,
                  text: notePopover.contents,
                })
              }
            >
              {notePopover.contents ||
                KIND_LABELS[notePopover.kind] ||
                notePopover.kind}
            </p>
          )}
          {firmaAnotacion(notePopover.author, notePopover.modified) && (
            <p className="annot-firma dato">
              {firmaAnotacion(notePopover.author, notePopover.modified)}
            </p>
          )}
          <div className="swatches" role="group" aria-label="Color del comentario">
            {ANNOT_COLORS.map((c) => {
              const actual = hexDeAnotacion(notePopover) === c;
              return (
                <button
                  key={c}
                  className={`swatch${actual ? " on" : ""}`}
                  style={{ background: c }}
                  title={NOMBRE_COLOR[c] ?? c}
                  aria-label={NOMBRE_COLOR[c] ?? c}
                  aria-pressed={actual}
                  onClick={() => cambiarColor(notePopover, c)}
                />
              );
            })}
          </div>
          <div className="card-actions">
            <button
              className="btn btn-danger"
              onClick={() => deleteAnnotation(notePopover)}
            >
              <Icon name="trash" size={13} />
              Eliminar
            </button>
            {editando ? (
              <>
                <button className="btn" onClick={() => setNoteEdit(null)}>
                  Cancelar
                </button>
                <button
                  className="btn btn-primary"
                  onClick={() => guardarContenido(notePopover, editando.text)}
                >
                  Guardar
                </button>
              </>
            ) : (
              <>
                <button
                  className="btn"
                  onClick={() =>
                    setNoteEdit({
                      index: notePopover.index,
                      text: notePopover.contents,
                    })
                  }
                >
                  Editar
                </button>
                <button className="btn" onClick={cerrarPopover}>
                  Cerrar
                </button>
              </>
            )}
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
            aria-label="Texto de la nota"
            placeholder={`Escribe la nota · Enter salta de línea, ${MOD}Enter la guarda`}
            value={noteDraft.text}
            onChange={(e) =>
              setNoteDraft({ ...noteDraft, text: e.target.value })
            }
            onKeyDown={(e) => {
              if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
                e.preventDefault();
                submitNote();
              } else if (e.key === "Escape") {
                e.stopPropagation();
                setNoteDraft(null);
              }
            }}
          />
        </div>
      )}
      {mode === "freetext" && freeTextDraft && (
        <div
          className="freetext-draft"
          style={{
            left: freeTextDraft.x * scale,
            top: freeTextDraft.y * scale,
            width: Math.max(40, freeTextDraft.w * scale),
            // la caja crece con las líneas que hace el texto al ancho que
            // tiene: lo que se ve escribiendo es lo que se guarda
            height:
              Math.max(
                24 / scale,
                freeTextDraft.h,
                altoCuadro(
                  ajustaLineas(
                    freeTextDraft.text,
                    freeTextDraft.w,
                    tool.freeTextSize,
                  ).length,
                  tool.freeTextSize,
                ),
              ) * scale,
            borderStyle: tool.freeTextBorder ? "solid" : "dashed",
            borderColor: tool.freeTextColor,
          }}
          onMouseDown={(e) => e.stopPropagation()}
        >
          <textarea
            autoFocus
            aria-label="Texto del cuadro"
            placeholder={`Escribe aquí · ${MOD}Enter lo añade`}
            style={{
              color: tool.freeTextColor,
              fontSize: tool.freeTextSize * scale,
            }}
            value={freeTextDraft.text}
            onChange={(e) =>
              setFreeTextDraft({ ...freeTextDraft, text: e.target.value })
            }
            onKeyDown={(e) => {
              if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
                e.preventDefault();
                commitFreeText();
              } else if (e.key === "Escape") {
                e.stopPropagation();
                setFreeTextDraft(null);
              }
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
