import { useState } from "react";
import { useModal } from "../hooks/useModal";
import { indicesDeRango } from "../tipos";
import RangoPaginas from "./RangoPaginas";

/** Las seis esquinas donde Acrobat deja poner el sello. */
const POSICIONES: [string, string][] = [
  ["se", "Abajo a la derecha"],
  ["s", "Abajo en el centro"],
  ["sw", "Abajo a la izquierda"],
  ["ne", "Arriba a la derecha"],
  ["n", "Arriba en el centro"],
  ["nw", "Arriba a la izquierda"],
];

export type BatesOpts = {
  prefijo: string;
  sufijo: string;
  digitos: number;
  empiezaEn: number;
  position: string;
  pageIndices: number[] | null;
};

/**
 * «Numeración Bates», el sello de los expedientes: un número corrido por
 * página, con ceros delante y con prefijo y sufijo, en una esquina fija.
 * Los defectos son los de Acrobat —seis dígitos, empieza en 1, abajo a la
 * derecha— porque son los que espera quien recibe el expediente.
 */
export default function DialogoBates({
  pageCount,
  onConfirm,
  onClose,
}: {
  pageCount: number;
  onConfirm: (opts: BatesOpts) => void;
  onClose: () => void;
}) {
  const [prefijo, setPrefijo] = useState("");
  const [sufijo, setSufijo] = useState("");
  const [digitos, setDigitos] = useState(6);
  const [empiezaEn, setEmpiezaEn] = useState(1);
  const [position, setPosition] = useState("se");
  const [todas, setTodas] = useState(true);
  const [rango, setRango] = useState("");

  const indices = indicesDeRango(todas, rango, pageCount);
  const cuantas = indices?.length ?? pageCount;
  const listo = cuantas > 0;
  const muestra = `${prefijo}${String(empiezaEn).padStart(
    Math.max(1, Math.min(15, digitos)),
    "0",
  )}${sufijo}`;

  const confirmar = () => {
    if (!listo) return;
    onConfirm({
      prefijo,
      sufijo,
      digitos: Math.max(1, Math.min(15, digitos)),
      empiezaEn: Math.max(0, empiezaEn),
      position,
      pageIndices: indices,
    });
  };
  const { ref, onKeyDown } = useModal({ onClose, onConfirm: confirmar });

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div
        className="modal"
        role="dialog"
        aria-modal="true"
        aria-label="Numeración Bates"
        onClick={(e) => e.stopPropagation()}
        onKeyDown={onKeyDown}
        ref={ref}
        tabIndex={-1}
      >
        <h3>Numeración Bates</h3>
        <div className="fila-campos">
          <label className="opt-check">
            Prefijo
            <input
              className="stamp-input"
              style={{ width: 110 }}
              aria-label="Prefijo del sello"
              placeholder="EXP-"
              value={prefijo}
              onChange={(e) => setPrefijo(e.target.value)}
            />
          </label>
          <label className="opt-check">
            Sufijo
            <input
              className="stamp-input"
              style={{ width: 110 }}
              aria-label="Sufijo del sello"
              placeholder="-A"
              value={sufijo}
              onChange={(e) => setSufijo(e.target.value)}
            />
          </label>
        </div>
        <div className="fila-campos">
          <label className="opt-check">
            Dígitos
            <input
              type="number"
              className="stamp-input"
              style={{ width: 64 }}
              min={1}
              max={15}
              aria-label="Cuántos dígitos ocupa el número"
              value={digitos}
              onChange={(e) => setDigitos(Number(e.target.value) || 1)}
            />
          </label>
          <label className="opt-check">
            Empieza en
            <input
              type="number"
              className="stamp-input"
              style={{ width: 90 }}
              min={0}
              aria-label="Número de la primera página"
              value={empiezaEn}
              onChange={(e) => setEmpiezaEn(Number(e.target.value) || 0)}
            />
          </label>
        </div>
        <span className="card-label">Dónde va</span>
        <select
          className="size-select"
          aria-label="Posición del sello"
          value={position}
          onChange={(e) => setPosition(e.target.value)}
        >
          {POSICIONES.map(([v, texto]) => (
            <option key={v} value={v}>
              {texto}
            </option>
          ))}
        </select>
        <RangoPaginas
          pageCount={pageCount}
          todas={todas}
          setTodas={setTodas}
          rango={rango}
          setRango={setRango}
        />
        <p className="modal-file" style={{ whiteSpace: "normal" }}>
          La primera llevará <span className="dato">{muestra}</span> y la
          última <span className="dato">
            {`${prefijo}${String(
              Math.max(0, empiezaEn) + cuantas - 1,
            ).padStart(Math.max(1, Math.min(15, digitos)), "0")}${sufijo}`}
          </span>
          . Se escribe como contenido del documento, igual que el pie de
          página.
        </p>
        <div className="card-actions">
          <button className="btn" onClick={onClose}>
            Cancelar
          </button>
          <button
            className="btn btn-primary"
            disabled={!listo}
            onClick={confirmar}
          >
            Numerar
          </button>
        </div>
      </div>
    </div>
  );
}
