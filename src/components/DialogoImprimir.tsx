import { useState } from "react";
import { useModal } from "../hooks/useModal";
import type { OpcionesImprimir } from "../tipos";

/**
 * Diálogo de impresión propio, el que Acrobat abre antes del diálogo del
 * sistema: rango, escala y qué se pinta encima del documento.
 */
export default function DialogoImprimir({
  inicial,
  pageCount,
  paginaActual,
  onConfirm,
  onClose,
}: {
  inicial: OpcionesImprimir;
  pageCount: number;
  /** Página que enseña la píldora, para «Página actual». */
  paginaActual: number;
  onConfirm: (opts: OpcionesImprimir) => void;
  onClose: () => void;
}) {
  const [o, setO] = useState<OpcionesImprimir>(inicial);
  const confirmar = () => onConfirm(o);
  const { ref, onKeyDown } = useModal({ onClose, onConfirm: confirmar });
  const cambia = (parte: Partial<OpcionesImprimir>) =>
    setO((v) => ({ ...v, ...parte }));

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div
        className="modal"
        role="dialog"
        aria-modal="true"
        aria-label="Imprimir"
        onClick={(e) => e.stopPropagation()}
        onKeyDown={onKeyDown}
        ref={ref}
        tabIndex={-1}
      >
        <h3>Imprimir</h3>

        <span className="card-label">Páginas</span>
        <label className="opt-check">
          <input
            type="radio"
            name="ambito"
            checked={o.ambito === "todas"}
            onChange={() => cambia({ ambito: "todas" })}
          />
          Todas <span className="dato">({pageCount})</span>
        </label>
        <label className="opt-check">
          <input
            type="radio"
            name="ambito"
            checked={o.ambito === "actual"}
            onChange={() => cambia({ ambito: "actual" })}
          />
          Página actual <span className="dato">({paginaActual + 1})</span>
        </label>
        <label className="opt-check">
          <input
            type="radio"
            name="ambito"
            checked={o.ambito === "rango"}
            onChange={() => cambia({ ambito: "rango" })}
          />
          Páginas
        </label>
        <input
          type="text"
          placeholder="1-3, 8"
          aria-label={`Páginas a imprimir, de 1 a ${pageCount}`}
          value={o.rango}
          onFocus={() => cambia({ ambito: "rango" })}
          onChange={(e) => cambia({ rango: e.target.value, ambito: "rango" })}
        />
        <select
          className="size-select"
          aria-label="Imprimir solo las pares o las impares"
          value={o.subconjunto}
          onChange={(e) =>
            cambia({
              subconjunto: e.target.value as OpcionesImprimir["subconjunto"],
            })
          }
        >
          <option value="todas">Todas las del rango</option>
          <option value="pares">Solo las pares</option>
          <option value="impares">Solo las impares</option>
        </select>

        <span className="card-label">Tamaño</span>
        <select
          className="size-select"
          aria-label="Tamaño de la página impresa"
          value={o.escala}
          onChange={(e) =>
            cambia({ escala: e.target.value as OpcionesImprimir["escala"] })
          }
        >
          <option value="ajustar">Ajustar al papel</option>
          <option value="real">Tamaño real</option>
          <option value="personalizada">Escala personalizada</option>
        </select>
        {o.escala === "personalizada" && (
          <label className="prop-field">
            <span className="card-label">Porcentaje</span>
            <input
              type="number"
              min={10}
              max={400}
              aria-label="Porcentaje de escala (10 a 400)"
              value={o.porcentaje}
              onChange={(e) =>
                cambia({
                  porcentaje: Math.min(
                    400,
                    Math.max(10, Number(e.target.value) || 100),
                  ),
                })
              }
            />
          </label>
        )}

        <span className="card-label">Comentarios y formularios</span>
        <select
          className="size-select"
          aria-label="Qué se imprime encima del documento"
          value={o.conMarcas ? "marcas" : "solo"}
          onChange={(e) => cambia({ conMarcas: e.target.value === "marcas" })}
        >
          <option value="marcas">Documento y marcas</option>
          <option value="solo">Solo el documento</option>
        </select>

        <div className="card-actions">
          <button className="btn" onClick={onClose}>
            Cancelar
          </button>
          <button className="btn btn-primary" onClick={confirmar}>
            Imprimir
          </button>
        </div>
      </div>
    </div>
  );
}
