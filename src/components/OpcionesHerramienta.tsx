import { ANNOT_COLORS, NOMBRE_COLOR, type Mode, type ShapeKind } from "../tipos";
import { STAMP_PRESETS } from "../hooks/useHerramienta";
import Icon from "./Icon";

const SHAPE_COLORS = ANNOT_COLORS;

/** Fila contextual de opciones de la herramienta activa (trazo, formas y
 *  sello); no pinta nada en el resto de modos. */
export default function OpcionesHerramienta({
  mode,
  drawColor,
  drawWidth,
  setDrawWidth,
  shapeKind,
  setShapeKind,
  shapeColor,
  shapeFill,
  setShapeFill,
  shapeWidth,
  setShapeWidth,
  stampText,
  setStampText,
  stampCustom,
  setStampCustom,
  stampColor,
  freeTextColor,
  freeTextSize,
  setFreeTextSize,
  freeTextBorder,
  setFreeTextBorder,
  cambiaColorAccion,
}: {
  mode: Mode;
  drawColor: string;
  drawWidth: number;
  setDrawWidth: (w: number) => void;
  shapeKind: ShapeKind;
  setShapeKind: (k: ShapeKind) => void;
  shapeColor: string;
  shapeFill: boolean;
  setShapeFill: (f: boolean) => void;
  shapeWidth: number;
  setShapeWidth: (w: number) => void;
  stampText: string;
  setStampText: (t: string) => void;
  stampCustom: string;
  setStampCustom: (t: string) => void;
  stampColor: string;
  freeTextColor: string;
  freeTextSize: number;
  setFreeTextSize: (s: number) => void;
  freeTextBorder: boolean;
  setFreeTextBorder: (b: boolean) => void;
  cambiaColorAccion: (
    accion: "dibujo" | "forma" | "sello" | "cuadro",
    color: string,
  ) => void;
}) {
  return (
    <>
      {mode === "draw" && (
        <div className="tool-options">
          <span>Trazo</span>
          <div className="swatches">
            {SHAPE_COLORS.map((c) => (
              <button
                key={c}
                className={`swatch${drawColor === c ? " on" : ""}`}
                style={{ background: c }}
                title={NOMBRE_COLOR[c] ?? c}
                aria-label={NOMBRE_COLOR[c] ?? c}
                aria-pressed={drawColor === c}
                onClick={() => cambiaColorAccion("dibujo", c)}
              />
            ))}
            <label
              className={`swatch swatch-custom${
                !SHAPE_COLORS.includes(drawColor) ? " on" : ""
              }`}
              title="Color personalizado"
            >
              <input
                type="color"
                value={drawColor}
                onChange={(e) => cambiaColorAccion("dibujo", e.target.value)}
              />
              <Icon name="plus" size={10} />
            </label>
          </div>
          <select
            className="size-select"
            title="Grosor"
            value={drawWidth}
            onChange={(e) => setDrawWidth(Number(e.target.value))}
          >
            {[1, 2, 3, 5, 8].map((w) => (
              <option key={w} value={w}>
                {w} pt
              </option>
            ))}
          </select>
        </div>
      )}
      {mode === "shape" && (
        <div className="tool-options">
          <div className="segmented">
            {(
              [
                ["rect", "Rectángulo"],
                ["ellipse", "Elipse"],
                ["line", "Línea"],
                ["arrow", "Flecha"],
              ] as [ShapeKind, string][]
            ).map(([k, label]) => (
              <button
                key={k}
                className={`btn${shapeKind === k ? " on" : ""}`}
                onClick={() => setShapeKind(k)}
              >
                {label}
              </button>
            ))}
          </div>
          <div className="swatches">
            {SHAPE_COLORS.map((c) => (
              <button
                key={c}
                className={`swatch${shapeColor === c ? " on" : ""}`}
                style={{ background: c }}
                title={NOMBRE_COLOR[c] ?? c}
                aria-label={NOMBRE_COLOR[c] ?? c}
                aria-pressed={shapeColor === c}
                onClick={() => cambiaColorAccion("forma", c)}
              />
            ))}
            <label
              className={`swatch swatch-custom${
                !SHAPE_COLORS.includes(shapeColor) ? " on" : ""
              }`}
              title="Color personalizado"
            >
              <input
                type="color"
                value={shapeColor}
                onChange={(e) => cambiaColorAccion("forma", e.target.value)}
              />
              <Icon name="plus" size={10} />
            </label>
          </div>
          <label
            className={`opt-check${
              shapeKind === "line" || shapeKind === "arrow" ? " disabled" : ""
            }`}
          >
            <input
              type="checkbox"
              checked={shapeFill}
              disabled={shapeKind === "line" || shapeKind === "arrow"}
              onChange={(e) => setShapeFill(e.target.checked)}
            />
            Relleno
          </label>
          <select
            className="size-select"
            title="Grosor"
            value={shapeWidth}
            onChange={(e) => setShapeWidth(Number(e.target.value))}
          >
            {[1, 2, 3, 5].map((s) => (
              <option key={s} value={s}>
                {s} pt
              </option>
            ))}
          </select>
        </div>
      )}
      {mode === "freetext" && (
        <div className="tool-options">
          <span>Cuadro</span>
          <div className="swatches">
            {SHAPE_COLORS.map((c) => (
              <button
                key={c}
                className={`swatch${freeTextColor === c ? " on" : ""}`}
                style={{ background: c }}
                title={NOMBRE_COLOR[c] ?? c}
                aria-label={NOMBRE_COLOR[c] ?? c}
                aria-pressed={freeTextColor === c}
                onClick={() => cambiaColorAccion("cuadro", c)}
              />
            ))}
            <label
              className={`swatch swatch-custom${
                !SHAPE_COLORS.includes(freeTextColor) ? " on" : ""
              }`}
              title="Color personalizado"
            >
              <input
                type="color"
                value={freeTextColor}
                onChange={(e) => cambiaColorAccion("cuadro", e.target.value)}
              />
              <Icon name="plus" size={10} />
            </label>
          </div>
          <select
            className="size-select"
            title="Tamaño de letra"
            aria-label="Tamaño de letra"
            value={freeTextSize}
            onChange={(e) => setFreeTextSize(Number(e.target.value))}
          >
            {[8, 10, 12, 14, 18, 24, 32].map((t) => (
              <option key={t} value={t}>
                {t} pt
              </option>
            ))}
          </select>
          <label className="opt-check">
            <input
              type="checkbox"
              checked={freeTextBorder}
              onChange={(e) => setFreeTextBorder(e.target.checked)}
            />
            Con borde
          </label>
          <span className="opt-hint">
            Arrastra el rectángulo y escribe dentro
          </span>
        </div>
      )}
      {mode === "stamp" && (
        <div className="tool-options">
          <select
            className="size-select"
            value={stampText}
            onChange={(e) => setStampText(e.target.value)}
          >
            {STAMP_PRESETS.map((s) => (
              <option key={s} value={s}>
                {s}
              </option>
            ))}
            <option value="custom">Personalizado…</option>
          </select>
          {stampText === "custom" && (
            <input
              type="text"
              className="stamp-input"
              placeholder="Texto del sello"
              value={stampCustom}
              onChange={(e) => setStampCustom(e.target.value.toUpperCase())}
            />
          )}
          <div className="swatches">
            {["#c0392b", "#2743c0", "#2ea043", "#1d1c18"].map((c) => (
              <button
                key={c}
                className={`swatch${stampColor === c ? " on" : ""}`}
                style={{ background: c }}
                title={NOMBRE_COLOR[c] ?? c}
                aria-label={NOMBRE_COLOR[c] ?? c}
                aria-pressed={stampColor === c}
                onClick={() => cambiaColorAccion("sello", c)}
              />
            ))}
            <label
              className={`swatch swatch-custom${
                !["#c0392b", "#2743c0", "#2ea043", "#1d1c18"].includes(
                  stampColor,
                )
                  ? " on"
                  : ""
              }`}
              title="Color personalizado"
            >
              <input
                type="color"
                value={stampColor}
                onChange={(e) => cambiaColorAccion("sello", e.target.value)}
              />
              <Icon name="plus" size={10} />
            </label>
          </div>
          <span
            className="sello-preview"
            style={{ color: stampColor, borderColor: stampColor }}
          >
            {(stampText === "custom" ? stampCustom || "SELLO" : stampText)}
          </span>
          <span className="opt-hint">Clic en la página para colocarlo</span>
        </div>
      )}
    </>
  );
}
