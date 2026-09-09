import { plural, type SearchMatch } from "../tipos";
import type { Reemplazador } from "../hooks/useReemplazo";

/**
 * El cajón que se despliega del campo de búsqueda: la lista de coincidencias
 * con la frase de alrededor y, debajo, «Reemplazar» y «Reemplazar todo». Sale
 * del campo que el usuario ya está usando, no de un modo nuevo, y va plegado
 * por defecto: quien solo quiere buscar no paga ni un paso.
 */
export default function CajonBusqueda({
  matches,
  matchIdx,
  query,
  irAMatch,
  reemplazo,
}: {
  matches: SearchMatch[];
  matchIdx: number;
  /** El término que se buscó, que es el que va resaltado en cada frase. */
  query: string;
  irAMatch: (i: number) => void;
  reemplazo: Reemplazador;
}) {
  const paginas = new Set(matches.map((m) => m.page_index)).size;

  return (
    <div className="search-cajon" onMouseDown={(e) => e.stopPropagation()}>
      <span className="dato search-resumen">
        {plural(matches.length, "coincidencia", "coincidencias")} en{" "}
        {plural(paginas, "página", "páginas")}
      </span>
      <div
        className="search-resultados"
        role="listbox"
        aria-label="Resultados de la búsqueda"
      >
        {matches.map((m, i) => (
          <button
            key={`${m.page_index}-${i}`}
            role="option"
            aria-selected={i === matchIdx}
            className={`search-resultado${i === matchIdx ? " on" : ""}`}
            onClick={() => irAMatch(i)}
          >
            <span className="dato search-pagina">pág. {m.page_index + 1}</span>
            <span className="search-frase">
              {m.before ?? ""}
              <mark>{query}</mark>
              {m.after ?? ""}
            </span>
          </button>
        ))}
      </div>
      <div className="search-reemplazo">
        <input
          type="text"
          placeholder="Reemplazar con…"
          aria-label="Reemplazar con"
          value={reemplazo.texto}
          onChange={(e) => reemplazo.setTexto(e.target.value)}
        />
        <button
          className="btn"
          title="Reemplaza la coincidencia señalada (y las de su misma línea)"
          disabled={reemplazo.trabajando}
          onClick={() => reemplazo.reemplazar(false)}
        >
          Reemplazar
        </button>
        {/* sin confirmación, como el resto de Vitela: se hace, se dice cuántas
            y ⌘Z lo devuelve entero */}
        <button
          className="btn"
          title="Reemplaza todas las coincidencias del documento"
          disabled={reemplazo.trabajando}
          onClick={() => reemplazo.reemplazar(true)}
        >
          Reemplazar todo
        </button>
      </div>
    </div>
  );
}
