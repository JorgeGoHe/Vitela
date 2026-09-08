/** Tarjeta flotante bajo la selección de texto: marcar (con color) o copiar. */
import { ANNOT_COLORS, type Rect, type Selection } from "../../tipos";
import { clampCardLeft } from "../../hooks/pagina/geometria";
import Icon from "../Icon";
import type { ToolProps } from "../Pagina";

type Props = {
  rect: Rect;
  scale: number;
  displayWidth: number;
  tool: ToolProps;
  markupSelection: (kind: "highlight" | "underline" | "strikeout") => void;
  copySelection: () => void;
  setSelection: (s: Selection | null) => void;
};

export default function TarjetaSeleccion({
  rect,
  scale,
  displayWidth,
  tool,
  markupSelection,
  copySelection,
  setSelection,
}: Props) {
  return (
    <div
      className="card sel-popover"
      style={{
        left: clampCardLeft(rect.x * scale, displayWidth),
        top: (rect.y + rect.h) * scale + 6,
      }}
      onMouseDown={(e) => e.stopPropagation()}
    >
      <div className="swatches" style={{ marginRight: 4 }}>
        {ANNOT_COLORS.map((c) => (
          <button
            key={c}
            className={`swatch${tool.markupPending === c ? " on" : ""}`}
            style={{ background: c }}
            title="Usar este color en la próxima marca"
            onClick={() =>
              tool.onMarkupPending(tool.markupPending === c ? null : c)
            }
          />
        ))}
        <label
          className={`swatch swatch-custom${
            tool.markupPending &&
            !ANNOT_COLORS.includes(tool.markupPending)
              ? " on"
              : ""
          }`}
          title="Color personalizado"
        >
          <input
            type="color"
            value={tool.markupPending ?? "#888888"}
            onChange={(e) => tool.onMarkupPending(e.target.value)}
          />
        </label>
      </div>
      <button
        className="btn"
        onClick={() => markupSelection("highlight")}
      >
        <span
          className="punto-color"
          style={{
            background: tool.markupPending ?? tool.markupColors.resaltar,
          }}
        />
        Resaltar
      </button>
      <button
        className="btn"
        onClick={() => markupSelection("underline")}
      >
        <span
          className="punto-color"
          style={{
            background: tool.markupPending ?? tool.markupColors.subrayar,
          }}
        />
        Subrayar
      </button>
      <button
        className="btn"
        onClick={() => markupSelection("strikeout")}
      >
        <span
          className="punto-color"
          style={{
            background: tool.markupPending ?? tool.markupColors.tachar,
          }}
        />
        Tachar
      </button>
      <button
        className="btn"
        onClick={() => {
          copySelection();
          setSelection(null);
        }}
      >
        <Icon name="copy" size={13} />
        Copiar
      </button>
    </div>
  );
}
