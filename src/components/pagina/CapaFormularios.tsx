/** Campos de formulario (modo selección), su tarjeta y la del campo nuevo. */
import type { Mode } from "../../tipos";
import { clampCardLeft } from "../../hooks/pagina/geometria";
import type { Formularios } from "../../hooks/pagina/useFormularios";
import Icon from "../Icon";

type Props = {
  mode: Mode;
  formularios: Formularios;
  scale: number;
  displayWidth: number;
};

export default function CapaFormularios({
  mode,
  formularios,
  scale,
  displayWidth,
}: Props) {
  const {
    formFields,
    fieldDraft,
    setFieldDraft,
    formDraft,
    setFormDraft,
    formName,
    setFormName,
    formKind,
    setFormKind,
    submitFieldDraft,
    onFieldClick,
    removeFormField,
    applyFormField,
  } = formularios;
  return (
    <>
      {mode === "select" &&
        formFields.map((f) => (
          <div
            key={`f${f.annot_index}`}
            className="form-field"
            title={f.name}
            style={{
              left: f.x * scale,
              top: f.y * scale,
              width: f.w * scale,
              height: f.h * scale,
            }}
            onMouseDown={(e) => e.stopPropagation()}
            onClick={(e) => {
              e.stopPropagation();
              onFieldClick(f);
            }}
          />
        ))}
      {fieldDraft && (
        <div
          className="card"
          style={{
            left: clampCardLeft(fieldDraft.field.x * scale, displayWidth),
            top: (fieldDraft.field.y + fieldDraft.field.h) * scale + 6,
          }}
          onMouseDown={(e) => e.stopPropagation()}
        >
          <textarea
            autoFocus
            placeholder={`${fieldDraft.field.name}…`}
            value={fieldDraft.text}
            onChange={(e) =>
              setFieldDraft({ ...fieldDraft, text: e.target.value })
            }
            onKeyDown={(e) => {
              if (e.key === "Enter" && !e.shiftKey) {
                e.preventDefault();
                submitFieldDraft();
              }
              if (e.key === "Escape") setFieldDraft(null);
            }}
          />
          <div className="card-actions">
            <button
              className="btn btn-danger"
              title="Borrar este campo del formulario"
              onClick={() => removeFormField(fieldDraft.field.name)}
            >
              <Icon name="trash" size={13} />
              Eliminar
            </button>
            <button className="btn" onClick={() => setFieldDraft(null)}>
              Cancelar
            </button>
            <button className="btn btn-primary" onClick={submitFieldDraft}>
              Guardar
            </button>
          </div>
        </div>
      )}
      {mode === "form-new" && formDraft && (
        <>
          <div
            className="crop-rect"
            style={{
              left: formDraft.x * scale,
              top: formDraft.y * scale,
              width: formDraft.w * scale,
              height: formDraft.h * scale,
            }}
          />
          <div
            className="card crop-actions"
            style={{
              left: clampCardLeft(formDraft.x * scale, displayWidth, 300),
              top: (formDraft.y + formDraft.h) * scale + 8,
            }}
            onMouseDown={(e) => e.stopPropagation()}
          >
            <div className="card-row">
              <input
                type="text"
                className="stamp-input"
                autoFocus
                placeholder="Nombre del campo"
                value={formName}
                onChange={(e) => setFormName(e.target.value)}
              />
              <select
                className="size-select"
                value={formKind}
                onChange={(e) =>
                  setFormKind(e.target.value as "text" | "checkbox")
                }
              >
                <option value="text">Texto</option>
                <option value="checkbox">Casilla</option>
              </select>
            </div>
            <div className="card-actions">
              <button
                className="btn btn-primary"
                disabled={!formName.trim()}
                onClick={applyFormField}
              >
                Crear campo
              </button>
              <button className="btn" onClick={() => setFormDraft(null)}>
                Cancelar
              </button>
            </div>
          </div>
        </>
      )}
    </>
  );
}
