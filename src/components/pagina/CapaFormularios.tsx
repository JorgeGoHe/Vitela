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
  /** «Resaltar campos existentes» de Acrobat. */
  resaltarCampos: boolean;
};

/** Los que se rellenan eligiendo, no escribiendo. */
const ELECCION = ["ComboBox", "ListBox"];

export default function CapaFormularios({
  mode,
  formularios,
  scale,
  displayWidth,
  resaltarCampos,
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
    tabulaCampo,
    elegirOpcion,
    onFieldClick,
    removeFormField,
    applyFormField,
  } = formularios;
  return (
    <>
      {mode === "select" &&
        formFields.map((f) => {
          const caja = {
            left: f.x * scale,
            top: f.y * scale,
            width: f.w * scale,
            height: f.h * scale,
          };
          // desplegables y listas se rellenan con un select nativo encima del
          // campo, con las opciones que trae el PDF
          if (ELECCION.includes(f.kind)) {
            return (
              <select
                key={`f${f.annot_index}`}
                className={`form-field form-choice${resaltarCampos ? " resaltado" : ""}`}
                title={f.name}
                aria-label={f.name}
                // una lista se pinta como lista: con `size` el navegador
                // enseña las opciones dentro del recuadro, en vez de estirar
                // un desplegable al alto del campo
                size={
                  f.kind === "ListBox"
                    ? Math.max(2, Math.min(f.options.length, 6))
                    : undefined
                }
                style={caja}
                value={f.value}
                onMouseDown={(e) => e.stopPropagation()}
                onClick={(e) => e.stopPropagation()}
                onChange={(e) => elegirOpcion(f, e.target.value)}
              >
                {!f.options.includes(f.value) && (
                  <option value={f.value}>{f.value || "—"}</option>
                )}
                {f.options.map((o) => (
                  <option key={o} value={o}>
                    {o}
                  </option>
                ))}
              </select>
            );
          }
          return (
            <div
              key={`f${f.annot_index}`}
              className={`form-field${resaltarCampos ? " resaltado" : ""}`}
              role="button"
              tabIndex={0}
              title={f.name}
              aria-label={f.name}
              style={caja}
              onMouseDown={(e) => e.stopPropagation()}
              onClick={(e) => {
                e.stopPropagation();
                onFieldClick(f);
              }}
              onKeyDown={(e) => {
                if (e.key !== "Enter" && e.key !== " ") return;
                e.preventDefault();
                onFieldClick(f);
              }}
            />
          );
        })}
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
              if (e.key === "Tab") {
                // Tab confirma y salta al campo siguiente, como en Acrobat
                e.preventDefault();
                tabulaCampo(e.shiftKey ? -1 : 1);
              } else if (e.key === "Enter" && !e.shiftKey) {
                e.preventDefault();
                submitFieldDraft();
              } else if (e.key === "Escape") {
                // revierte: el valor del PDF no se ha tocado
                e.stopPropagation();
                setFieldDraft(null);
              }
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
