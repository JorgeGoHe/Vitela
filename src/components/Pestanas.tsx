import Icon from "./Icon";

/** Lo que la fila necesita saber de cada documento abierto. */
export type PestanaVista = {
  id: number;
  /** Nombre del fichero, o el provisional de un documento sin ruta. */
  nombre: string;
  /** Ruta completa, para el tooltip. */
  ruta: string | null;
  modificado: boolean;
};

/**
 * Fila de pestañas: **solo aparece con más de un documento**. Con uno, la
 * app se ve exactamente igual que antes de que existieran, que es lo que la
 * separa de un IDE: nada de barra vacía, nada de «+».
 *
 * Cada pestaña lleva el nombre del fichero, su «•» si tiene cambios sin
 * guardar y una «×» al pasar el ratón, como en Acrobat.
 */
export default function Pestanas({
  pestanas,
  activa,
  onElegir,
  onCerrar,
}: {
  pestanas: PestanaVista[];
  activa: number | null;
  onElegir: (id: number) => void;
  onCerrar: (id: number) => void;
}) {
  if (pestanas.length < 2) return null;
  return (
    <div className="pestanas" role="tablist" aria-label="Documentos abiertos">
      {pestanas.map((p) => (
        <div
          key={p.id}
          className={`pestana${p.id === activa ? " on" : ""}`}
          role="tab"
          aria-selected={p.id === activa}
          tabIndex={p.id === activa ? 0 : -1}
          title={p.ruta ?? p.nombre}
          onClick={() => onElegir(p.id)}
          onKeyDown={(e) => {
            if (e.key === "Enter" || e.key === " ") {
              e.preventDefault();
              onElegir(p.id);
            }
          }}
          // el botón central del ratón cierra la pestaña, como en cualquier
          // navegador
          onAuxClick={(e) => {
            if (e.button === 1) onCerrar(p.id);
          }}
        >
          <span className="pestana-nombre">{p.nombre}</span>
          {p.modificado && (
            <span className="pestana-punto" aria-label="con cambios sin guardar">
              •
            </span>
          )}
          <button
            className="pestana-cerrar"
            title={`Cerrar ${p.nombre}`}
            aria-label={`Cerrar ${p.nombre}`}
            onClick={(e) => {
              e.stopPropagation();
              onCerrar(p.id);
            }}
          >
            <Icon name="close" size={11} />
          </button>
        </div>
      ))}
    </div>
  );
}
