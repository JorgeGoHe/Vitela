/**
 * Capa de «Reconocer campos…»: los campos que la detección **propone**,
 * pintados sobre la página con el borde del acento y su nombre. Nada de
 * esto está escrito todavía en el PDF: cada propuesta se puede renombrar
 * (doble clic), cambiar de tipo —texto o casilla, con la tecla T o el
 * conmutador de la etiqueta— y quitar (Supr) antes de crear nada, que es la
 * regla de la función —una heurística no acierta siempre y no puede fingir
 * que sí—.
 */
import { useState } from "react";
import type { PageSize } from "../../tipos";
import { rectAVista } from "../../hooks/pagina/geometria";
import type { CampoPropuesto } from "../../api";

/** Debajo de esto la propuesta se marca como dudosa y se dice arriba. */
export const CONFIANZA_MINIMA = 0.6;

/** El nombre del tipo, en español, para el tooltip. */
const TIPOS: Record<string, string> = {
  text: "campo de texto",
  checkbox: "casilla",
  radio: "opción",
  combo: "desplegable",
  list: "lista",
};

export default function CapaPropuestas({
  propuestas,
  actual,
  size,
  scale,
  onQuitar,
  onRenombrar,
  onCambiarTipo,
}: {
  /** Las de esta página, con su posición en la lista entera. */
  propuestas: { i: number; campo: CampoPropuesto }[];
  /** La que se está revisando una a una, si hay alguna. */
  actual: number | null;
  size: PageSize;
  scale: number;
  onQuitar: (i: number) => void;
  onRenombrar: (i: number, nombre: string) => void;
  /** Conmuta el tipo entre campo de texto y casilla, con el rect intacto. */
  onCambiarTipo: (i: number) => void;
}) {
  const [editando, setEditando] = useState<{ i: number; texto: string } | null>(
    null,
  );

  if (propuestas.length === 0) return null;

  return (
    <>
      {propuestas.map(({ i, campo }) => {
        const r = rectAVista(campo.rect, size);
        const dudoso = campo.confianza < CONFIANZA_MINIMA;
        return (
          <div
            key={i}
            className={`campo-propuesto${actual === i ? " actual" : ""}${
              dudoso ? " duda" : ""
            }`}
            style={{
              left: r.x * scale,
              top: r.y * scale,
              width: r.w * scale,
              height: r.h * scale,
            }}
            tabIndex={0}
            role="button"
            aria-label={`${campo.name}, ${TIPOS[campo.kind] ?? campo.kind}${
              dudoso ? ", sin confirmar" : ""
            }; doble clic para renombrar, T para cambiar el tipo, Supr para quitar`}
            title={`${TIPOS[campo.kind] ?? campo.kind}${
              dudoso ? " · no está claro, repásalo" : ""
            } · doble clic renombra · T cambia el tipo · Supr lo quita`}
            onMouseDown={(e) => e.stopPropagation()}
            onDoubleClick={() => setEditando({ i, texto: campo.name })}
            onKeyDown={(e) => {
              if (e.key === "Delete" || e.key === "Backspace") {
                e.preventDefault();
                e.stopPropagation();
                onQuitar(i);
              } else if (e.key === "F2" || e.key === "Enter") {
                e.preventDefault();
                setEditando({ i, texto: campo.name });
              } else if (e.key === "t" || e.key === "T") {
                // texto ↔ casilla: la heurística confunde una casilla con
                // una raya de escribir, y volver a dibujar el campo cuesta
                // más que corregirle el tipo
                e.preventDefault();
                e.stopPropagation();
                onCambiarTipo(i);
              }
            }}
          >
            <div className="campo-propuesto-etiqueta">
              {editando?.i === i ? (
                <input
                  autoFocus
                  className="campo-propuesto-input"
                  aria-label={`Nombre del campo propuesto (${campo.name})`}
                  value={editando.texto}
                  onChange={(e) => setEditando({ i, texto: e.target.value })}
                  onMouseDown={(e) => e.stopPropagation()}
                  onKeyDown={(e) => {
                    e.stopPropagation();
                    if (e.key === "Enter") {
                      if (editando.texto.trim())
                        onRenombrar(i, editando.texto.trim());
                      setEditando(null);
                    } else if (e.key === "Escape") setEditando(null);
                  }}
                  onBlur={() => setEditando(null)}
                />
              ) : (
                <>
                  <span className="campo-propuesto-nombre dato">
                    {campo.name}
                  </span>
                  {(campo.kind === "text" || campo.kind === "checkbox") && (
                    <button
                      className="campo-propuesto-tipo dato"
                      title="Cambiar el tipo del campo (T)"
                      aria-label={`Tipo: ${TIPOS[campo.kind]}. Cambiar a ${
                        campo.kind === "text" ? "casilla" : "campo de texto"
                      }`}
                      onMouseDown={(e) => e.stopPropagation()}
                      onClick={(e) => {
                        e.stopPropagation();
                        onCambiarTipo(i);
                      }}
                    >
                      {campo.kind === "text" ? "Texto" : "Casilla"}
                    </button>
                  )}
                </>
              )}
            </div>
          </div>
        );
      })}
    </>
  );
}
