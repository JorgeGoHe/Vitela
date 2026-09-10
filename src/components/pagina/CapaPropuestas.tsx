/**
 * Capa de «Reconocer campos…»: los campos que la detección **propone**,
 * pintados sobre la página con el borde del acento y su nombre. Nada de
 * esto está escrito todavía en el PDF: cada propuesta se puede renombrar
 * (doble clic) y quitar (Supr) antes de crear nada, que es la regla de la
 * función —una heurística no acierta siempre y no puede fingir que sí—.
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
}: {
  /** Las de esta página, con su posición en la lista entera. */
  propuestas: { i: number; campo: CampoPropuesto }[];
  /** La que se está revisando una a una, si hay alguna. */
  actual: number | null;
  size: PageSize;
  scale: number;
  onQuitar: (i: number) => void;
  onRenombrar: (i: number, nombre: string) => void;
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
            }; doble clic para renombrar, Supr para quitar`}
            title={`${TIPOS[campo.kind] ?? campo.kind}${
              dudoso ? " · no está claro, repásalo" : ""
            } · doble clic renombra · Supr lo quita`}
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
              }
            }}
          >
            {editando?.i === i ? (
              <input
                autoFocus
                className="campo-propuesto-input"
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
              <span className="campo-propuesto-nombre dato">{campo.name}</span>
            )}
          </div>
        );
      })}
    </>
  );
}
