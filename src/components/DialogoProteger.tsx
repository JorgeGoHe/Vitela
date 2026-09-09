import type { Permisos } from "../api";
import { useModal } from "../hooks/useModal";

export type ProtegerDraft = { user: string; owner: string } & Permisos;

/**
 * Proteger el documento con contraseña (AES-256). Como en Acrobat, la
 * contraseña de permisos es la que fija qué se puede hacer con el fichero;
 * los tres permisos vienen marcados y solo se pueden restringir si hay
 * contraseña de permisos, porque sin ella no hay nada que los sostenga.
 */
export default function DialogoProteger({
  valor,
  onChange,
  onConfirm,
  onCopia,
  onClose,
}: {
  valor: ProtegerDraft;
  onChange: (v: ProtegerDraft) => void;
  /** Protege el documento abierto. */
  onConfirm: () => void;
  /** Escribe una copia protegida y deja el documento como está. */
  onCopia: () => void;
  onClose: () => void;
}) {
  const { ref, onKeyDown } = useModal({
    onClose,
    onConfirm: () => {
      if (valor.user) onConfirm();
    },
  });
  const conPermisos = valor.owner.trim().length > 0;

  function permiso(
    clave: keyof Permisos,
    etiqueta: string,
  ) {
    return (
      <label className={`opt-check${conPermisos ? "" : " disabled"}`}>
        <input
          type="checkbox"
          checked={valor[clave]}
          disabled={!conPermisos}
          onChange={(e) => onChange({ ...valor, [clave]: e.target.checked })}
        />
        {etiqueta}
      </label>
    );
  }

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div
        className="modal"
        role="dialog"
        aria-modal="true"
        aria-label="Proteger con contraseña"
        onClick={(e) => e.stopPropagation()}
        onKeyDown={onKeyDown}
        ref={ref}
        tabIndex={-1}
      >
        <h3>Proteger con contraseña</h3>
        <input
          type="password"
          placeholder="Contraseña (necesaria para abrir)"
          aria-label="Contraseña necesaria para abrir el documento"
          value={valor.user}
          onChange={(e) => onChange({ ...valor, user: e.target.value })}
        />
        <input
          type="password"
          placeholder="Contraseña de permisos (opcional)"
          aria-label="Contraseña de permisos"
          value={valor.owner}
          onChange={(e) => onChange({ ...valor, owner: e.target.value })}
        />
        {permiso("imprimir", "Permitir imprimir")}
        {permiso("copiar", "Permitir copiar texto")}
        {permiso("editar", "Permitir editar y comentar")}
        <p className="modal-file" style={{ whiteSpace: "normal" }}>
          Cifrado AES-256. Sin contraseña de permisos no se puede restringir
          nada: quien abra el documento podrá hacerlo todo. Si el documento va
          a llevar firma digital, fírmalo por separado.
        </p>
        <div className="card-actions">
          <button className="btn" onClick={onClose}>
            Cancelar
          </button>
          <button className="btn" disabled={!valor.user} onClick={onCopia}>
            Guardar una copia protegida…
          </button>
          <button
            className="btn btn-primary"
            disabled={!valor.user}
            onClick={onConfirm}
          >
            Proteger
          </button>
        </div>
      </div>
    </div>
  );
}
