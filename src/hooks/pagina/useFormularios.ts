import { useEffect, useRef, useState } from "react";
import { invoke } from "../../ipc";
import { createFormField, deleteFormField } from "../../api";
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
  const [formKind, setFormKind] = useState<"text" | "checkbox">("text");

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
        if (!cancelled) setFormFields(f);
      })
      .catch(() => {
        if (!cancelled) setFormFields([]);
      });
    return () => {
      cancelled = true;
    };
  }, [workPath, index, visible, docVersion, annotVersion, pageVersion]);

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

  async function toggleFormCheck(field: FormFieldInfo) {
    if (!workPath) return;
    try {
      await invoke("set_form_checked", {
        workPath,
        pageIndex: index,
        annotIndex: field.annot_index,
        checked: !field.checked,
      });
      onAnnotated(index);
    } catch (e) {
      onError(e);
    }
  }

  function onFieldClick(field: FormFieldInfo) {
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

  async function applyFormField() {
    if (!workPath || !formDraft || !formName.trim()) return;
    try {
      await createFormField({
        workPath,
        pageIndex: index,
        kind: formKind,
        rect: rectAPagina(formDraft, size),
        name: formName.trim(),
      });
      setFormDraft(null);
      onModeChange("select");
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
    submitFieldDraft,
    toggleFormCheck,
    onFieldClick,
    removeFormField,
    applyFormField,
  };
}

export type Formularios = ReturnType<typeof useFormularios>;
