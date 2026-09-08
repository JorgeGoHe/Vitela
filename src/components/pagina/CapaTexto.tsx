/** Modo edición: bloques de texto clicables y tarjetas de texto nuevo / edición. */
import { FONT_CHOICES, type Mode } from "../../tipos";
import { clampCardLeft } from "../../hooks/pagina/geometria";
import type { Texto } from "../../hooks/pagina/useTexto";
import Icon from "../Icon";

type Props = {
  mode: Mode;
  texto: Texto;
  scale: number;
  displayWidth: number;
};

export default function CapaTexto({ mode, texto, scale, displayWidth }: Props) {
  const {
    textBlocks,
    blockDraft,
    setBlockDraft,
    newTextDraft,
    setNewTextDraft,
    submitNewText,
    submitBlockDraft,
    deleteBlock,
  } = texto;
  return (
    <>
      {mode === "edit" &&
        textBlocks.map((b) => (
          <div
            key={`b${b.object_index}`}
            className="text-block"
            style={{
              left: b.x * scale,
              top: b.y * scale,
              width: b.w * scale,
              height: b.h * scale,
            }}
            onMouseDown={(e) => e.stopPropagation()}
            onClick={(e) => {
              e.stopPropagation();
              setBlockDraft({ block: b, text: b.text });
            }}
          />
        ))}
      {newTextDraft && (
        <div
          className="card"
          style={{
            left: clampCardLeft(newTextDraft.x * scale, displayWidth, 288),
            top: newTextDraft.y * scale,
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
            top: (blockDraft.block.y + blockDraft.block.h) * scale + 6,
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
