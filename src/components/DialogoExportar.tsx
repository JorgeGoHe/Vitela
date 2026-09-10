import { useState } from "react";
import { indicesDeRango } from "../tipos";
import { useModal } from "../hooks/useModal";
import RangoPaginas from "./RangoPaginas";

/** Modal de exportación de las páginas como imágenes (formato y resolución). */
export default function DialogoExportar({
  fmt,
  setFmt,
  dpi,
  setDpi,
  pageCount,
  onConfirm,
  onClose,
}: {
  fmt: "png" | "jpeg";
  setFmt: (f: "png" | "jpeg") => void;
  dpi: number;
  setDpi: (d: number) => void;
  pageCount: number;
  /** Las páginas elegidas, o `null` si son todas. */
  onConfirm: (pageIndices: number[] | null) => void;
  onClose: () => void;
}) {
  // el mismo bloque «todas / 1-3, 8» de imprimir, marca de agua y Word:
  // exportar el documento entero para quedarse con tres páginas era lo que
  // faltaba desde el ciclo 2
  const [todas, setTodas] = useState(true);
  const [rango, setRango] = useState("");
  const indices = indicesDeRango(todas, rango, pageCount);
  const listo = todas || (indices?.length ?? 0) > 0;
  const confirmar = () => {
    if (listo) onConfirm(indices);
  };
  const { ref, onKeyDown } = useModal({ onClose, onConfirm: confirmar });

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div
        className="modal"
        role="dialog"
        aria-modal="true"
        aria-label="Exportar como imágenes"
        onClick={(e) => e.stopPropagation()}
        onKeyDown={onKeyDown}
        ref={ref}
        tabIndex={-1}
      >
        <h3>Exportar como imágenes</h3>
        <div className="card-row">
          <select
            className="size-select"
            value={fmt}
            onChange={(e) => setFmt(e.target.value as "png" | "jpeg")}
          >
            <option value="png">PNG</option>
            <option value="jpeg">JPEG</option>
          </select>
          <select
            className="size-select"
            value={dpi}
            onChange={(e) => setDpi(Number(e.target.value))}
          >
            {[96, 150, 200, 300].map((d) => (
              <option key={d} value={d}>
                {d} ppp
              </option>
            ))}
          </select>
        </div>
        <RangoPaginas
          pageCount={pageCount}
          todas={todas}
          setTodas={setTodas}
          rango={rango}
          setRango={setRango}
        />
        <p className="modal-file">Una imagen por página.</p>
        <div className="card-actions">
          <button className="btn" onClick={onClose}>
            Cancelar
          </button>
          <button
            className="btn btn-primary"
            disabled={!listo}
            title={listo ? undefined : "Ese rango no deja ninguna página"}
            onClick={confirmar}
          >
            Elegir carpeta…
          </button>
        </div>
      </div>
    </div>
  );
}
