/**
 * Capa propia del modo «Medir»: el trazo o la figura por vértices, su cifra
 * en Fragment Mono junto al cursor y la tarjeta que pregunta cuánto mide de
 * verdad al fijar la escala. No pinta nada en el resto de modos y no toca el
 * documento.
 */
import { useState } from "react";
import type { Mode } from "../../tipos";
import { clampCardLeft, puntoEnCapa } from "../../hooks/pagina/geometria";
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
  viewRotation,
  displayWidth,
  tool,
}: {
  mode: Mode;
  medida: MedidaHook;
  scale: number;
  viewRotation: number;
  displayWidth: number;
  tool: ToolProps;
}) {
  const {
    medidaDraft,
    calibre,
    etiquetaDe,
    etiquetaPoligono,
    fijaEscala,
    setCalibre,
    enCurso,
    cerrada,
    vertices,
    setCursor,
  } = medida;
  const [cuanto, setCuanto] = useState("");
  const [unidad, setUnidad] = useState(1);

  if (mode !== "medir") return null;
  const d = calibre ?? medidaDraft;
  // el área se cierra sola para verse como lo que se está midiendo; el
  // perímetro se queda abierto hasta que el usuario lo cierra
  const puntos = enCurso.map((p) => `${p.x * scale},${p.y * scale}`).join(" ");
  const ultimo = enCurso[enCurso.length - 1];

  // el vértice que se está colocando sigue al ratón, y el ratón sin botón
  // pulsado no llega al despachador de la página: lo escucha esta capa, que
  // es de quien es el gesto
  const porVertices =
    (tool.medidaTipo === "perimetro" || tool.medidaTipo === "area") &&
    !tool.calibrando;

  return (
    <>
      {porVertices && vertices.length > 0 && !cerrada && (
        <div
          className="medida-captura"
          onMouseMove={(e) => setCursor(puntoEnCapa(e, scale, viewRotation))}
        />
      )}
      {enCurso.length > 0 && (
        <>
          <svg className="medida-linea">
            {tool.medidaTipo === "area" && enCurso.length > 2 ? (
              <polygon points={puntos} className="medida-relleno" />
            ) : null}
            <polyline
              points={
                tool.medidaTipo === "area" && cerrada && enCurso.length > 2
                  ? `${puntos} ${enCurso[0].x * scale},${enCurso[0].y * scale}`
                  : puntos
              }
            />
            {enCurso.map((p, i) => (
              <circle key={i} cx={p.x * scale} cy={p.y * scale} r={2.5} />
            ))}
          </svg>
          {ultimo && (
            <span
              className="medida-etiqueta dato"
              style={{ left: ultimo.x * scale + 10, top: ultimo.y * scale + 10 }}
            >
              {etiquetaPoligono(enCurso)}
            </span>
          )}
        </>
      )}
      {d && (
        <>
          <svg className="medida-linea">
            <line
              x1={d.x1 * scale}
              y1={d.y1 * scale}
              x2={d.x2 * scale}
              y2={d.y2 * scale}
            />
          </svg>
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
