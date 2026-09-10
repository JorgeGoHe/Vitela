import { useEffect, useRef, useState } from "react";
import { invoke } from "../../ipc";
import {
  createFormField,
  deleteFormField,
  setFormChoice,
  type TipoCampo,
} from "../../api";
import type { FormFieldInfo, Mode, PageSize, Rect } from "../../tipos";
import { rectAPagina } from "./geometria";

/**
 * Formularios AcroForm: los campos de la página (rellenar texto, marcar
 * casillas, borrar) y el borrador del campo nuevo (modo form-new).
 */
export function useFormularios(ctx: {
  workPath: string;
  index: number;
  visible: boolean;
  docVersion: number;
  annotVersion: number;
  pageVersion: number;
  mode: Mode;
  size: PageSize;
  onAnnotated: (page: number) => void;
  onPageMutated: (page: number) => void;
  onError: (e: unknown) => void;
  onNotice: (texto: string) => void;
  /** Cuántos campos tiene esta página (para avisar de que se puede rellenar). */
  onFormularios: (n: number) => void;
  onModeChange: (m: Mode) => void;
}) {
  const {
    workPath,
    index,
    visible,
    docVersion,
    annotVersion,
    pageVersion,
    mode,
    size,
    onAnnotated,
    onPageMutated,
    onError,
    onNotice,
    onFormularios,
    onModeChange,
  } = ctx;
  const [formFields, setFormFields] = useState<FormFieldInfo[]>([]);
  const [fieldDraft, setFieldDraft] = useState<{
    field: FormFieldInfo;
    text: string;
  } | null>(null);
  const [formDraft, setFormDraft] = useState<Rect | null>(null);
  const formStartRef = useRef<{ x: number; y: number } | null>(null);
  const formLiveRef = useRef<Rect | null>(null);
  const [formName, setFormName] = useState("campo");
  const [formKind, setFormKind] = useState<TipoCampo>("text");
  // botón de radio: el grupo es la entrada de `/Fields` que comparten todas
  // las opciones, y el valor de exportación lo que se escribe al marcar esta
  const [formGroup, setFormGroup] = useState("");
  const [formExport, setFormExport] = useState("");
  // desplegable y lista: una opción por línea
  const [formOptions, setFormOptions] = useState("");
  // «Más opciones»: las propiedades del primer panel de Acrobat
  const [formTooltip, setFormTooltip] = useState("");
  const [formValorDefecto, setFormValorDefecto] = useState("");
  const [formObligatorio, setFormObligatorio] = useState(false);
  const [formSoloLectura, setFormSoloLectura] = useState(false);
  // grupos de radio ya creados en esta sesión: con uno dibujado, el
  // siguiente propone el mismo y lo dice, que es el defecto de Acrobat que
  // NO hay que copiar (allí salen tres campos que se marcan todos a la vez)
  const gruposRef = useRef<Set<string>>(new Set());
  // campo que hay que abrir en cuanto lleguen los datos frescos: al tabular,
  // guardar el valor recarga la lista y borraría el borrador recién puesto
  const proximoRef = useRef<number | null>(null);

  // Al cambiar de modo: fuera borradores
  useEffect(() => {
    setFormDraft(null);
    formStartRef.current = null;
    formLiveRef.current = null;
    setFieldDraft(null);
  }, [mode]);

  // Campos de formulario de la página
  useEffect(() => {
    if (!workPath || !visible) return;
    let cancelled = false;
    setFieldDraft(null);
    invoke<FormFieldInfo[]>("get_form_fields", { path: workPath, pageIndex: index })
      .then((f) => {
        if (cancelled) return;
        setFormFields(f);
        onFormularios(f.length);
        const prox = proximoRef.current;
        proximoRef.current = null;
        if (prox === null) return;
        const campo = f.find((x) => x.annot_index === prox);
        if (campo) setFieldDraft({ field: campo, text: campo.value });
      })
      .catch(() => {
        if (!cancelled) setFormFields([]);
      });
    return () => {
      cancelled = true;
    };
  }, [
    workPath,
    index,
    visible,
    docVersion,
    annotVersion,
    pageVersion,
    onFormularios,
  ]);

  async function submitFieldDraft() {
    if (!workPath || !fieldDraft) return;
    try {
      await invoke("set_form_text", {
        workPath,
        pageIndex: index,
        annotIndex: fieldDraft.field.annot_index,
        value: fieldDraft.text,
      });
      setFieldDraft(null);
      onAnnotated(index);
    } catch (e) {
      onError(e);
    }
  }

  /** Campo de texto anterior o siguiente en el orden de lectura (por `y` y
   *  luego por `x`), que es como tabula Acrobat. */
  function campoVecino(delta: number): FormFieldInfo | null {
    if (!fieldDraft) return null;
    const orden = [...formFields].sort((a, b) => a.y - b.y || a.x - b.x);
    let i = orden.findIndex(
      (f) => f.annot_index === fieldDraft.field.annot_index,
    );
    if (i < 0) return null;
    for (i += delta; i >= 0 && i < orden.length; i += delta) {
      if (orden[i].kind === "Text") return orden[i];
    }
    return null;
  }

  /** Tab y ⇧Tab: confirman lo escrito y abren el campo siguiente/anterior. */
  function tabulaCampo(delta: number) {
    if (!fieldDraft) return;
    const siguiente = campoVecino(delta);
    if (fieldDraft.text === fieldDraft.field.value) {
      // sin cambios no hace falta escribir (ni gastar un paso de historial)
      setFieldDraft(
        siguiente ? { field: siguiente, text: siguiente.value } : null,
      );
      return;
    }
    proximoRef.current = siguiente ? siguiente.annot_index : null;
    submitFieldDraft();
  }

  async function elegirOpcion(field: FormFieldInfo, value: string) {
    if (!workPath || field.read_only) return;
    try {
      await setFormChoice(workPath, index, field.annot_index, value);
      onAnnotated(index);
    } catch (e) {
      onError(e);
    }
  }

  /** Vuelve a leer los campos de la página: marcar un radio cambia a sus
   *  hermanos, y hasta que llegan los datos frescos el grupo miente. */
  async function releeCampos() {
    if (!workPath) return;
    try {
      const f = await invoke<FormFieldInfo[]>("get_form_fields", {
        path: workPath,
        pageIndex: index,
      });
      setFormFields(f);
    } catch {
      /* si no se pueden releer, el refresco de la página los traerá */
    }
  }

  async function toggleFormCheck(field: FormFieldInfo) {
    if (!workPath || field.read_only) return;
    try {
      await invoke("set_form_checked", {
        workPath,
        pageIndex: index,
        annotIndex: field.annot_index,
        // un radio se **marca**, no se conmuta: es lo que hace Acrobat, y
        // conmutarlo era lo que dejaba el grupo entero sin poder marcarse
        // cuando la lectura decía que ya lo estaban todos
        checked: field.kind === "RadioButton" ? true : !field.checked,
      });
      // marcar una opción apaga a sus hermanas: el grupo se relee entero
      await releeCampos();
      onAnnotated(index);
    } catch (e) {
      onError(e);
    }
  }

  // campos de solo lectura sobre los que ya se ha avisado: el aviso sale la
  // primera vez que se pulsa cada uno, no en cada clic
  const avisadosRef = useRef<Set<number>>(new Set());

  function onFieldClick(field: FormFieldInfo) {
    // un campo de solo lectura no se toca: en Acrobat ni siquiera coge el
    // foco, y hasta ahora aquí se dejaba cambiar y el cambio iba al fichero
    if (field.read_only) {
      // un campo que no responde al clic y no dice por qué parece la app
      // rota: la primera vez se cuenta en la banda, con el texto de ayuda
      // que trae el PDF si lo trae
      if (avisadosRef.current.has(field.annot_index)) return;
      avisadosRef.current.add(field.annot_index);
      const ayuda = field.tooltip?.trim();
      onNotice(
        ayuda
          ? `${field.name} es de solo lectura · ${ayuda}`
          : `${field.name} es de solo lectura: lo ha bloqueado quien hizo el formulario`,
      );
      return;
    }
    if (field.kind === "Text") {
      setFieldDraft({ field, text: field.value });
    } else if (field.kind === "Checkbox" || field.kind === "RadioButton") {
      toggleFormCheck(field);
    }
  }

  async function removeFormField(name: string) {
    if (!workPath) return;
    try {
      await deleteFormField(workPath, name);
      setFieldDraft(null);
      onPageMutated(index);
    } catch (e) {
      onError(e);
    }
  }

  /** Las opciones del desplegable o de la lista, una por línea. */
  const opcionesCampo = formOptions
    .split("\n")
    .map((o) => o.trim())
    .filter(Boolean);

  /** En un radio el nombre del campo ES el del grupo: los hermanos cuelgan
   *  de una sola entrada en `/Fields`. */
  const esRadio = formKind === "radio";
  const esEleccion = formKind === "combo" || formKind === "list";
  const nombreCampo = (esRadio ? formGroup : formName).trim();
  /** Ya hay una opción de este grupo: el diálogo lo dice antes de crear. */
  const grupoConocido = esRadio && gruposRef.current.has(nombreCampo);
  const puedeCrearCampo =
    nombreCampo !== "" &&
    (!esRadio || formExport.trim() !== "") &&
    (!esEleccion || opcionesCampo.length > 0);

  async function applyFormField() {
    if (!workPath || !formDraft || !puedeCrearCampo) return;
    try {
      await createFormField({
        workPath,
        pageIndex: index,
        kind: formKind,
        rect: rectAPagina(formDraft, size),
        name: nombreCampo,
        group: esRadio ? nombreCampo : "",
        exportValue: esRadio ? formExport.trim() : "",
        options: opcionesCampo,
        props: {
          tooltip: formTooltip.trim() || null,
          obligatorio: formObligatorio,
          solo_lectura: formSoloLectura,
          valor_defecto: formValorDefecto.trim() || null,
          orden_tab: null,
        },
      });
      setFormDraft(null);
      if (esRadio) {
        // el grupo se queda puesto para la opción siguiente y el valor se
        // vacía: dibujar el segundo radio no debe repetir el primero
        gruposRef.current.add(nombreCampo);
        setFormExport("");
      } else {
        onModeChange("select");
      }
      onPageMutated(index);
    } catch (e) {
      onError(e);
    }
  }

  return {
    formFields,
    fieldDraft,
    setFieldDraft,
    formDraft,
    setFormDraft,
    formStartRef,
    formLiveRef,
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
    toggleFormCheck,
    onFieldClick,
    removeFormField,
    applyFormField,
  };
}

export type Formularios = ReturnType<typeof useFormularios>;
