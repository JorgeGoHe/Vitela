import { useState } from "react";
import { useModal } from "../hooks/useModal";
import type { OrdenComentarios } from "../api";
import {
  paginasImprimibles,
  parseRango,
  plural,
  type OpcionesImprimir,
} from "../tipos";

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
  // el rango vacío se dice AQUÍ, junto al campo: la banda roja global queda
  // detrás del velo del modal, que es donde nadie la lee
  const rangoVacio =
    o.ambito === "rango" && parseRango(o.rango, pageCount).length === 0;
  // y lo mismo con el filtro de pares/impares: si no deja ninguna página se
  // dice aquí, con las opciones delante y sin cerrar nada
  const hojas = paginasImprimibles(o, pageCount, paginaActual).length;
  const sinPaginas = !rangoVacio && hojas === 0;
  const confirmar = () => {
    if (!rangoVacio && !sinPaginas) onConfirm(o);
  };
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
        {rangoVacio && (
          <p className="modal-error" role="alert">
            Escribe qué páginas quieres imprimir, por ejemplo «1-3, 8» (el
            documento tiene {pageCount}).
          </p>
        )}
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
        {/* cuántas hojas van a salir, antes de pulsar: es el dato que hace
            falta para decidir el rango, y sale una hoja por página */}
        {!rangoVacio && !sinPaginas && (
          <span className="dato">
            {plural(hojas, "hoja", "hojas")} de {pageCount}
            {o.resumen && " · más el resumen de comentarios"}
          </span>
        )}
        {sinPaginas && (
          <p className="modal-error" role="alert">
            Ninguna de las páginas elegidas es{" "}
            {o.subconjunto === "pares" ? "par" : "impar"}: cambia el filtro o
            las páginas.
          </p>
        )}

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
        {/* la casilla de Acrobat: detrás del documento, una hoja con una
            fila por comentario, que es lo que se lleva a una reunión */}
        <label className="opt-check">
          <input
            type="checkbox"
            checked={o.resumen}
            onChange={(e) => cambia({ resumen: e.target.checked })}
          />
          Imprimir el resumen de comentarios
        </label>
        {o.resumen && (
          <select
            className="size-select"
            aria-label="Orden del resumen de comentarios"
            value={o.ordenResumen}
            onChange={(e) =>
              cambia({ ordenResumen: e.target.value as OrdenComentarios })
            }
          >
            <option value="pagina">Por página</option>
            <option value="autor">Por autor</option>
            <option value="fecha">Por fecha</option>
            <option value="tipo">Por tipo</option>
          </select>
        )}

        <div className="card-actions">
          <button className="btn" onClick={onClose}>
            Cancelar
          </button>
          <button
            className="btn btn-primary"
            disabled={rangoVacio || sinPaginas}
            onClick={confirmar}
          >
            Imprimir
          </button>
        </div>
      </div>
    </div>
  );
}
