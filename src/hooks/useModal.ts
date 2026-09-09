import { useCallback, useEffect, useRef } from "react";

/** Controles que pueden recibir el foco dentro de un diálogo. */
const ENFOCABLES =
  'input:not([type="hidden"]), select, textarea, button, [href], [tabindex]:not([tabindex="-1"])';

/**
 * Comportamiento común de todos los modales (U-3 y la regla «todo diálogo:
 * Esc cierra, Enter confirma, foco inicial y Cancelar»):
 *
 * - Escape cierra (y no deja que el atajo llegue al documento de debajo).
 * - Enter ejecuta la acción principal, salvo dentro de un `textarea` (salto
 *   de línea) o sobre un botón (lo activa el navegador).
 * - Foco inicial en el primer campo; si el diálogo solo tiene botones, en la
 *   acción principal, que es la que confirma con Enter.
 * - Trampa de foco: el tabulador no sale del diálogo mientras está abierto.
 *
 * Se aplica poniendo `ref` y `onKeyDown` en el `.modal`.
 */
export function useModal(opts: {
  onClose: () => void;
  onConfirm?: () => void;
}) {
  const { onClose, onConfirm } = opts;
  const ref = useRef<HTMLDivElement | null>(null);

  const enfocables = useCallback((): HTMLElement[] => {
    const el = ref.current;
    if (!el) return [];
    return Array.from(el.querySelectorAll<HTMLElement>(ENFOCABLES)).filter(
      (n) =>
        !n.hasAttribute("disabled") &&
        n.tabIndex !== -1 &&
        n.getClientRects().length > 0,
    );
  }, []);

  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const lista = enfocables();
    const campo = lista.find((n) =>
      ["INPUT", "SELECT", "TEXTAREA"].includes(n.tagName),
    );
    const principal = el.querySelector<HTMLElement>(".btn-primary, .btn-danger");
    (campo ?? principal ?? lista[0] ?? el).focus();
  }, [enfocables]);

  const onKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (e.key === "Escape") {
        // el modal se cierra y el atajo no sigue hasta la app (donde Esc
        // también sale del modo de herramienta)
        e.stopPropagation();
        onClose();
        return;
      }
      if (e.key === "Tab") {
        const lista = enfocables();
        if (lista.length === 0) return;
        const primero = lista[0];
        const ultimo = lista[lista.length - 1];
        const activo = document.activeElement as HTMLElement | null;
        if (e.shiftKey && (activo === primero || activo === ref.current)) {
          e.preventDefault();
          ultimo.focus();
        } else if (!e.shiftKey && activo === ultimo) {
          e.preventDefault();
          primero.focus();
        }
        return;
      }
      if (e.key === "Enter" && onConfirm) {
        const destino = e.target as HTMLElement;
        // en un textarea Enter es un salto de línea; sobre un botón lo activa
        // el propio navegador (confirmar dos veces borraría dos páginas)
        if (destino.tagName === "TEXTAREA" || destino.tagName === "BUTTON") return;
        if ((e.nativeEvent as KeyboardEvent).isComposing) return;
        e.preventDefault();
        onConfirm();
      }
    },
    [enfocables, onClose, onConfirm],
  );

  return { ref, onKeyDown };
}
