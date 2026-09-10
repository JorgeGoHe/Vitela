import { useState } from "react";
import { useModal } from "../hooks/useModal";
import { indicesDeRango } from "../tipos";
import RangoPaginas from "./RangoPaginas";

/**
 * «Word (.docx)…» y «Página web (.html)…»: el aviso de lo que no sale va
 * ANTES de elegir destino, porque las dos prometen mucho y dan menos, y
 * descubrirlo con el fichero ya escrito es tarde. Es **el mismo diálogo**
 * porque es el mismo trato: el texto y las imágenes salen en su sitio; las
 * columnas y las tablas, no.
 *
 * Y con el rango de páginas de Acrobat: exportar un contrato de 200 páginas
 * para quedarse con dos era el camino largo a un fichero que no se quería.
 */
export default function DialogoExportarDoc({
  formato,
  pageCount,
  onConfirm,
  onClose,
}: {
  /** A qué se exporta: cambia los textos, no el trato. */
  formato: "docx" | "html";
  pageCount: number;
  /** Índices de página, o null para el documento entero. */
  onConfirm: (paginas: number[] | null) => void;
  onClose: () => void;
}) {
  const titulo =
    formato === "html" ? "Exportar a página web" : "Exportar a Word";
  const [todas, setTodas] = useState(true);
  const [rango, setRango] = useState("");
  const indices = indicesDeRango(todas, rango, pageCount);
  const puede = todas || (indices?.length ?? 0) > 0;
  const confirmar = () => {
    if (puede) onConfirm(indices);
  };
  const { ref, onKeyDown } = useModal({ onClose, onConfirm: confirmar });

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div
        className="modal"
        role="dialog"
        aria-modal="true"
        aria-label={titulo}
        onClick={(e) => e.stopPropagation()}
        onKeyDown={onKeyDown}
        ref={ref}
        tabIndex={-1}
      >
        <h3>{titulo}</h3>
        <p className="modal-file" style={{ whiteSpace: "normal" }}>
          El texto y las imágenes salen en su sitio; las columnas y las
          tablas, no. Para un documento sencillo suele bastar.
          {formato === "html" &&
            " Las imágenes van a una carpeta al lado, y el .html no lleva JavaScript ni depende de nada."}
        </p>
        <RangoPaginas
          pageCount={pageCount}
          todas={todas}
          setTodas={setTodas}
          rango={rango}
          setRango={setRango}
        />
        <div className="card-actions">
          <button className="btn" onClick={onClose}>
            Cancelar
          </button>
          <button
            className="btn btn-primary"
            disabled={!puede}
            title={puede ? undefined : "Ese rango no deja ninguna página"}
            onClick={confirmar}
          >
            Elegir dónde y exportar…
          </button>
        </div>
      </div>
    </div>
  );
}
