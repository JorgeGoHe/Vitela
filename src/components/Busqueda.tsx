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
  gotoMatch: (delta: number) => void;
}) {
  return (
    <div className="search">
      <Icon name="search" size={13} />
      <input
        type="text"
        placeholder="Buscar"
        value={query}
        onChange={(e) => setQuery(e.target.value)}
        onKeyDown={(e) => {
          if (e.key !== "Enter") return;
          if (searched && total > 0 && query === lastQuery) {
            gotoMatch(e.shiftKey ? -1 : 1);
          } else {
            runSearch();
          }
        }}
      />
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
