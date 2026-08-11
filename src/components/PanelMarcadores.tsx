import { useState } from "react";
import type { OutlineNode } from "../api";

type Path = number[];

function actualiza(
  nodes: OutlineNode[],
  path: Path,
  fn: (n: OutlineNode) => OutlineNode | null,
): OutlineNode[] {
  if (path.length === 0) return nodes;
  const [i, ...resto] = path;
  return nodes.flatMap((n, j) => {
    if (j !== i) return [n];
    if (resto.length === 0) {
      const r = fn(n);
      return r ? [r] : [];
    }
    return [{ ...n, children: actualiza(n.children, resto, fn) }];
  });
}

/**
 * Árbol de marcadores del sidebar: navegar con clic, renombrar en línea,
 * borrar y añadir un marcador de la página actual.
 */
export default function PanelMarcadores({
  outline,
  currentPage,
  onGoto,
  onChange,
}: {
  outline: OutlineNode[];
  currentPage: number;
  onGoto: (page: number) => void;
  onChange: (nodes: OutlineNode[]) => void;
}) {
  const [editing, setEditing] = useState<{ path: Path; text: string } | null>(
    null,
  );

  function fila(n: OutlineNode, path: Path) {
    const key = path.join(".");
    const esEdicion = editing && editing.path.join(".") === key;
    return (
      <div key={key}>
        <div
          className="bm-row"
          style={{ paddingLeft: 8 + (path.length - 1) * 14 }}
        >
          {esEdicion ? (
            <input
              autoFocus
              className="bm-input"
              value={editing.text}
              onChange={(e) => setEditing({ path, text: e.target.value })}
              onKeyDown={(e) => {
                if (e.key === "Enter" && editing.text.trim()) {
                  onChange(
                    actualiza(outline, path, (m) => ({
                      ...m,
                      title: editing.text.trim(),
                    })),
                  );
                  setEditing(null);
                }
                if (e.key === "Escape") setEditing(null);
              }}
              onBlur={() => setEditing(null)}
            />
          ) : (
            <button
              className="bm-title"
              title={
                n.page_index !== null
                  ? `Ir a la página ${n.page_index + 1}`
                  : n.title
              }
              onClick={() => {
                if (n.page_index !== null) onGoto(n.page_index);
              }}
            >
              {n.title || "(sin título)"}
            </button>
          )}
          {!esEdicion && (
            <span className="bm-actions">
              <button
                title="Renombrar"
                onClick={() => setEditing({ path, text: n.title })}
              >
                ✎
              </button>
              <button
                title="Eliminar marcador"
                onClick={() => onChange(actualiza(outline, path, () => null))}
              >
                ✕
              </button>
            </span>
          )}
        </div>
        {n.children.map((c, i) => fila(c, [...path, i]))}
      </div>
    );
  }

  return (
    <div className="bm-panel">
      {outline.length === 0 && (
        <p className="sign-empty">Este documento no tiene marcadores.</p>
      )}
      {outline.map((n, i) => fila(n, [i]))}
      <button
        className="btn bm-add"
        onClick={() =>
          onChange([
            ...outline,
            {
              title: `Página ${currentPage + 1}`,
              page_index: currentPage,
              children: [],
            },
          ])
        }
      >
        ＋ Marcador en esta página
      </button>
    </div>
  );
}
