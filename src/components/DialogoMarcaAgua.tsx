import { useState } from "react";
import { cargaColores, guardaColor } from "../tipos";
import Icon from "./Icon";
import { useModal } from "../hooks/useModal";

const COLORS = ["#c0392b", "#6f6a5c", "#2743c0", "#2ea043"];

export type PosicionMarca = "nw" | "n" | "ne" | "w" | "c" | "e" | "sw" | "s" | "se";

const POSICIONES: PosicionMarca[] = ["nw", "n", "ne", "w", "c", "e", "sw", "s", "se"];

/** Diálogo de marca de agua: texto diagonal u horizontal en todas las páginas. */
export default function DialogoMarcaAgua({
  onApply,
  onClose,
}: {
  onApply: (opts: {
    text: string;
    fontSize: number;
    color: string;
    opacity: number;
    diagonal: boolean;
    position: PosicionMarca;
  }) => void;
  onClose: () => void;
}) {
  const [text, setText] = useState("BORRADOR");
  const [fontSize, setFontSize] = useState(64);
  const [color, setColor] = useState(() => cargaColores().marcaAgua ?? COLORS[1]);
  const [opacity, setOpacity] = useState(35);
  const [diagonal, setDiagonal] = useState(true);
  const [position, setPosition] = useState<PosicionMarca>("c");

  function aplicar() {
    if (!text.trim()) return;
    onApply({ text, fontSize, color, opacity, diagonal, position });
  }

  const { ref, onKeyDown } = useModal({ onClose, onConfirm: aplicar });

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div
        className="modal"
        role="dialog"
        aria-modal="true"
        aria-label="Marca de agua"
        onClick={(e) => e.stopPropagation()}
        onKeyDown={onKeyDown}
        ref={ref}
        tabIndex={-1}
      >
        <h3>Marca de agua</h3>
        <input
          type="text"
          placeholder="Texto de la marca de agua"
          value={text}
          onChange={(e) => setText(e.target.value)}
        />
        <div className="card-row">
          <select
            className="size-select"
            title="Tamaño"
            value={fontSize}
            onChange={(e) => setFontSize(Number(e.target.value))}
          >
            {[36, 48, 64, 80, 100].map((s) => (
              <option key={s} value={s}>
                {s} pt
              </option>
            ))}
          </select>
          <select
            className="size-select"
            title="Opacidad"
            value={opacity}
            onChange={(e) => setOpacity(Number(e.target.value))}
          >
            {[15, 25, 35, 50, 75].map((o) => (
              <option key={o} value={o}>
                {o} %
              </option>
            ))}
          </select>
          <div className="swatches">
            {COLORS.map((c) => (
              <button
                key={c}
                className={`swatch${color === c ? " on" : ""}`}
                style={{ background: c }}
                onClick={() => {
                  setColor(c);
                  guardaColor("marcaAgua", c);
                }}
              />
            ))}
            <label
              className={`swatch swatch-custom${
                !COLORS.includes(color) ? " on" : ""
              }`}
              title="Color personalizado"
            >
              <input
                type="color"
                value={color}
                onChange={(e) => {
                  setColor(e.target.value);
                  guardaColor("marcaAgua", e.target.value);
                }}
              />
              <Icon name="plus" size={10} />
            </label>
          </div>
        </div>
        <div className="card-row">
          <div className="pos-grid" title="Posición en la página">
            {POSICIONES.map((p) => (
              <button
                key={p}
                className={`pos-cell${position === p ? " on" : ""}`}
                aria-label={`Posición ${p}`}
                onClick={() => setPosition(p)}
              />
            ))}
          </div>
          <label className="opt-check">
            <input
              type="checkbox"
              checked={diagonal}
              onChange={(e) => setDiagonal(e.target.checked)}
            />
            En diagonal
          </label>
        </div>
        <p className="modal-file">
          Se añade a todas las páginas como contenido del documento.
        </p>
        <div className="card-actions">
          <button className="btn" onClick={onClose}>
            Cancelar
          </button>
          <button
            className="btn btn-primary"
            disabled={!text.trim()}
            onClick={aplicar}
          >
            Aplicar
          </button>
        </div>
      </div>
    </div>
  );
}
