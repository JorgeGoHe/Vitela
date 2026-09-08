import { useCallback, useMemo, useState } from "react";
import { invoke } from "../ipc";
import type { SearchMatch } from "../tipos";
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

  /** Descarta los resultados; con `conQuery` vacía también el campo. */
  const limpiar = useCallback((conQuery = false) => {
    setMatches([]);
    setSearched(false);
    setLastQuery("");
    if (conQuery) setQuery("");
  }, []);

  async function runSearch() {
    const { workPath, gotoPage, onError } = opts;
    if (!workPath) return;
    if (!query.trim()) {
      limpiar();
      return;
    }
    try {
      const res = await invoke<SearchMatch[]>("search_pdf", {
        path: workPath,
        query,
      });
      setMatches(res);
      setMatchIdx(0);
      setSearched(true);
      setLastQuery(query);
      if (res.length > 0) gotoPage(res[0].page_index);
    } catch (e) {
      onError(e);
    }
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
    gotoMatch,
    matchesByPage,
    limpiar,
  };
}
