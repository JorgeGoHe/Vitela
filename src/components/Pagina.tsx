/**
 * Una página del documento en el scroll continuo: render, capa de texto,
 * anotaciones, formularios y todas las herramientas de edición. Carga sus
 * datos de forma perezosa cuando entra en el viewport (± un viewport de
 * margen) y mientras tanto ocupa su sitio con un hueco del tamaño real.
 */
import { memo, useEffect, useRef, useState } from "react";
import {
  charIndexAt,
  type Mode,
  type PageSize,
  type Rect,
  type ShapeKind,
} from "../tipos";
import type { Alineacion, CampoPropuesto } from "../api";
import {
  cajaLlamada,
  pagePoint,
  rectAPagina,
  resizeRect,
  zonaGoma,
} from "../hooks/pagina/geometria";
import { useEnlaces } from "../hooks/pagina/useEnlaces";
import { useFormularios } from "../hooks/pagina/useFormularios";
import { useImagenes } from "../hooks/pagina/useImagenes";
import { useAnotaciones } from "../hooks/pagina/useAnotaciones";
import { useAreas } from "../hooks/pagina/useAreas";
import { useMedida } from "../hooks/pagina/useMedida";
import { useSeleccionTexto } from "../hooks/pagina/useSeleccionTexto";
import { useTexto } from "../hooks/pagina/useTexto";
import CapaAnotaciones, { MarcasAnotaciones } from "./pagina/CapaAnotaciones";
import CapaAreas from "./pagina/CapaAreas";
import CapaLlamada from "./pagina/CapaLlamada";
import CapaMedida from "./pagina/CapaMedida";
import CapaPropuestas from "./pagina/CapaPropuestas";
import CapaEnlaces from "./pagina/CapaEnlaces";
import CapaFormularios from "./pagina/CapaFormularios";
import CapaImagenes from "./pagina/CapaImagenes";
import CapaTexto from "./pagina/CapaTexto";
import TarjetaSeleccion from "./pagina/TarjetaSeleccion";

/** Las marcas de «rellenar y firmar»: con las que se rellena un formulario
 *  que no es interactivo. «texto» no es una marca, lleva al cuadro de texto. */
export type MarcaRellenar = "check" | "cross" | "dot" | "line";

/** Estado global de herramienta que necesitan los handlers de la página. */
export type ToolProps = {
  drawColor: string;
  drawWidth: number;
  /** Goma de borrar armada dentro del modo Dibujar. */
  goma: boolean;
  /** Diámetro del borrado, en puntos de página. */
  gomaAncho: number;
  /** Modo «Medir»: qué se mide, si se deja puesta y si toca calibrar. */
  medidaTipo: "distancia" | "perimetro" | "area";
  medidaDejar: boolean;
  calibrando: boolean;
  /** Milímetros por punto de página del documento abierto. */
  escalaMm: number;
  /** Fija la escala del documento (la guarda App por ruta). */
  onEscala: (mmPorPunto: number) => void;
  /** El arrastre de calibración ha terminado: desarma el botón. */
  onCalibrado: () => void;
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
  freeTextColor: string;
  freeTextSize: number;
  freeTextBorder: boolean;
  /** Color del texto del documento; `null` = el que ya tenga. */
  textColor: string | null;
  textAlign: Alineacion | null;
  /** Interlineado: a qué distancia se coloca cada línea, que en un PDF es
   *  dónde va el objeto siguiente y no un operador. `null` = el del
   *  documento. */
  textLineHeight: number | null;
  /** Espaciado entre caracteres en puntos (operador `Tc`): 0 es lo normal,
   *  que es de donde parte Acrobat. */
  textCharSpacing: number;
  /** Avisa a la fila contextual del color real del bloque seleccionado, que
   *  es lo que pinta el swatch «el que ya tenga». */
  onTextBlockPicked: (color: string | null) => void;
  /** Marca de «rellenar y firmar» armada, si la hay. */
  fillMark: MarcaRellenar | null;
  fillColor: string;
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
  /** Giro solo de la vista (⇧⌘+/⇧⌘−): rotación CSS, el fichero no cambia. */
  viewRotation: number;
  devicePixelRatio: number;
  docVersion: number;
  annotVersion: number;
  pageVersion: number;
  mode: Mode;
  tool: ToolProps;
  matches?: PageMatch[];
  currentGroup: number;
  /** La página que marca la píldora: la que responde a ⌘A. */
  esActual: boolean;
  selOwner: number | null;
  /** Índice del comentario elegido en el panel, si está en esta página. */
  seleccionExterna: number | null;
  claimSel: (page: number | null) => void;
  /** El texto que hay seleccionado en esta página: ⌘B pone un marcador con
   *  él, como Acrobat. */
  onSeleccion: (texto: string) => void;
  requestRender: (page: number, width: number, pv: number) => Promise<string>;
  registerEl: (page: number, el: HTMLDivElement | null) => void;
  onAnnotated: (page: number) => void;
  onPageMutated: (page: number) => void;
  onDocMutated: (newCount: number, nextPage?: number) => void;
  onError: (e: unknown) => void;
  onNotice: (texto: string) => void;
  /** Cuántos campos de formulario tiene esta página. */
  onFormularios: (n: number) => void;
  /** «Resaltar campos existentes»: los pinta aunque no se pase el ratón. */
  resaltarCampos: boolean;
  onModeChange: (m: Mode) => void;
  onLinkGoto: (page: number) => void;
  onLinkUri: (uri: string) => void;
  onSigStamped: () => void;
  /** Recuadro dibujado para la firma con certificado, ya en el espacio
   *  propio de la página: lo resuelve App, que abre el diálogo. */
  onFirmaRect: (page: number, rect: Rect) => void;
  /** Zonas marcadas para censurar que caen en esta página. */
  marcas: { annotIndex: number; rect: Rect }[];
  /** Campos propuestos por «Reconocer campos…» que caen en esta página,
   *  con su posición en la lista entera. Todavía no están en el PDF. */
  propuestas: { i: number; campo: CampoPropuesto }[];
  /** La propuesta que se está revisando una a una, si está aquí. */
  propuestaActual: number | null;
  onPropuestaQuitar: (i: number) => void;
  onPropuestaRenombrar: (i: number, nombre: string) => void;
  onPropuestaTipo: (i: number) => void;
  /** Quita una marca de esta página (la lista la lleva App). */
  quitarMarca: (page: number, annotIndex: number) => void;
  /** La lista de marcas ha cambiado: que App la relea. */
  onMarcasCambian: () => void;
  /** Suben cuando la fila contextual pide «Añadir texto» o «Insertar
   *  imagen…»: la página actual abre el borrador sin obligar a descubrir el
   *  clic en zona libre. */
  pedirTextoNuevo: number;
  pedirImagen: number;
};

function Pagina({
  index,
  workPath,
  size,
  pageCount,
  displayWidth,
  viewRotation,
  devicePixelRatio,
  docVersion,
  annotVersion,
  pageVersion,
  mode,
  tool,
  matches,
  currentGroup,
  esActual,
  selOwner,
  seleccionExterna,
  claimSel,
  onSeleccion,
  requestRender,
  registerEl,
  onAnnotated,
  onPageMutated,
  onDocMutated,
  onError,
  onNotice,
  onFormularios,
  resaltarCampos,
  onModeChange,
  onLinkGoto,
  onLinkUri,
  onSigStamped,
  onFirmaRect,
  marcas,
  propuestas,
  propuestaActual,
  onPropuestaQuitar,
  onPropuestaRenombrar,
  onPropuestaTipo,
  quitarMarca,
  onMarcasCambian,
  pedirTextoNuevo,
  pedirImagen,
}: Props) {
  const wrapRef = useRef<HTMLDivElement | null>(null);
  const [visible, setVisible] = useState(false);
  const [imgSrc, setImgSrc] = useState<string | null>(null);

  const hitRef = useRef<HTMLDivElement | null>(null);

  const scale = displayWidth / size.width;
  // tope de píxeles físicos: a zoom alto el CSS escala el resto (un render
  // de 7000px en base64 congela el hilo; a 4096 no se nota y va fluido)
  const renderWidth = Math.min(4096, Math.round(displayWidth * devicePixelRatio));
  const { activeSig } = tool;

  const enlaces = useEnlaces({
    workPath,
    index,
    visible,
    docVersion,
    annotVersion,
    pageVersion,
    mode,
    size,
    pageCount,
    onAnnotated,
    onPageMutated,
    onError,
    onModeChange,
    onLinkGoto,
    onLinkUri,
  });
  const texto = useTexto({
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
  });
  const formularios = useFormularios({
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
  });
  const imagenes = useImagenes({
    workPath,
    index,
    visible,
    docVersion,
    pageVersion,
    mode,
    scale,
    size,
    viewRotation,
    wrapRef,
    onPageMutated,
    onError,
    onNotice,
  });
  const seleccion = useSeleccionTexto({
    workPath,
    index,
    visible,
    docVersion,
    pageVersion,
    mode,
    size,
    selOwner,
    esActual,
    claimSel,
    onSeleccion,
    onError,
    onNotice,
  });
  const anotaciones = useAnotaciones({
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
  });
  const areas = useAreas({
    workPath,
    index,
    pageCount,
    mode,
    size,
    activeSig,
    onPageMutated,
    onDocMutated,
    onError,
    onModeChange,
    onSigStamped,
    onMarcasCambian,
  });
  const medida = useMedida({
    workPath,
    index,
    mode,
    size,
    tool,
    onAnnotated,
    onError,
    onNotice,
  });

  // «Añadir texto» e «Insertar imagen…» de la fila contextual: los abre la
  // página actual, en el margen superior izquierdo del área de texto, que es
  // un sitio libre y previsible. El ref evita que un re-render los repita.
  const ultimoTextoRef = useRef(pedirTextoNuevo);
  const ultimaImagenRef = useRef(pedirImagen);
  useEffect(() => {
    if (pedirTextoNuevo === ultimoTextoRef.current) return;
    ultimoTextoRef.current = pedirTextoNuevo;
    if (!esActual || mode !== "edit") return;
    texto.setBlockDraft(null);
    texto.setNewTextDraft({
      x: Math.min(72, size.width * 0.12),
      y: Math.min(96, size.height * 0.12),
      text: "",
      size: 12,
      font: "auto",
    });
  }, [pedirTextoNuevo, esActual, mode, size, texto]);
  useEffect(() => {
    if (pedirImagen === ultimaImagenRef.current) return;
    ultimaImagenRef.current = pedirImagen;
    if (!esActual || mode !== "image") return;
    imagenes.setImagePopover(null);
    imagenes.insertImageAt(
      Math.min(72, size.width * 0.12),
      Math.min(96, size.height * 0.12),
    );
  }, [pedirImagen, esActual, mode, size, imagenes]);

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
  }, [visible, workPath, index, renderWidth, docVersion, annotVersion, pageVersion, requestRender, onError]);

  // Centrar el visor en la coincidencia de búsqueda actual si está aquí.
  // Depende de imgSrc para re-centrar cuando termina el render de la página.
  useEffect(() => {
    if (!matches || !matches.some((g) => g.groupIndex === currentGroup)) return;
    hitRef.current?.scrollIntoView({ block: "center", inline: "center" });
  }, [matches, currentGroup, imgSrc]);

  function onMouseDown(e: React.MouseEvent<HTMLDivElement>) {
    if (e.button !== 0) return;
    const { x, y } = pagePoint(e, scale, viewRotation);
    if (mode === "draw") {
      // la goma es un conmutador del mismo modo, no un modo aparte
      if (tool.goma) {
        anotaciones.gomaStartRef.current = { x, y };
        anotaciones.setGomaPos({ x, y });
        anotaciones.gomaLiveRef.current = null;
        anotaciones.setGomaRect(null);
        return;
      }
      anotaciones.strokeLiveRef.current = [[x, y]];
      anotaciones.setStrokePts([[x, y]]);
      return;
    }
    if (mode === "note") {
      anotaciones.setNoteDraft({ x, y, text: "" });
      return;
    }
    if (mode === "edit") {
      // clic en zona libre: añadir texto nuevo ahí (los bloques existentes
      // capturan su propio clic con stopPropagation)
      texto.setBlockDraft(null);
      texto.setNewTextDraft({ x, y, text: "", size: 12, font: "auto" });
      return;
    }
    if (mode === "image") {
      // clic en zona libre: insertar imagen ahí (las cajas de imagen
      // capturan su propio mousedown con stopPropagation)
      imagenes.setImagePopover(null);
      imagenes.insertImageAt(x, y);
      return;
    }
    if (mode === "firmar") {
      // con una marca armada, el clic la coloca (Acrobat: se ponen con un
      // clic y se mueven después); si no, manda la firma manuscrita
      if (tool.fillMark) {
        anotaciones.colocaMarca(tool.fillMark, x, y);
        return;
      }
      if (!activeSig) return;
      areas.sigDragRef.current = { x, y };
      areas.setSigDraft(null);
      return;
    }
    if (mode === "shape") {
      anotaciones.shapeStartRef.current = { x, y };
      anotaciones.setShapeDraft(null);
      return;
    }
    if (mode === "stamp") {
      anotaciones.placeStamp(x, y);
      return;
    }
    if (mode === "crop") {
      areas.cropStartRef.current = { x, y };
      areas.setCropDraft(null);
      return;
    }
    if (mode === "medir") {
      // perímetro y área se ponen por vértices —clic por punto, doble clic
      // cierra—, como las tres herramientas de Acrobat; la distancia y la
      // calibración siguen siendo un arrastre
      if (
        !tool.calibrando &&
        (tool.medidaTipo === "perimetro" || tool.medidaTipo === "area")
      ) {
        if (e.detail >= 2) medida.cierra();
        else medida.anadeVertice({ x, y });
        return;
      }
      medida.medidaStartRef.current = { x, y };
      medida.medidaLiveRef.current = null;
      medida.setMedidaDraft(null);
      medida.setCalibre(null);
      return;
    }
    if (mode === "redact") {
      areas.redactStartRef.current = { x, y };
      areas.setRedactDraft(null);
      return;
    }
    if (mode === "firma-cert") {
      areas.certStartRef.current = { x, y };
      areas.setCertDraft(null);
      return;
    }
    if (mode === "form-new") {
      formularios.formStartRef.current = { x, y };
      formularios.setFormDraft(null);
      return;
    }
    if (mode === "link-new") {
      enlaces.linkStartRef.current = { x, y };
      enlaces.setLinkDraft(null);
      return;
    }
    if (mode === "freetext") {
      anotaciones.freeTextStartRef.current = { x, y };
      anotaciones.setFreeTextDraft(null);
      return;
    }
    if (mode === "callout") {
      // el clic marca la PUNTA (lo que se señala) y el arrastre lleva la
      // caja del texto, que es el gesto de Acrobat
      anotaciones.calloutStartRef.current = { x, y };
      anotaciones.setCalloutDraft(null);
      return;
    }
    if (!seleccion.pageText) return;
    claimSel(index);
    // doble clic: palabra; triple: línea (no arranca arrastre)
    if (e.detail >= 2) {
      seleccion.seleccionaBloque(x, y, e.detail >= 3);
      return;
    }
    seleccion.anchorRef.current = charIndexAt(seleccion.pageText, x, y);
    // el jitter de un clic simple no debe crear una selección de 1 carácter
    // (bloquearía el clic-para-borrar de las anotaciones)
    seleccion.downPosRef.current = { x: e.clientX, y: e.clientY };
    seleccion.setDragging(true);
    seleccion.setSelection(null);
    // un clic fuera confirma lo que se estuviera escribiendo en el popover
    anotaciones.cerrarPopover();
  }

  function onMouseMove(e: React.MouseEvent<HTMLDivElement>) {
    if (!(e.buttons & 1)) return;
    if (mode === "select" && anotaciones.annotActionRef.current) {
      const a = anotaciones.annotActionRef.current;
      const { x, y } = pagePoint(e, scale, viewRotation);
      const dx = x - a.startX;
      const dy = y - a.startY;
      if (Math.abs(dx) + Math.abs(dy) > 1) a.moved = true;
      const o = { x: a.orig.x, y: a.orig.y, w: a.orig.w, h: a.orig.h };
      const r =
        a.kind === "move"
          ? { ...o, x: o.x + dx, y: o.y + dy }
          : resizeRect(o, a.handle ?? "se", dx, dy, e.shiftKey);
      const d = { index: a.orig.index, ...r };
      anotaciones.annotLiveRef.current = d;
      anotaciones.setAnnotDraft(d);
      return;
    }
    if (mode === "image" && imagenes.imgActionRef.current) {
      const a = imagenes.imgActionRef.current;
      const { x, y } = pagePoint(e, scale, viewRotation);
      const dx = x - a.startX;
      const dy = y - a.startY;
      if (Math.abs(dx) + Math.abs(dy) > 1) a.moved = true;
      const d =
        a.kind === "move"
          ? { ...a.orig, x: a.orig.x + dx, y: a.orig.y + dy }
          : { ...a.orig, ...resizeRect(a.orig, a.handle ?? "se", dx, dy, e.shiftKey) };
      imagenes.imgLiveRef.current = d;
      imagenes.setImgDraft(d);
      return;
    }
    if (mode === "edit" && texto.txtActionRef.current) {
      const a = texto.txtActionRef.current;
      const { x, y } = pagePoint(e, scale, viewRotation);
      const dx = x - a.startX;
      const dy = y - a.startY;
      if (Math.abs(dx) + Math.abs(dy) > 1) a.moved = true;
      const d =
        a.kind === "move"
          ? { ...a.orig, x: a.orig.x + dx, y: a.orig.y + dy }
          : { ...a.orig, ...resizeRect(a.orig, a.handle ?? "se", dx, dy, e.shiftKey) };
      texto.txtLiveRef.current = d;
      texto.setTxtDraft(d);
      return;
    }
    if (mode === "draw") {
      if (tool.goma) {
        const p = pagePoint(e, scale, viewRotation);
        anotaciones.setGomaPos(p);
        const start = anotaciones.gomaStartRef.current;
        if (!start) return;
        const d = zonaGoma(start, p, tool.gomaAncho);
        anotaciones.gomaLiveRef.current = d;
        anotaciones.setGomaRect(d);
        return;
      }
      if (anotaciones.strokeLiveRef.current.length === 0) return;
      const { x, y } = pagePoint(e, scale, viewRotation);
      anotaciones.strokeLiveRef.current = [...anotaciones.strokeLiveRef.current, [x, y]];
      anotaciones.setStrokePts(anotaciones.strokeLiveRef.current);
      return;
    }
    if (mode === "firmar") {
      const start = areas.sigDragRef.current;
      if (!activeSig || !start) return;
      const { x, y } = pagePoint(e, scale, viewRotation);
      const w = Math.abs(x - start.x);
      if (w < 4) return;
      const h = w * activeSig.ratio;
      const d = {
        x: Math.min(x, start.x),
        y: y >= start.y ? start.y : start.y - h,
        w,
        h,
      };
      areas.sigLiveRef.current = d;
      areas.setSigDraft(d);
      return;
    }
    if (mode === "shape") {
      const start = anotaciones.shapeStartRef.current;
      if (!start) return;
      const { x, y } = pagePoint(e, scale, viewRotation);
      const d = { x1: start.x, y1: start.y, x2: x, y2: y };
      anotaciones.shapeLiveRef.current = d;
      anotaciones.setShapeDraft(d);
      return;
    }
    if (mode === "medir") {
      const start = medida.medidaStartRef.current;
      if (!start) return;
      const { x, y } = pagePoint(e, scale, viewRotation);
      const d = { x1: start.x, y1: start.y, x2: x, y2: y };
      medida.medidaLiveRef.current = d;
      medida.setMedidaDraft(d);
      return;
    }
    if (mode === "crop") {
      const start = areas.cropStartRef.current;
      if (!start) return;
      const { x, y } = pagePoint(e, scale, viewRotation);
      areas.setCropDraft({
        x: Math.min(x, start.x),
        y: Math.min(y, start.y),
        w: Math.abs(x - start.x),
        h: Math.abs(y - start.y),
      });
      return;
    }
    if (mode === "redact") {
      const start = areas.redactStartRef.current;
      if (!start) return;
      const { x, y } = pagePoint(e, scale, viewRotation);
      const d = {
        x: Math.min(x, start.x),
        y: Math.min(y, start.y),
        w: Math.abs(x - start.x),
        h: Math.abs(y - start.y),
      };
      areas.redactLiveRef.current = d;
      areas.setRedactDraft(d);
      return;
    }
    if (mode === "firma-cert") {
      const start = areas.certStartRef.current;
      if (!start) return;
      const { x, y } = pagePoint(e, scale, viewRotation);
      const d = {
        x: Math.min(x, start.x),
        y: Math.min(y, start.y),
        w: Math.abs(x - start.x),
        h: Math.abs(y - start.y),
      };
      areas.certLiveRef.current = d;
      areas.setCertDraft(d);
      return;
    }
    if (mode === "freetext") {
      const start = anotaciones.freeTextStartRef.current;
      if (!start) return;
      const { x, y } = pagePoint(e, scale, viewRotation);
      const d = {
        x: Math.min(x, start.x),
        y: Math.min(y, start.y),
        w: Math.abs(x - start.x),
        h: Math.abs(y - start.y),
        text: anotaciones.freeTextDraft?.text ?? "",
      };
      anotaciones.freeTextLiveRef.current = d;
      anotaciones.setFreeTextDraft(d);
      return;
    }
    if (mode === "callout") {
      const start = anotaciones.calloutStartRef.current;
      if (!start) return;
      const { x, y } = pagePoint(e, scale, viewRotation);
      const d = cajaLlamada(start, { x, y }, size);
      anotaciones.calloutLiveRef.current = d;
      anotaciones.setCalloutDraft(d);
      return;
    }
    if (mode === "form-new" || mode === "link-new") {
      const start = (mode === "form-new" ? formularios.formStartRef : enlaces.linkStartRef).current;
      if (!start) return;
      const { x, y } = pagePoint(e, scale, viewRotation);
      const d = {
        x: Math.min(x, start.x),
        y: Math.min(y, start.y),
        w: Math.abs(x - start.x),
        h: Math.abs(y - start.y),
      };
      if (mode === "form-new") {
        formularios.formLiveRef.current = d;
        formularios.setFormDraft(d);
      } else {
        enlaces.linkLiveRef.current = d;
        enlaces.setLinkDraft(d);
      }
      return;
    }
    if (mode !== "select" || !seleccion.pageText || seleccion.anchorRef.current === null) return;
    const down = seleccion.downPosRef.current;
    if (
      down &&
      Math.abs(e.clientX - down.x) < 4 &&
      Math.abs(e.clientY - down.y) < 4
    )
      return;
    const { x, y } = pagePoint(e, scale, viewRotation);
    const idx = charIndexAt(seleccion.pageText, x, y);
    if (idx === null) return;
    const a = seleccion.anchorRef.current;
    seleccion.setSelection({ start: Math.min(a, idx), end: Math.max(a, idx) });
  }

  function onMouseUp() {
    seleccion.anchorRef.current = null;
    seleccion.setDragging(false);
    if (mode === "select" && anotaciones.annotActionRef.current) {
      const a = anotaciones.annotActionRef.current;
      anotaciones.annotActionRef.current = null;
      const draft = anotaciones.annotLiveRef.current;
      anotaciones.annotLiveRef.current = null;
      if (!a.moved) {
        // clic simple: opciones de la anotación (borrar)
        anotaciones.setAnnotDraft(null);
        anotaciones.setNotePopover(a.orig);
      } else if (draft) {
        anotaciones.commitAnnot(a.orig, draft);
      }
      return;
    }
    if (mode === "firmar") {
      const start = areas.sigDragRef.current;
      areas.sigDragRef.current = null;
      if (!activeSig || !start) return;
      // ref espejo: en un arrastre en un solo frame el estado de React aún
      // no se ha re-renderizado y sigDraft sería el del render anterior
      const draft = areas.sigLiveRef.current;
      areas.sigLiveRef.current = null;
      areas.setSigDraft(null);
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
      areas.stampActiveSignature(r);
      return;
    }
    if (mode === "shape") {
      anotaciones.shapeStartRef.current = null;
      const d = anotaciones.shapeLiveRef.current;
      anotaciones.shapeLiveRef.current = null;
      anotaciones.setShapeDraft(null);
      if (d && Math.abs(d.x2 - d.x1) + Math.abs(d.y2 - d.y1) > 4) {
        anotaciones.commitShape(d);
      }
      return;
    }
    if (mode === "crop") {
      areas.cropStartRef.current = null;
      // el borrador se queda visible; se confirma con los botones
      return;
    }
    if (mode === "medir") {
      medida.medidaStartRef.current = null;
      const d = medida.medidaLiveRef.current;
      medida.medidaLiveRef.current = null;
      if (!d || Math.abs(d.x2 - d.x1) + Math.abs(d.y2 - d.y1) < 4) {
        medida.setMedidaDraft(null);
        return;
      }
      if (tool.calibrando) {
        // el trazo se queda hasta que se diga cuánto mide de verdad
        medida.setMedidaDraft(null);
        medida.setCalibre(d);
        tool.onCalibrado();
        return;
      }
      // la medida se queda en pantalla; solo toca el documento si se ha
      // pedido dejarla puesta
      if (tool.medidaDejar) medida.dejaMedida(d);
      return;
    }
    if (mode === "redact") {
      areas.redactStartRef.current = null;
      const d = areas.redactLiveRef.current;
      areas.redactLiveRef.current = null;
      // soltar deja una MARCA revisable, no una censura: lo destructivo es
      // «Aplicar redacción», que va aparte y con su informe previo
      if (d && d.w > 6 && d.h > 6) areas.marcarRedaccion(d);
      else areas.setRedactDraft(null);
      return;
    }
    if (mode === "firma-cert") {
      const start = areas.certStartRef.current;
      areas.certStartRef.current = null;
      const d = areas.certLiveRef.current;
      areas.certLiveRef.current = null;
      areas.setCertDraft(null);
      if (!start) return;
      // un clic simple vale: recuadro por defecto, como el de Acrobat
      const caja = d && d.w > 24 && d.h > 12 ? d : { x: start.x, y: start.y, w: 200, h: 60 };
      onFirmaRect(index, rectAPagina(caja, size));
      return;
    }
    if (mode === "form-new") {
      formularios.formStartRef.current = null;
      // el borrador queda visible; se confirma en la tarjeta
      return;
    }
    if (mode === "link-new") {
      enlaces.linkStartRef.current = null;
      return;
    }
    if (mode === "callout") {
      const start = anotaciones.calloutStartRef.current;
      anotaciones.calloutStartRef.current = null;
      const d = anotaciones.calloutLiveRef.current;
      anotaciones.calloutLiveRef.current = null;
      if (!start) return;
      // un clic simple vale: la caja sale al lado de la punta, para no
      // obligar a arrastrar cuando solo se quiere señalar
      anotaciones.setCalloutDraft(
        d ?? cajaLlamada(start, { x: start.x + 60, y: start.y + 40 }, size),
      );
      return;
    }
    if (mode === "freetext") {
      const start = anotaciones.freeTextStartRef.current;
      anotaciones.freeTextStartRef.current = null;
      const d = anotaciones.freeTextLiveRef.current;
      anotaciones.freeTextLiveRef.current = null;
      if (!start) return;
      // un clic simple vale: caja por defecto para no obligar a arrastrar
      const caja =
        d && d.w > 20 && d.h > 12
          ? d
          : { x: start.x, y: start.y, w: 180, h: 48, text: "" };
      anotaciones.setFreeTextDraft(caja);
      return;
    }
    if (mode === "image" && imagenes.imgActionRef.current) {
      const a = imagenes.imgActionRef.current;
      imagenes.imgActionRef.current = null;
      const draft = imagenes.imgLiveRef.current ?? imagenes.imgDraft;
      imagenes.imgLiveRef.current = null;
      if (!a.moved) {
        // clic simple: abrir el popover de la imagen
        imagenes.setImgDraft(null);
        imagenes.setImgPatch(null);
        imagenes.setImagePopover(a.orig);
      } else if (draft) {
        // el borrador y el parche se quedan hasta que llegan los datos
        // frescos (efecto de imágenes): sin salto atrás ni doble copia
        imagenes.commitImage(a.orig.object_index, draft);
      }
      return;
    }
    if (mode === "edit" && texto.txtActionRef.current) {
      const a = texto.txtActionRef.current;
      texto.txtActionRef.current = null;
      const draft = texto.txtLiveRef.current ?? texto.txtDraft;
      texto.txtLiveRef.current = null;
      if (!a.moved) {
        // clic simple: la tarjeta de edición, como siempre
        texto.setTxtDraft(null);
        texto.setBlockDraft({ block: a.orig, text: a.orig.text });
      } else if (draft) {
        texto.setBlockDraft(null);
        texto.commitTextBlock(a.orig, draft);
      }
      return;
    }
    if (mode === "draw" && tool.goma) {
      const start = anotaciones.gomaStartRef.current;
      anotaciones.gomaStartRef.current = null;
      const d = anotaciones.gomaLiveRef.current;
      anotaciones.gomaLiveRef.current = null;
      anotaciones.setGomaRect(null);
      if (!start) return;
      // un toque sin arrastre también borra: la goma se usa a base de
      // toquecitos sobre lo que sobra
      anotaciones.borraConGoma(d ?? zonaGoma(start, start, tool.gomaAncho));
      return;
    }
    if (mode === "draw" && anotaciones.strokeLiveRef.current.length > 0) anotaciones.finishStroke();
  }

  // La hoja se gira con un transform; la caja exterior intercambia alto y
  // ancho para que el scroll continuo siga colocando bien las páginas.
  const altoHoja = (displayWidth * size.height) / size.width;
  const cuarto = viewRotation === 90 || viewRotation === 270;
  return (
    <div
      ref={wrapRef}
      className="page-wrap"
      style={{
        width: cuarto ? altoHoja : displayWidth,
        height: cuarto ? displayWidth : altoHoja,
      }}
      data-page={index}
    >
      <div
        className="page-rot"
        style={{
          width: displayWidth,
          height: altoHoja,
          transform: `translate(-50%, -50%) rotate(${viewRotation}deg)`,
        }}
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
          decoding="async"
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
          onClick={anotaciones.onClickLayer}
        >
          <MarcasAnotaciones
            mode={mode}
            anotaciones={anotaciones}
            scale={scale}
          />
          <CapaTexto
            mode={mode}
            texto={texto}
            scale={scale}
            displayWidth={displayWidth}
            displayHeight={altoHoja}
          />
          <CapaImagenes
            mode={mode}
            imagenes={imagenes}
            scale={scale}
            displayWidth={displayWidth}
            displayHeight={altoHoja}
          />
          <CapaEnlaces
            mode={mode}
            enlaces={enlaces}
            scale={scale}
            displayWidth={displayWidth}
            pageCount={pageCount}
          />
          <CapaFormularios
            mode={mode}
            formularios={formularios}
            scale={scale}
            displayWidth={displayWidth}
            resaltarCampos={resaltarCampos}
          />
          <CapaAnotaciones
            mode={mode}
            anotaciones={anotaciones}
            scale={scale}
            displayWidth={displayWidth}
            displayHeight={altoHoja}
            tool={tool}
            onQuitarMarca={(annotIndex) => quitarMarca(index, annotIndex)}
          />
          <CapaLlamada
            mode={mode}
            anotaciones={anotaciones}
            scale={scale}
            tool={tool}
          />
          <CapaPropuestas
            propuestas={propuestas}
            actual={propuestaActual}
            size={size}
            scale={scale}
            onQuitar={onPropuestaQuitar}
            onRenombrar={onPropuestaRenombrar}
            onCambiarTipo={onPropuestaTipo}
          />
          <CapaMedida
            mode={mode}
            medida={medida}
            scale={scale}
            viewRotation={viewRotation}
            displayWidth={displayWidth}
            tool={tool}
          />
          <CapaAreas
            mode={mode}
            areas={areas}
            scale={scale}
            displayWidth={displayWidth}
            activeSig={activeSig}
            onModeChange={onModeChange}
            marcas={marcas}
            onQuitarMarca={(annotIndex) => quitarMarca(index, annotIndex)}
          />
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
          {seleccion.selectionRects.map((r, i) => (
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
          {seleccion.lastSelRect && !seleccion.dragging && (
            <TarjetaSeleccion
              rect={seleccion.lastSelRect}
              scale={scale}
              displayWidth={displayWidth}
              tool={tool}
              markupSelection={anotaciones.markupSelection}
              copySelection={seleccion.copySelection}
              setSelection={seleccion.setSelection}
            />
          )}
        </div>
      )}
      </div>
    </div>
  );
}

// memo: al hacer zoom o buscar, App re-renderiza; sin esto las ~N páginas
// reconcilian todos sus overlays aunque sus props no hayan cambiado
export default memo(Pagina);
