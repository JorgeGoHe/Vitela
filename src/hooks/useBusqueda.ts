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
  // el último término que se llegó a buscar; sobrevive a `limpiar` porque es
  // lo que repite ⌘G cuando ya no hay coincidencias en pantalla
  const [ultimoTermino, setUltimoTermino] = useState("");

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
  async function ejecuta(
    o: OpcionesBusqueda,
    mantener: boolean,
    termino = query,
  ) {
    const { workPath, contexto, gotoPage, onError } = opts;
    if (!workPath) return;
    if (!termino.trim()) {
      limpiar();
      return;
    }
    try {
      const res = await searchPdf(
        workPath,
        termino,
        o.matchCase,
        o.wholeWord,
        contexto,
      );
      setMatches(res);
      setSearched(true);
      setLastQuery(termino);
      setUltimoTermino(termino);
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

  /** Busca un término en un documento **que se acaba de abrir**, cuya ruta
   *  todavía no ha llegado al estado del hook, y se queda en la coincidencia
   *  de la página pedida. Es lo que hace que abrir un resultado de la
   *  búsqueda en carpeta llegue con la palabra ya resaltada: sin esto el
   *  documento se abría en la página buena y sin una sola marca. */
  async function buscarEn(work: string, termino: string, pagina: number) {
    if (!termino.trim()) return;
    try {
      const res = await searchPdf(
        work,
        termino,
        opciones.matchCase,
        opciones.wholeWord,
        opts.contexto,
      );
      setMatches(res);
      setSearched(true);
      setQuery(termino);
      setLastQuery(termino);
      setUltimoTermino(termino);
      setConContexto(opts.contexto);
      const i = res.findIndex((m) => m.page_index === pagina);
      setMatchIdx(i < 0 ? 0 : i);
    } catch (e) {
      opts.onError(e);
    }
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

  // Tras mutar el documento las cajas de las coincidencias ya no valen. Se
  // rehace la búsqueda en vez de tirar la lista: reemplazar (o un ⌘Z detrás)
  // dejaba al usuario sin resultados y con el término aún escrito, teniendo
  // que volver a pulsar Enter. Acrobat mantiene el panel.
  const trasMutacionRef = useRef(() => {});
  useEffect(() => {
    trasMutacionRef.current = () => {
      if (searched && query.trim()) void ejecuta(opciones, true);
      else limpiar();
    };
  });
  const trasMutacion = useCallback(() => trasMutacionRef.current(), []);

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

  /** ⌘G sin coincidencias en pantalla: Acrobat repite la última búsqueda en
   *  vez de quedarse mudo. Devuelve si ha habido algo que repetir. */
  function repetirUltima(): boolean {
    const termino = query.trim() || ultimoTermino;
    if (!termino) return false;
    if (query !== termino) setQuery(termino);
    void ejecuta(opciones, false, termino);
    return true;
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
    buscarEn,
    opciones,
    cambiaOpcion,
    gotoMatch,
    repetirUltima,
    trasMutacion,
    irAMatch,
    matchesByPage,
    limpiar,
  };
}
