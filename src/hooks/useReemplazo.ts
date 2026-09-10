import { useCallback, useMemo, useState } from "react";
import { replaceText, type Reemplazo } from "../api";
import { MOD, plural, type SearchMatch } from "../tipos";

/**
 * «Reemplazar» y «Reemplazar todo» del cajón de búsqueda. El motor ya existía
 * (`edit_text_block`), lo que faltaba era el flujo: la UI agrupa las
 * coincidencias por el bloque de texto en el que caen —el que trae cada una
 * en `block_index`, calculado por el backend— y manda el lote entero a
 * `replace_text`, que lo hace en una sola mutación. Nada destructivo sin un
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
  matches: SearchMatch[];
  matchIdx: number;
  pageCount: number;
  onNotice: (texto: string, opts?: { persistente?: boolean }) => void;
  onError: (e: unknown) => void;
  afterMutation: (newCount: number) => void;
}) {
  const {
    workPath,
    query,
    matches,
    matchIdx,
    pageCount,
    onNotice,
    onError,
    afterMutation,
  } = ctx;
  const [texto, setTexto] = useState("");
  const [trabajando, setTrabajando] = useState(false);

  /** Lo que se va a poder hacer, para decirlo antes de pulsar: cuántas
   *  coincidencias hay, cuántas se van a reemplazar y cuántas se quedan
   *  fuera porque el backend no las ha sabido situar en ningún bloque. Es
   *  `null` mientras las coincidencias no traigan esa información (una
   *  búsqueda hecha con el cajón plegado). */
  const previo = useMemo(() => {
    if (matches.length === 0) return null;
    if (!matches.some((m) => m.block_index !== undefined)) return null;
    const tratables = matches.filter((m) => m.block_index !== undefined).length;
    return {
      total: matches.length,
      tratables,
      fuera: matches.length - tratables,
    };
  }, [matches]);

  /** Con `todas`, el documento entero; sin ella, solo la coincidencia que
   *  está señalada (y las de su misma línea, que viven en el mismo bloque). */
  const reemplazar = useCallback(
    async (todas: boolean) => {
      if (!workPath || matches.length === 0 || !query) return;
      const aTratar = todas ? matches : matches.slice(matchIdx, matchIdx + 1);
      // UNA entrada por coincidencia, también cuando dos caen en la misma
      // línea: `replace_text` agrupa las del mismo bloque y cambia tantas
      // apariciones como entradas le lleguen (`veces`), así que mandar una
      // sola dejaba viva la segunda «Vitela» de la línea y el recuento decía
      // 3 de 4 (AC-053)
      const lote: Reemplazo[] = [];
      let fuera = 0;
      for (const m of aTratar) {
        if (m.block_index === undefined) {
          fuera++;
          continue;
        }
        lote.push({
          page_index: m.page_index,
          block_index: m.block_index,
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
      setTrabajando(true);
      try {
        onNotice("Reemplazando…", { persistente: true });
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
      pageCount,
      onNotice,
      onError,
      afterMutation,
    ],
  );

  return { texto, setTexto, trabajando, previo, reemplazar };
}

export type Reemplazador = ReturnType<typeof useReemplazo>;
