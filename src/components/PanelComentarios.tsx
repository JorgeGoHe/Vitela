import { useEffect, useMemo, useRef } from "react";
import type { AnotacionDoc } from "../api";
import {
  firmaAnotacion,
  KIND_ICONS,
  KIND_PLURALS,
  MOD,
  plural,
  type FiltroComentarios,
} from "../tipos";
import Icon from "./Icon";

/**
 * Pestaña «Comentarios» del sidebar: todos los comentarios del documento en
 * una sola lista, ordenados por página. Clic (o Enter) lleva a la página y
 * selecciona el comentario; ↑/↓ recorren la lista y Supr borra el que tenga
 * el foco. Se entra con Tab desde el resto de la app o con su atajo, que
 * pide el foco subiendo `focoPedido`.
 */
export default function PanelComentarios({
  comentarios,
  filtro,
  setFiltro,
  filtroAutor,
  setFiltroAutor,
  seleccionada,
  focoPedido,
  onSelect,
  onDelete,
}: {
  comentarios: AnotacionDoc[];
  filtro: FiltroComentarios;
  setFiltro: (f: FiltroComentarios) => void;
  /** «todos» o el autor exacto: la otra mitad del filtro que pedía C2. */
  filtroAutor: string;
  setFiltroAutor: (a: string) => void;
  /** Comentario seleccionado ahora mismo, si está en esta lista. */
  seleccionada: { page: number; index: number } | null;
  /** Sube cada vez que el atajo del panel pide el foco de la lista. */
  focoPedido: number;
  onSelect: (c: AnotacionDoc) => void;
  onDelete: (c: AnotacionDoc) => void;
}) {
  // los tipos que hay de verdad en el documento: un filtro con opciones
  // vacías no ayuda a nadie
  const tipos = useMemo(() => {
    const vistos = new Map<string, string>();
    for (const c of comentarios) {
      const etiqueta = KIND_PLURALS[c.kind] ?? c.kind;
      if (!vistos.has(etiqueta)) vistos.set(etiqueta, etiqueta);
    }
    return [...vistos.keys()].sort((a, b) => a.localeCompare(b, "es"));
  }, [comentarios]);

  // los autores que hay de verdad, igual que con los tipos
  const autores = useMemo(() => {
    const vistos = new Set<string>();
    for (const c of comentarios) if (c.author) vistos.add(c.author);
    return [...vistos].sort((a, b) => a.localeCompare(b, "es"));
  }, [comentarios]);

  const lista = comentarios.filter(
    (c) =>
      (filtro === "todos" || (KIND_PLURALS[c.kind] ?? c.kind) === filtro) &&
      (filtroAutor === "todos" ||
        (c.author || "Sin autor") === filtroAutor),
  );
  const filtrando = filtro !== "todos" || filtroAutor !== "todos";

  const panelRef = useRef<HTMLDivElement | null>(null);
  // posición a la que hay que devolver el foco cuando la lista se rehaga
  // tras borrar con el teclado (los índices de anotación se corren, así que
  // las filas son otras y el navegador pierde el foco)
  const volverARef = useRef<number | null>(null);

  /** Las filas de la lista, en orden de pantalla. */
  const filas = () =>
    Array.from(
      panelRef.current?.querySelectorAll<HTMLElement>(".com-row") ?? [],
    );

  // el atajo del panel (⌥⌘3) trae el foco a la fila elegida, o a la primera.
  // Solo cuando se pide: la lista se rehace con cada anotación nueva y no
  // debe robar el foco de donde esté el usuario
  useEffect(() => {
    const el = panelRef.current;
    if (focoPedido === 0 || !el) return;
    const f = Array.from(el.querySelectorAll<HTMLElement>(".com-row"));
    (f.find((n) => n.dataset.elegida === "1") ?? f[0] ?? el).focus();
  }, [focoPedido]);

  // tras borrar con Supr el foco se queda en la lista, en el sitio del que
  // se fue (como en Acrobat), no en el principio de la página
  useEffect(() => {
    const el = panelRef.current;
    const pos = volverARef.current;
    if (pos === null || !el) return;
    volverARef.current = null;
    const f = Array.from(el.querySelectorAll<HTMLElement>(".com-row"));
    (f.length === 0 ? el : f[Math.min(pos, f.length - 1)]).focus();
  }, [comentarios, filtro, filtroAutor]);

  const hayElegida = lista.some(
    (c) => seleccionada?.page === c.page_index && seleccionada.index === c.index,
  );

  /** ↑/↓ mueven el foco por la lista; Enter y Espacio activan la fila;
   *  Supr y Retroceso la borran. */
  function onKeyDown(e: React.KeyboardEvent<HTMLDivElement>, c: AnotacionDoc) {
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      const f = filas();
      const i = f.indexOf(e.currentTarget);
      const delta = e.key === "ArrowDown" ? 1 : -1;
      f[Math.max(0, Math.min(i + delta, f.length - 1))]?.focus();
    } else if (e.key === "Home" || e.key === "End") {
      e.preventDefault();
      const f = filas();
      (e.key === "Home" ? f[0] : f[f.length - 1])?.focus();
    } else if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      onSelect(c);
    } else if (e.key === "Delete" || e.key === "Backspace") {
      // la tecla es de la lista: si sigue, la app borraría además la
      // anotación seleccionada en la página
      e.preventDefault();
      e.stopPropagation();
      volverARef.current = filas().indexOf(e.currentTarget);
      onDelete(c);
    }
  }

  return (
    <div className="com-panel" ref={panelRef} tabIndex={-1}>
      {comentarios.length > 0 && (
        <span className="com-total dato">
          {filtrando
            ? `${lista.length} de ${plural(comentarios.length, "comentario", "comentarios")}`
            : plural(comentarios.length, "comentario", "comentarios")}
        </span>
      )}
      {tipos.length > 1 && (
        <select
          className="size-select com-filtro"
          aria-label="Filtrar los comentarios por tipo"
          value={filtro}
          onChange={(e) => setFiltro(e.target.value)}
        >
          <option value="todos">Todos los tipos</option>
          {tipos.map((t) => (
            <option key={t} value={t}>
              {t}
            </option>
          ))}
        </select>
      )}
      {autores.length > 1 && (
        <select
          className="size-select com-filtro"
          aria-label="Filtrar los comentarios por autor"
          value={filtroAutor}
          onChange={(e) => setFiltroAutor(e.target.value)}
        >
          <option value="todos">Todos los autores</option>
          {autores.map((a) => (
            <option key={a} value={a}>
              {a}
            </option>
          ))}
        </select>
      )}
      {comentarios.length === 0 && (
        <p className="sign-empty">Todavía no hay comentarios.</p>
      )}
      {comentarios.length > 0 && lista.length === 0 && (
        <p className="sign-empty">Ningún comentario con ese filtro.</p>
      )}
      {lista.length > 0 && (
        <p className="opt-hint com-pista">
          ↑ ↓ recorren · Enter va · Supr borra
        </p>
      )}
      <div role="listbox" aria-label="Comentarios del documento">
        {lista.map((c, i) => {
          const elegida =
            seleccionada?.page === c.page_index &&
            seleccionada.index === c.index;
          const firma = firmaAnotacion(c.author, c.modified);
          return (
            <div
              key={`${c.page_index}-${c.index}`}
              className={`com-row${elegida ? " on" : ""}`}
              role="option"
              aria-selected={elegida}
              data-elegida={elegida ? "1" : "0"}
              // sin nada elegido, la primera fila es la que recibe el Tab:
              // con `-1` en todas la lista no se alcanzaba con el teclado
              tabIndex={elegida || (!hayElegida && i === 0) ? 0 : -1}
              title={c.contents || undefined}
              onClick={() => onSelect(c)}
              onKeyDown={(e) => onKeyDown(e, c)}
            >
              <Icon name={KIND_ICONS[c.kind] ?? "note"} size={13} />
              <span className="com-cuerpo">
                <span className="com-firma dato">
                  <span className="com-autor">{firma || "Sin autor"}</span>
                  <span>pág. {c.page_index + 1}</span>
                </span>
                <span className="com-texto">
                  {c.contents || KIND_PLURALS[c.kind] || c.kind}
                </span>
              </span>
              <button
                className="com-borrar"
                tabIndex={-1}
                title={`Eliminar el comentario (${MOD}Z lo devuelve)`}
                aria-label="Eliminar el comentario"
                onClick={(e) => {
                  e.stopPropagation();
                  onDelete(c);
                }}
              >
                <Icon name="close" size={12} />
              </button>
            </div>
          );
        })}
      </div>
    </div>
  );
}
