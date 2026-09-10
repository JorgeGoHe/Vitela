import { useState } from "react";
import type { TamanoImagenes } from "../api";
import { open } from "../dialogos";
import { useModal } from "../hooks/useModal";
import { plural } from "../tipos";
import Icon from "./Icon";

/** Nombre y carpeta de una ruta, para distinguir dos ficheros iguales. */
function partesDe(ruta: string): { nombre: string; dir: string } {
  const trozos = ruta.split(/[\\/]/);
  const nombre = trozos.pop() ?? ruta;
  return { nombre, dir: trozos.join("/") };
}

/**
 * «Crear PDF desde imágenes…»: unas fotos o unos escaneos sueltos que hay
 * que mandar como un solo documento, que es el «Crear PDF ▸ Varios
 * archivos» de Acrobat para el caso que se usa de verdad.
 *
 * Tres pasos y ninguno de más: elegir las fotos, elegir el tamaño (A4 por
 * defecto, que es el papel de aquí) y decir dónde se guarda. La lista se
 * ordena con ▲▼ y se poda por fila, porque el orden de la lista es el orden
 * de las páginas.
 */
export default function DialogoImagenes({
  fallos,
  onConfirm,
  onClose,
}: {
  /** Rutas de las imágenes que el backend no ha podido leer en el intento
   *  anterior —todas, o las que se saltó por el camino—: se marcan en su
   *  fila para poder quitarlas y seguir con las demás, en vez de perder la
   *  lista entera. */
  fallos: string[];
  onConfirm: (opts: { rutas: string[]; tamano: TamanoImagenes }) => void;
  onClose: () => void;
}) {
  const [rutas, setRutas] = useState<string[]>([]);
  const [tamano, setTamano] = useState<TamanoImagenes>("a4");
  const confirmar = () => {
    if (rutas.length > 0) onConfirm({ rutas, tamano });
  };
  const { ref, onKeyDown } = useModal({ onClose, onConfirm: confirmar });

  async function anadir() {
    const sel = await open({
      filters: [
        { name: "Imágenes", extensions: ["png", "jpg", "jpeg"] },
        { name: "PNG", extensions: ["png"] },
        { name: "JPEG", extensions: ["jpg", "jpeg"] },
      ],
      multiple: true,
      title: "Imágenes del PDF",
    });
    const nuevas =
      typeof sel === "string" ? [sel] : Array.isArray(sel) ? sel : [];
    if (nuevas.length > 0) setRutas((v) => [...v, ...nuevas]);
  }

  /** Subir y bajar una fila con el teclado, como en «Combinar ficheros». */
  function intercambia(i: number, delta: number) {
    const j = i + delta;
    if (j < 0 || j >= rutas.length) return;
    setRutas((v) => {
      const copia = [...v];
      [copia[i], copia[j]] = [copia[j], copia[i]];
      return copia;
    });
  }

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div
        className="modal modal-hf"
        role="dialog"
        aria-modal="true"
        aria-label="Crear PDF desde imágenes"
        onClick={(e) => e.stopPropagation()}
        onKeyDown={onKeyDown}
        ref={ref}
        tabIndex={-1}
      >
        <h3>Crear PDF desde imágenes</h3>
        {rutas.length === 0 && (
          <p className="sign-empty">
            Una imagen por página, en el orden de la lista. El documento que
            tengas abierto no se toca: sale un fichero nuevo.
          </p>
        )}
        {rutas.length > 0 && (
          <div className="combinar-lista">
            {rutas.map((r, i) => {
              const { nombre, dir } = partesDe(r);
              const mala = fallos.includes(r);
              return (
                <div key={`${r}-${i}`} className="combinar-fila">
                  <span className="dato combinar-orden">{i + 1}</span>
                  <span className="combinar-nombre">
                    <span>{nombre}</span>
                    <span className="reciente-dir">{dir}</span>
                    {mala && (
                      <span className="dato combinar-info mal">
                        no se ha podido leer: quítala y vuelve a intentarlo
                      </span>
                    )}
                  </span>
                  <button
                    className="btn btn-icon"
                    title="Subir"
                    aria-label={`Subir ${nombre}`}
                    disabled={i === 0}
                    onClick={() => intercambia(i, -1)}
                  >
                    <Icon name="up" size={13} />
                  </button>
                  <button
                    className="btn btn-icon"
                    title="Bajar"
                    aria-label={`Bajar ${nombre}`}
                    disabled={i === rutas.length - 1}
                    onClick={() => intercambia(i, 1)}
                  >
                    <Icon name="down" size={13} />
                  </button>
                  <button
                    className="btn"
                    aria-label={`Quitar ${nombre} de la lista`}
                    onClick={() => setRutas((v) => v.filter((_, j) => j !== i))}
                  >
                    Quitar
                  </button>
                </div>
              );
            })}
          </div>
        )}
        <div className="card-actions" style={{ justifyContent: "flex-start" }}>
          <button className="btn" onClick={anadir}>
            <Icon name="image" size={14} />
            Elegir imágenes…
          </button>
          {rutas.length > 0 && (
            <span className="dato">
              {plural(rutas.length, "imagen", "imágenes")} ·{" "}
              {plural(rutas.length, "página", "páginas")}
            </span>
          )}
        </div>
        <span className="card-label">Tamaño de página</span>
        <select
          className="size-select"
          aria-label="Tamaño de página"
          value={tamano}
          onChange={(e) => setTamano(e.target.value as TamanoImagenes)}
        >
          <option value="a4">A4, la foto centrada con margen</option>
          <option value="carta">Carta, la foto centrada con margen</option>
          <option value="imagen">El de cada imagen, sin margen</option>
        </select>
        <div className="card-actions">
          <button className="btn" onClick={onClose}>
            Cancelar
          </button>
          <button
            className="btn btn-primary"
            disabled={rutas.length === 0}
            title={
              rutas.length === 0 ? "Elige antes alguna imagen" : undefined
            }
            onClick={confirmar}
          >
            Elegir dónde y crear…
          </button>
        </div>
      </div>
    </div>
  );
}
