import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type MouseEvent,
} from "react";
import { invoke } from "../../ipc";
import { abrirRuta, open, save } from "../../dialogos";
import {
  addCallout,
  addFileAttachmentAnnotation,
  openPageAttachment,
  savePageAttachment,
  addFreeText,
  addMarkup,
  eraseInkArea,
  addShape,
  addStamp,
  setAnnotationColor,
  setAnnotationContents,
  transformAnnotation,
} from "../../api";
import {
  ajustaLineas,
  altoCuadro,
  autorComentarios,
  hexToRgba,
  MOD,
  mergeLineRects,
  PLANTILLAS_DINAMICAS,
  selloDinamico,
  type AnnotationInfo,
  type Mode,
  type PageSize,
  type Rect,
  type ResizeHandle,
} from "../../tipos";
import type { MarcaRellenar, ToolProps } from "../../components/Pagina";
import type { SeleccionTexto } from "./useSeleccionTexto";
import {
  cajaLlamada,
  pagePoint,
  puntoAPagina,
  puntoEnCapa,
  rectAPagina,
} from "./geometria";

/**
 * Anotaciones de la página: la lista (iconos de nota, overlays de marcas,
 * popover) y los borradores de nota, trazo (Ink), forma y arrastre de
 * sellos/dibujos en modo selección. Los handlers de ratón viven en `Pagina`.
 */
export function useAnotaciones(ctx: {
  workPath: string;
  index: number;
  visible: boolean;
  docVersion: number;
  annotVersion: number;
  pageVersion: number;
  mode: Mode;
  scale: number;
  size: PageSize;
  /** Giro solo de la vista (⇧⌘+/⇧⌘−): no toca el fichero. */
  viewRotation: number;
  tool: ToolProps;
  selOwner: number | null;
  /** Índice de la anotación que el panel de comentarios quiere seleccionar
   *  en esta página (null si la elegida no está aquí). */
  seleccionExterna: number | null;
  seleccion: Pick<SeleccionTexto, "selection" | "pageText" | "setSelection">;
  onAnnotated: (page: number) => void;
  onError: (e: unknown) => void;
  onNotice: (texto: string) => void;
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
    scale,
    size,
    viewRotation,
    tool,
    selOwner,
    seleccionExterna,
    seleccion,
    onAnnotated,
    onError,
    onNotice,
    onModeChange,
  } = ctx;
  const [annots, setAnnots] = useState<AnnotationInfo[]>([]);
  const [notePopover, setNotePopover] = useState<AnnotationInfo | null>(null);
  // texto que se está corrigiendo en el popover, atado al comentario al que
  // pertenece: si se elige otro, la edición anterior deja de aplicarse sola
  const [noteEdit, setNoteEdit] = useState<{
    index: number;
    text: string;
  } | null>(null);
  const [noteDraft, setNoteDraft] = useState<{
    x: number;
    y: number;
    text: string;
  } | null>(null);
  const [strokePts, setStrokePts] = useState<[number, number][]>([]);
  const strokeLiveRef = useRef<[number, number][]>([]);

  // arrastre/redimensionado de anotaciones (sellos y dibujos) en modo select
  const [annotDraft, setAnnotDraft] = useState<(Rect & { index: number }) | null>(null);
  const annotActionRef = useRef<{
    kind: "move" | "resize";
    handle?: ResizeHandle;
    startX: number;
    startY: number;
    orig: AnnotationInfo;
    moved: boolean;
  } | null>(null);
  const annotLiveRef = useRef<(Rect & { index: number }) | null>(null);

  // cuadro de texto (FreeText): el rectángulo se arrastra y el texto se
  // escribe dentro antes de crear la anotación
  const [freeTextDraft, setFreeTextDraft] = useState<
    (Rect & { text: string }) | null
  >(null);
  const freeTextStartRef = useRef<{ x: number; y: number } | null>(null);
  const freeTextLiveRef = useRef<(Rect & { text: string }) | null>(null);

  // llamada (callout): arrastrar da la recta de siempre —la punta donde
  // empieza el arrastre y la caja donde acaba—; a clics se pone además el
  // **codo**, que es la línea de tres tramos que dibuja Acrobat: primer
  // clic la punta, segundo el codo, tercero la caja
  const [calloutDraft, setCalloutDraft] = useState<
    | (Rect & {
        punta: { x: number; y: number };
        codo?: { x: number; y: number } | null;
        text: string;
      })
    | null
  >(null);
  const calloutStartRef = useRef<{ x: number; y: number } | null>(null);
  const calloutLiveRef = useRef<
    (Rect & { punta: { x: number; y: number }; text: string }) | null
  >(null);
  // los puntos ya puestos a clics (punta, y luego codo); el ref es el que
  // leen los despachadores de ratón, que no ven el estado del render
  const [calloutPuntos, setCalloutPuntos] = useState<
    { x: number; y: number }[]
  >([]);
  const calloutPuntosRef = useRef<{ x: number; y: number }[]>([]);

  // goma de borrar: la zona que se va a llevar (lo que se ve es lo que se
  // borra) y la posición del cursor redondo
  const [gomaRect, setGomaRect] = useState<Rect | null>(null);
  const [gomaPos, setGomaPos] = useState<{ x: number; y: number } | null>(null);
  const gomaStartRef = useRef<{ x: number; y: number } | null>(null);
  const gomaLiveRef = useRef<Rect | null>(null);

  const [shapeDraft, setShapeDraft] = useState<{
    x1: number;
    y1: number;
    x2: number;
    y2: number;
  } | null>(null);
  const shapeStartRef = useRef<{ x: number; y: number } | null>(null);
  const shapeLiveRef = useRef<{ x1: number; y1: number; x2: number; y2: number } | null>(null);

  // Al cambiar de modo: fuera borradores y estado transitorio
  useEffect(() => {
    setStrokePts([]);
    strokeLiveRef.current = [];
    setNoteDraft(null);
    setNotePopover(null);
    setNoteEdit(null);
    setShapeDraft(null);
    shapeStartRef.current = null;
    shapeLiveRef.current = null;
    setFreeTextDraft(null);
    freeTextStartRef.current = null;
    freeTextLiveRef.current = null;
    setCalloutDraft(null);
    calloutStartRef.current = null;
    calloutLiveRef.current = null;
    calloutPuntosRef.current = [];
    setCalloutPuntos([]);
    setGomaRect(null);
    setGomaPos(null);
    gomaStartRef.current = null;
    gomaLiveRef.current = null;
    setAnnotDraft(null);
    annotActionRef.current = null;
    annotLiveRef.current = null;
  }, [mode]);

  // Solo una página puede tener la selección viva
  useEffect(() => {
    if (selOwner !== index) {
      setNotePopover(null);
    }
  }, [selOwner, index]);

  // Anotaciones de la página (iconos de nota, overlays y popovers)
  useEffect(() => {
    if (!workPath || !visible) return;
    let cancelled = false;
    setNotePopover(null);
    invoke<AnnotationInfo[]>("get_annotations", { path: workPath, pageIndex: index })
      .then((a) => {
        if (cancelled) return;
        setAnnots(a);
        // el borrador de un arrastre aguanta hasta aquí para no ver saltos
        setAnnotDraft(null);
      })
      .catch(() => {
        if (!cancelled) setAnnots([]);
      });
    return () => {
      cancelled = true;
    };
  }, [workPath, index, visible, docVersion, annotVersion, pageVersion]);

  // El panel de comentarios elige uno: en cuanto esta página tiene sus
  // anotaciones cargadas, se abre su popover
  useEffect(() => {
    if (seleccionExterna === null) return;
    const a = annots.find((x) => x.index === seleccionExterna);
    if (a) setNotePopover(a);
  }, [seleccionExterna, annots]);

  /** Resalta, subraya o tacha la selección actual. */
  async function markupSelection(kind: "highlight" | "underline" | "strikeout") {
    if (!workPath || !seleccion.selection || !seleccion.pageText) return;
    const rects = mergeLineRects(
      seleccion.pageText.chars.slice(seleccion.selection.start, seleccion.selection.end + 1),
    );
    if (rects.length === 0) return;
    try {
      const accion =
        kind === "highlight"
          ? "resaltar"
          : kind === "underline"
            ? "subrayar"
            : "tachar";
      const colorHex =
        tool.markupPending ?? tool.markupColors[accion];
      await addMarkup({
        workPath,
        pageIndex: index,
        // los comandos que escriben trabajan en el espacio propio de la
        // página; en una página girada no es el mismo que el de la vista
        rects: rects.map((r) => rectAPagina(r, size)),
        kind,
        color: hexToRgba(colorHex, kind === "highlight" ? 140 : 255),
        author: autorComentarios(),
      });
      tool.onMarkupUsed(kind, colorHex);
      seleccion.setSelection(null);
      onAnnotated(index);
    } catch (e) {
      onError(e);
    }
  }

  async function commitShape(d: {
    x1: number;
    y1: number;
    x2: number;
    y2: number;
  }) {
    if (!workPath) return;
    const fillable = tool.shapeKind === "rect" || tool.shapeKind === "ellipse";
    const p1 = puntoAPagina({ x: d.x1, y: d.y1 }, size);
    const p2 = puntoAPagina({ x: d.x2, y: d.y2 }, size);
    try {
      await addShape({
        workPath,
        pageIndex: index,
        kind: tool.shapeKind,
        x1: p1.x,
        y1: p1.y,
        x2: p2.x,
        y2: p2.y,
        stroke: hexToRgba(tool.shapeColor),
        fill: tool.shapeFill && fillable ? hexToRgba(tool.shapeColor, 70) : null,
        strokeWidth: tool.shapeWidth,
        author: autorComentarios(),
      });
      onAnnotated(index);
    } catch (e) {
      onError(e);
    }
  }

  /** Crea el cuadro de texto con lo escrito dentro (⌘Enter o «Añadir»).
   *  El texto se reparte en líneas al ancho de la caja antes de mandarlo:
   *  la apariencia que se guarda en el PDF es la que se ve al escribir, y no
   *  una frase que se sale por el borde derecho en cualquier visor. */
  async function commitFreeText() {
    const d = freeTextDraft;
    if (!workPath || !d) return;
    if (!d.text.trim()) {
      setFreeTextDraft(null);
      return;
    }
    const lineas = ajustaLineas(d.text, d.w, tool.freeTextSize);
    // la caja crece hasta caber, como en Acrobat: nunca recorta lo escrito
    const alto = Math.max(d.h, altoCuadro(lineas.length, tool.freeTextSize));
    try {
      await addFreeText({
        workPath,
        pageIndex: index,
        rect: rectAPagina({ x: d.x, y: d.y, w: d.w, h: alto }, size),
        text: lineas.join("\n"),
        fontSize: tool.freeTextSize,
        color: hexToRgba(tool.freeTextColor),
        border: tool.freeTextBorder,
        author: autorComentarios(),
      });
      setFreeTextDraft(null);
      onModeChange("select");
      onAnnotated(index);
    } catch (e) {
      onError(e);
    }
  }

  /** Un clic en modo llamada cuando no ha habido arrastre: pone la punta,
   *  luego el codo y luego la caja. Con dos clics y un arrastre sale la
   *  recta de siempre, así que quien no quiera codo no se entera. */
  function clicCallout(p: { x: number; y: number }) {
    const puntos = [...calloutPuntosRef.current, p];
    if (puntos.length < 3) {
      calloutPuntosRef.current = puntos;
      setCalloutPuntos(puntos);
      return;
    }
    const [punta, codo, caja] = puntos;
    calloutPuntosRef.current = [];
    setCalloutPuntos([]);
    setCalloutDraft({ ...cajaLlamada(punta, caja, size), codo });
  }

  /** Olvida los puntos a medio poner (Esc, o un arrastre que manda). */
  function limpiaPuntosCallout() {
    if (calloutPuntosRef.current.length === 0) return;
    calloutPuntosRef.current = [];
    setCalloutPuntos([]);
  }

  /** Crea la llamada con lo escrito dentro (⌘Enter o «Añadir»). La caja se
   *  parte en líneas al ancho pedido, igual que el cuadro de texto: la
   *  apariencia que se guarda es la que se ve al escribir. */
  async function commitCallout() {
    const d = calloutDraft;
    if (!workPath || !d) return;
    if (!d.text.trim()) {
      setCalloutDraft(null);
      return;
    }
    const lineas = ajustaLineas(d.text, d.w, tool.freeTextSize);
    const alto = Math.max(d.h, altoCuadro(lineas.length, tool.freeTextSize));
    const punta = puntoAPagina(d.punta, size);
    const codoEnPagina = (c: { x: number; y: number }): [number, number] => {
      const q = puntoAPagina(c, size);
      return [q.x, q.y];
    };
    try {
      await addCallout({
        workPath,
        pageIndex: index,
        rect: rectAPagina({ x: d.x, y: d.y, w: d.w, h: alto }, size),
        punta: [punta.x, punta.y],
        text: lineas.join("\n"),
        color: hexToRgba(tool.freeTextColor),
        codo: d.codo ? codoEnPagina(d.codo) : null,
        author: autorComentarios(),
      });
      setCalloutDraft(null);
      onModeChange("select");
      onAnnotated(index);
    } catch (e) {
      onError(e);
    }
  }

  /** Goma: se lleva de cada trazo (`Ink`) los segmentos que caen dentro de
   *  la zona borrada, no el trazo entero. La zona es la que se pinta
   *  mientras se arrastra, así que lo que se ve es lo que se va.
   *
   *  Qué trazos toca la zona lo decide el backend, que es de quien es esa
   *  geometría, y el lote entero va en una sola mutación: un pase de goma,
   *  un ⌘Z. */
  async function borraConGoma(zona: Rect) {
    if (!workPath) return;
    const pr = rectAPagina(zona, size);
    try {
      const { tocados } = await eraseInkArea(workPath, index, pr);
      if (tocados === 0) return;
      onAnnotated(index);
      onNotice(`Borrado · ${MOD}Z lo devuelve`);
    } catch (e) {
      onError(e);
    }
  }

  /** Marca de «rellenar y firmar»: ✓, ✗, ● o una línea, colocada con un
   *  clic donde el usuario señala. Son las de siempre —Ink y formas—, así
   *  que se mueven después con el mismo arrastre que un sello y ⌘Z las
   *  quita. Es el flujo con el que se rellena un formulario escaneado. */
  async function colocaMarca(kind: MarcaRellenar, x: number, y: number) {
    if (!workPath) return;
    const color = hexToRgba(tool.fillColor);
    // tamaños fijos, los de Acrobat: se colocan y se ajustan arrastrando
    const r = 7;
    const punto = (px: number, py: number) => {
      const p = puntoAPagina({ x: px, y: py }, size);
      return [p.x, p.y] as [number, number];
    };
    try {
      if (kind === "dot" || kind === "line") {
        const p1 = puntoAPagina(
          kind === "dot" ? { x: x - r, y: y - r } : { x: x - 30, y },
          size,
        );
        const p2 = puntoAPagina(
          kind === "dot" ? { x: x + r, y: y + r } : { x: x + 30, y },
          size,
        );
        await addShape({
          workPath,
          pageIndex: index,
          kind: kind === "dot" ? "ellipse" : "line",
          x1: p1.x,
          y1: p1.y,
          x2: p2.x,
          y2: p2.y,
          stroke: color,
          fill: kind === "dot" ? color : null,
          strokeWidth: 2,
          author: autorComentarios(),
        });
      } else {
        // el aspa se traza de una sola pasada volviendo sobre su diagonal:
        // dos trazos serían dos pasos de deshacer
        const puntos =
          kind === "check"
            ? [
                punto(x - r, y),
                punto(x - r / 3, y + r),
                punto(x + r, y - r),
              ]
            : [
                punto(x - r, y - r),
                punto(x + r, y + r),
                punto(x, y),
                punto(x + r, y - r),
                punto(x - r, y + r),
              ];
        await invoke("add_stroke", {
          workPath,
          pageIndex: index,
          points: puntos,
          color,
          width: 2,
          author: autorComentarios(),
        });
      }
      onAnnotated(index);
    } catch (e) {
      onError(e);
    }
  }

  async function placeStamp(x: number, y: number) {
    const text = tool.stampText.trim();
    if (!workPath || !text) return;
    const p = puntoAPagina({ x, y }, size);
    // Un sello dinámico lleva debajo quién sella y cuándo. Con el nombre
    // puesto en Preferencias se compone aquí; **sin él se manda la
    // plantilla** y la compone el backend, que sí conoce el usuario del
    // sistema: componerla aquí dejaba el sello sin el nombre que promete.
    const autor = autorComentarios();
    const dinamico = !tool.stampDinamico
      ? null
      : autor
        ? selloDinamico(autor)
        : (PLANTILLAS_DINAMICAS[text.toUpperCase()] ?? selloDinamico(""));
    try {
      await addStamp({
        workPath,
        pageIndex: index,
        text,
        color: hexToRgba(tool.stampColor),
        x: p.x,
        y: p.y,
        fontSize: 22,
        author: autor,
        dinamico,
      });
      tool.onStampUsed(text, tool.stampDinamico);
      onAnnotated(index);
    } catch (e) {
      onError(e);
    }
  }

  /** «Adjuntar aquí»: el fichero se mete dentro del PDF y queda como un
   *  comentario con chincheta en ese punto, que es el `/FileAttachment` de
   *  Acrobat. El adjunto del documento (`add_attachment`) es otra cosa: va
   *  en `/EmbeddedFiles` y no está en ninguna página. */
  async function adjuntaFichero(x: number, y: number) {
    if (!workPath) return;
    const sel = await open({ multiple: false, title: "Adjuntar a la página" });
    if (typeof sel !== "string") return;
    const p = puntoAPagina({ x, y }, size);
    try {
      await addFileAttachmentAnnotation({
        workPath,
        pageIndex: index,
        punto: [p.x, p.y],
        srcPath: sel,
        author: autorComentarios(),
      });
      onAnnotated(index);
      onNotice(
        `${sel.split(/[\\/]/).pop()} va dentro del documento · ${MOD}Z lo quita`,
      );
    } catch (e) {
      onError(e);
    }
  }

  /** Doble clic en la chincheta: el backend saca el fichero al temporal y
   *  el visor del sistema lo abre, que es lo que hace Acrobat y lo que ya
   *  hace el panel de adjuntos del documento. Sin esto, lo que se adjunta a
   *  una página no se puede volver a sacar. */
  async function abreAdjuntoDePagina(a: AnnotationInfo) {
    if (!workPath) return;
    try {
      const ruta = await openPageAttachment(workPath, index, a.index);
      await abrirRuta(ruta);
      onNotice(`Abriendo ${a.contents || "el adjunto"} con el visor del sistema…`);
    } catch (e) {
      onError(e);
    }
  }

  /** «Guardar como…» del popover: el fichero sale del PDF con su nombre y
   *  su extensión. */
  async function guardaAdjuntoDePagina(a: AnnotationInfo) {
    if (!workPath) return;
    const dest = await save({
      defaultPath: a.contents || "adjunto",
      title: "Guardar el adjunto",
    });
    if (!dest) return;
    try {
      await savePageAttachment(workPath, index, a.index, dest);
      onNotice(`${a.contents || "Adjunto"} guardado en ${dest}`);
    } catch (e) {
      onError(e);
    }
  }

  async function submitNote() {
    if (!workPath || !noteDraft || !noteDraft.text.trim()) {
      setNoteDraft(null);
      return;
    }
    const p = puntoAPagina({ x: noteDraft.x, y: noteDraft.y }, size);
    try {
      await invoke("add_note", {
        workPath,
        pageIndex: index,
        x: p.x,
        y: p.y,
        text: noteDraft.text,
        author: autorComentarios(),
      });
      setNoteDraft(null);
      onModeChange("select");
      onAnnotated(index);
    } catch (e) {
      onError(e);
    }
  }

  async function finishStroke() {
    const pts = strokeLiveRef.current;
    strokeLiveRef.current = [];
    setStrokePts([]);
    if (!workPath || pts.length < 2) return;
    try {
      await invoke("add_stroke", {
        workPath,
        pageIndex: index,
        points: pts.map((q) => {
          const p = puntoAPagina({ x: q[0], y: q[1] }, size);
          return [p.x, p.y];
        }),
        color: hexToRgba(tool.drawColor),
        width: tool.drawWidth,
        author: autorComentarios(),
      });
      onAnnotated(index);
    } catch (e) {
      onError(e);
    }
  }

  /** Guarda el texto corregido de un comentario (⌘Enter, «Guardar» o un
   *  clic fuera, que es como confirma Acrobat). */
  const guardarContenido = useCallback(
    async (annot: AnnotationInfo, texto: string) => {
      setNoteEdit(null);
      if (!workPath || texto === annot.contents) return;
      try {
        await setAnnotationContents({
          workPath,
          pageIndex: index,
          annotIndex: annot.index,
          contents: texto,
          author: autorComentarios(),
        });
        onAnnotated(index);
      } catch (e) {
        onError(e);
      }
    },
    [workPath, index, onAnnotated, onError],
  );

  /** Recolorea un comentario al instante, sin «Aceptar» (Acrobat). Un solo
   *  comando, así que ⌘Z lo devuelve de una vez. */
  const cambiarColor = useCallback(
    async (annot: AnnotationInfo, hex: string) => {
      if (!workPath) return;
      try {
        await setAnnotationColor({
          workPath,
          pageIndex: index,
          annotIndex: annot.index,
          // el resaltado va traslúcido para no tapar el texto que marca
          color: hexToRgba(hex, annot.kind === "Highlight" ? 140 : 255),
        });
        onAnnotated(index);
      } catch (e) {
        onError(e);
      }
    },
    [workPath, index, onAnnotated, onError],
  );

  /** Cierra el popover confirmando antes lo que se estuviera escribiendo. */
  const cerrarPopover = useCallback(() => {
    if (notePopover && noteEdit?.index === notePopover.index) {
      guardarContenido(notePopover, noteEdit.text);
    }
    setNoteEdit(null);
    setNotePopover(null);
  }, [notePopover, noteEdit, guardarContenido]);

  const deleteAnnotation = useCallback(
    async (annot: AnnotationInfo) => {
      if (!workPath) return;
      try {
        await invoke("remove_annotation", {
          workPath,
          pageIndex: index,
          annotIndex: annot.index,
        });
        setNoteEdit(null);
        setNotePopover(null);
        onAnnotated(index);
        // sin confirmación: borrar un comentario es rutinario y ⌘Z lo
        // devuelve, pero el aviso lo dice para que nadie se quede con la duda
        onNotice(`Comentario eliminado · ${MOD}Z para deshacer`);
      } catch (e) {
        onError(e);
      }
    },
    [workPath, index, onAnnotated, onError, onNotice],
  );

  // Supr o Retroceso borran el comentario seleccionado, como en Acrobat
  useEffect(() => {
    const elegida = notePopover;
    if (!elegida) return;
    function onKey(e: KeyboardEvent) {
      if (e.key !== "Delete" && e.key !== "Backspace") return;
      const tag = (e.target as HTMLElement)?.tagName;
      if (tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT") return;
      if (document.querySelector(".modal-backdrop")) return;
      e.preventDefault();
      if (elegida) deleteAnnotation(elegida);
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [notePopover, deleteAnnotation]);

  /** Clic simple en modo selección: abre el popover de la anotación pulsada. */
  function onClickLayer(e: MouseEvent<HTMLDivElement>) {
    if (mode !== "select" || seleccion.selection) return;
    const { x, y } = pagePoint(e, scale, viewRotation);
    // Ink y Stamp tienen su propio overlay arrastrable con su mousedown
    const CLICKABLE = ["Highlight", "Underline", "Strikeout", "StrikeOut"];
    const hit = annots.find((a) => {
      if (!CLICKABLE.includes(a.kind)) return false;
      const zonas =
        a.rects.length > 0 ? a.rects : [{ x: a.x, y: a.y, w: a.w, h: a.h }];
      return zonas.some(
        (r) => x >= r.x && x <= r.x + r.w && y >= r.y && y <= r.y + r.h,
      );
    });
    if (hit) setNotePopover(hit);
  }

  /** Doble clic sobre un resaltado, un subrayado o un tachado: abre el
   *  mismo `textarea` que ya abre sobre una nota, que es lo que hace
   *  Acrobat cuando se le asocia un comentario a una marca. */
  const comentarMarca = useCallback((a: AnnotationInfo) => {
    setNotePopover(a);
    setNoteEdit({ index: a.index, text: a.contents });
  }, []);

  function startAnnotAction(
    e: MouseEvent<HTMLElement>,
    a: AnnotationInfo,
    kind: "move" | "resize",
    handle: ResizeHandle = "se",
  ) {
    e.stopPropagation();
    if (e.button !== 0) return;
    const p = puntoEnCapa(e, scale, viewRotation);
    annotActionRef.current = {
      kind,
      handle,
      startX: p.x,
      startY: p.y,
      orig: a,
      moved: false,
    };
    cerrarPopover();
    const d = { index: a.index, x: a.x, y: a.y, w: a.w, h: a.h };
    annotLiveRef.current = d;
    setAnnotDraft(d);
  }

  async function commitAnnot(a: AnnotationInfo, r: Rect) {
    if (!workPath) return;
    const pr = rectAPagina(r, size);
    try {
      await transformAnnotation({
        workPath,
        pageIndex: index,
        annotIndex: a.index,
        x: pr.x,
        y: pr.y,
        w: pr.w,
        h: pr.h,
      });
      onAnnotated(index);
    } catch (e) {
      setAnnotDraft(null);
      onError(e);
    }
  }

  return {
    annots,
    notePopover,
    setNotePopover,
    noteEdit,
    setNoteEdit,
    guardarContenido,
    cambiarColor,
    cerrarPopover,
    noteDraft,
    setNoteDraft,
    strokePts,
    setStrokePts,
    strokeLiveRef,
    annotDraft,
    setAnnotDraft,
    annotActionRef,
    annotLiveRef,
    shapeDraft,
    setShapeDraft,
    shapeStartRef,
    shapeLiveRef,
    freeTextDraft,
    setFreeTextDraft,
    freeTextStartRef,
    freeTextLiveRef,
    commitFreeText,
    calloutDraft,
    setCalloutDraft,
    calloutStartRef,
    calloutLiveRef,
    calloutPuntos,
    clicCallout,
    limpiaPuntosCallout,
    commitCallout,
    gomaRect,
    setGomaRect,
    gomaPos,
    setGomaPos,
    gomaStartRef,
    gomaLiveRef,
    borraConGoma,
    comentarMarca,
    markupSelection,
    commitShape,
    placeStamp,
    adjuntaFichero,
    abreAdjuntoDePagina,
    guardaAdjuntoDePagina,
    colocaMarca,
    submitNote,
    finishStroke,
    deleteAnnotation,
    onClickLayer,
    startAnnotAction,
    commitAnnot,
  };
}

export type Anotaciones = ReturnType<typeof useAnotaciones>;
