import { useMemo } from "react";
import type { AnotacionDoc } from "../api";
import {
  firmaAnotacion,
  KIND_ICONS,
  KIND_PLURALS,
  MOD,
  type FiltroComentarios,
} from "../tipos";
import Icon from "./Icon";

/**
 * Pestaña «Comentarios» del sidebar: todos los comentarios del documento en
 * una sola lista, ordenados por página. Clic (o Enter) lleva a la página y
 * selecciona el comentario; ↑/↓ recorren la lista.
 */
export default function PanelComentarios({
  comentarios,
  filtro,
  setFiltro,
  seleccionada,
  onSelect,
  onDelete,
}: {
  comentarios: AnotacionDoc[];
  filtro: FiltroComentarios;
  setFiltro: (f: FiltroComentarios) => void;
  /** Comentario seleccionado ahora mismo, si está en esta lista. */
  seleccionada: { page: number; index: number } | null;
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

  const lista =
    filtro === "todos"
      ? comentarios
      : comentarios.filter((c) => (KIND_PLURALS[c.kind] ?? c.kind) === filtro);

  /** ↑/↓ mueven el foco por la lista; Enter y Espacio activan la fila. */
  function onKeyDown(e: React.KeyboardEvent<HTMLDivElement>, c: AnotacionDoc) {
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      const filas = Array.from(
        e.currentTarget.parentElement?.querySelectorAll<HTMLElement>(
          ".com-row",
        ) ?? [],
      );
      const i = filas.indexOf(e.currentTarget);
      const delta = e.key === "ArrowDown" ? 1 : -1;
      filas[Math.max(0, Math.min(i + delta, filas.length - 1))]?.focus();
    } else if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      onSelect(c);
    }
  }

  return (
    <div className="com-panel">
      <span className="com-total dato">
        {comentarios.length === 1
          ? "1 comentario"
          : `${comentarios.length} comentarios`}
      </span>
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
      {comentarios.length === 0 && (
        <p className="sign-empty">Todavía no hay comentarios.</p>
      )}
      {comentarios.length > 0 && lista.length === 0 && (
        <p className="sign-empty">Ningún comentario de ese tipo.</p>
      )}
      <div role="listbox" aria-label="Comentarios del documento">
        {lista.map((c) => {
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
              tabIndex={elegida ? 0 : -1}
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
