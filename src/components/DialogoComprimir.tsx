import { useModal } from "../hooks/useModal";
import type { CategoriaPeso } from "../api";
import { plural, tamanoFichero, type OpcionesComprimir } from "../tipos";

/** Las nueve categorías de la auditoría en español. Lo que llega del
 *  backend es su **clave** (`lo_demas`, `marcadores_y_enlaces`…), un
 *  identificador con guion bajo y sin tildes que no se puede enseñar. En
 *  cuanto el comando devuelva su propia `etiqueta`, manda esa. */
const CATEGORIA_LLANA: Record<string, string> = {
  imagenes: "Imágenes",
  fuentes: "Fuentes incrustadas",
  contenido: "Contenido de página",
  anotaciones: "Comentarios y campos",
  adjuntos: "Adjuntos",
  marcadores_y_enlaces: "Marcadores y enlaces",
  metadatos: "Metadatos",
  estructura: "Estructura del documento",
  lo_demas: "Lo demás",
};

/** El nombre que se enseña de una fila de la auditoría. */
function nombreCategoria(c: CategoriaPeso): string {
  return c.etiqueta || CATEGORIA_LLANA[c.categoria] || c.categoria;
}

/** Qué categoría de la auditoría se lleva cada casilla, para poder decir el
 *  ahorro **antes** de pulsar. Se busca por palabra sobre la clave, que es
 *  lo estable: la etiqueta la escribe el backend y puede crecer. */
const CATEGORIA_DE: Record<keyof OpcionesComprimir, string[]> = {
  quitarAdjuntos: ["adjunt"],
  quitarMetadatos: ["metadat", "xmp"],
  aplanarFormularios: ["formulario", "anotacion"],
};

/** Los bytes que ocupa lo que se lleva una casilla, según la auditoría. */
function bytesDe(
  auditoria: CategoriaPeso[] | null,
  clave: keyof OpcionesComprimir,
): number | null {
  if (!auditoria) return null;
  const claves = CATEGORIA_DE[clave];
  const suma = auditoria
    .filter((c) =>
      claves.some((k) =>
        c.categoria
          .toLowerCase()
          .normalize("NFD")
          .replace(/[̀-ͯ]/g, "")
          .includes(k),
      ),
    )
    .reduce((n, c) => n + c.bytes, 0);
  return suma;
}

/**
 * Reducir el tamaño del PDF, con **la auditoría delante**: una barra y una
 * tabla que dicen de qué está hecho el fichero antes de tocar nada. Sin
 * eso, «reducir tamaño» es una ruleta; con eso contesta la pregunta que se
 * hace de verdad, que es por qué pesa 40 MB.
 */
export default function DialogoComprimir({
  quality,
  setQuality,
  dpi,
  setDpi,
  auditoria,
  opciones,
  setOpciones,
  onConfirm,
  onClose,
}: {
  quality: number;
  setQuality: (q: number) => void;
  dpi: number;
  setDpi: (d: number) => void;
  /** De qué está hecho el fichero; `null` mientras se está mirando. */
  auditoria: CategoriaPeso[] | null;
  opciones: OpcionesComprimir;
  setOpciones: (o: OpcionesComprimir) => void;
  onConfirm: () => void;
  onClose: () => void;
}) {
  const { ref, onKeyDown } = useModal({ onClose, onConfirm });
  const total = (auditoria ?? []).reduce((n, c) => n + c.bytes, 0);
  // de mayor a menor: la primera fila es la respuesta a «por qué pesa esto»
  const filas = [...(auditoria ?? [])].sort((a, b) => b.bytes - a.bytes);

  /** El ahorro estimado de una casilla, dicho como estimación. */
  function ahorro(clave: keyof OpcionesComprimir): string {
    const bytes = bytesDe(auditoria, clave);
    if (bytes === null || bytes === 0) return "";
    return ` · ≈ ${tamanoFichero(bytes)}`;
  }

  function cambia(clave: keyof OpcionesComprimir) {
    setOpciones({ ...opciones, [clave]: !opciones[clave] });
  }

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div
        className="modal modal-hf"
        role="dialog"
        aria-modal="true"
        aria-label="Reducir tamaño del PDF"
        onClick={(e) => e.stopPropagation()}
        onKeyDown={onKeyDown}
        ref={ref}
        tabIndex={-1}
      >
        <h3>Reducir tamaño del PDF</h3>

        <span className="card-label">De qué está hecho</span>
        {auditoria === null ? (
          <p className="modal-file">Mirando el fichero…</p>
        ) : (
          <>
            <div
              className="peso-barra"
              role="img"
              aria-label={`Reparto del tamaño: ${filas
                .map(
                  (c) => `${nombreCategoria(c)} ${Math.round(c.porcentaje)} %`,
                )
                .join(", ")}`}
            >
              {filas.map((c, i) => (
                <span
                  key={c.categoria}
                  className="peso-tramo"
                  title={`${nombreCategoria(c)} · ${tamanoFichero(c.bytes)}`}
                  style={{
                    width: `${c.porcentaje}%`,
                    // un solo acento con la tinta más o menos diluida: la
                    // paleta de la casa no tiene nueve colores que gastar
                    background: `color-mix(in srgb, var(--accent) ${Math.max(
                      12,
                      92 - i * 12,
                    )}%, var(--fg-muted))`,
                  }}
                />
              ))}
            </div>
            <ul className="prop-ficha peso-tabla">
              {filas.map((c) => (
                <li key={c.categoria}>
                  <span className="peso-nombre">{nombreCategoria(c)}</span>
                  <span className="dato">{tamanoFichero(c.bytes)}</span>
                  <span className="dato peso-pct">
                    {Math.round(c.porcentaje)} %
                  </span>
                </li>
              ))}
              {filas.length === 0 && <li>No se ha podido repartir el tamaño.</li>}
            </ul>
            {total > 0 && (
              <span className="dato">
                {tamanoFichero(total)} en {plural(filas.length, "categoría", "categorías")}
              </span>
            )}
          </>
        )}

        <span className="card-label">Qué se hace</span>
        <div className="card-row">
          <select
            className="size-select"
            title="Calidad JPEG"
            aria-label="Calidad de las imágenes"
            value={quality}
            onChange={(e) => setQuality(Number(e.target.value))}
          >
            <option value={60}>Calidad baja (más pequeño)</option>
            <option value={75}>Calidad media</option>
            <option value={85}>Calidad alta</option>
          </select>
          <select
            className="size-select"
            title="Resolución máxima"
            aria-label="Resolución máxima de las imágenes"
            value={dpi}
            onChange={(e) => setDpi(Number(e.target.value))}
          >
            {[110, 150, 200, 300].map((d) => (
              <option key={d} value={d}>
                Submuestrear a {d} ppp
              </option>
            ))}
          </select>
        </div>
        <label className="opt-check">
          <input
            type="checkbox"
            checked={opciones.quitarAdjuntos}
            onChange={() => cambia("quitarAdjuntos")}
          />
          Descartar los ficheros adjuntos{ahorro("quitarAdjuntos")}
        </label>
        <label className="opt-check">
          <input
            type="checkbox"
            checked={opciones.quitarMetadatos}
            onChange={() => cambia("quitarMetadatos")}
          />
          Descartar los metadatos privados{ahorro("quitarMetadatos")}
        </label>
        <label className="opt-check">
          <input
            type="checkbox"
            checked={opciones.aplanarFormularios}
            onChange={() => cambia("aplanarFormularios")}
          />
          Aplanar los formularios{ahorro("aplanarFormularios")}
        </label>
        <p className="modal-file" style={{ whiteSpace: "normal" }}>
          Recomprime las imágenes del documento (las que tienen transparencia
          se conservan tal cual). El texto no se toca, y ⌘Z lo devuelve todo.
        </p>
        <div className="card-actions">
          <button className="btn" onClick={onClose}>
            Cancelar
          </button>
          <button className="btn btn-primary" onClick={onConfirm}>
            Reducir
          </button>
        </div>
      </div>
    </div>
  );
}
