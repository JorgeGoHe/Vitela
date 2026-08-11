import { useState } from "react";
import type { Metadata } from "../api";

/** Propiedades del documento (metadatos del diccionario /Info). */
export default function DialogoPropiedades({
  initial,
  onSave,
  onClose,
}: {
  initial: Metadata;
  onSave: (meta: Metadata) => void;
  onClose: () => void;
}) {
  const [meta, setMeta] = useState<Metadata>(initial);

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
      <div className="modal" onClick={(e) => e.stopPropagation()}>
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
