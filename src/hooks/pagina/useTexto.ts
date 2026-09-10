import { useEffect, useRef, useState, type MouseEvent } from "react";
import { invoke } from "../../ipc";
import {
  addTextBlock,
  editTextBlock,
  moveTextBlock,
  resizeTextBlock,
} from "../../api";
import {
  ajustaLineas,
  altoCuadro,
  hexToRgba,
  rgbaToHex,
  type Mode,
  type PageSize,
  type ResizeHandle,
  type TextBlock,
  type TxtAction,
} from "../../tipos";
import type { ToolProps } from "../../components/Pagina";
import { puntoAPagina, puntoAVista, puntoEnCapa, rectAPagina } from "./geometria";

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
  scale: number;
  size: PageSize;
  viewRotation: number;
  tool: ToolProps;
  onPageMutated: (page: number) => void;
  onError: (e: unknown) => void;
  onNotice: (texto: string) => void;
}) {
  const {
    workPath,
    index,
    visible,
    docVersion,
    pageVersion,
    mode,
    scale,
    size,
    viewRotation,
    tool,
    onPageMutated,
    onError,
    onNotice,
  } = ctx;
  // color y alineación de la fila contextual; `null` es «como esté», que es
  // lo que hace falta para que editar un párrafo no lo recoloree sin querer
  const formato = {
    color: tool.textColor ? hexToRgba(tool.textColor) : null,
    align: tool.textAlign,
    lineHeight: tool.textLineHeight,
  };
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
  // colocar y estirar el bloque, con el mismo gesto (y el mismo espejo en un
  // ref) que las imágenes y los sellos: en un arrastre de un solo frame el
  // estado de React va por detrás al llegar el mouseup
  const [txtDraft, setTxtDraft] = useState<TextBlock | null>(null);
  const txtLiveRef = useRef<TextBlock | null>(null);
  const txtActionRef = useRef<TxtAction | null>(null);

  // Al cambiar de modo: fuera borradores
  useEffect(() => {
    setBlockDraft(null);
    setNewTextDraft(null);
    setTxtDraft(null);
    txtLiveRef.current = null;
    txtActionRef.current = null;
  }, [mode]);

  // el swatch «el que ya tenga» se pinta del color del bloque señalado
  const avisaColor = tool.onTextBlockPicked;
  useEffect(() => {
    avisaColor(blockDraft ? rgbaToHex(blockDraft.block.color) : null);
  }, [blockDraft, avisaColor]);

  // Bloques de texto (solo en modo edición)
  useEffect(() => {
    setNewTextDraft(null);
    // la tarjeta de edición guarda un object_index que deja de valer si el
    // documento cambia por debajo (deshacer, rehacer)
    setBlockDraft(null);
    setTxtDraft(null);
    txtLiveRef.current = null;
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

  /** Empieza a mover o a estirar el bloque señalado. */
  function startTxtAction(
    e: MouseEvent<HTMLDivElement>,
    b: TextBlock,
    kind: TxtAction["kind"],
    handle: ResizeHandle = "se",
  ) {
    e.stopPropagation();
    if (e.button !== 0) return;
    const p = puntoEnCapa(e, scale, viewRotation);
    txtActionRef.current = {
      kind,
      handle,
      startX: p.x,
      startY: p.y,
      orig: b,
      moved: false,
    };
    txtLiveRef.current = b;
    setTxtDraft(b);
  }

  /** Guarda la caja nueva del bloque: mover y estirar son dos comandos,
   *  porque en el PDF son dos cosas distintas (la matriz y el cuerpo de la
   *  fuente). Si el gesto ha hecho las dos, se mandan las dos. */
  async function commitTextBlock(orig: TextBlock, b: TextBlock) {
    if (!workPath) return;
    const pr = rectAPagina(b, size);
    const or = rectAPagina(orig, size);
    try {
      if (Math.abs(pr.w - or.w) > 0.5 || Math.abs(pr.h - or.h) > 0.5)
        await resizeTextBlock(workPath, index, orig.object_index, pr.w, pr.h);
      if (Math.abs(pr.x - or.x) > 0.5 || Math.abs(pr.y - or.y) > 0.5)
        await moveTextBlock(workPath, index, orig.object_index, pr.x, pr.y);
      onPageMutated(index);
    } catch (e) {
      setTxtDraft(null);
      txtLiveRef.current = null;
      onError(e);
    }
  }

  async function submitNewText() {
    if (!workPath || !newTextDraft) return;
    if (!newTextDraft.text.trim()) {
      setNewTextDraft(null);
      return;
    }
    const p = puntoAPagina({ x: newTextDraft.x, y: newTextDraft.y }, size);
    try {
      await addTextBlock({
        workPath,
        pageIndex: index,
        x: p.x,
        y: p.y,
        text: newTextDraft.text,
        fontSize: newTextDraft.size,
        font: newTextDraft.font === "auto" ? null : newTextDraft.font,
        ...formato,
      });
      setNewTextDraft(null);
      onPageMutated(index);
    } catch (e) {
      onError(e);
    }
  }

  /** El bloque ocupa más de una línea: entonces hay párrafo que recolocar y
   *  el reflujo tiene sentido. Un rótulo de una línea se reescribe como
   *  siempre. */
  function esParrafo(b: TextBlock, texto: string): boolean {
    return b.h > b.font_size * 1.5 || b.text.includes("\n") || texto.includes("\n");
  }

  async function submitBlockDraft() {
    if (!workPath || !blockDraft) return;
    const parrafo = esParrafo(blockDraft.block, blockDraft.text);
    try {
      await editTextBlock({
        workPath,
        pageIndex: index,
        objectIndex: blockDraft.block.object_index,
        newText: blockDraft.text,
        // el párrafo se reparte de nuevo al ancho que tenía; una línea
        // suelta se reescribe como siempre
        reflow: parrafo,
        ...formato,
      });
      setBlockDraft(null);
      onPageMutated(index);
      // el párrafo puede crecer más de lo que queda de papel: se dice, en
      // vez de escribir fuera de la página en silencio
      if (parrafo) {
        const lineas = ajustaLineas(
          blockDraft.text,
          blockDraft.block.w,
          blockDraft.block.font_size,
        ).length;
        const alto = altoCuadro(lineas, blockDraft.block.font_size);
        if (blockDraft.block.y + alto > size.height) {
          onNotice(
            "El párrafo no cabe en la página; el texto que sobra queda fuera del papel",
          );
        }
      }
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
    txtDraft,
    setTxtDraft,
    txtLiveRef,
    txtActionRef,
    startTxtAction,
    commitTextBlock,
    submitNewText,
    submitBlockDraft,
    deleteBlock,
  };
}

export type Texto = ReturnType<typeof useTexto>;
