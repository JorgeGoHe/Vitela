import { useState } from "react";
import type { DocumentInfo, Metadata } from "../api";
import { plural, tamanoFichero } from "../tipos";
import { useModal } from "../hooks/useModal";

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
 *  que solo se lee. Es lo que Acrobat reparte en cuatro pestañas, aquí en
 *  una sola columna: son quince líneas, no hacen falta pestañas. */
export default function DialogoPropiedades({
  initial,
  ficha,
  onSave,
  onClose,
}: {
  initial: Metadata;
  /** La ficha de solo lectura; puede no haber llegado todavía. */
  ficha: DocumentInfo | null;
  onSave: (meta: Metadata) => void;
  onClose: () => void;
}) {
  const [meta, setMeta] = useState<Metadata>(initial);
  const { ref, onKeyDown } = useModal({
    onClose,
    onConfirm: () => onSave(meta),
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
          <button className="btn btn-primary" onClick={() => onSave(meta)}>
            Guardar
          </button>
        </div>
      </div>
    </div>
  );
}
