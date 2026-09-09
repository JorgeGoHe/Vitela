/** Modo edición: bloques de texto que se colocan y se estiran, y tarjetas
 *  de texto nuevo / edición. */
import { FONT_CHOICES, type Mode } from "../../tipos";
import { cardTop, clampCardLeft } from "../../hooks/pagina/geometria";
import type { Texto } from "../../hooks/pagina/useTexto";
import Icon from "../Icon";

/** Los ocho tiradores, los mismos que ya tienen los sellos y las imágenes. */
const TIRADORES = ["nw", "n", "ne", "e", "se", "s", "sw", "w"] as const;

type Props = {
  mode: Mode;
  texto: Texto;
  scale: number;
  displayWidth: number;
  displayHeight: number;
};

export default function CapaTexto({
  mode,
  texto,
  scale,
  displayWidth,
  displayHeight,
}: Props) {
  const {
    textBlocks,
    blockDraft,
    setBlockDraft,
    newTextDraft,
    setNewTextDraft,
    txtDraft,
    startTxtAction,
    submitNewText,
    submitBlockDraft,
    deleteBlock,
  } = texto;
  return (
    <>
      {mode === "edit" &&
        textBlocks.map((t) => {
          const arrastrando =
            txtDraft !== null && txtDraft.object_index === t.object_index;
          const b = arrastrando ? txtDraft : t;
          const senalado =
            arrastrando ||
            blockDraft?.block.object_index === t.object_index;
          return (
            <div
              key={`b${t.object_index}`}
              className={`text-block${senalado ? " on" : ""}`}
              style={{
                left: b.x * scale,
                top: b.y * scale,
                width: b.w * scale,
                height: b.h * scale,
              }}
              onMouseDown={(e) => startTxtAction(e, t, "move")}
            >
              {senalado &&
                TIRADORES.map((hd) => (
                  <div
                    key={hd}
                    className={`image-handle h-${hd}`}
                    title="Estirar el bloque (Shift: libre en esquinas)"
                    onMouseDown={(e) => startTxtAction(e, t, "resize", hd)}
                  />
                ))}
            </div>
          );
        })}
      {newTextDraft && (
        <div
          className="card"
          style={{
            left: clampCardLeft(newTextDraft.x * scale, displayWidth, 288),
            top: cardTop(
              newTextDraft.y * scale,
              newTextDraft.y * scale,
              displayHeight,
              210,
            ),
            width: 280,
          }}
          onMouseDown={(e) => e.stopPropagation()}
        >
          <textarea
            autoFocus
            placeholder="Texto nuevo… (Enter añade, Esc cancela)"
            value={newTextDraft.text}
            onChange={(e) =>
              setNewTextDraft({
                ...newTextDraft,
                text: e.target.value,
              })
            }
            onKeyDown={(e) => {
              if (e.key === "Enter" && !e.shiftKey) {
                e.preventDefault();
                submitNewText();
              }
              if (e.key === "Escape") setNewTextDraft(null);
            }}
          />
          <div className="card-row">
            <select
              className="size-select font-select"
              title="Fuente"
              value={newTextDraft.font}
              onChange={(e) =>
                setNewTextDraft({
                  ...newTextDraft,
                  font: e.target.value,
                })
              }
            >
              {FONT_CHOICES.map((f) => (
                <option key={f.value} value={f.value}>
                  {f.label}
                </option>
              ))}
            </select>
            <select
              className="size-select"
              title="Tamaño"
              value={newTextDraft.size}
              onChange={(e) =>
                setNewTextDraft({
                  ...newTextDraft,
                  size: Number(e.target.value),
                })
              }
            >
              {[8, 10, 12, 14, 18, 24, 32].map((s) => (
                <option key={s} value={s}>
                  {s} pt
                </option>
              ))}
            </select>
          </div>
          <div className="card-actions">
            <button className="btn" onClick={() => setNewTextDraft(null)}>
              Cancelar
            </button>
            <button className="btn btn-primary" onClick={submitNewText}>
              Añadir
            </button>
          </div>
        </div>
      )}
      {blockDraft && (
        <div
          className="card"
          style={{
            left: clampCardLeft(blockDraft.block.x * scale, displayWidth),
            // en la última línea de la página la tarjeta se vuelca hacia
            // arriba: si no, había que hacer scroll para llegar a «Guardar»
            top: cardTop(
              blockDraft.block.y * scale,
              (blockDraft.block.y + blockDraft.block.h) * scale,
              displayHeight,
              190,
            ),
            width: Math.max(260, blockDraft.block.w * scale),
          }}
          onMouseDown={(e) => e.stopPropagation()}
        >
          <span className="card-label">
            {blockDraft.block.font_family || "Fuente del documento"}
            {" · "}
            {Math.round(blockDraft.block.font_size)} pt
          </span>
          <textarea
            autoFocus
            value={blockDraft.text}
            onChange={(e) =>
              setBlockDraft({ ...blockDraft, text: e.target.value })
            }
            onKeyDown={(e) => {
              if (e.key === "Enter" && !e.shiftKey) {
                e.preventDefault();
                submitBlockDraft();
              }
              if (e.key === "Escape") setBlockDraft(null);
            }}
          />
          <div className="card-actions">
            <button className="btn btn-danger" onClick={deleteBlock}>
              <Icon name="trash" size={13} />
              Eliminar
            </button>
            <button className="btn" onClick={() => setBlockDraft(null)}>
              Cancelar
            </button>
            <button className="btn btn-primary" onClick={submitBlockDraft}>
              Guardar
            </button>
          </div>
        </div>
      )}
    </>
  );
}
