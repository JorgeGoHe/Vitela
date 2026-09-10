import { useState } from "react";
import type { OrdenComentarios } from "../api";
import { useModal } from "../hooks/useModal";
import { plural } from "../tipos";

/** Los tres formatos con los que salen los comentarios de aquí. */
export type FormatoComentarios = "txt" | "pdf" | "xfdf";

const FORMATOS: [FormatoComentarios, string, string][] = [
  ["txt", "Texto (.txt)", "La lista en llano, para leerla o pegarla"],
  [
    "pdf",
    "Resumen en PDF (.pdf)",
    "Una fila por comentario, imprimible; se abre al terminar",
  ],
  [
    "xfdf",
    "Comentarios (.xfdf)",
    "Para que otro revisor los importe sobre su copia del documento",
  ],
];

/** Por qué se ordena el resumen; «por página» es el defecto de Acrobat. */
const ORDENES: [OrdenComentarios, string][] = [
  ["pagina", "Por página"],
  ["autor", "Por autor"],
  ["fecha", "Por fecha"],
  ["tipo", "Por tipo"],
];

/**
 * «Exportar comentarios…»: el formato se elige **antes** del diálogo de
 * guardar, porque de él dependen la extensión y las opciones. El orden solo
 * aparece con el resumen en PDF, que es el único que lo usa.
 */
export default function DialogoComentarios({
  cuantos,
  onConfirm,
  onClose,
}: {
  cuantos: number;
  onConfirm: (opts: {
    formato: FormatoComentarios;
    orden: OrdenComentarios;
  }) => void;
  onClose: () => void;
}) {
  const [formato, setFormato] = useState<FormatoComentarios>("txt");
  const [orden, setOrden] = useState<OrdenComentarios>("pagina");
  const confirmar = () => onConfirm({ formato, orden });
  const { ref, onKeyDown } = useModal({ onClose, onConfirm: confirmar });

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div
        className="modal"
        role="dialog"
        aria-modal="true"
        aria-label="Exportar comentarios"
        onClick={(e) => e.stopPropagation()}
        onKeyDown={onKeyDown}
        ref={ref}
        tabIndex={-1}
      >
        <h3>Exportar comentarios</h3>
        <p className="modal-file" style={{ whiteSpace: "normal" }}>
          {plural(cuantos, "comentario", "comentarios")} en este documento.
        </p>
        <span className="card-label">Formato</span>
        {FORMATOS.map(([v, etiqueta, ayuda]) => (
          <label key={v} className="opt-check">
            <input
              type="radio"
              name="formato-comentarios"
              checked={formato === v}
              onChange={() => setFormato(v)}
            />
            {etiqueta}
            <span className="opt-hint"> · {ayuda}</span>
          </label>
        ))}
        {formato === "pdf" && (
          <>
            <span className="card-label">Orden</span>
            <select
              className="size-select"
              aria-label="Orden del resumen"
              value={orden}
              onChange={(e) => setOrden(e.target.value as OrdenComentarios)}
            >
              {ORDENES.map(([v, etiqueta]) => (
                <option key={v} value={v}>
                  {etiqueta}
                </option>
              ))}
            </select>
          </>
        )}
        <div className="card-actions">
          <button className="btn" onClick={onClose}>
            Cancelar
          </button>
          <button className="btn btn-primary" onClick={confirmar}>
            Elegir dónde y exportar…
          </button>
        </div>
      </div>
    </div>
  );
}
