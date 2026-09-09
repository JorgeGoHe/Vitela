import { MOD, type OpcionesBusqueda } from "../tipos";
import Icon from "./Icon";

/** Campo de búsqueda de la barra superior con el contador y las flechas
 *  para saltar entre coincidencias (Enter repite el salto; ⇧Enter va atrás). */
export default function Busqueda({
  query,
  setQuery,
  lastQuery,
  total,
  matchIdx,
  searched,
  runSearch,
  limpiar,
  opciones,
  cambiaOpcion,
  gotoMatch,
}: {
  query: string;
  setQuery: (q: string) => void;
  lastQuery: string;
  /** Número de coincidencias. */
  total: number;
  matchIdx: number;
  searched: boolean;
  runSearch: () => void;
  /** Descarta las coincidencias y, con `true`, también el término. */
  limpiar: (conQuery?: boolean) => void;
  /** Coincidir mayúsculas y palabra completa (recordadas entre sesiones). */
  opciones: OpcionesBusqueda;
  cambiaOpcion: (clave: keyof OpcionesBusqueda) => void;
  gotoMatch: (delta: number) => void;
}) {
  return (
    <div className="search">
      <Icon name="search" size={13} />
      <input
        type="text"
        placeholder="Buscar"
        title={`Buscar en el documento (${MOD}F)`}
        aria-label="Buscar en el documento"
        value={query}
        onChange={(e) => setQuery(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Escape") {
            // Esc cierra la búsqueda y devuelve el foco al documento; el
            // atajo no sigue hasta la app (allí sale de la herramienta)
            e.stopPropagation();
            limpiar(true);
            e.currentTarget.blur();
            return;
          }
          if (e.key !== "Enter") return;
          if (searched && total > 0 && query === lastQuery) {
            gotoMatch(e.shiftKey ? -1 : 1);
          } else {
            runSearch();
          }
        }}
      />
      <button
        className={`btn btn-icon search-opt${opciones.matchCase ? " on" : ""}`}
        title="Coincidir mayúsculas y minúsculas"
        aria-label="Coincidir mayúsculas y minúsculas"
        aria-pressed={opciones.matchCase}
        onClick={() => cambiaOpcion("matchCase")}
      >
        Aa
      </button>
      <button
        className={`btn btn-icon search-opt${opciones.wholeWord ? " on" : ""}`}
        title="Solo palabras completas"
        aria-label="Solo palabras completas"
        aria-pressed={opciones.wholeWord}
        onClick={() => cambiaOpcion("wholeWord")}
      >
        |ab|
      </button>
      {searched && (
        <>
          <span className="match-count">
            {total > 0 ? `${matchIdx + 1}/${total}` : "0"}
          </span>
          {total > 0 && (
            <>
              <button
                className="btn btn-icon"
                title="Coincidencia anterior"
                aria-label="Coincidencia anterior"
                onClick={() => gotoMatch(-1)}
              >
                <Icon name="up" size={13} />
              </button>
              <button
                className="btn btn-icon"
                title="Coincidencia siguiente"
                aria-label="Coincidencia siguiente"
                onClick={() => gotoMatch(1)}
              >
                <Icon name="down" size={13} />
              </button>
            </>
          )}
        </>
      )}
    </div>
  );
}
