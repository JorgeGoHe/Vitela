import { parseRango, plural } from "../tipos";

/**
 * «Páginas: todas / 1-3, 8», el bloque que Acrobat pone en todo diálogo que
 * actúa sobre el documento. Con «todas» marcado el campo se apaga en vez de
 * desaparecer, para que se vea que existe.
 */
export default function RangoPaginas({
  pageCount,
  todas,
  setTodas,
  rango,
  setRango,
}: {
  pageCount: number;
  todas: boolean;
  setTodas: (v: boolean) => void;
  rango: string;
  setRango: (v: string) => void;
}) {
  const cuantas = todas ? pageCount : parseRango(rango, pageCount).length;

  return (
    <>
      <span className="card-label">Páginas</span>
      <div className="card-row rango-fila">
        <label className="opt-check">
          <input
            type="radio"
            name="rango-paginas"
            checked={todas}
            onChange={() => setTodas(true)}
          />
          Todas
        </label>
        <label className={`opt-check${todas ? " disabled" : ""}`}>
          <input
            type="radio"
            name="rango-paginas"
            checked={!todas}
            onChange={() => setTodas(false)}
          />
          Estas
        </label>
        <input
          type="text"
          className="stamp-input"
          placeholder="1-3, 8"
          aria-label="Páginas a las que se aplica"
          disabled={todas}
          value={rango}
          onFocus={() => setTodas(false)}
          onChange={(e) => setRango(e.target.value)}
        />
        <span className="dato opt-hint">
          {plural(cuantas, "página", "páginas")}
        </span>
      </div>
    </>
  );
}
