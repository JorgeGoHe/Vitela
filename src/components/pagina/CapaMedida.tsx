/**
 * Capa propia del modo «Medir»: el trazo, su etiqueta en Fragment Mono y la
 * tarjeta que pregunta cuánto mide de verdad al fijar la escala. No pinta
 * nada en el resto de modos y no toca el documento.
 */
import { useState } from "react";
import type { Mode } from "../../tipos";
import { clampCardLeft } from "../../hooks/pagina/geometria";
import type { MedidaHook } from "../../hooks/pagina/useMedida";
import type { ToolProps } from "../Pagina";

/** Las tres unidades con las que se dice una medida conocida. */
const UNIDADES: [string, string, number][] = [
  ["mm", "milímetros", 1],
  ["cm", "centímetros", 10],
  ["m", "metros", 1000],
];

export default function CapaMedida({
  mode,
  medida,
  scale,
  displayWidth,
  tool,
}: {
  mode: Mode;
  medida: MedidaHook;
  scale: number;
  displayWidth: number;
  tool: ToolProps;
}) {
  const { medidaDraft, calibre, etiquetaDe, fijaEscala, setCalibre } = medida;
  const [cuanto, setCuanto] = useState("");
  const [unidad, setUnidad] = useState(1);

  if (mode !== "medir") return null;
  const d = calibre ?? medidaDraft;

  return (
    <>
      {d && (
        <>
          {tool.medidaTipo === "area" ? (
            <div
              className="medida-caja"
              style={{
                left: Math.min(d.x1, d.x2) * scale,
                top: Math.min(d.y1, d.y2) * scale,
                width: Math.abs(d.x2 - d.x1) * scale,
                height: Math.abs(d.y2 - d.y1) * scale,
              }}
            />
          ) : (
            <svg className="medida-linea">
              <line
                x1={d.x1 * scale}
                y1={d.y1 * scale}
                x2={d.x2 * scale}
                y2={d.y2 * scale}
              />
            </svg>
          )}
          <span
            className="medida-etiqueta dato"
            style={{
              left: ((d.x1 + d.x2) / 2) * scale,
              top: ((d.y1 + d.y2) / 2) * scale,
            }}
          >
            {calibre ? "¿cuánto mide?" : etiquetaDe(d)}
          </span>
        </>
      )}
      {calibre && (
        <div
          className="card"
          style={{
            left: clampCardLeft(Math.min(calibre.x1, calibre.x2) * scale, displayWidth),
            top: Math.max(calibre.y1, calibre.y2) * scale + 8,
          }}
          onMouseDown={(e) => e.stopPropagation()}
        >
          <span className="card-label">Cuánto mide ese trazo de verdad</span>
          <div className="card-row">
            <input
              type="number"
              className="stamp-input"
              autoFocus
              min={0}
              step="any"
              aria-label="Medida real del trazo"
              placeholder="100"
              value={cuanto}
              onChange={(e) => setCuanto(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") {
                  e.preventDefault();
                  fijaEscala(Number(cuanto) * unidad);
                } else if (e.key === "Escape") {
                  e.stopPropagation();
                  setCalibre(null);
                }
              }}
            />
            <select
              className="size-select"
              aria-label="Unidad"
              value={unidad}
              onChange={(e) => setUnidad(Number(e.target.value))}
            >
              {UNIDADES.map(([corta, larga, f]) => (
                <option key={corta} value={f}>
                  {larga}
                </option>
              ))}
            </select>
          </div>
          <div className="card-actions">
            <button className="btn" onClick={() => setCalibre(null)}>
              Cancelar
            </button>
            <button
              className="btn btn-primary"
              disabled={!(Number(cuanto) > 0)}
              onClick={() => fijaEscala(Number(cuanto) * unidad)}
            >
              Fijar la escala
            </button>
          </div>
        </div>
      )}
    </>
  );
}
