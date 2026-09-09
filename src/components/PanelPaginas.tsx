import { useRef, useState } from "react";
import { plural } from "../tipos";
import Icon from "./Icon";

/**
 * Lista de miniaturas del panel lateral: navegación, arrastre para
 * reordenar (con línea de inserción), selección múltiple con ⌘/⇧ y las
 * acciones por página. Todo lo que se hace arrastrando se puede hacer con
 * el teclado: los botones Subir/Bajar siguen ahí y las flechas recorren la
 * lista.
 */
export default function PanelPaginas({
  thumbs,
  pageIndex,
  pageCount,
  seleccion,
  setSeleccion,
  gotoPage,
  movePage,
  rotatePage,
  duplicatePageAt,
  blankPageAfter,
  deletePage,
  girarLote,
  eliminarLote,
  extraerLote,
}: {
  thumbs: (string | null)[];
  pageIndex: number;
  pageCount: number;
  /** Páginas marcadas para actuar en lote (índices desde 0). */
  seleccion: Set<number>;
  setSeleccion: (s: Set<number>) => void;
  gotoPage: (i: number) => void;
  movePage: (from: number, to: number) => void;
  rotatePage: (i: number) => void;
  duplicatePageAt: (i: number) => void;
  blankPageAfter: (i: number) => void;
  deletePage: (i: number) => void;
  girarLote: (cuartos: number) => void;
  eliminarLote: () => void;
  extraerLote: () => void;
}) {
  // arrastre para reordenar: la página que se lleva y el hueco donde caería
  const arrastreRef = useRef<number | null>(null);
  const [dropIdx, setDropIdx] = useState<number | null>(null);
  const dropRef = useRef<number | null>(null);
  // ancla del ⇧+clic (el último clic sin modificadores)
  const anclaRef = useRef<number>(0);

  function marcar(s: Set<number>) {
    setSeleccion(s);
  }

  function onClickThumb(e: React.MouseEvent, i: number) {
    if (e.metaKey || e.ctrlKey) {
      const s = new Set(seleccion);
      if (s.has(i)) s.delete(i);
      else s.add(i);
      anclaRef.current = i;
      marcar(s);
      return;
    }
    if (e.shiftKey) {
      const desde = Math.min(anclaRef.current, i);
      const hasta = Math.max(anclaRef.current, i);
      const s = new Set(seleccion);
      for (let j = desde; j <= hasta; j++) s.add(j);
      marcar(s);
      return;
    }
    anclaRef.current = i;
    // en Acrobat el clic simple hace las dos cosas: llevar a la página y
    // dejarla seleccionada, que es de donde salen las acciones en lote
    marcar(new Set([i]));
    gotoPage(i);
  }

  function onKeyDownPanel(e: React.KeyboardEvent<HTMLDivElement>) {
    if ((e.metaKey || e.ctrlKey) && (e.key === "a" || e.key === "A")) {
      // ⌘A dentro del panel selecciona todas las páginas, no el texto
      e.preventDefault();
      e.stopPropagation();
      marcar(new Set(Array.from({ length: pageCount }, (_, i) => i)));
    } else if (e.key === "Escape" && seleccion.size > 0) {
      e.stopPropagation();
      marcar(new Set());
    }
  }

  function onKeyDownThumb(e: React.KeyboardEvent<HTMLDivElement>, i: number) {
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      const filas = Array.from(
        e.currentTarget.parentElement?.querySelectorAll<HTMLElement>(".thumb") ??
          [],
      );
      const j = filas.indexOf(e.currentTarget);
      const delta = e.key === "ArrowDown" ? 1 : -1;
      filas[Math.max(0, Math.min(j + delta, filas.length - 1))]?.focus();
    } else if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      gotoPage(i);
    }
  }

  return (
    <div
      className="paginas-panel"
      role="listbox"
      aria-multiselectable
      aria-label="Páginas del documento"
      onKeyDown={onKeyDownPanel}
    >
      {seleccion.size === 0 && pageCount > 1 && (
        <p className="opt-hint paginas-pista">
          Clic selecciona la página · ⌘ o ⇧ para varias
        </p>
      )}
      {seleccion.size > 0 && (
        <div className="sel-bar">
          <span className="dato">
            {plural(seleccion.size, "página seleccionada", "páginas seleccionadas")}
          </span>
          <div className="sel-bar-acciones">
            <button
              className="btn btn-icon"
              title="Girar 90° a la derecha"
              aria-label="Girar las páginas seleccionadas 90 grados a la derecha"
              onClick={() => girarLote(1)}
            >
              <Icon name="rotate" size={13} />
            </button>
            <button
              className="btn btn-icon espejo"
              title="Girar 90° a la izquierda"
              aria-label="Girar las páginas seleccionadas 90 grados a la izquierda"
              onClick={() => girarLote(-1)}
            >
              <Icon name="rotate" size={13} />
            </button>
            <button
              className="btn btn-icon"
              title="Extraer las páginas seleccionadas"
              aria-label="Extraer las páginas seleccionadas"
              onClick={extraerLote}
            >
              <Icon name="extract" size={13} />
            </button>
            <button
              className="btn btn-icon"
              title="Eliminar las páginas seleccionadas"
              aria-label="Eliminar las páginas seleccionadas"
              disabled={seleccion.size >= pageCount}
              onClick={eliminarLote}
            >
              <Icon name="trash" size={13} />
            </button>
          </div>
        </div>
      )}
      {thumbs.map((src, i) => (
        <div
          key={i}
          className={`thumb${i === pageIndex ? " active" : ""}${
            seleccion.has(i) ? " marcada" : ""
          }${dropIdx === i ? " drop-antes" : ""}${
            dropIdx === i + 1 ? " drop-despues" : ""
          }`}
          role="option"
          aria-selected={seleccion.has(i)}
          aria-label={`Página ${i + 1} de ${pageCount}`}
          tabIndex={i === pageIndex ? 0 : -1}
          draggable
          onDragStart={(e) => {
            arrastreRef.current = i;
            e.dataTransfer.effectAllowed = "move";
          }}
          onDragOver={(e) => {
            if (arrastreRef.current === null) return;
            e.preventDefault();
            const r = e.currentTarget.getBoundingClientRect();
            const destino = e.clientY < r.top + r.height / 2 ? i : i + 1;
            dropRef.current = destino;
            setDropIdx(destino);
          }}
          onDrop={(e) => {
            e.preventDefault();
            const desde = arrastreRef.current;
            const hueco = dropRef.current;
            arrastreRef.current = null;
            dropRef.current = null;
            setDropIdx(null);
            if (desde === null || hueco === null) return;
            const hasta = hueco > desde ? hueco - 1 : hueco;
            if (hasta !== desde) movePage(desde, hasta);
          }}
          onDragEnd={() => {
            arrastreRef.current = null;
            dropRef.current = null;
            setDropIdx(null);
          }}
          onClick={(e) => onClickThumb(e, i)}
          onKeyDown={(e) => onKeyDownThumb(e, i)}
        >
          {src ? (
            <img
              src={src}
              draggable={false}
              decoding="async"
              alt={`Página ${i + 1}`}
            />
          ) : (
            <div className="thumb-placeholder" />
          )}
          <span className="thumb-num">{i + 1}</span>
          <div className="thumb-actions">
            <button
              title="Subir"
              aria-label={`Subir la página ${i + 1}`}
              disabled={i === 0}
              onClick={(e) => {
                e.stopPropagation();
                movePage(i, i - 1);
              }}
            >
              <Icon name="up" size={13} />
            </button>
            <button
              title="Bajar"
              aria-label={`Bajar la página ${i + 1}`}
              disabled={i === pageCount - 1}
              onClick={(e) => {
                e.stopPropagation();
                movePage(i, i + 1);
              }}
            >
              <Icon name="down" size={13} />
            </button>
            <button
              title="Rotar la página (cambia el documento)"
              aria-label={`Rotar la página ${i + 1} (cambia el documento)`}
              onClick={(e) => {
                e.stopPropagation();
                rotatePage(i);
              }}
            >
              <Icon name="rotate" size={13} />
            </button>
            <button
              title="Duplicar página"
              aria-label={`Duplicar la página ${i + 1}`}
              onClick={(e) => {
                e.stopPropagation();
                duplicatePageAt(i);
              }}
            >
              <Icon name="copy" size={13} />
            </button>
            <button
              title="Página en blanco después"
              aria-label={`Insertar una página en blanco después de la ${i + 1}`}
              onClick={(e) => {
                e.stopPropagation();
                blankPageAfter(i);
              }}
            >
              <Icon name="plus" size={13} />
            </button>
            <button
              title="Eliminar página"
              aria-label={`Eliminar la página ${i + 1}`}
              disabled={pageCount <= 1}
              onClick={(e) => {
                e.stopPropagation();
                deletePage(i);
              }}
            >
              <Icon name="trash" size={13} />
            </button>
          </div>
        </div>
      ))}
    </div>
  );
}
