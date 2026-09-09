import { useModal } from "../hooks/useModal";
import { ES_MAC } from "../tipos";

/**
 * «⇧⌘S» en Mac y «Ctrl+Shift+S» en Windows y Linux: la misma tecla escrita
 * como la escribe cada sistema. `mods` lleva los modificadores separados
 * por espacios («shift cmd»); vacío para las teclas sueltas (Esc, ←, Supr).
 */
function atajo(tecla: string, mods = "cmd"): string {
  const m = mods.split(" ").filter(Boolean);
  const tiene = (x: string) => m.includes(x);
  if (ES_MAC) {
    return `${tiene("shift") ? "⇧" : ""}${tiene("alt") ? "⌥" : ""}${
      tiene("cmd") ? "⌘" : ""
    }${tecla}`;
  }
  return `${tiene("cmd") ? "Ctrl+" : ""}${tiene("shift") ? "Shift+" : ""}${
    tiene("alt") ? "Alt+" : ""
  }${tecla}`;
}

/** Una fila: la tecla y qué hace. */
type Atajo = [tecla: string, que: string];

const GRUPOS: { titulo: string; atajos: Atajo[] }[] = [
  {
    titulo: "Archivo",
    atajos: [
      [atajo("O"), "Abrir un PDF"],
      [atajo("S"), "Guardar"],
      [atajo("S", "shift cmd"), "Guardar como"],
      [atajo("P"), "Imprimir"],
    ],
  },
  {
    titulo: "Editar",
    atajos: [
      [atajo("Z"), "Deshacer"],
      [atajo("Z", "shift cmd"), "Rehacer"],
      [atajo("C"), "Copiar la selección"],
      [atajo("A"), "Seleccionar el texto de la página"],
      [atajo("F"), "Buscar"],
      [atajo("G"), "Coincidencia siguiente"],
      [atajo("G", "shift cmd"), "Coincidencia anterior"],
      [atajo(","), "Preferencias"],
      [atajo("Supr", ""), "Borrar el comentario seleccionado"],
    ],
  },
  {
    titulo: "Ver",
    atajos: [
      [`${atajo("+")} · ${atajo("−")}`, "Acercar y alejar"],
      [atajo("0"), "Página entera"],
      [atajo("1"), "Tamaño real"],
      [atajo("2"), "Ajustar al ancho"],
      [
        `${atajo("+", "shift cmd")} · ${atajo("−", "shift cmd")}`,
        "Girar la vista (no toca el fichero)",
      ],
      [atajo("1", "alt cmd"), "Panel lateral"],
      [atajo("2", "alt cmd"), "Marcadores"],
      [atajo("3", "alt cmd"), "Comentarios"],
      [atajo("L"), "Pantalla completa (Esc sale)"],
      [atajo("L", "shift cmd"), "Modo nocturno del documento"],
      [`${atajo("←", "alt")} · ${atajo("→", "alt")}`, "Vista anterior y siguiente"],
      [atajo("N", "shift cmd"), "Ir a la página"],
      [`${atajo("←", "")} · ${atajo("→", "")}`, "Página anterior y siguiente"],
      [atajo("Esc", ""), "Quitar las coincidencias, o salir de la herramienta"],
    ],
  },
  {
    titulo: "Documento",
    atajos: [
      [atajo("D"), "Propiedades del documento"],
      [atajo("Enter"), "Confirmar una nota o un cuadro de texto"],
      [atajo("Tab", ""), "Confirmar el campo y saltar al siguiente"],
      [atajo("Tab", "shift"), "Volver al campo anterior"],
    ],
  },
];

/**
 * «Ayuda ▸ Atajos de teclado»: el único sitio donde están escritos todos.
 * Hasta ahora la única forma de descubrirlos era pasar el ratón por cada
 * botón y leer su tooltip.
 */
export default function DialogoAtajos({ onClose }: { onClose: () => void }) {
  const { ref, onKeyDown } = useModal({ onClose });

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div
        className="modal modal-atajos"
        role="dialog"
        aria-modal="true"
        aria-label="Atajos de teclado"
        onClick={(e) => e.stopPropagation()}
        onKeyDown={onKeyDown}
        ref={ref}
        tabIndex={-1}
      >
        <h3>Atajos de teclado</h3>
        {GRUPOS.map((g) => (
          <div className="atajos-grupo" key={g.titulo}>
            <span className="card-label">{g.titulo}</span>
            {g.atajos.map(([tecla, que]) => (
              <div className="atajos-fila" key={`${g.titulo}-${que}`}>
                <span className="dato atajos-tecla">{tecla}</span>
                <span>{que}</span>
              </div>
            ))}
          </div>
        ))}
        <div className="card-actions">
          <button className="btn btn-primary" onClick={onClose}>
            Cerrar
          </button>
        </div>
      </div>
    </div>
  );
}
