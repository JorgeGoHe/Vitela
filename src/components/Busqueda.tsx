import { MOD, type OpcionesBusqueda, type SearchMatch } from "../tipos";
import type { BusquedaCarpeta } from "../hooks/useBusquedaCarpeta";
import CajonBusqueda from "./CajonBusqueda";
import Icon from "./Icon";
import type { Reemplazador } from "../hooks/useReemplazo";

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
  matches,
  irAMatch,
  cajonAbierto,
  setCajonAbierto,
  reemplazo,
  carpeta,
  hayDocumento,
  onAbrirCoincidencia,
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
  /** Las coincidencias con su frase de contexto, para la lista del cajón. */
  matches: SearchMatch[];
  irAMatch: (i: number) => void;
  cajonAbierto: boolean;
  setCajonAbierto: (v: boolean) => void;
  reemplazo: Reemplazador;
  /** El otro ámbito: buscar en todos los PDF de una carpeta. */
  carpeta: BusquedaCarpeta;
  /** Hay documento abierto. Sin él el campo sigue en pie, pero solo para
   *  buscar en una carpeta: eso se hace antes de abrir nada. */
  hayDocumento: boolean;
  onAbrirCoincidencia: (path: string, pageIndex: number) => void;
}) {
  const enCarpeta = carpeta.ambito === "carpeta" || !hayDocumento;
  const hayCajon = searched && total > 0;
  const lanzarEnCarpeta = () => {
    setCajonAbierto(true);
    void carpeta.buscar(query, opciones);
  };
  return (
    <div className="search">
      <Icon name="search" size={13} />
      <input
        type="text"
        placeholder={enCarpeta ? "Buscar en una carpeta" : "Buscar"}
        title={
          enCarpeta
            ? `Buscar en todos los PDF de una carpeta (⇧${MOD}F)`
            : `Buscar en el documento (${MOD}F)`
        }
        aria-label={
          enCarpeta
            ? "Buscar en todos los PDF de una carpeta"
            : "Buscar en el documento"
        }
        value={query}
        onChange={(e) => setQuery(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Escape") {
            // Esc cierra la búsqueda y devuelve el foco al documento; el
            // atajo no sigue hasta la app (allí sale de la herramienta)
            e.stopPropagation();
            // buscando en una carpeta, Esc pliega el cajón y **no** cancela
            // la búsqueda: lo que está en marcha sigue y se puede volver
            if (enCarpeta) setCajonAbierto(false);
            else limpiar(true);
            e.currentTarget.blur();
            return;
          }
          // con el cajón abierto, ↑ y ↓ recorren la lista sin salir del campo
          if (hayCajon && cajonAbierto && (e.key === "ArrowDown" || e.key === "ArrowUp")) {
            e.preventDefault();
            gotoMatch(e.key === "ArrowDown" ? 1 : -1);
            return;
          }
          if (e.key !== "Enter") return;
          if (enCarpeta) {
            lanzarEnCarpeta();
            return;
          }
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
      <button
        className={`btn btn-icon${cajonAbierto ? " on" : ""}`}
        title={
          cajonAbierto
            ? "Ocultar la lista de resultados"
            : `Lista de resultados, buscar en una carpeta y reemplazar (⇧${MOD}F)`
        }
        aria-label="Lista de resultados, buscar en una carpeta y reemplazar"
        aria-expanded={cajonAbierto}
        onClick={() => setCajonAbierto(!cajonAbierto)}
      >
        <Icon name="more" size={13} />
      </button>
      {searched && !enCarpeta && (
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
      {cajonAbierto && (hayCajon || enCarpeta) && (
        <CajonBusqueda
          matches={matches}
          matchIdx={matchIdx}
          query={lastQuery}
          termino={query}
          hayDocumento={hayDocumento}
          irAMatch={irAMatch}
          reemplazo={reemplazo}
          carpeta={carpeta}
          onBuscarCarpeta={lanzarEnCarpeta}
          onAbrirCoincidencia={onAbrirCoincidencia}
        />
      )}
    </div>
  );
}
