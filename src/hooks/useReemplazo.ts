import { useCallback, useEffect, useState } from "react";
import { getTextBlocks, replaceText, type Reemplazo } from "../api";
import { MOD, plural, type SearchMatch, type TextBlock } from "../tipos";

/** El bloque de texto que contiene una coincidencia: el que se lleva su
 *  centro. Las cajas de `search_pdf` y las de `get_text_blocks` están en el
 *  mismo espacio (el propio de la página), así que se cruzan sin convertir. */
function bloqueDe(
  bloques: TextBlock[],
  match: SearchMatch,
): TextBlock | undefined {
  const r = match.rects[0];
  if (!r) return undefined;
  const cx = r.x + r.w / 2;
  const cy = r.y + r.h / 2;
  return bloques.find(
    (b) => cx >= b.x && cx <= b.x + b.w && cy >= b.y && cy <= b.y + b.h,
  );
}

/** Dónde cae cada coincidencia: el bloque que la contiene, o `null` si no
 *  hay ninguno que se pueda reescribir. Un elemento por coincidencia y en su
 *  mismo orden, para poder contar las que se quedan fuera. */
type Sitio = number | null;

/**
 * «Reemplazar» y «Reemplazar todo» del cajón de búsqueda. El motor ya existía
 * (`edit_text_block`), lo que faltaba era el flujo: la UI cruza cada
 * coincidencia con el bloque de texto que la contiene y manda el lote entero
 * a `replace_text`, que lo hace en una sola mutación. Nada destructivo sin un
 * ⌘Z que lo devuelva de una vez.
 *
 * Los recuentos son en **coincidencias**, que es lo que el usuario ha
 * contado en la lista: ni «bloques» (vocabulario del motor) ni un número que
 * no cuadre con el que acaba de leer. Y lo que no se va a poder hacer se
 * dice **antes**, no después.
 */
export function useReemplazo(ctx: {
  workPath: string | null;
  /** El término que se buscó, no el que se está tecleando. */
  query: string;
  matchCase: boolean;
  matches: SearchMatch[];
  matchIdx: number;
  pageCount: number;
  /** El cajón está abierto: solo entonces se sitúan las coincidencias, que
   *  es trabajo que quien únicamente busca no tiene por qué pagar. */
  activo: boolean;
  onNotice: (texto: string, opts?: { persistente?: boolean }) => void;
  onError: (e: unknown) => void;
  afterMutation: (newCount: number) => void;
}) {
  const {
    workPath,
    query,
    matchCase,
    matches,
    matchIdx,
    pageCount,
    activo,
    onNotice,
    onError,
    afterMutation,
  } = ctx;
  const [texto, setTexto] = useState("");
  const [trabajando, setTrabajando] = useState(false);
  const [sitios, setSitios] = useState<Sitio[] | null>(null);

  /** Sitúa cada coincidencia en su bloque de texto. */
  const situa = useCallback(async (): Promise<Sitio[]> => {
    if (!workPath || !query) return matches.map(() => null);
    const porPagina = new Map<number, TextBlock[]>();
    for (const p of new Set(matches.map((m) => m.page_index))) {
      porPagina.set(p, await getTextBlocks(workPath, p));
    }
    const buscado = matchCase ? query : query.toLowerCase();
    return matches.map((m) => {
      const b = bloqueDe(porPagina.get(m.page_index) ?? [], m);
      if (!b) return null;
      const suyo = matchCase ? b.text : b.text.toLowerCase();
      return suyo.includes(buscado) ? b.object_index : null;
    });
  }, [workPath, query, matchCase, matches]);

  // Con el cajón abierto se sitúan las coincidencias por adelantado: es lo
  // que permite avisar de las que no se van a poder tocar ANTES de pulsar
  useEffect(() => {
    if (!activo || !workPath || !query || matches.length === 0) {
      setSitios(null);
      return;
    }
    let cancelado = false;
    situa()
      .then((s) => {
        if (!cancelado) setSitios(s);
      })
      .catch(() => {
        if (!cancelado) setSitios(null);
      });
    return () => {
      cancelado = true;
    };
  }, [activo, workPath, query, matches, situa]);

  /** Lo que se va a poder hacer, para decirlo antes: cuántas coincidencias
   *  hay, cuántas se van a reemplazar y cuántas se quedan fuera. */
  const previo =
    sitios && sitios.length === matches.length
      ? {
          total: matches.length,
          tratables: sitios.filter((s) => s !== null).length,
          fuera: sitios.filter((s) => s === null).length,
        }
      : null;

  /** Con `todas`, el documento entero; sin ella, solo la coincidencia que
   *  está señalada (y las de su misma línea, que viven en el mismo bloque). */
  const reemplazar = useCallback(
    async (todas: boolean) => {
      if (!workPath || matches.length === 0 || !query) return;
      setTrabajando(true);
      try {
        onNotice("Reemplazando…", { persistente: true });
        const donde =
          sitios && sitios.length === matches.length ? sitios : await situa();
        const cuales = todas ? matches.map((_, i) => i) : [matchIdx];
        // dos coincidencias de la misma línea son un solo bloque, y
        // `replace_text` cambia las dos de una pasada
        const vistos = new Set<string>();
        const lote: Reemplazo[] = [];
        let fuera = 0;
        for (const i of cuales) {
          const bloque = donde[i];
          if (bloque === null || bloque === undefined) {
            fuera++;
            continue;
          }
          const clave = `${matches[i].page_index}:${bloque}`;
          if (vistos.has(clave)) continue;
          vistos.add(clave);
          lote.push({
            page_index: matches[i].page_index,
            block_index: bloque,
            from: query,
            to: texto,
          });
        }
        if (lote.length === 0) {
          onNotice(
            "No se ha podido reemplazar: ese texto no está en ningún sitio que se pueda reescribir.",
          );
          return;
        }
        const { hechas, saltadas } = await replaceText(workPath, lote);
        afterMutation(pageCount);
        // el recuento es honesto y en coincidencias, que es lo que el
        // usuario acaba de contar en la lista: las que el backend se saltó
        // por la fuente y las que ni llegaron a mandarse suman igual
        const sinTocar = saltadas + fuera;
        if (hechas === 0) {
          onNotice(
            "No se ha cambiado nada: ese texto está en una fuente que no se puede reescribir.",
          );
          return;
        }
        onNotice(
          `${plural(hechas, "coincidencia reemplazada", "coincidencias reemplazadas")}${
            sinTocar > 0
              ? ` · ${sinTocar} en un texto que no se puede reescribir`
              : ""
          } · ${MOD}Z lo devuelve`,
        );
      } catch (e) {
        onError(e);
      } finally {
        setTrabajando(false);
      }
    },
    [
      workPath,
      matches,
      matchIdx,
      query,
      texto,
      sitios,
      situa,
      pageCount,
      onNotice,
      onError,
      afterMutation,
    ],
  );

  return { texto, setTexto, trabajando, previo, reemplazar };
}

export type Reemplazador = ReturnType<typeof useReemplazo>;
