import { useRef, useState } from "react";
import type { OutlineNode } from "../api";
import { MOD } from "../tipos";
import Icon from "./Icon";

type Path = number[];

/** Dónde cae lo que se arrastra: encima de la fila, debajo, o **dentro**
 *  de ella, que es como se anida un marcador en Acrobat. */
type Zona = "antes" | "despues" | "dentro";

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

/** El nodo que hay en una ruta, si sigue estando. */
function nodoEn(nodes: OutlineNode[], path: Path): OutlineNode | null {
  let actual: OutlineNode | undefined;
  let lista = nodes;
  for (const i of path) {
    actual = lista[i];
    if (!actual) return null;
    lista = actual.children;
  }
  return actual ?? null;
}

/** Mete un nodo en la posición que dice la ruta (el último índice es el
 *  hueco entre hermanos). */
function inserta(
  nodes: OutlineNode[],
  path: Path,
  nodo: OutlineNode,
): OutlineNode[] {
  const [i, ...resto] = path;
  if (resto.length === 0) {
    const copia = [...nodes];
    copia.splice(i, 0, nodo);
    return copia;
  }
  return nodes.map((n, j) =>
    j === i ? { ...n, children: inserta(n.children, resto, nodo) } : n,
  );
}

/** Quitar un nodo mueve a sus hermanos de detrás: la ruta de destino se
 *  corrige antes de insertar, o el marcador aterriza una posición más
 *  abajo de donde se soltó. */
function corrige(destino: Path, quitado: Path): Path {
  const nivel = quitado.length - 1;
  if (destino.length <= nivel) return destino;
  const mismoPadre = quitado
    .slice(0, nivel)
    .every((v, k) => destino[k] === v);
  if (!mismoPadre || quitado[nivel] >= destino[nivel]) return destino;
  const copia = [...destino];
  copia[nivel] -= 1;
  return copia;
}

/** Un marcador no se puede soltar dentro de sí mismo ni de un hijo suyo. */
function esDescendiente(padre: Path, hijo: Path): boolean {
  return padre.length <= hijo.length && padre.every((v, i) => hijo[i] === v);
}

/**
 * Árbol de marcadores del sidebar: navegar con clic —al punto exacto que
 * guarda el marcador, no al principio de la página—, renombrar con F2 o con
 * el lápiz, borrar con Supr y **anidar arrastrando**, que es el árbol
 * completo de Acrobat. Nada pide confirmación: todo lo devuelve ⌘Z.
 */
export default function PanelMarcadores({
  outline,
  onGoto,
  onChange,
  onAnadir,
}: {
  outline: OutlineNode[];
  /** Seguir un marcador: la página **y** su punto de vista. */
  onGoto: (n: OutlineNode) => void;
  onChange: (nodes: OutlineNode[]) => void;
  /** «Añadir marcador aquí»: lo compone `App`, que es quien sabe por dónde
   *  va la lectura y con qué aumento. */
  onAnadir: () => void;
}) {
  const [editing, setEditing] = useState<{ path: Path; text: string } | null>(
    null,
  );
  const [destino, setDestino] = useState<{ path: Path; zona: Zona } | null>(
    null,
  );
  const arrastreRef = useRef<Path | null>(null);
  const destinoRef = useRef<{ path: Path; zona: Zona } | null>(null);

  function suelta() {
    const desde = arrastreRef.current;
    const a = destinoRef.current;
    arrastreRef.current = null;
    destinoRef.current = null;
    setDestino(null);
    if (!desde || !a) return;
    if (esDescendiente(desde, a.path)) return;
    const nodo = nodoEn(outline, desde);
    if (!nodo) return;
    const sinEl = actualiza(outline, desde, () => null);
    const bruto: Path =
      a.zona === "dentro"
        ? [...a.path, nodoEn(outline, a.path)?.children.length ?? 0]
        : a.zona === "antes"
          ? a.path
          : [...a.path.slice(0, -1), a.path[a.path.length - 1] + 1];
    onChange(inserta(sinEl, corrige(bruto, desde), nodo));
  }

  function fila(n: OutlineNode, path: Path) {
    const key = path.join(".");
    const esEdicion = editing && editing.path.join(".") === key;
    const marca =
      destino && destino.path.join(".") === key ? destino.zona : null;
    return (
      <div key={key}>
        <div
          className={`bm-row${marca ? ` drop-${marca}` : ""}`}
          style={{ paddingLeft: 8 + (path.length - 1) * 14 }}
          draggable={!esEdicion}
          onDragStart={(e) => {
            arrastreRef.current = path;
            e.dataTransfer.effectAllowed = "move";
          }}
          onDragOver={(e) => {
            if (!arrastreRef.current) return;
            e.preventDefault();
            const r = e.currentTarget.getBoundingClientRect();
            const y = (e.clientY - r.top) / r.height;
            // el tercio de en medio anida: es el gesto de Acrobat y el de
            // cualquier árbol de carpetas
            const zona: Zona = y < 0.3 ? "antes" : y > 0.7 ? "despues" : "dentro";
            destinoRef.current = { path, zona };
            setDestino({ path, zona });
          }}
          onDrop={(e) => {
            e.preventDefault();
            suelta();
          }}
          onDragEnd={suelta}
        >
          {esEdicion ? (
            <input
              autoFocus
              className="bm-input"
              value={editing.text}
              onChange={(e) => setEditing({ path, text: e.target.value })}
              onKeyDown={(e) => {
                if (e.key === "Enter") {
                  // un título vacío no se aplica: Enter cancela como Esc
                  if (editing.text.trim()) {
                    onChange(
                      actualiza(outline, path, (m) => ({
                        ...m,
                        title: editing.text.trim(),
                      })),
                    );
                  }
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
                  ? `Ir a la página ${n.page_index + 1} · F2 renombra · Supr lo quita`
                  : n.title
              }
              onClick={() => onGoto(n)}
              onKeyDown={(e) => {
                if (e.key === "F2") {
                  e.preventDefault();
                  setEditing({ path, text: n.title });
                } else if (e.key === "Delete" || e.key === "Backspace") {
                  e.preventDefault();
                  onChange(actualiza(outline, path, () => null));
                }
              }}
            >
              {n.title || "(sin título)"}
            </button>
          )}
          {!esEdicion && (
            <span className="bm-actions">
              <button
                title="Renombrar (F2)"
                aria-label={`Renombrar «${n.title}»`}
                onClick={() => setEditing({ path, text: n.title })}
              >
                <Icon name="pen" size={12} />
              </button>
              <button
                title="Eliminar marcador (Supr)"
                aria-label={`Eliminar el marcador «${n.title}»`}
                onClick={() => onChange(actualiza(outline, path, () => null))}
              >
                <Icon name="close" size={12} />
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
        title={`Guarda esta página, este punto y este aumento (${MOD}B)`}
        onClick={onAnadir}
      >
        <Icon name="plus" size={12} />
        Añadir marcador aquí
      </button>
      {outline.length > 0 && (
        <span className="opt-hint bm-hint">
          Arrastra un marcador encima de otro para anidarlo
        </span>
      )}
    </div>
  );
}
