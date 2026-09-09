import { useCallback, useState } from "react";
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

/**
 * «Reemplazar» y «Reemplazar todo» del cajón de búsqueda. El motor ya existía
 * (`edit_text_block`), lo que faltaba era el flujo: la UI cruza cada
 * coincidencia con el bloque de texto que la contiene y manda el lote entero
 * a `replace_text`, que lo hace en una sola mutación. Nada destructivo sin un
 * ⌘Z que lo devuelva de una vez.
 */
export function useReemplazo(ctx: {
  workPath: string | null;
  /** El término que se buscó, no el que se está tecleando. */
  query: string;
  matchCase: boolean;
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
    matchCase,
    matches,
    matchIdx,
    pageCount,
    onNotice,
    onError,
    afterMutation,
  } = ctx;
  const [texto, setTexto] = useState("");
  const [trabajando, setTrabajando] = useState(false);

  /** Los bloques a cambiar, sin repetir: dos coincidencias de la misma línea
   *  son un solo bloque, y `replace_text` cambia las dos de una pasada. */
  const juntaBloques = useCallback(
    async (aTratar: SearchMatch[]): Promise<Reemplazo[]> => {
      if (!workPath) return [];
      const paginas = [...new Set(aTratar.map((m) => m.page_index))];
      const porPagina = new Map<number, TextBlock[]>();
      for (const p of paginas) {
        porPagina.set(p, await getTextBlocks(workPath, p));
      }
      const vistos = new Set<string>();
      const salida: Reemplazo[] = [];
      const buscado = matchCase ? query : query.toLowerCase();
      for (const m of aTratar) {
        const b = bloqueDe(porPagina.get(m.page_index) ?? [], m);
        if (!b) continue;
        const suyo = matchCase ? b.text : b.text.toLowerCase();
        if (!suyo.includes(buscado)) continue;
        const clave = `${m.page_index}:${b.object_index}`;
        if (vistos.has(clave)) continue;
        vistos.add(clave);
        salida.push({
          page_index: m.page_index,
          block_index: b.object_index,
          from: query,
          to: texto,
        });
      }
      return salida;
    },
    [workPath, query, matchCase, texto],
  );

  /** Con `todas`, el documento entero; sin ella, solo la coincidencia que
   *  está señalada (y las de su misma línea, que viven en el mismo bloque). */
  const reemplazar = useCallback(
    async (todas: boolean) => {
      if (!workPath || matches.length === 0 || !query) return;
      const aTratar = todas ? matches : matches.slice(matchIdx, matchIdx + 1);
      setTrabajando(true);
      try {
        onNotice("Reemplazando…", { persistente: true });
        const lote = await juntaBloques(aTratar);
        if (lote.length === 0) {
          onNotice(
            "No se ha podido reemplazar: el texto no está en un bloque que se pueda reescribir.",
          );
          return;
        }
        const { hechas: hechos, saltadas } = await replaceText(workPath, lote);
        afterMutation(pageCount);
        if (hechos === 0) {
          onNotice(
            "No se ha cambiado nada: ese texto está en una fuente que no se puede reescribir.",
          );
          return;
        }
        // el recuento es honesto: si el backend se ha saltado bloques que no
        // sabía reescribir, se dice, en vez de cantar un éxito redondo
        const saltados = saltadas;
        onNotice(
          `${plural(hechos, "bloque de texto reemplazado", "bloques de texto reemplazados")}${
            saltados > 0
              ? ` · ${saltados} en una fuente que no se puede reescribir`
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
      pageCount,
      juntaBloques,
      onNotice,
      onError,
      afterMutation,
    ],
  );

  return { texto, setTexto, trabajando, reemplazar };
}

export type Reemplazador = ReturnType<typeof useReemplazo>;
