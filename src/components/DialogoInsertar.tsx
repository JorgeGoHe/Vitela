import { useState } from "react";
import { useModal } from "../hooks/useModal";
import { plural } from "../tipos";

/**
 * «Insertar PDF…»: dónde entra el documento que se acaba de elegir. Acrobat
 * pregunta **antes o después** de una página concreta, y hasta ahora Vitela
 * lo metía siempre detrás de la que se estaba leyendo, que es la mitad de
 * los casos: la portada de un informe va delante.
 */
export default function DialogoInsertar({
  nombre,
  paginasQueEntran,
  pageCount,
  paginaActual,
  onConfirm,
  onClose,
}: {
  /** Nombre del fichero elegido, para no preguntar «¿cuál era?». */
  nombre: string;
  /** Cuántas páginas trae, si se han podido leer. */
  paginasQueEntran: number | null;
  pageCount: number;
  paginaActual: number;
  /** Índice (desde 0) en el que entra la primera página que llega. */
  onConfirm: (index: number) => void;
  onClose: () => void;
}) {
  const [donde, setDonde] = useState<"antes" | "despues">("despues");
  const [pagina, setPagina] = useState(paginaActual + 1);

  const n = Math.min(Math.max(1, pagina), pageCount);
  const index = donde === "antes" ? n - 1 : n;
  const confirmar = () => onConfirm(index);
  const { ref, onKeyDown } = useModal({ onClose, onConfirm: confirmar });

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div
        className="modal"
        role="dialog"
        aria-modal="true"
        aria-label="Insertar PDF"
        onClick={(e) => e.stopPropagation()}
        onKeyDown={onKeyDown}
        ref={ref}
        tabIndex={-1}
      >
        <h3>Insertar PDF</h3>
        <p className="modal-file">{nombre}</p>
        <div className="fila-campos">
          <select
            className="size-select"
            style={{ width: 120 }}
            aria-label="Antes o después"
            value={donde}
            onChange={(e) => setDonde(e.target.value as "antes" | "despues")}
          >
            <option value="antes">Antes</option>
            <option value="despues">Después</option>
          </select>
          <label className="opt-check">
            de la página
            <input
              type="number"
              className="stamp-input"
              style={{ width: 72 }}
              min={1}
              max={pageCount}
              aria-label="Número de página"
              value={pagina}
              onChange={(e) => setPagina(Number(e.target.value) || 1)}
            />
          </label>
        </div>
        <p className="modal-file" style={{ whiteSpace: "normal" }}>
          {paginasQueEntran === null
            ? `El documento entero entra ${donde === "antes" ? "antes" : "después"} de la página ${n} de ${pageCount}.`
            : `${plural(paginasQueEntran, "página entra", "páginas entran")} ${
                donde === "antes" ? "antes" : "después"
              } de la página ${n} de ${pageCount}.`}
        </p>
        <div className="card-actions">
          <button className="btn" onClick={onClose}>
            Cancelar
          </button>
          <button className="btn btn-primary" onClick={confirmar}>
            Insertar
          </button>
        </div>
      </div>
    </div>
  );
}
