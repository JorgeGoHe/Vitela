import { useState } from "react";
import type { DocumentInfo, Metadata, VistaInicial } from "../api";
import { plural, tamanoFichero } from "../tipos";
import { useModal } from "../hooks/useModal";

/** Las opciones de zoom de arranque, con el nombre que usa la píldora. */
const ZOOMS: [string, string][] = [
  ["defecto", "La del visor (predeterminado)"],
  ["pagina", "La página entera"],
  ["ancho", "El ancho de la página"],
  ["100", "Tamaño real (100 %)"],
];

/** Las cuatro disposiciones de Acrobat, más «la del visor». */
const DISPOSICIONES: [string, string][] = [
  ["defecto", "La del visor (predeterminada)"],
  ["una", "Una sola página"],
  ["continuo", "Continua"],
  ["dos", "Dos páginas"],
  ["dos-continuo", "Dos páginas, continua"],
];

/** Los puntos de un PDF son 1/72 de pulgada: en milímetros se reconoce el
 *  A4 y en pulgadas la carta, así que se dan los dos. */
function tamanoPagina(w: number, h: number): string {
  const mm = (v: number) => Math.round((v / 72) * 25.4);
  return `${mm(w)} × ${mm(h)} mm (${Math.round(w)} × ${Math.round(h)} pt)`;
}

/** Qué deja hacer la protección, en llano y sin hablar de bits. */
function resumenPermisos(f: DocumentInfo): string {
  if (!f.cifrado) return "sin contraseña, todo permitido";
  const si = [
    f.permisos.imprimir ? "imprimir" : null,
    f.permisos.copiar ? "copiar texto" : null,
    f.permisos.editar ? "editar y comentar" : null,
  ].filter(Boolean);
  return si.length === 0
    ? "con contraseña · no permite ni imprimir, ni copiar, ni editar"
    : `con contraseña · permite ${si.join(", ")}`;
}

/** Propiedades del documento: los metadatos del diccionario `/Info`, que se
 *  escriben, y la ficha del fichero —tamaño, versión, fuentes, seguridad—,
 *  que solo se lee, más la **vista inicial**, que también se escribe. Es lo
 *  que Acrobat reparte en cuatro pestañas, aquí en una sola columna: son
 *  veinte líneas, no hacen falta pestañas. */
export default function DialogoPropiedades({
  initial,
  ficha,
  vista,
  pageCount,
  onSave,
  onClose,
}: {
  initial: Metadata;
  /** La ficha de solo lectura; puede no haber llegado todavía. */
  ficha: DocumentInfo | null;
  /** Con qué cara se abre el documento; puede no haber llegado todavía. */
  vista: VistaInicial | null;
  pageCount: number;
  onSave: (meta: Metadata, vista: VistaInicial | null) => void;
  onClose: () => void;
}) {
  const [meta, setMeta] = useState<Metadata>(initial);
  const [v, setV] = useState<VistaInicial | null>(vista);
  // la vista llega después que los metadatos (dos viajes distintos): en
  // cuanto está, se recoge sin pisar lo que el usuario ya haya tocado
  const [vistaVista, setVistaVista] = useState(vista);
  if (vista !== vistaVista) {
    setVistaVista(vista);
    setV(vista);
  }
  const { ref, onKeyDown } = useModal({
    onClose,
    onConfirm: () => onSave(meta, v),
  });

  function campo(key: keyof Metadata, label: string) {
    return (
      <label className="prop-field">
        <span className="card-label">{label}</span>
        <input
          type="text"
          value={meta[key]}
          onChange={(e) => setMeta({ ...meta, [key]: e.target.value })}
        />
      </label>
    );
  }

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div
        className="modal"
        role="dialog"
        aria-modal="true"
        aria-label="Propiedades del documento"
        onClick={(e) => e.stopPropagation()}
        onKeyDown={onKeyDown}
        ref={ref}
        tabIndex={-1}
      >
        <h3>Propiedades del documento</h3>
        {campo("title", "Título")}
        {campo("author", "Autor")}
        {campo("subject", "Asunto")}
        {campo("keywords", "Palabras clave")}
        {(initial.creator || initial.producer) && (
          <p className="modal-file">
            {initial.creator && `Creado con: ${initial.creator}`}
            {initial.creator && initial.producer && " · "}
            {initial.producer && `Generador: ${initial.producer}`}
          </p>
        )}
        {v && (
          <>
            <span className="card-label">Vista inicial</span>
            <div className="fila-campos">
              <label className="prop-field">
                <span className="card-label">Abrir por la página</span>
                <input
                  type="number"
                  className="stamp-input"
                  style={{ width: 80 }}
                  min={1}
                  max={Math.max(1, pageCount)}
                  value={v.page_index + 1}
                  onChange={(e) =>
                    setV({
                      ...v,
                      page_index: Math.min(
                        Math.max(Number(e.target.value) - 1, 0),
                        Math.max(0, pageCount - 1),
                      ),
                    })
                  }
                />
              </label>
              <label className="prop-field">
                <span className="card-label">Zoom</span>
                <select
                  className="size-select"
                  value={v.zoom}
                  onChange={(e) => setV({ ...v, zoom: e.target.value })}
                >
                  {ZOOMS.map(([valor, etiqueta]) => (
                    <option key={valor} value={valor}>
                      {etiqueta}
                    </option>
                  ))}
                </select>
              </label>
            </div>
            <label className="prop-field">
              <span className="card-label">Disposición de las páginas</span>
              <select
                className="size-select"
                value={v.disposicion}
                onChange={(e) => setV({ ...v, disposicion: e.target.value })}
              >
                {DISPOSICIONES.map(([valor, etiqueta]) => (
                  <option key={valor} value={valor}>
                    {etiqueta}
                  </option>
                ))}
              </select>
            </label>
            <label className="opt-check">
              <input
                type="checkbox"
                checked={v.marcadores}
                onChange={(e) => setV({ ...v, marcadores: e.target.checked })}
              />
              Abrir con el panel de marcadores a la vista
            </label>
            <p className="modal-file" style={{ whiteSpace: "normal" }}>
              Es con lo que se encuentra quien abra el documento, aquí y en
              cualquier otro visor.
            </p>
          </>
        )}
        {ficha && (
          <>
            <span className="card-label">El fichero</span>
            <ul className="prop-ficha">
              <li>
                <span className="dato">{tamanoFichero(ficha.bytes)}</span> ·{" "}
                <span className="dato">
                  {plural(ficha.page_count, "página", "páginas")}
                </span>{" "}
                · PDF <span className="dato">{ficha.version}</span>
              </li>
              <li>
                Tamaño de página:{" "}
                <span className="dato">
                  {tamanoPagina(ficha.page_width, ficha.page_height)}
                </span>
              </li>
              <li>
                Formulario: {ficha.formulario ? "sí, se puede rellenar" : "no"}
              </li>
              <li>Seguridad: {resumenPermisos(ficha)}</li>
            </ul>
            <span className="card-label">
              {ficha.fuentes.length === 0
                ? "Fuentes"
                : plural(ficha.fuentes.length, "fuente", "fuentes")}
            </span>
            {ficha.fuentes.length === 0 ? (
              <p className="modal-file">
                Este documento no usa ninguna fuente (solo imágenes o dibujo).
              </p>
            ) : (
              <ul className="prop-ficha prop-fuentes">
                {ficha.fuentes.map((f) => (
                  <li key={`${f.nombre}-${f.tipo}`}>
                    <span className="dato">{f.nombre}</span> · {f.tipo} ·{" "}
                    {/* una fuente no incrustada la pone el sistema de quien
                        abra el PDF, y ahí es donde cambia la maquetación */}
                    {f.incrustada ? "incrustada" : "no incrustada"}
                  </li>
                ))}
              </ul>
            )}
          </>
        )}
        <div className="card-actions">
          <button className="btn" onClick={onClose}>
            Cancelar
          </button>
          <button className="btn btn-primary" onClick={() => onSave(meta, v)}>
            Guardar
          </button>
        </div>
      </div>
    </div>
  );
}
