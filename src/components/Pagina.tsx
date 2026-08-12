/**
 * Una página del documento en el scroll continuo: render, capa de texto,
 * anotaciones, formularios y todas las herramientas de edición. Carga sus
 * datos de forma perezosa cuando entra en el viewport (± un viewport de
 * margen) y mientras tanto ocupa su sitio con un hueco del tamaño real.
 */
import { memo, useEffect, useRef, useState } from "react";
import { invoke } from "../ipc";
import { open } from "../dialogos";
import {
  addMarkup,
  addShape,
  addStamp,
  createFormField,
  createLink,
  cropPage,
  deleteFormField,
  getImageData,
  getLinks,
  redactArea,
  stampSignature,
  transformAnnotation,
  type LinkInfo,
  type RedactReport,
} from "../api";
import {
  ANNOT_COLORS,
  charIndexAt,
  copyToClipboard,
  FONT_CHOICES,
  hexToRgba,
  KIND_LABELS,
  mergeLineRects,
  type AnnotationInfo,
  type FormFieldInfo,
  type ImageInfo,
  type ImgAction,
  type Mode,
  type PageSize,
  type PageText,
  type Rect,
  type ResizeHandle,
  type Selection,
  type ShapeKind,
  type TextBlock,
} from "../tipos";
import Icon from "./Icon";

/** Estado global de herramienta que necesitan los handlers de la página. */
export type ToolProps = {
  drawColor: string;
  drawWidth: number;
  markupPending: string | null;
  onMarkupPending: (c: string | null) => void;
  markupColors: { resaltar: string; subrayar: string; tachar: string };
  onMarkupUsed: (
    kind: "highlight" | "underline" | "strikeout",
    color: string,
  ) => void;
  shapeKind: ShapeKind;
  shapeColor: string;
  shapeFill: boolean;
  shapeWidth: number;
  stampText: string;
  stampCustom: string;
  stampColor: string;
  activeSig: { png: string; ratio: number } | null;
};

/** Coincidencia de búsqueda de esta página (groupIndex = índice global). */
export type PageMatch = { rects: Rect[]; groupIndex: number };

type Props = {
  index: number;
  workPath: string;
  size: PageSize;
  pageCount: number;
  displayWidth: number;
  devicePixelRatio: number;
  docVersion: number;
  annotVersion: number;
  pageVersion: number;
  mode: Mode;
  tool: ToolProps;
  matches?: PageMatch[];
  currentGroup: number;
  selOwner: number | null;
  claimSel: (page: number | null) => void;
  requestRender: (page: number, width: number, pv: number) => Promise<string>;
  registerEl: (page: number, el: HTMLDivElement | null) => void;
  onAnnotated: (page: number, pushUndo?: boolean) => void;
  onPageMutated: (page: number) => void;
  onDocMutated: (newCount: number, nextPage?: number) => void;
  onError: (e: unknown) => void;
  onModeChange: (m: Mode) => void;
  onLinkGoto: (page: number) => void;
  onLinkUri: (uri: string) => void;
  onSigStamped: () => void;
};

function Pagina({
  index,
  workPath,
  size,
  pageCount,
  displayWidth,
  devicePixelRatio,
  docVersion,
  annotVersion,
  pageVersion,
  mode,
  tool,
  matches,
  currentGroup,
  selOwner,
  claimSel,
  requestRender,
  registerEl,
  onAnnotated,
  onPageMutated,
  onDocMutated,
  onError,
  onModeChange,
  onLinkGoto,
  onLinkUri,
  onSigStamped,
}: Props) {
  const wrapRef = useRef<HTMLDivElement | null>(null);
  const [visible, setVisible] = useState(false);
  const [imgSrc, setImgSrc] = useState<string | null>(null);

  const [pageText, setPageText] = useState<PageText | null>(null);
  const [selection, setSelection] = useState<Selection | null>(null);
  const [dragging, setDragging] = useState(false);
  const anchorRef = useRef<number | null>(null);
  const downPosRef = useRef<{ x: number; y: number } | null>(null);

  const [annots, setAnnots] = useState<AnnotationInfo[]>([]);
  const [notePopover, setNotePopover] = useState<AnnotationInfo | null>(null);
  const [noteDraft, setNoteDraft] = useState<{
    x: number;
    y: number;
    text: string;
  } | null>(null);
  const [strokePts, setStrokePts] = useState<[number, number][]>([]);
  const strokeLiveRef = useRef<[number, number][]>([]);

  const [formFields, setFormFields] = useState<FormFieldInfo[]>([]);
  const [fieldDraft, setFieldDraft] = useState<{
    field: FormFieldInfo;
    text: string;
  } | null>(null);
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
  const [images, setImages] = useState<ImageInfo[]>([]);
  const [imgPreviews, setImgPreviews] = useState<Record<number, string>>({});
  const [imgDraft, setImgDraft] = useState<ImageInfo | null>(null);
  const [imagePopover, setImagePopover] = useState<ImageInfo | null>(null);
  const imgActionRef = useRef<ImgAction | null>(null);
  // parche que tapa la copia original (quemada en el bitmap) durante un
  // arrastre de imagen; se limpia cuando llega el bitmap actualizado
  const [imgPatch, setImgPatch] = useState<{ rect: Rect; color: string } | null>(null);
  // arrastre/redimensionado de anotaciones (sellos y dibujos) en modo select
  const [annotDraft, setAnnotDraft] = useState<(Rect & { index: number }) | null>(null);
  const annotActionRef = useRef<{
    kind: "move" | "resize";
    startX: number;
    startY: number;
    orig: AnnotationInfo;
    moved: boolean;
  } | null>(null);
  const annotLiveRef = useRef<(Rect & { index: number }) | null>(null);
  const [links, setLinks] = useState<LinkInfo[]>([]);

  const [shapeDraft, setShapeDraft] = useState<{
    x1: number;
    y1: number;
    x2: number;
    y2: number;
  } | null>(null);
  const shapeStartRef = useRef<{ x: number; y: number } | null>(null);
  const shapeLiveRef = useRef<{ x1: number; y1: number; x2: number; y2: number } | null>(null);
  const [cropDraft, setCropDraft] = useState<Rect | null>(null);
  const cropStartRef = useRef<{ x: number; y: number } | null>(null);
  const [redactDraft, setRedactDraft] = useState<Rect | null>(null);
  const redactStartRef = useRef<{ x: number; y: number } | null>(null);
  const redactLiveRef = useRef<Rect | null>(null);
  const [redactReport, setRedactReport] = useState<RedactReport | null>(null);
  const [formDraft, setFormDraft] = useState<Rect | null>(null);
  const formStartRef = useRef<{ x: number; y: number } | null>(null);
  const formLiveRef = useRef<Rect | null>(null);
  const [formName, setFormName] = useState("campo");
  const [formKind, setFormKind] = useState<"text" | "checkbox">("text");
  const [linkDraft, setLinkDraft] = useState<Rect | null>(null);
  const linkStartRef = useRef<{ x: number; y: number } | null>(null);
  const linkLiveRef = useRef<Rect | null>(null);
  const [linkTipo, setLinkTipo] = useState<"url" | "pagina">("url");
  const [linkValor, setLinkValor] = useState("");
  const [sigDraft, setSigDraft] = useState<Rect | null>(null);
  const sigLiveRef = useRef<Rect | null>(null);
  const sigDragRef = useRef<{ x: number; y: number } | null>(null);

  const hitRef = useRef<HTMLDivElement | null>(null);

  const scale = displayWidth / size.width;
  // tope de píxeles físicos: a zoom alto el CSS escala el resto (un render
  // de 7000px en base64 congela el hilo; a 4096 no se nota y va fluido)
  const renderWidth = Math.min(4096, Math.round(displayWidth * devicePixelRatio));
  const { activeSig } = tool;

  // Visibilidad dentro del visor (± un viewport de margen): fuera de ahí la
  // página es solo un hueco y no carga nada.
  useEffect(() => {
    const el = wrapRef.current;
    if (!el) return;
    const io = new IntersectionObserver(
      ([entry]) => setVisible(entry.isIntersecting),
      { root: el.closest(".viewer"), rootMargin: "100% 0px" },
    );
    io.observe(el);
    return () => io.disconnect();
  }, []);

  // Registro del elemento para gotoPage y el seguimiento del scroll
  useEffect(() => {
    registerEl(index, wrapRef.current);
    return () => registerEl(index, null);
  }, [index, registerEl]);

  // Render de la página (instantáneo si ya está en el caché global). Los
  // cambios de ancho (zoom) se debouncean: el PNG anterior se estira por CSS
  // al momento y el render nítido se pide cuando se deja de pulsar — así una
  // ráfaga de ⌘± no encola renders obsoletos en el hilo de PDFium.
  const lastWidthRef = useRef(0);
  useEffect(() => {
    if (!visible) return;
    let cancelled = false;
    const go = () => {
      lastWidthRef.current = renderWidth;
      requestRender(index, renderWidth, pageVersion)
        .then((src) => {
          if (!cancelled) setImgSrc(src);
        })
        .catch((e) => {
          if (!cancelled) onError(e);
        });
    };
    const soloZoom = lastWidthRef.current !== 0 && lastWidthRef.current !== renderWidth;
    const t = setTimeout(go, soloZoom ? 160 : 0);
    return () => {
      cancelled = true;
      clearTimeout(t);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [visible, workPath, index, renderWidth, docVersion, annotVersion, pageVersion]);

  // Al cambiar de modo: fuera borradores y estado transitorio
  useEffect(() => {
    setSelection(null);
    setStrokePts([]);
    strokeLiveRef.current = [];
    setNoteDraft(null);
    setNotePopover(null);
    setSigDraft(null);
    sigLiveRef.current = null;
    sigDragRef.current = null;
    setShapeDraft(null);
    shapeStartRef.current = null;
    shapeLiveRef.current = null;
    setCropDraft(null);
    cropStartRef.current = null;
    setRedactDraft(null);
    setRedactReport(null);
    redactStartRef.current = null;
    redactLiveRef.current = null;
    setFormDraft(null);
    formStartRef.current = null;
    formLiveRef.current = null;
    setLinkDraft(null);
    linkStartRef.current = null;
    linkLiveRef.current = null;
    setImagePopover(null);
    setImgDraft(null);
    imgActionRef.current = null;
    setImgPatch(null);
    setAnnotDraft(null);
    annotActionRef.current = null;
    annotLiveRef.current = null;
    setFieldDraft(null);
    setBlockDraft(null);
    setNewTextDraft(null);
  }, [mode]);

  // Solo una página puede tener la selección viva
  useEffect(() => {
    if (selOwner !== index) {
      setSelection(null);
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

  // Capa de texto de la página
  useEffect(() => {
    if (!workPath || !visible) return;
    let cancelled = false;
    setPageText(null);
    setSelection(null);
    invoke<PageText>("get_page_text", { path: workPath, pageIndex: index })
      .then((t) => {
        if (!cancelled) setPageText(t);
      })
      .catch((e) => {
        if (!cancelled) onError(e);
      });
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [workPath, index, visible, docVersion, pageVersion]);

  // Enlaces de la página (zonas clicables en modo selección)
  useEffect(() => {
    if (!workPath || !visible) {
      setLinks([]);
      return;
    }
    let cancelled = false;
    getLinks(workPath, index)
      .then((l) => {
        if (!cancelled) setLinks(l);
      })
      .catch(() => {
        if (!cancelled) setLinks([]);
      });
    return () => {
      cancelled = true;
    };
  }, [workPath, index, visible, docVersion, pageVersion]);

  // Bloques de texto (solo en modo edición)
  useEffect(() => {
    setNewTextDraft(null);
    if (!workPath || !visible || mode !== "edit") {
      setTextBlocks([]);
      setBlockDraft(null);
      return;
    }
    let cancelled = false;
    invoke<TextBlock[]>("get_text_blocks", { path: workPath, pageIndex: index })
      .then((b) => {
        if (!cancelled) setTextBlocks(b);
      })
      .catch((e) => {
        if (!cancelled) onError(e);
      });
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [workPath, index, visible, docVersion, mode, pageVersion]);

  // Imágenes de la página (solo en modo imagen). Se precarga también su
  // contenido para que al arrastrar se mueva la imagen, no solo el recuadro.
  useEffect(() => {
    setImagePopover(null);
    imgActionRef.current = null;
    setImgPreviews({});
    if (!workPath || !visible || mode !== "image") {
      setImages([]);
      setImgDraft(null);
      setImgPatch(null);
      return;
    }
    let cancelled = false;
    invoke<ImageInfo[]>("get_images", { path: workPath, pageIndex: index })
      .then((list) => {
        if (cancelled) return;
        setImages(list);
        // el borrador y el parche del arrastre aguantan hasta que llegan los
        // datos frescos: así no reaparece la copia vieja mientras se re-renderiza
        setImgDraft(null);
        setImgPatch(null);
        for (const im of list) {
          getImageData(workPath, index, im.object_index)
            .then((b64) => {
              if (!cancelled)
                setImgPreviews((p) => ({ ...p, [im.object_index]: b64 }));
            })
            .catch(() => {
              // sin vista previa: al arrastrar se verá solo el recuadro
            });
        }
      })
      .catch((e) => {
        if (!cancelled) onError(e);
      });
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [workPath, index, visible, docVersion, mode, pageVersion]);

  // Centrar el visor en la coincidencia de búsqueda actual si está aquí.
  // Depende de imgSrc para re-centrar cuando termina el render de la página.
  useEffect(() => {
    if (!matches || !matches.some((g) => g.groupIndex === currentGroup)) return;
    hitRef.current?.scrollIntoView({ block: "center", inline: "center" });
  }, [matches, currentGroup, imgSrc]);

  function copySelection() {
    if (!selection || !pageText) return;
    const text = pageText.chars
      .slice(selection.start, selection.end + 1)
      .map((c) => c.ch)
      .join("");
    copyToClipboard(text);
  }

  // Copiar selección con ⌘C / Ctrl+C
  useEffect(() => {
    if (!selection) return;
    function onKey(e: KeyboardEvent) {
      if ((e.metaKey || e.ctrlKey) && e.key === "c") {
        copySelection();
        e.preventDefault();
      }
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [selection, pageText]);

  /** Resalta, subraya o tacha la selección actual. */
  async function markupSelection(kind: "highlight" | "underline" | "strikeout") {
    if (!workPath || !selection || !pageText) return;
    const rects = mergeLineRects(
      pageText.chars.slice(selection.start, selection.end + 1),
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
        rects,
        kind,
        color: hexToRgba(colorHex, kind === "highlight" ? 140 : 255),
      });
      tool.onMarkupUsed(kind, colorHex);
      setSelection(null);
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
    try {
      await addShape({
        workPath,
        pageIndex: index,
        kind: tool.shapeKind,
        x1: d.x1,
        y1: d.y1,
        x2: d.x2,
        y2: d.y2,
        stroke: hexToRgba(tool.shapeColor),
        fill: tool.shapeFill && fillable ? hexToRgba(tool.shapeColor, 70) : null,
        strokeWidth: tool.shapeWidth,
      });
      onAnnotated(index);
    } catch (e) {
      onError(e);
    }
  }

  async function placeStamp(x: number, y: number) {
    const text =
      tool.stampText === "custom" ? tool.stampCustom.trim() : tool.stampText;
    if (!workPath || !text) return;
    try {
      await addStamp({
        workPath,
        pageIndex: index,
        text,
        color: hexToRgba(tool.stampColor),
        x,
        y,
        fontSize: 22,
      });
      onAnnotated(index);
    } catch (e) {
      onError(e);
    }
  }

  async function submitNote() {
    if (!workPath || !noteDraft || !noteDraft.text.trim()) {
      setNoteDraft(null);
      return;
    }
    try {
      await invoke("add_note", {
        workPath,
        pageIndex: index,
        x: noteDraft.x,
        y: noteDraft.y,
        text: noteDraft.text,
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
        points: pts,
        color: hexToRgba(tool.drawColor),
        width: tool.drawWidth,
      });
      onAnnotated(index);
    } catch (e) {
      onError(e);
    }
  }

  async function deleteAnnotation(annot: AnnotationInfo) {
    if (!workPath) return;
    try {
      await invoke("remove_annotation", {
        workPath,
        pageIndex: index,
        annotIndex: annot.index,
      });
      setNotePopover(null);
      onAnnotated(index, false);
    } catch (e) {
      onError(e);
    }
  }

  /** Clic simple en modo selección: abre el popover de la anotación pulsada. */
  function onClickLayer(e: React.MouseEvent<HTMLDivElement>) {
    if (mode !== "select" || selection) return;
    const { x, y } = pagePoint(e);
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

  async function insertImageAt(x: number, y: number) {
    if (!workPath) return;
    const sel = await open({
      filters: [
        {
          name: "Imagen",
          extensions: ["png", "jpg", "jpeg", "webp", "gif", "bmp"],
        },
      ],
      multiple: false,
      title: "Insertar imagen",
    });
    if (typeof sel !== "string") return;
    try {
      await invoke("add_image", {
        workPath,
        pageIndex: index,
        imagePath: sel,
        x,
        y,
      });
      onPageMutated(index);
    } catch (e) {
      onError(e);
    }
  }

  async function commitImage(objectIndex: number, b: ImageInfo) {
    if (!workPath) return;
    try {
      await invoke("transform_image", {
        workPath,
        pageIndex: index,
        objectIndex,
        x: b.x,
        y: b.y,
        w: b.w,
        h: b.h,
      });
      onPageMutated(index);
    } catch (e) {
      setImgDraft(null);
      setImgPatch(null);
      onError(e);
    }
  }

  /** Color medio del perímetro de un rect en el bitmap de la página, para
   *  tapar la copia original mientras se arrastra (fallback: blanco papel). */
  function sampleAround(r: Rect): string {
    try {
      const el = wrapRef.current?.querySelector("img.page") as HTMLImageElement | null;
      if (!el || !el.naturalWidth) return "#ffffff";
      const cw = Math.min(800, el.naturalWidth);
      const k = cw / size.width;
      const cv = document.createElement("canvas");
      cv.width = cw;
      cv.height = Math.round(el.naturalHeight * (cw / el.naturalWidth));
      const ctx = cv.getContext("2d", { willReadFrequently: true });
      if (!ctx) return "#ffffff";
      ctx.drawImage(el, 0, 0, cv.width, cv.height);
      const pad = 3 * k;
      const x0 = r.x * k - pad;
      const x1 = (r.x + r.w) * k + pad;
      const y0 = r.y * k - pad;
      const y1 = (r.y + r.h) * k + pad;
      const cx = (x0 + x1) / 2;
      const cy = (y0 + y1) / 2;
      let cr = 0;
      let cg = 0;
      let cb = 0;
      let n = 0;
      for (const [px, py] of [
        [cx, y0],
        [cx, y1],
        [x0, cy],
        [x1, cy],
        [x0, y0],
        [x1, y0],
        [x0, y1],
        [x1, y1],
      ]) {
        const xx = Math.round(Math.min(Math.max(px, 0), cv.width - 1));
        const yy = Math.round(Math.min(Math.max(py, 0), cv.height - 1));
        const d = ctx.getImageData(xx, yy, 1, 1).data;
        cr += d[0];
        cg += d[1];
        cb += d[2];
        n++;
      }
      return `rgb(${Math.round(cr / n)}, ${Math.round(cg / n)}, ${Math.round(cb / n)})`;
    } catch {
      return "#ffffff";
    }
  }

  async function replaceImagePick(im: ImageInfo) {
    if (!workPath) return;
    const sel = await open({
      filters: [
        {
          name: "Imagen",
          extensions: ["png", "jpg", "jpeg", "webp", "gif", "bmp"],
        },
      ],
      multiple: false,
      title: "Imagen de reemplazo",
    });
    if (typeof sel !== "string") return;
    try {
      await invoke("replace_image", {
        workPath,
        pageIndex: index,
        objectIndex: im.object_index,
        imagePath: sel,
      });
      setImagePopover(null);
      onPageMutated(index);
    } catch (e) {
      onError(e);
    }
  }

  async function deleteImage(im: ImageInfo) {
    if (!workPath) return;
    try {
      await invoke("delete_image", {
        workPath,
        pageIndex: index,
        objectIndex: im.object_index,
      });
      setImagePopover(null);
      onPageMutated(index);
    } catch (e) {
      onError(e);
    }
  }

  function startImgAction(
    e: React.MouseEvent<HTMLDivElement>,
    im: ImageInfo,
    kind: ImgAction["kind"],
    handle: ResizeHandle = "se",
  ) {
    e.stopPropagation();
    if (e.button !== 0) return;
    const rect = (
      e.currentTarget.closest(".textlayer") as HTMLElement
    ).getBoundingClientRect();
    imgActionRef.current = {
      kind,
      handle,
      startX: (e.clientX - rect.left) / scale,
      startY: (e.clientY - rect.top) / scale,
      orig: im,
      moved: false,
    };
    setImagePopover(null);
    setImgDraft(im);
    setImgPatch({
      rect: { x: im.x, y: im.y, w: im.w, h: im.h },
      color: sampleAround(im),
    });
  }

  function startAnnotAction(
    e: React.MouseEvent<HTMLDivElement>,
    a: AnnotationInfo,
    kind: "move" | "resize",
  ) {
    e.stopPropagation();
    if (e.button !== 0) return;
    const rect = (
      e.currentTarget.closest(".textlayer") as HTMLElement
    ).getBoundingClientRect();
    annotActionRef.current = {
      kind,
      startX: (e.clientX - rect.left) / scale,
      startY: (e.clientY - rect.top) / scale,
      orig: a,
      moved: false,
    };
    setNotePopover(null);
    const d = { index: a.index, x: a.x, y: a.y, w: a.w, h: a.h };
    annotLiveRef.current = d;
    setAnnotDraft(d);
  }

  async function commitAnnot(a: AnnotationInfo, r: Rect) {
    if (!workPath) return;
    try {
      await transformAnnotation({
        workPath,
        pageIndex: index,
        annotIndex: a.index,
        x: r.x,
        y: r.y,
        w: r.w,
        h: r.h,
      });
      onAnnotated(index, false);
    } catch (e) {
      setAnnotDraft(null);
      onError(e);
    }
  }

  async function submitNewText() {
    if (!workPath || !newTextDraft) return;
    if (!newTextDraft.text.trim()) {
      setNewTextDraft(null);
      return;
    }
    try {
      await invoke("add_text_block", {
        workPath,
        pageIndex: index,
        x: newTextDraft.x,
        y: newTextDraft.y,
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
      onAnnotated(index, false);
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
      onAnnotated(index, false);
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

  async function applyCrop(allPages: boolean) {
    if (!workPath || !cropDraft) return;
    try {
      await cropPage(workPath, index, cropDraft, allPages);
      setCropDraft(null);
      onModeChange("select");
      onDocMutated(pageCount);
    } catch (e) {
      onError(e);
    }
  }

  async function previewRedact(r: Rect) {
    if (!workPath) return;
    try {
      setRedactReport(await redactArea(workPath, index, r, true));
    } catch (e) {
      onError(e);
    }
  }

  async function applyRedact() {
    if (!workPath || !redactDraft) return;
    try {
      await redactArea(workPath, index, redactDraft, false);
      setRedactDraft(null);
      setRedactReport(null);
      onModeChange("select");
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
        rect: formDraft,
        name: formName.trim(),
      });
      setFormDraft(null);
      onModeChange("select");
      onPageMutated(index);
    } catch (e) {
      onError(e);
    }
  }

  async function applyLink() {
    if (!workPath || !linkDraft || !linkValor.trim()) return;
    try {
      await createLink({
        workPath,
        pageIndex: index,
        rect: linkDraft,
        uri: linkTipo === "url" ? linkValor.trim() : null,
        destPage:
          linkTipo === "pagina"
            ? Math.max(0, Math.min(pageCount - 1, Number(linkValor) - 1))
            : null,
      });
      setLinkDraft(null);
      onModeChange("select");
      onPageMutated(index);
    } catch (e) {
      onError(e);
    }
  }

  async function stampActiveSignature(r: Rect) {
    if (!workPath || !activeSig) return;
    try {
      await stampSignature({
        workPath,
        pageIndex: index,
        pngBase64: activeSig.png,
        x: r.x,
        y: r.y,
        w: r.w,
        h: r.h,
      });
      setSigDraft(null);
      // la firma estampada es una imagen: el modo imagen permite moverla,
      // redimensionarla o borrarla al instante
      onSigStamped();
      onPageMutated(index);
    } catch (e) {
      onError(e);
    }
  }

  function onLinkClick(l: LinkInfo) {
    if (l.uri) {
      onLinkUri(l.uri);
    } else if (l.dest_page !== null) {
      onLinkGoto(l.dest_page);
    }
  }

  /** Evita que una tarjeta flotante se salga del borde de la página. */
  function clampCardLeft(left: number, w = 260) {
    return Math.max(0, Math.min(left, displayWidth - w));
  }

  function pagePoint(e: React.MouseEvent<HTMLDivElement>) {
    const rect = e.currentTarget.getBoundingClientRect();
    return {
      x: (e.clientX - rect.left) / scale,
      y: (e.clientY - rect.top) / scale,
    };
  }

  function onMouseDown(e: React.MouseEvent<HTMLDivElement>) {
    if (e.button !== 0) return;
    const { x, y } = pagePoint(e);
    if (mode === "draw") {
      strokeLiveRef.current = [[x, y]];
      setStrokePts([[x, y]]);
      return;
    }
    if (mode === "note") {
      setNoteDraft({ x, y, text: "" });
      return;
    }
    if (mode === "edit") {
      // clic en zona libre: añadir texto nuevo ahí (los bloques existentes
      // capturan su propio clic con stopPropagation)
      setBlockDraft(null);
      setNewTextDraft({ x, y, text: "", size: 12, font: "auto" });
      return;
    }
    if (mode === "image") {
      // clic en zona libre: insertar imagen ahí (las cajas de imagen
      // capturan su propio mousedown con stopPropagation)
      setImagePopover(null);
      insertImageAt(x, y);
      return;
    }
    if (mode === "firmar") {
      if (!activeSig) return;
      sigDragRef.current = { x, y };
      setSigDraft(null);
      return;
    }
    if (mode === "shape") {
      shapeStartRef.current = { x, y };
      setShapeDraft(null);
      return;
    }
    if (mode === "stamp") {
      placeStamp(x, y);
      return;
    }
    if (mode === "crop") {
      cropStartRef.current = { x, y };
      setCropDraft(null);
      return;
    }
    if (mode === "redact") {
      redactStartRef.current = { x, y };
      setRedactDraft(null);
      setRedactReport(null);
      return;
    }
    if (mode === "form-new") {
      formStartRef.current = { x, y };
      setFormDraft(null);
      return;
    }
    if (mode === "link-new") {
      linkStartRef.current = { x, y };
      setLinkDraft(null);
      return;
    }
    if (!pageText) return;
    claimSel(index);
    anchorRef.current = charIndexAt(pageText, x, y);
    // el jitter de un clic simple no debe crear una selección de 1 carácter
    // (bloquearía el clic-para-borrar de las anotaciones)
    downPosRef.current = { x: e.clientX, y: e.clientY };
    setDragging(true);
    setSelection(null);
    setNotePopover(null);
  }

  /** Redimensiona un rect desde un tirador: bordes mueven un solo eje,
   *  esquinas mantienen la proporción (con Shift, libre). */
  function resizeRect(
    o: Rect,
    hd: ResizeHandle,
    dx: number,
    dy: number,
    free: boolean,
  ): Rect {
    let left = o.x;
    let top = o.y;
    let right = o.x + o.w;
    let bottom = o.y + o.h;
    if (hd.includes("e")) right = Math.max(left + 8, right + dx);
    if (hd.includes("w")) left = Math.min(right - 8, left + dx);
    if (hd.includes("s")) bottom = Math.max(top + 8, bottom + dy);
    if (hd.includes("n")) top = Math.min(bottom - 8, top + dy);
    let w = right - left;
    let h = bottom - top;
    const esquina = hd.length === 2;
    if (esquina && !free && o.w > 0 && o.h > 0) {
      const ratio = o.h / o.w;
      if (Math.abs(w - o.w) >= Math.abs(h - o.h)) h = w * ratio;
      else w = h / ratio;
      // el ancla es la esquina opuesta al tirador
      if (hd.includes("w")) left = right - w;
      if (hd.includes("n")) top = bottom - h;
    }
    return { x: left, y: top, w, h };
  }

  function onMouseMove(e: React.MouseEvent<HTMLDivElement>) {
    if (!(e.buttons & 1)) return;
    if (mode === "select" && annotActionRef.current) {
      const a = annotActionRef.current;
      const { x, y } = pagePoint(e);
      const dx = x - a.startX;
      const dy = y - a.startY;
      if (Math.abs(dx) + Math.abs(dy) > 1) a.moved = true;
      const o = { x: a.orig.x, y: a.orig.y, w: a.orig.w, h: a.orig.h };
      const r =
        a.kind === "move"
          ? { ...o, x: o.x + dx, y: o.y + dy }
          : resizeRect(o, "se", dx, dy, e.shiftKey);
      const d = { index: a.orig.index, ...r };
      annotLiveRef.current = d;
      setAnnotDraft(d);
      return;
    }
    if (mode === "image" && imgActionRef.current) {
      const a = imgActionRef.current;
      const { x, y } = pagePoint(e);
      const dx = x - a.startX;
      const dy = y - a.startY;
      if (Math.abs(dx) + Math.abs(dy) > 1) a.moved = true;
      if (a.kind === "move") {
        setImgDraft({ ...a.orig, x: a.orig.x + dx, y: a.orig.y + dy });
      } else {
        const r = resizeRect(a.orig, a.handle ?? "se", dx, dy, e.shiftKey);
        setImgDraft({ ...a.orig, ...r });
      }
      return;
    }
    if (mode === "draw") {
      if (strokeLiveRef.current.length === 0) return;
      const { x, y } = pagePoint(e);
      strokeLiveRef.current = [...strokeLiveRef.current, [x, y]];
      setStrokePts(strokeLiveRef.current);
      return;
    }
    if (mode === "firmar") {
      const start = sigDragRef.current;
      if (!activeSig || !start) return;
      const { x, y } = pagePoint(e);
      const w = Math.abs(x - start.x);
      if (w < 4) return;
      const h = w * activeSig.ratio;
      const d = {
        x: Math.min(x, start.x),
        y: y >= start.y ? start.y : start.y - h,
        w,
        h,
      };
      sigLiveRef.current = d;
      setSigDraft(d);
      return;
    }
    if (mode === "shape") {
      const start = shapeStartRef.current;
      if (!start) return;
      const { x, y } = pagePoint(e);
      const d = { x1: start.x, y1: start.y, x2: x, y2: y };
      shapeLiveRef.current = d;
      setShapeDraft(d);
      return;
    }
    if (mode === "crop") {
      const start = cropStartRef.current;
      if (!start) return;
      const { x, y } = pagePoint(e);
      setCropDraft({
        x: Math.min(x, start.x),
        y: Math.min(y, start.y),
        w: Math.abs(x - start.x),
        h: Math.abs(y - start.y),
      });
      return;
    }
    if (mode === "redact") {
      const start = redactStartRef.current;
      if (!start) return;
      const { x, y } = pagePoint(e);
      const d = {
        x: Math.min(x, start.x),
        y: Math.min(y, start.y),
        w: Math.abs(x - start.x),
        h: Math.abs(y - start.y),
      };
      redactLiveRef.current = d;
      setRedactDraft(d);
      return;
    }
    if (mode === "form-new" || mode === "link-new") {
      const start = (mode === "form-new" ? formStartRef : linkStartRef).current;
      if (!start) return;
      const { x, y } = pagePoint(e);
      const d = {
        x: Math.min(x, start.x),
        y: Math.min(y, start.y),
        w: Math.abs(x - start.x),
        h: Math.abs(y - start.y),
      };
      if (mode === "form-new") {
        formLiveRef.current = d;
        setFormDraft(d);
      } else {
        linkLiveRef.current = d;
        setLinkDraft(d);
      }
      return;
    }
    if (mode !== "select" || !pageText || anchorRef.current === null) return;
    const down = downPosRef.current;
    if (
      down &&
      Math.abs(e.clientX - down.x) < 4 &&
      Math.abs(e.clientY - down.y) < 4
    )
      return;
    const { x, y } = pagePoint(e);
    const idx = charIndexAt(pageText, x, y);
    if (idx === null) return;
    const a = anchorRef.current;
    setSelection({ start: Math.min(a, idx), end: Math.max(a, idx) });
  }

  function onMouseUp() {
    anchorRef.current = null;
    setDragging(false);
    if (mode === "select" && annotActionRef.current) {
      const a = annotActionRef.current;
      annotActionRef.current = null;
      const draft = annotLiveRef.current;
      annotLiveRef.current = null;
      if (!a.moved) {
        // clic simple: opciones de la anotación (borrar)
        setAnnotDraft(null);
        setNotePopover(a.orig);
      } else if (draft) {
        commitAnnot(a.orig, draft);
      }
      return;
    }
    if (mode === "firmar") {
      const start = sigDragRef.current;
      sigDragRef.current = null;
      if (!activeSig || !start) return;
      // ref espejo: en un arrastre en un solo frame el estado de React aún
      // no se ha re-renderizado y sigDraft sería el del render anterior
      const draft = sigLiveRef.current;
      sigLiveRef.current = null;
      setSigDraft(null);
      let r: Rect;
      if (draft && draft.w > 12) {
        r = draft;
      } else {
        // clic simple: tamaño por defecto centrado en el punto
        const w = Math.min(180, size.width * 0.5);
        const h = w * activeSig.ratio;
        r = { x: start.x - w / 2, y: start.y - h / 2, w, h };
      }
      r.x = Math.max(0, Math.min(r.x, size.width - r.w));
      r.y = Math.max(0, Math.min(r.y, size.height - r.h));
      stampActiveSignature(r);
      return;
    }
    if (mode === "shape") {
      shapeStartRef.current = null;
      const d = shapeLiveRef.current;
      shapeLiveRef.current = null;
      setShapeDraft(null);
      if (d && Math.abs(d.x2 - d.x1) + Math.abs(d.y2 - d.y1) > 4) {
        commitShape(d);
      }
      return;
    }
    if (mode === "crop") {
      cropStartRef.current = null;
      // el borrador se queda visible; se confirma con los botones
      return;
    }
    if (mode === "redact") {
      redactStartRef.current = null;
      const d = redactLiveRef.current;
      redactLiveRef.current = null;
      if (d && d.w > 6 && d.h > 6) {
        setRedactDraft(d);
        previewRedact(d);
      }
      return;
    }
    if (mode === "form-new") {
      formStartRef.current = null;
      // el borrador queda visible; se confirma en la tarjeta
      return;
    }
    if (mode === "link-new") {
      linkStartRef.current = null;
      return;
    }
    if (mode === "image" && imgActionRef.current) {
      const a = imgActionRef.current;
      imgActionRef.current = null;
      const draft = imgDraft;
      if (!a.moved) {
        // clic simple: abrir el popover de la imagen
        setImgDraft(null);
        setImgPatch(null);
        setImagePopover(a.orig);
      } else if (draft) {
        // el borrador y el parche se quedan hasta que llegan los datos
        // frescos (efecto de imágenes): sin salto atrás ni doble copia
        commitImage(a.orig.object_index, draft);
      }
      return;
    }
    if (mode === "draw" && strokeLiveRef.current.length > 0) finishStroke();
  }

  const selectionRects =
    selection && pageText
      ? mergeLineRects(pageText.chars.slice(selection.start, selection.end + 1))
      : [];
  const lastSelRect =
    selectionRects.length > 0
      ? selectionRects[selectionRects.length - 1]
      : null;

  return (
    <div
      ref={wrapRef}
      className="page-wrap"
      style={{ width: displayWidth }}
      data-page={index}
    >
      <span className="esquina a" />
      <span className="esquina b" />
      <span className="esquina c" />
      <span className="esquina d" />
      {imgSrc ? (
        <img
          className="page"
          src={imgSrc}
          draggable={false}
          alt={`Página ${index + 1}`}
        />
      ) : (
        <div
          className="page-hueco"
          style={{ height: (displayWidth * size.height) / size.width }}
        />
      )}
      {visible && imgSrc && (
        <div
          className={`textlayer mode-${mode}`}
          onMouseDown={onMouseDown}
          onMouseMove={onMouseMove}
          onMouseUp={onMouseUp}
          onClick={onClickLayer}
        >
          {annots
            .filter((a) => a.kind === "Highlight")
            .flatMap((a) =>
              a.rects.map((r, j) => (
                <div
                  key={`h${a.index}-${j}`}
                  className="annot-highlight"
                  style={{
                    left: r.x * scale,
                    top: r.y * scale,
                    width: r.w * scale,
                    height: r.h * scale,
                    background: a.color
                      ? `rgba(${a.color[0]}, ${a.color[1]}, ${a.color[2]}, 0.45)`
                      : undefined,
                  }}
                />
              )),
            )}
          {annots
            .filter(
              (a) =>
                a.kind === "Underline" ||
                a.kind === "Strikeout" ||
                a.kind === "StrikeOut",
            )
            .flatMap((a) =>
              a.rects.map((r, j) => (
                <div
                  key={`u${a.index}-${j}`}
                  className={
                    a.kind === "Underline" ? "annot-underline" : "annot-strike"
                  }
                  style={{
                    left: r.x * scale,
                    top:
                      a.kind === "Underline"
                        ? (r.y + r.h) * scale - 2
                        : (r.y + r.h * 0.55) * scale - 1,
                    width: r.w * scale,
                    background: a.color
                      ? `rgba(${a.color[0]}, ${a.color[1]}, ${a.color[2]}, 0.9)`
                      : undefined,
                  }}
                />
              )),
            )}
          {mode === "select" &&
            annots
              .filter((a) => a.kind === "Ink" || a.kind === "Stamp")
              .map((a) => {
                const d =
                  annotDraft && annotDraft.index === a.index ? annotDraft : a;
                return (
                  <div
                    key={`an${a.index}`}
                    className="annot-hit"
                    title="Arrastrar para mover · clic para opciones"
                    style={{
                      left: d.x * scale,
                      top: d.y * scale,
                      width: d.w * scale,
                      height: d.h * scale,
                    }}
                    onMouseDown={(e) => startAnnotAction(e, a, "move")}
                  >
                    <div
                      className="image-handle h-se"
                      title="Redimensionar (Shift: libre)"
                      onMouseDown={(e) => startAnnotAction(e, a, "resize")}
                    />
                  </div>
                );
              })}
          {mode === "edit" &&
            textBlocks.map((b) => (
              <div
                key={`b${b.object_index}`}
                className="text-block"
                style={{
                  left: b.x * scale,
                  top: b.y * scale,
                  width: b.w * scale,
                  height: b.h * scale,
                }}
                onMouseDown={(e) => e.stopPropagation()}
                onClick={(e) => {
                  e.stopPropagation();
                  setBlockDraft({ block: b, text: b.text });
                }}
              />
            ))}
          {mode === "image" && imgPatch && (
            <div
              className="img-patch"
              style={{
                left: imgPatch.rect.x * scale,
                top: imgPatch.rect.y * scale,
                width: imgPatch.rect.w * scale,
                height: imgPatch.rect.h * scale,
                background: imgPatch.color,
              }}
            />
          )}
          {mode === "image" &&
            images.map((im) => {
              const isDragging =
                imgDraft !== null && imgDraft.object_index === im.object_index;
              const b = isDragging ? imgDraft : im;
              const preview = imgPreviews[im.object_index];
              return (
                <div
                  key={`im${im.object_index}`}
                  className="image-box"
                  style={{
                    left: b.x * scale,
                    top: b.y * scale,
                    width: b.w * scale,
                    height: b.h * scale,
                  }}
                  onMouseDown={(e) => startImgAction(e, im, "move")}
                >
                  {isDragging && preview && (
                    <img
                      className="image-preview"
                      src={`data:image/png;base64,${preview}`}
                      draggable={false}
                      alt=""
                    />
                  )}
                  {(["nw", "n", "ne", "e", "se", "s", "sw", "w"] as const).map(
                    (hd) => (
                      <div
                        key={hd}
                        className={`image-handle h-${hd}`}
                        title="Redimensionar (Shift: libre en esquinas)"
                        onMouseDown={(e) => startImgAction(e, im, "resize", hd)}
                      />
                    ),
                  )}
                </div>
              );
            })}
          {imagePopover && (
            <div
              className="card"
              style={{
                left: clampCardLeft(imagePopover.x * scale),
                top: (imagePopover.y + imagePopover.h) * scale + 6,
              }}
              onMouseDown={(e) => e.stopPropagation()}
            >
              <div className="card-actions">
                <button
                  className="btn"
                  onClick={() => replaceImagePick(imagePopover)}
                >
                  <Icon name="image" size={13} />
                  Reemplazar…
                </button>
                <button
                  className="btn btn-danger"
                  onClick={() => deleteImage(imagePopover)}
                >
                  <Icon name="trash" size={13} />
                  Eliminar
                </button>
                <button className="btn" onClick={() => setImagePopover(null)}>
                  Cerrar
                </button>
              </div>
            </div>
          )}
          {newTextDraft && (
            <div
              className="card"
              style={{
                left: clampCardLeft(newTextDraft.x * scale, 288),
                top: newTextDraft.y * scale,
                width: 280,
              }}
              onMouseDown={(e) => e.stopPropagation()}
            >
              <textarea
                autoFocus
                placeholder="Texto nuevo… (Enter añade, Esc cancela)"
                value={newTextDraft.text}
                onChange={(e) =>
                  setNewTextDraft({
                    ...newTextDraft,
                    text: e.target.value,
                  })
                }
                onKeyDown={(e) => {
                  if (e.key === "Enter" && !e.shiftKey) {
                    e.preventDefault();
                    submitNewText();
                  }
                  if (e.key === "Escape") setNewTextDraft(null);
                }}
              />
              <div className="card-row">
                <select
                  className="size-select font-select"
                  title="Fuente"
                  value={newTextDraft.font}
                  onChange={(e) =>
                    setNewTextDraft({
                      ...newTextDraft,
                      font: e.target.value,
                    })
                  }
                >
                  {FONT_CHOICES.map((f) => (
                    <option key={f.value} value={f.value}>
                      {f.label}
                    </option>
                  ))}
                </select>
                <select
                  className="size-select"
                  title="Tamaño"
                  value={newTextDraft.size}
                  onChange={(e) =>
                    setNewTextDraft({
                      ...newTextDraft,
                      size: Number(e.target.value),
                    })
                  }
                >
                  {[8, 10, 12, 14, 18, 24, 32].map((s) => (
                    <option key={s} value={s}>
                      {s} pt
                    </option>
                  ))}
                </select>
              </div>
              <div className="card-actions">
                <button className="btn" onClick={() => setNewTextDraft(null)}>
                  Cancelar
                </button>
                <button className="btn btn-primary" onClick={submitNewText}>
                  Añadir
                </button>
              </div>
            </div>
          )}
          {blockDraft && (
            <div
              className="card"
              style={{
                left: clampCardLeft(blockDraft.block.x * scale),
                top: (blockDraft.block.y + blockDraft.block.h) * scale + 6,
                width: Math.max(260, blockDraft.block.w * scale),
              }}
              onMouseDown={(e) => e.stopPropagation()}
            >
              <span className="card-label">
                {blockDraft.block.font_family || "Fuente del documento"}
                {" · "}
                {Math.round(blockDraft.block.font_size)} pt
              </span>
              <textarea
                autoFocus
                value={blockDraft.text}
                onChange={(e) =>
                  setBlockDraft({ ...blockDraft, text: e.target.value })
                }
                onKeyDown={(e) => {
                  if (e.key === "Enter" && !e.shiftKey) {
                    e.preventDefault();
                    submitBlockDraft();
                  }
                  if (e.key === "Escape") setBlockDraft(null);
                }}
              />
              <div className="card-actions">
                <button className="btn btn-danger" onClick={deleteBlock}>
                  <Icon name="trash" size={13} />
                  Eliminar
                </button>
                <button className="btn" onClick={() => setBlockDraft(null)}>
                  Cancelar
                </button>
                <button className="btn btn-primary" onClick={submitBlockDraft}>
                  Guardar
                </button>
              </div>
            </div>
          )}
          {mode === "select" &&
            links.map((l, i) => (
              <div
                key={`lk${i}`}
                className="link-zone"
                title={l.uri ?? `Ir a la página ${(l.dest_page ?? 0) + 1}`}
                style={{
                  left: l.x * scale,
                  top: l.y * scale,
                  width: l.w * scale,
                  height: l.h * scale,
                }}
                onMouseDown={(e) => e.stopPropagation()}
                onClick={(e) => {
                  e.stopPropagation();
                  onLinkClick(l);
                }}
              />
            ))}
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
                left: clampCardLeft(fieldDraft.field.x * scale),
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
          {annots
            .filter((a) => a.kind === "Text")
            .map((a) => (
              <button
                key={`n${a.index}`}
                className="note-icon"
                style={{
                  left: a.x * scale,
                  top: a.y * scale,
                  width: Math.max(18, a.w * scale),
                  height: Math.max(18, a.h * scale),
                }}
                title={a.contents}
                onMouseDown={(e) => e.stopPropagation()}
                onClick={(e) => {
                  e.stopPropagation();
                  setNotePopover((p) => (p?.index === a.index ? null : a));
                }}
              >
                <Icon name="note" size={12} />
              </button>
            ))}
          {notePopover && (
            <div
              className="card"
              style={{
                left: clampCardLeft(notePopover.x * scale),
                top: (notePopover.y + notePopover.h) * scale + 6,
              }}
              onMouseDown={(e) => e.stopPropagation()}
            >
              <p>
                {notePopover.contents ||
                  KIND_LABELS[notePopover.kind] ||
                  notePopover.kind}
              </p>
              <div className="card-actions">
                <button
                  className="btn btn-danger"
                  onClick={() => deleteAnnotation(notePopover)}
                >
                  <Icon name="trash" size={13} />
                  Eliminar
                </button>
                <button className="btn" onClick={() => setNotePopover(null)}>
                  Cerrar
                </button>
              </div>
            </div>
          )}
          {noteDraft && (
            <div
              className="card"
              style={{
                left: clampCardLeft(noteDraft.x * scale),
                top: noteDraft.y * scale,
              }}
              onMouseDown={(e) => e.stopPropagation()}
            >
              <textarea
                autoFocus
                placeholder="Escribe la nota y pulsa Enter…"
                value={noteDraft.text}
                onChange={(e) =>
                  setNoteDraft({ ...noteDraft, text: e.target.value })
                }
                onKeyDown={(e) => {
                  if (e.key === "Enter" && !e.shiftKey) {
                    e.preventDefault();
                    submitNote();
                  }
                  if (e.key === "Escape") setNoteDraft(null);
                }}
              />
            </div>
          )}
          {mode === "crop" && cropDraft && (
            <>
              <div
                className="crop-rect"
                style={{
                  left: cropDraft.x * scale,
                  top: cropDraft.y * scale,
                  width: cropDraft.w * scale,
                  height: cropDraft.h * scale,
                }}
              />
              <div
                className="card crop-actions"
                style={{
                  left: clampCardLeft(cropDraft.x * scale, 320),
                  top: (cropDraft.y + cropDraft.h) * scale + 8,
                }}
                onMouseDown={(e) => e.stopPropagation()}
              >
                <div className="card-actions">
                  <button
                    className="btn btn-primary"
                    onClick={() => applyCrop(false)}
                  >
                    Recortar
                  </button>
                  <button className="btn" onClick={() => applyCrop(true)}>
                    Todas las páginas
                  </button>
                  <button
                    className="btn"
                    onClick={() => {
                      setCropDraft(null);
                      onModeChange("select");
                    }}
                  >
                    Cancelar
                  </button>
                </div>
              </div>
            </>
          )}
          {mode === "redact" && redactDraft && (
            <>
              <div
                className="redact-rect"
                style={{
                  left: redactDraft.x * scale,
                  top: redactDraft.y * scale,
                  width: redactDraft.w * scale,
                  height: redactDraft.h * scale,
                }}
              />
              <div
                className="card crop-actions"
                style={{
                  left: clampCardLeft(redactDraft.x * scale, 320),
                  top: (redactDraft.y + redactDraft.h) * scale + 8,
                }}
                onMouseDown={(e) => e.stopPropagation()}
              >
                <p>
                  {redactReport
                    ? `Se eliminarán ${redactReport.textos} bloque(s) de texto y ${redactReport.imagenes} imagen(es).`
                    : "Calculando…"}
                </p>
                <div className="card-actions">
                  <button
                    className="btn btn-danger"
                    disabled={!redactReport}
                    onClick={applyRedact}
                  >
                    Redactar
                  </button>
                  <button
                    className="btn"
                    onClick={() => {
                      setRedactDraft(null);
                      setRedactReport(null);
                    }}
                  >
                    Cancelar
                  </button>
                </div>
              </div>
            </>
          )}
          {(mode === "form-new" || mode === "link-new") &&
            (mode === "form-new" ? formDraft : linkDraft) && (
              <>
                <div
                  className="crop-rect"
                  style={{
                    left:
                      (mode === "form-new" ? formDraft : linkDraft)!.x * scale,
                    top:
                      (mode === "form-new" ? formDraft : linkDraft)!.y * scale,
                    width:
                      (mode === "form-new" ? formDraft : linkDraft)!.w * scale,
                    height:
                      (mode === "form-new" ? formDraft : linkDraft)!.h * scale,
                  }}
                />
                <div
                  className="card crop-actions"
                  style={{
                    left: clampCardLeft(
                      (mode === "form-new" ? formDraft : linkDraft)!.x * scale,
                      300,
                    ),
                    top:
                      ((mode === "form-new" ? formDraft : linkDraft)!.y +
                        (mode === "form-new" ? formDraft : linkDraft)!.h) *
                        scale +
                      8,
                  }}
                  onMouseDown={(e) => e.stopPropagation()}
                >
                  {mode === "form-new" ? (
                    <>
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
                        <button
                          className="btn"
                          onClick={() => setFormDraft(null)}
                        >
                          Cancelar
                        </button>
                      </div>
                    </>
                  ) : (
                    <>
                      <div className="card-row">
                        <select
                          className="size-select"
                          value={linkTipo}
                          onChange={(e) => {
                            setLinkTipo(e.target.value as "url" | "pagina");
                            setLinkValor("");
                          }}
                        >
                          <option value="url">URL</option>
                          <option value="pagina">Página</option>
                        </select>
                        <input
                          type={linkTipo === "url" ? "text" : "number"}
                          className="stamp-input"
                          autoFocus
                          placeholder={
                            linkTipo === "url" ? "https://…" : `1-${pageCount}`
                          }
                          value={linkValor}
                          onChange={(e) => setLinkValor(e.target.value)}
                        />
                      </div>
                      <div className="card-actions">
                        <button
                          className="btn btn-primary"
                          disabled={!linkValor.trim()}
                          onClick={applyLink}
                        >
                          Crear enlace
                        </button>
                        <button
                          className="btn"
                          onClick={() => setLinkDraft(null)}
                        >
                          Cancelar
                        </button>
                      </div>
                    </>
                  )}
                </div>
              </>
            )}
          {mode === "firmar" && activeSig && sigDraft && (
            <img
              className="sign-ghost"
              src={`data:image/png;base64,${activeSig.png}`}
              draggable={false}
              alt="Vista previa de la firma"
              style={{
                left: sigDraft.x * scale,
                top: sigDraft.y * scale,
                width: sigDraft.w * scale,
                height: sigDraft.h * scale,
              }}
            />
          )}
          {strokePts.length > 1 && (
            <svg className="stroke-preview">
              <polyline
                points={strokePts
                  .map((p) => `${p[0] * scale},${p[1] * scale}`)
                  .join(" ")}
                stroke={tool.drawColor}
                strokeWidth={tool.drawWidth * scale}
              />
            </svg>
          )}
          {mode === "shape" && shapeDraft && (
            <svg className="shape-preview">
              {(() => {
                const d = shapeDraft;
                const stroke = tool.shapeColor;
                const sw = tool.shapeWidth * scale;
                const fillable =
                  tool.shapeKind === "rect" || tool.shapeKind === "ellipse";
                const fill =
                  tool.shapeFill && fillable ? `${tool.shapeColor}46` : "none";
                const x = Math.min(d.x1, d.x2) * scale;
                const y = Math.min(d.y1, d.y2) * scale;
                const w = Math.abs(d.x2 - d.x1) * scale;
                const h = Math.abs(d.y2 - d.y1) * scale;
                if (tool.shapeKind === "rect")
                  return (
                    <rect
                      x={x}
                      y={y}
                      width={w}
                      height={h}
                      stroke={stroke}
                      strokeWidth={sw}
                      fill={fill}
                    />
                  );
                if (tool.shapeKind === "ellipse")
                  return (
                    <ellipse
                      cx={x + w / 2}
                      cy={y + h / 2}
                      rx={w / 2}
                      ry={h / 2}
                      stroke={stroke}
                      strokeWidth={sw}
                      fill={fill}
                    />
                  );
                const pts = {
                  x1: d.x1 * scale,
                  y1: d.y1 * scale,
                  x2: d.x2 * scale,
                  y2: d.y2 * scale,
                };
                const head = (12 + tool.shapeWidth * 2) * scale;
                const ang = Math.atan2(pts.y2 - pts.y1, pts.x2 - pts.x1);
                return (
                  <>
                    <line {...pts} stroke={stroke} strokeWidth={sw} />
                    {tool.shapeKind === "arrow" &&
                      [Math.PI / 6, -Math.PI / 6].map((delta, i) => {
                        const a2 = ang + Math.PI - delta;
                        return (
                          <line
                            key={i}
                            x1={pts.x2}
                            y1={pts.y2}
                            x2={pts.x2 + head * Math.cos(a2)}
                            y2={pts.y2 + head * Math.sin(a2)}
                            stroke={stroke}
                            strokeWidth={sw}
                          />
                        );
                      })}
                  </>
                );
              })()}
            </svg>
          )}
          {matches?.map((g) =>
            g.rects.map((r, j) => (
              <div
                key={`m${g.groupIndex}-${j}`}
                ref={g.groupIndex === currentGroup && j === 0 ? hitRef : null}
                className={`hit${g.groupIndex === currentGroup ? " current" : ""}`}
                style={{
                  left: r.x * scale,
                  top: r.y * scale,
                  width: r.w * scale,
                  height: r.h * scale,
                }}
              />
            )),
          )}
          {selectionRects.map((r, i) => (
            <div
              key={`s${i}`}
              className="sel"
              style={{
                left: r.x * scale,
                top: r.y * scale,
                width: r.w * scale,
                height: r.h * scale,
              }}
            />
          ))}
          {lastSelRect && !dragging && (
            <div
              className="card sel-popover"
              style={{
                left: clampCardLeft(lastSelRect.x * scale),
                top: (lastSelRect.y + lastSelRect.h) * scale + 6,
              }}
              onMouseDown={(e) => e.stopPropagation()}
            >
              <div className="swatches" style={{ marginRight: 4 }}>
                {ANNOT_COLORS.map((c) => (
                  <button
                    key={c}
                    className={`swatch${tool.markupPending === c ? " on" : ""}`}
                    style={{ background: c }}
                    title="Usar este color en la próxima marca"
                    onClick={() =>
                      tool.onMarkupPending(tool.markupPending === c ? null : c)
                    }
                  />
                ))}
                <label
                  className={`swatch swatch-custom${
                    tool.markupPending &&
                    !ANNOT_COLORS.includes(tool.markupPending)
                      ? " on"
                      : ""
                  }`}
                  title="Color personalizado"
                >
                  <input
                    type="color"
                    value={tool.markupPending ?? "#888888"}
                    onChange={(e) => tool.onMarkupPending(e.target.value)}
                  />
                </label>
              </div>
              <button
                className="btn"
                onClick={() => markupSelection("highlight")}
              >
                <span
                  className="punto-color"
                  style={{
                    background: tool.markupPending ?? tool.markupColors.resaltar,
                  }}
                />
                Resaltar
              </button>
              <button
                className="btn"
                onClick={() => markupSelection("underline")}
              >
                <span
                  className="punto-color"
                  style={{
                    background: tool.markupPending ?? tool.markupColors.subrayar,
                  }}
                />
                Subrayar
              </button>
              <button
                className="btn"
                onClick={() => markupSelection("strikeout")}
              >
                <span
                  className="punto-color"
                  style={{
                    background: tool.markupPending ?? tool.markupColors.tachar,
                  }}
                />
                Tachar
              </button>
              <button
                className="btn"
                onClick={() => {
                  copySelection();
                  setSelection(null);
                }}
              >
                <Icon name="copy" size={13} />
                Copiar
              </button>
            </div>
          )}
        </div>
      )}
    </div>
  );
}

// memo: al hacer zoom o buscar, App re-renderiza; sin esto las ~N páginas
// reconcilian todos sus overlays aunque sus props no hayan cambiado
export default memo(Pagina);
