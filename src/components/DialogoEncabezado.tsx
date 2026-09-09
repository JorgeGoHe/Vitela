import { useState } from "react";
import type { HeaderFooter } from "../api";
import { useModal } from "../hooks/useModal";
import { indicesDeRango, type PageSize } from "../tipos";
import RangoPaginas from "./RangoPaginas";

/** Ancho en píxeles de la vista previa. */
const PREVIA = 190;

/** Resuelve las plantillas para enseñar la previa con datos de verdad. */
function resuelve(plantilla: string, pagina: number, total: number): string {
  return plantilla
    .replace(/\{n\}/g, String(pagina))
    .replace(/\{total\}/g, String(total))
    .replace(/\{fecha\}/g, new Date().toLocaleDateString("es-ES"));
}

/**
 * Diálogo de encabezado y pie: seis zonas con plantillas {n}, {total} y
 * {fecha}, rango de páginas y vista previa en vivo sobre la página actual.
 * El botón «Solo numerar» rellena el pie centrado con {n} / {total}.
 */
export default function DialogoEncabezado({
  pageCount,
  paginaActual,
  previaSrc,
  previaSize,
  onApply,
  onClose,
}: {
  pageCount: number;
  paginaActual: number;
  previaSrc: string | null;
  previaSize: PageSize | undefined;
  onApply: (
    zonas: HeaderFooter,
    fontSize: number,
    pageIndices: number[] | null,
  ) => void;
  onClose: () => void;
}) {
  const [zonas, setZonas] = useState<HeaderFooter>({});
  const [fontSize, setFontSize] = useState(10);
  const [todas, setTodas] = useState(true);
  const [rango, setRango] = useState("");

  function campo(key: keyof HeaderFooter, placeholder: string) {
    return (
      <input
        type="text"
        placeholder={placeholder}
        value={zonas[key] ?? ""}
        onChange={(e) => setZonas({ ...zonas, [key]: e.target.value })}
      />
    );
  }

  const vacio = Object.values(zonas).every((v) => !v?.trim());
  const indices = indicesDeRango(todas, rango, pageCount);
  const listo = !vacio && (todas || (indices?.length ?? 0) > 0);

  function aplicar() {
    if (listo) onApply(zonas, fontSize, indices);
  }

  const { ref, onKeyDown } = useModal({ onClose, onConfirm: aplicar });

  // la previa mide en puntos de la página y se dibuja a escala
  const escala = previaSize ? PREVIA / previaSize.width : 0;
  const altoPrevia = previaSize ? previaSize.height * escala : 0;
  const banda = (
    zona: "header" | "footer",
    lado: "Left" | "Center" | "Right",
  ) => {
    const texto = zonas[`${zona}${lado}` as keyof HeaderFooter];
    if (!texto?.trim()) return null;
    return resuelve(texto, paginaActual + 1, pageCount);
  };

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div
        className="modal modal-hf"
        role="dialog"
        aria-modal="true"
        aria-label="Encabezado y pie de página"
        onClick={(e) => e.stopPropagation()}
        onKeyDown={onKeyDown}
        ref={ref}
        tabIndex={-1}
      >
        <h3>Encabezado y pie de página</h3>
        <span className="card-label">Encabezado</span>
        <div className="hf-grid">
          {campo("headerLeft", "Izquierda")}
          {campo("headerCenter", "Centro")}
          {campo("headerRight", "Derecha")}
        </div>
        <span className="card-label">Pie</span>
        <div className="hf-grid">
          {campo("footerLeft", "Izquierda")}
          {campo("footerCenter", "Centro")}
          {campo("footerRight", "Derecha")}
        </div>
        <div className="card-row">
          <select
            className="size-select"
            title="Tamaño"
            value={fontSize}
            onChange={(e) => setFontSize(Number(e.target.value))}
          >
            {[8, 9, 10, 11, 12, 14].map((s) => (
              <option key={s} value={s}>
                {s} pt
              </option>
            ))}
          </select>
          <button
            className="btn"
            onClick={() => setZonas({ ...zonas, footerCenter: "{n} / {total}" })}
          >
            Solo numerar
          </button>
        </div>
        <RangoPaginas
          pageCount={pageCount}
          todas={todas}
          setTodas={setTodas}
          rango={rango}
          setRango={setRango}
        />
        {previaSrc && previaSize && (
          <div
            className="previa-pagina previa-centrada"
            style={{ width: PREVIA, height: altoPrevia }}
          >
            <img src={previaSrc} alt={`Página ${paginaActual + 1}`} />
            {(["header", "footer"] as const).map((zona) => (
              <div
                key={zona}
                className={`previa-banda previa-${zona}`}
                style={{ fontSize: Math.max(4, fontSize * escala) }}
              >
                <span>{banda(zona, "Left")}</span>
                <span>{banda(zona, "Center")}</span>
                <span>{banda(zona, "Right")}</span>
              </div>
            ))}
          </div>
        )}
        <p className="modal-file" style={{ whiteSpace: "normal" }}>
          Plantillas: {"{n}"} = página, {"{total}"} = total, {"{fecha}"} = hoy.
        </p>
        <div className="card-actions">
          <button className="btn" onClick={onClose}>
            Cancelar
          </button>
          <button
            className="btn btn-primary"
            disabled={!listo}
            onClick={aplicar}
          >
            Aplicar
          </button>
        </div>
      </div>
    </div>
  );
}
