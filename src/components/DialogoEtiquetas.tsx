import { useState } from "react";
import { useModal } from "../hooks/useModal";
import {
  aplicaRangoEtiquetas,
  etiquetaDePagina,
  type RangoEtiquetas,
} from "../tipos";
import type { EstiloEtiqueta } from "../api";

/** Los estilos del spec, dichos como los dice Acrobat. */
const ESTILOS: [EstiloEtiqueta, string][] = [
  ["arabigo", "1, 2, 3…"],
  ["romano_min", "i, ii, iii…"],
  ["romano", "I, II, III…"],
  ["letra_min", "a, b, c…"],
  ["letra", "A, B, C…"],
  ["ninguno", "Sin número (solo el prefijo)"],
];

/**
 * «Numerar páginas…», el diálogo de Acrobat: a un tramo de páginas se le
 * pone un estilo, un prefijo y el número por el que empieza. Lo que va
 * detrás conserva su numeración, así que numerar el prólogo en romanos no
 * renumera el libro entero.
 *
 * La numeración es del papel, no del contenido: cambia cómo se llama cada
 * página en la píldora, en las miniaturas y en «Ir a la página», que es
 * exactamente lo que se necesita cuando el PDF trae un prólogo en romanos.
 */
export default function DialogoEtiquetas({
  pageCount,
  pageIndex,
  rangos,
  onConfirm,
  onQuitar,
  onClose,
}: {
  pageCount: number;
  /** La página que se está leyendo: el tramo empieza aquí por defecto. */
  pageIndex: number;
  rangos: RangoEtiquetas[];
  onConfirm: (rangos: RangoEtiquetas[]) => void;
  /** Quitar la numeración entera y volver a 1..N. */
  onQuitar: () => void;
  onClose: () => void;
}) {
  const [desde, setDesde] = useState(pageIndex + 1);
  const [hasta, setHasta] = useState(pageCount);
  const [estilo, setEstilo] = useState<EstiloEtiqueta>("arabigo");
  const [prefijo, setPrefijo] = useState("");
  const [empiezaEn, setEmpiezaEn] = useState(1);

  const a = Math.min(Math.max(1, desde), pageCount) - 1;
  const b = Math.min(Math.max(a + 1, hasta), pageCount) - 1;
  const propuesta = aplicaRangoEtiquetas(
    rangos,
    a,
    b,
    { estilo, prefijo, empieza_en: empiezaEn },
    pageCount,
  );
  const confirmar = () => onConfirm(propuesta);
  const { ref, onKeyDown } = useModal({ onClose, onConfirm: confirmar });

  /** Cómo quedarán las primeras páginas del tramo, para no tener que
   *  imaginárselo: es la misma cuenta que hace el visor. */
  const muestra = [a, a + 1, b]
    .filter((i, j, v) => i <= b && v.indexOf(i) === j)
    .map((i) => `${etiquetaDePagina(propuesta, i)} (${i + 1})`)
    .join(" · ");

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div
        className="modal"
        role="dialog"
        aria-modal="true"
        aria-label="Numerar páginas"
        onClick={(e) => e.stopPropagation()}
        onKeyDown={onKeyDown}
        ref={ref}
        tabIndex={-1}
      >
        <h3>Numerar páginas</h3>
        <span className="card-label">Páginas</span>
        <div className="fila-campos">
          <label className="opt-check">
            De
            <input
              type="number"
              className="stamp-input"
              style={{ width: 72 }}
              min={1}
              max={pageCount}
              aria-label="Primera página del tramo"
              value={desde}
              onChange={(e) => setDesde(Number(e.target.value) || 1)}
            />
          </label>
          <label className="opt-check">
            a
            <input
              type="number"
              className="stamp-input"
              style={{ width: 72 }}
              min={1}
              max={pageCount}
              aria-label="Última página del tramo"
              value={hasta}
              onChange={(e) => setHasta(Number(e.target.value) || pageCount)}
            />
          </label>
        </div>
        <span className="card-label">Numeración</span>
        <select
          className="size-select"
          aria-label="Estilo de numeración"
          value={estilo}
          onChange={(e) => setEstilo(e.target.value as EstiloEtiqueta)}
        >
          {ESTILOS.map(([v, texto]) => (
            <option key={v} value={v}>
              {texto}
            </option>
          ))}
        </select>
        <div className="fila-campos">
          <label className="opt-check">
            Prefijo
            <input
              className="stamp-input"
              style={{ width: 120 }}
              aria-label="Prefijo de la numeración"
              placeholder="A-"
              value={prefijo}
              onChange={(e) => setPrefijo(e.target.value)}
            />
          </label>
          <label className="opt-check">
            Empieza en
            <input
              type="number"
              className="stamp-input"
              style={{ width: 72 }}
              min={1}
              aria-label="Número por el que empieza el tramo"
              value={empiezaEn}
              onChange={(e) => setEmpiezaEn(Number(e.target.value) || 1)}
            />
          </label>
        </div>
        <p className="modal-file" style={{ whiteSpace: "normal" }}>
          Quedará <span className="dato">{muestra}</span> — la etiqueta y, entre
          paréntesis, el número físico. Las páginas de después conservan la
          numeración que tenían.
        </p>
        <div className="card-actions">
          {rangos.length > 0 && (
            <button
              className="btn"
              title="El documento vuelve a numerarse 1, 2, 3…"
              onClick={onQuitar}
            >
              Quitar la numeración
            </button>
          )}
          <button className="btn" onClick={onClose}>
            Cancelar
          </button>
          <button className="btn btn-primary" onClick={confirmar}>
            Numerar
          </button>
        </div>
      </div>
    </div>
  );
}
