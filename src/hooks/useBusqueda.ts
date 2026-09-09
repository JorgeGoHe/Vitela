import { useCallback, useEffect, useMemo, useRef, useState } from "react";
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
  /** Se va a usar la lista de resultados (el cajón está abierto), así que
   *  la búsqueda pide `context`: la frase de alrededor y el bloque de cada
   *  coincidencia. Con el cajón plegado esa pasada extra no se paga. */
  contexto: boolean;
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

  // si la última búsqueda se pidió con contexto: sin él las coincidencias
  // no traen ni la frase de alrededor ni el bloque, y el cajón las necesita
  const [conContexto, setConContexto] = useState(false);

  /** Descarta los resultados; con `conQuery` vacía también el campo. */
  const limpiar = useCallback((conQuery = false) => {
    setMatches([]);
    setSearched(false);
    setLastQuery("");
    setConContexto(false);
    if (conQuery) setQuery("");
  }, []);

  /** El cuerpo de la búsqueda. Con `mantener` no se salta a la primera
   *  coincidencia ni se reinicia el recorrido: es la repetición silenciosa
   *  que hace falta al desplegar el cajón, no una búsqueda nueva. */
  async function ejecuta(o: OpcionesBusqueda, mantener: boolean) {
    const { workPath, contexto, gotoPage, onError } = opts;
    if (!workPath) return;
    if (!query.trim()) {
      limpiar();
      return;
    }
    try {
      const res = await searchPdf(
        workPath,
        query,
        o.matchCase,
        o.wholeWord,
        contexto,
      );
      setMatches(res);
      setSearched(true);
      setLastQuery(query);
      setConContexto(contexto);
      if (!mantener) {
        setMatchIdx(0);
        if (res.length > 0) gotoPage(res[0].page_index);
      }
    } catch (e) {
      onError(e);
    }
  }

  async function runSearch(con?: OpcionesBusqueda) {
    await ejecuta(con ?? opciones, false);
  }

  // Al desplegar el cajón sobre una búsqueda que se hizo sin contexto se
  // repite una vez, ya con él: es el único momento en que hace falta, y sin
  // mover al usuario de donde estaba leyendo.
  const repetir = useRef(() => {});
  useEffect(() => {
    repetir.current = () => void ejecuta(opciones, true);
  });
  useEffect(() => {
    if (opts.contexto && searched && !conContexto) repetir.current();
  }, [opts.contexto, searched, conContexto]);

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
