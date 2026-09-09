import { useCallback, useMemo, useState } from "react";
import { searchPdf } from "../api";
import {
  cargaOpcionesBusqueda,
  guardaOpcionesBusqueda,
  type OpcionesBusqueda,
  type SearchMatch,
} from "../tipos";
import type { PageMatch } from "../components/Pagina";

/**
 * Búsqueda de texto en el documento: consulta, coincidencias con sus
 * rectángulos por página y la coincidencia actual. `limpiar` descarta los
 * resultados (tras mutar el documento las cajas ya no valen).
 */
export function useBusqueda(opts: {
  workPath: string | null;
  gotoPage: (i: number) => void;
  onError: (e: unknown) => void;
}) {
  const [query, setQuery] = useState("");
  const [lastQuery, setLastQuery] = useState("");
  const [matches, setMatches] = useState<SearchMatch[]>([]);
  const [matchIdx, setMatchIdx] = useState(0);
  const [searched, setSearched] = useState(false);
  // «Aa» y «|ab|»: apagadas por defecto, como en Acrobat, y recordadas
  const [opciones, setOpciones] = useState<OpcionesBusqueda>(
    cargaOpcionesBusqueda,
  );

  /** Descarta los resultados; con `conQuery` vacía también el campo. */
  const limpiar = useCallback((conQuery = false) => {
    setMatches([]);
    setSearched(false);
    setLastQuery("");
    if (conQuery) setQuery("");
  }, []);

  async function runSearch(con?: OpcionesBusqueda) {
    const { workPath, gotoPage, onError } = opts;
    const o = con ?? opciones;
    if (!workPath) return;
    if (!query.trim()) {
      limpiar();
      return;
    }
    try {
      const res = await searchPdf(workPath, query, o.matchCase, o.wholeWord);
      setMatches(res);
      setMatchIdx(0);
      setSearched(true);
      setLastQuery(query);
      if (res.length > 0) gotoPage(res[0].page_index);
    } catch (e) {
      onError(e);
    }
  }

  /** Cambia una opción y, si ya había resultados, repite la búsqueda. */
  function cambiaOpcion(clave: keyof OpcionesBusqueda) {
    const next = { ...opciones, [clave]: !opciones[clave] };
    setOpciones(next);
    guardaOpcionesBusqueda(next);
    if (searched) runSearch(next);
  }

  /** Va a una coincidencia concreta: es lo que hace un clic en la lista del
   *  cajón, donde no se recorre de una en una. */
  function irAMatch(i: number) {
    if (i < 0 || i >= matches.length) return;
    setMatchIdx(i);
    opts.gotoPage(matches[i].page_index);
  }

  function gotoMatch(delta: number) {
    if (matches.length === 0) return;
    const next = (matchIdx + delta + matches.length) % matches.length;
    setMatchIdx(next);
    opts.gotoPage(matches[next].page_index);
  }

  // Coincidencias agrupadas por página, con su índice global para saber
  // cuál es la actual
  const matchesByPage = useMemo(() => {
    const m = new Map<number, PageMatch[]>();
    matches.forEach((match, i) => {
      const list = m.get(match.page_index) ?? [];
      list.push({ rects: match.rects, groupIndex: i });
      m.set(match.page_index, list);
    });
    return m;
  }, [matches]);

  return {
    query,
    setQuery,
    lastQuery,
    matches,
    matchIdx,
    searched,
    runSearch,
    opciones,
    cambiaOpcion,
    gotoMatch,
    irAMatch,
    matchesByPage,
    limpiar,
  };
}
