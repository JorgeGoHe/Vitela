import { useEffect, useState } from "react";
import { invoke } from "../../ipc";
import type { Mode, PageSize, TextBlock } from "../../tipos";
import { puntoAPagina, puntoAVista } from "./geometria";

/**
 * Edición real de texto (modo edit): los bloques de la página, la tarjeta
 * de edición de un bloque y la de texto nuevo en un punto libre.
 */
export function useTexto(ctx: {
  workPath: string;
  index: number;
  visible: boolean;
  docVersion: number;
  pageVersion: number;
  mode: Mode;
  size: PageSize;
  onPageMutated: (page: number) => void;
  onError: (e: unknown) => void;
}) {
  const {
    workPath,
    index,
    visible,
    docVersion,
    pageVersion,
    mode,
    size,
    onPageMutated,
    onError,
  } = ctx;
  const [textBlocks, setTextBlocks] = useState<TextBlock[]>([]);
  const [blockDraft, setBlockDraft] = useState<{
    block: TextBlock;
    text: string;
  } | null>(null);
  const [newTextDraft, setNewTextDraft] = useState<{
    x: number;
    y: number;
    text: string;
    size: number;
    font: string;
  } | null>(null);

  // Al cambiar de modo: fuera borradores
  useEffect(() => {
    setBlockDraft(null);
    setNewTextDraft(null);
  }, [mode]);

  // Bloques de texto (solo en modo edición)
  useEffect(() => {
    setNewTextDraft(null);
    // la tarjeta de edición guarda un object_index que deja de valer si el
    // documento cambia por debajo (deshacer, rehacer)
    setBlockDraft(null);
    if (!workPath || !visible || mode !== "edit") {
      setTextBlocks([]);
      return;
    }
    let cancelled = false;
    invoke<TextBlock[]>("get_text_blocks", { path: workPath, pageIndex: index })
      .then((b) => {
        // los bloques vienen en el espacio propio de la página: al overlay
        // le hacen falta en el de la vista para caer sobre lo que se ve
        if (!cancelled)
          setTextBlocks(
            b.map((t) => ({ ...t, ...puntoAVista({ x: t.x, y: t.y }, size) })),
          );
      })
      .catch((e) => {
        if (!cancelled) onError(e);
      });
    return () => {
      cancelled = true;
    };
  }, [workPath, index, visible, docVersion, mode, pageVersion, size, onError]);

  async function submitNewText() {
    if (!workPath || !newTextDraft) return;
    if (!newTextDraft.text.trim()) {
      setNewTextDraft(null);
      return;
    }
    const p = puntoAPagina({ x: newTextDraft.x, y: newTextDraft.y }, size);
    try {
      await invoke("add_text_block", {
        workPath,
        pageIndex: index,
        x: p.x,
        y: p.y,
        text: newTextDraft.text,
        fontSize: newTextDraft.size,
        font: newTextDraft.font === "auto" ? null : newTextDraft.font,
      });
      setNewTextDraft(null);
      onPageMutated(index);
    } catch (e) {
      onError(e);
    }
  }

  async function submitBlockDraft() {
    if (!workPath || !blockDraft) return;
    try {
      await invoke("edit_text_block", {
        workPath,
        pageIndex: index,
        objectIndex: blockDraft.block.object_index,
        newText: blockDraft.text,
      });
      setBlockDraft(null);
      onPageMutated(index);
    } catch (e) {
      onError(e);
    }
  }

  async function deleteBlock() {
    if (!workPath || !blockDraft) return;
    try {
      await invoke("delete_text_block", {
        workPath,
        pageIndex: index,
        objectIndex: blockDraft.block.object_index,
      });
      setBlockDraft(null);
      onPageMutated(index);
    } catch (e) {
      onError(e);
    }
  }

  return {
    textBlocks,
    blockDraft,
    setBlockDraft,
    newTextDraft,
    setNewTextDraft,
    submitNewText,
    submitBlockDraft,
    deleteBlock,
  };
}

export type Texto = ReturnType<typeof useTexto>;
