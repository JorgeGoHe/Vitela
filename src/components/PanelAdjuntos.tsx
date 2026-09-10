import { useRef } from "react";
import type { Adjunto } from "../api";
import { MOD, tamanoFichero } from "../tipos";
import Icon from "./Icon";

/**
 * Pestaña «Adjuntos» del sidebar: los ficheros que el PDF lleva dentro
 * (`/EmbeddedFiles`), con su tamaño y su fecha.
 *
 * «Abrir» es la acción principal de la fila, como en Acrobat: un adjunto de
 * una factura es un XML que se quiere **ver**, no guardar. Borrar va además
 * en Supr, con la confirmación que usa el resto de la app; ⌘Z lo devuelve.
 */
export default function PanelAdjuntos({
  adjuntos,
  onAbrir,
  onGuardar,
  onBorrar,
  onAnadir,
}: {
  adjuntos: Adjunto[];
  /** Lo saca a un temporal y lo abre con el visor del sistema. */
  onAbrir: (index: number, a: Adjunto) => void;
  onGuardar: (index: number, a: Adjunto) => void;
  /** Pregunta antes de quitarlo: es trabajo del usuario. */
  onBorrar: (index: number, a: Adjunto) => void;
  onAnadir: () => void;
}) {
  const panelRef = useRef<HTMLDivElement | null>(null);

  const filas = () =>
    Array.from(panelRef.current?.querySelectorAll<HTMLElement>(".adj-row") ?? []);

  /** ↑/↓ recorren la lista, Enter abre y Supr quita (preguntando). */
  function onKeyDown(e: React.KeyboardEvent<HTMLDivElement>, i: number, a: Adjunto) {
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      const f = filas();
      const j = f.indexOf(e.currentTarget);
      f[Math.max(0, Math.min(j + (e.key === "ArrowDown" ? 1 : -1), f.length - 1))]?.focus();
    } else if (e.key === "Home" || e.key === "End") {
      e.preventDefault();
      const f = filas();
      (e.key === "Home" ? f[0] : f[f.length - 1])?.focus();
    } else if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      onAbrir(i, a);
    } else if (e.key === "Delete" || e.key === "Backspace") {
      // la tecla es de la lista: si siguiera, la app borraría además la
      // anotación seleccionada en la página
      e.preventDefault();
      e.stopPropagation();
      onBorrar(i, a);
    }
  }

  return (
    <div className="com-panel" ref={panelRef}>
      <span className="com-total dato">
        {adjuntos.length === 1 ? "1 adjunto" : `${adjuntos.length} adjuntos`}
      </span>
      {adjuntos.length === 0 && (
        <p className="sign-empty">Este documento no lleva ningún fichero.</p>
      )}
      {adjuntos.length > 0 && (
        <p className="opt-hint com-pista">↑ ↓ recorren · Enter abre · Supr quita</p>
      )}
      {adjuntos.map((a, i) => (
        <div
          className="adj-row"
          key={`${a.name}-${i}`}
          role="option"
          aria-selected={false}
          // sin nada elegido, la primera fila es la que recibe el Tab
          tabIndex={i === 0 ? 0 : -1}
          onDoubleClick={() => onAbrir(i, a)}
          onKeyDown={(e) => onKeyDown(e, i, a)}
        >
          <span className="adj-nombre">{a.name}</span>
          <span className="adj-dato dato">
            {tamanoFichero(a.bytes)}
            {a.created ? ` · ${a.created}` : ""}
          </span>
          {a.description && (
            <span className="adj-descripcion">{a.description}</span>
          )}
          <div className="adj-acciones">
            <button
              className="btn adj-abrir"
              title="Abrirlo con el visor del sistema"
              aria-label={`Abrir ${a.name}`}
              onClick={() => onAbrir(i, a)}
            >
              <Icon name="open" size={13} />
              Abrir
            </button>
            <button
              className="btn"
              aria-label={`Guardar ${a.name} en el disco`}
              onClick={() => onGuardar(i, a)}
            >
              <Icon name="save" size={13} />
              Guardar como…
            </button>
            <button
              className="btn btn-icon adj-quitar"
              title={`Quitar el adjunto (${MOD}Z lo devuelve)`}
              aria-label={`Quitar ${a.name} del documento`}
              onClick={() => onBorrar(i, a)}
            >
              <Icon name="trash" size={13} />
            </button>
          </div>
        </div>
      ))}
      <button className="btn" onClick={onAnadir}>
        <Icon name="merge" size={13} />
        Añadir…
      </button>
    </div>
  );
}
