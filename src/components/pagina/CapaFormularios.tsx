/** Campos de formulario (modo selección), su tarjeta y la del campo nuevo. */
import { useState } from "react";
import type { TipoCampo } from "../../api";
import type { FormFieldInfo, Mode } from "../../tipos";
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

/** Lo que se lee al pasar el ratón: el texto de ayuda que trae el PDF
 *  (`/TU`), que es lo que enseña Acrobat, y si no lo trae, el nombre del
 *  campo. Detrás va lo que hay que saber antes de tocarlo. */
function tituloCampo(f: FormFieldInfo): string {
  const base = f.tooltip?.trim() || f.name;
  const notas = [
    f.required ? "obligatorio" : null,
    f.read_only ? "solo lectura" : null,
  ].filter(Boolean);
  return notas.length > 0 ? `${base} · ${notas.join(" · ")}` : base;
}

/** Los que se rellenan eligiendo, no escribiendo. */
const ELECCION = ["ComboBox", "ListBox"];

/** Los cinco tipos de «Preparar formulario», con su nombre en español. */
const TIPOS_CAMPO: [TipoCampo, string][] = [
  ["text", "Texto"],
  ["checkbox", "Casilla"],
  ["radio", "Botón de radio"],
  ["combo", "Desplegable"],
  ["list", "Lista"],
];

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
    formGroup,
    setFormGroup,
    formExport,
    setFormExport,
    formOptions,
    setFormOptions,
    formTooltip,
    setFormTooltip,
    formValorDefecto,
    setFormValorDefecto,
    formObligatorio,
    setFormObligatorio,
    formSoloLectura,
    setFormSoloLectura,
    esRadio,
    esEleccion,
    grupoConocido,
    puedeCrearCampo,
    submitFieldDraft,
    tabulaCampo,
    elegirOpcion,
    onFieldClick,
    removeFormField,
    applyFormField,
  } = formularios;
  // «Más opciones» va plegado: quien crea un campo suele querer solo el
  // nombre, y las propiedades del panel de Acrobat estorban hasta que hacen
  // falta
  const [masOpciones, setMasOpciones] = useState(false);
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
                title={tituloCampo(f)}
                aria-label={tituloCampo(f)}
                disabled={f.read_only}
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
              className={`form-field${resaltarCampos ? " resaltado" : ""}${
                f.required ? " obligatorio" : ""
              }${f.read_only ? " solo-lectura" : ""}`}
              role="button"
              tabIndex={0}
              title={tituloCampo(f)}
              aria-label={tituloCampo(f)}
              aria-required={f.required || undefined}
              aria-disabled={f.read_only || undefined}
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
                aria-label={esRadio ? "Nombre del grupo" : "Nombre del campo"}
                placeholder={
                  esRadio ? "Nombre del grupo (sexo)" : "Nombre del campo"
                }
                value={esRadio ? formGroup : formName}
                onChange={(e) =>
                  esRadio
                    ? setFormGroup(e.target.value)
                    : setFormName(e.target.value)
                }
              />
              <select
                className="size-select"
                aria-label="Tipo de campo"
                value={formKind}
                onChange={(e) => setFormKind(e.target.value as TipoCampo)}
              >
                {TIPOS_CAMPO.map(([v, etiqueta]) => (
                  <option key={v} value={v}>
                    {etiqueta}
                  </option>
                ))}
              </select>
            </div>
            {esRadio && (
              <>
                <div className="card-row">
                  <input
                    type="text"
                    className="stamp-input"
                    aria-label="Valor de esta opción"
                    placeholder="Valor de esta opción (Mujer)"
                    value={formExport}
                    onChange={(e) => setFormExport(e.target.value)}
                  />
                </div>
                {/* el defecto de Acrobat que NO se copia: allí tres radios
                    sueltos se marcan todos a la vez y nadie lo avisa */}
                {grupoConocido && (
                  <span className="opt-hint">
                    Se añadirá al grupo «{formGroup.trim()}»; solo se podrá
                    marcar uno.
                  </span>
                )}
              </>
            )}
            {esEleccion && (
              <textarea
                className="stamp-input"
                rows={3}
                aria-label="Opciones, una por línea"
                placeholder="Una opción por línea"
                value={formOptions}
                onChange={(e) => setFormOptions(e.target.value)}
              />
            )}
            <button
              className="btn opt-mas"
              aria-expanded={masOpciones}
              onClick={() => setMasOpciones((v) => !v)}
            >
              {masOpciones ? "▾" : "▸"} Más opciones
            </button>
            {masOpciones && (
              <>
                <div className="card-row">
                  <input
                    type="text"
                    className="stamp-input"
                    aria-label="Texto de ayuda"
                    placeholder="Texto de ayuda al pasar el ratón"
                    value={formTooltip}
                    onChange={(e) => setFormTooltip(e.target.value)}
                  />
                </div>
                <div className="card-row">
                  <input
                    type="text"
                    className="stamp-input"
                    aria-label="Valor por defecto"
                    placeholder="Valor por defecto"
                    value={formValorDefecto}
                    onChange={(e) => setFormValorDefecto(e.target.value)}
                  />
                </div>
                <label className="opt-check">
                  <input
                    type="checkbox"
                    checked={formObligatorio}
                    onChange={(e) => setFormObligatorio(e.target.checked)}
                  />
                  Obligatorio
                </label>
                <label className="opt-check">
                  <input
                    type="checkbox"
                    checked={formSoloLectura}
                    onChange={(e) => setFormSoloLectura(e.target.checked)}
                  />
                  Solo lectura
                </label>
              </>
            )}
            <div className="card-actions">
              <button
                className="btn btn-primary"
                disabled={!puedeCrearCampo}
                title={
                  puedeCrearCampo
                    ? undefined
                    : esRadio
                      ? "Pon el nombre del grupo y el valor de esta opción"
                      : esEleccion
                        ? "Escribe al menos una opción"
                        : "Ponle un nombre al campo"
                }
                onClick={applyFormField}
              >
                {esRadio ? "Crear la opción" : "Crear campo"}
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
