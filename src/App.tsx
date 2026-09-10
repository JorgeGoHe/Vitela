import {
  type CSSProperties,
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  useSyncExternalStore,
} from "react";
import { createPortal } from "react-dom";
import {
  busyCount,
  invoke,
  onAbrirFichero,
  onArrastreFicheros,
  onCerrarSolicitado,
  onMenuAccion,
  onPantallaCompleta,
  ponerPantallaCompleta,
  subscribeBusy,
} from "./ipc";
import { useHistorial } from "./hooks/useHistorial";
import { useRenderCache } from "./hooks/useRenderCache";
import { useMiniaturas } from "./hooks/useMiniaturas";
import { useBusqueda } from "./hooks/useBusqueda";
import { useBusquedaCarpeta } from "./hooks/useBusquedaCarpeta";
import { useReemplazo } from "./hooks/useReemplazo";
import { useFirmas } from "./hooks/useFirmas";
import { useHerramienta } from "./hooks/useHerramienta";
import { useLectura } from "./hooks/useLectura";
import { useMano } from "./hooks/useMano";
import { destinoDe, esquemaDe, esquemaPermitido } from "./enlaces";
import { abrirRuta, open, save, openUrl } from "./dialogos";
import {
  addBlankPage,
  removeMarginalText,
  addBackground,
  removeBackground,
  addHeaderFooter,
  addBates,
  addWatermark,
  duplicatePage,
  insertPdfAt,
  pdfFromImages,
  type TamanoImagenes,
  deletePages,
  extractEachPage,
  extractPages,
  getDocumentAnnotations,
  autosaveState,
  borraSesion,
  listRecent,
  pdfInfo,
  type PdfInfo,
  listRedactions,
  applyRedactions,
  mergeMany,
  replacePages,
  splitPdf,
  sanitizePdf,
  unmarkRedaction,
  unmarkAllRedactions,
  rotatePages,
  recoverSession,
  removeRecent,
  renderPageSrc,
  setMenuState,
  signPdf,
  signPdfP12,
  certifyPdf,
  touchRecent,
  uiLista,
  verifySignatures,
  type AnotacionDoc,
  type FirmaInfo,
  type HeaderFooter,
  type Redaccion,
  type RedactReport,
  type Sesion,
  type SanitizeReport,
  type Reciente,
} from "./api";
import {
  compressPdf,
  encryptPdf,
  exportPagesPng,
  exportText,
  exportDocx,
  exportComments,
  listAttachments,
  saveAttachment,
  openAttachment,
  addAttachment,
  deleteAttachment,
  listLayers,
  setLayerVisible,
  type Adjunto,
  type Capa,
  replyAnnotation,
  setAnnotationState,
  type EstadoComentario,
  flattenPdf,
  getDocumentInfo,
  getOpenAction,
  setOpenAction,
  type VistaInicial,
  type DocumentInfo,
  getMetadata,
  getPageLabels,
  setPageLabels,
  historyState,
  removeEncryption,
  TODO_PERMITIDO,
  getOutline,
  setMetadata,
  setOutline,
  type Metadata,
  type OutlineNode,
  detectFormFields,
  createFormFields,
  type CampoPropuesto,
  exportCommentsPdf,
  exportCommentsXfdf,
  exportFormDataXfdf,
  importFormDataXfdf,
  importCommentsXfdf,
  type OrdenComentarios,
} from "./api";
import Pestanas from "./components/Pestanas";
import DialogoComentarios, {
  type FormatoComentarios,
} from "./components/DialogoComentarios";
import { CONFIANZA_MINIMA } from "./components/pagina/CapaPropuestas";
import {
  ATAJO_COMENTARIOS,
  ATAJO_MARCADORES,
  ATAJO_PANEL,
  avisoPantallaVisto,
  cargaPreferencias,
  cargaZoom,
  cargaVista,
  cuandoLlano,
  etiquetaDePagina,
  paginaEscrita,
  pagineoLlano,
  type RangoEtiquetas,
  estadoDeFirma,
  fechaLarga,
  permisosCertificacion,
  type NivelFirma,
  filasDePaginas,
  FIRMA_VACIA,
  type FirmaDraft,
  guardaPreferencias,
  guardaVista,
  IMPRIMIR_POR_DEFECTO,
  marcaAvisoPantalla,
  type ModoPagina,
  type OpcionesImprimir,
  cargaEscala,
  cargaGuias,
  guardaGuias,
  type Guias,
  SIN_GUIAS,
  guiasDePagina,
  cargaResaltarCampos,
  copyToClipboard,
  guardaEscala,
  MM_POR_PUNTO,
  formateaRango,
  hexToRgba,
  guardaResaltarCampos,
  MOD,
  guardaZoom,
  parseRango,
  paginasImprimibles,
  plural,
  autorComentarios,
  nombreEstado,
  tamanoFichero,
  type FiltroComentarios,
  type Mode,
  type PageSize,
  type Preferencias,
  type Zoom,
  ES_MAC,
} from "./tipos";
import Icon from "./components/Icon";
import Busqueda from "./components/Busqueda";
import OpcionesHerramienta from "./components/OpcionesHerramienta";
import MenuAcciones from "./components/MenuAcciones";
import PanelPaginas from "./components/PanelPaginas";
import Pagina from "./components/Pagina";
import PanelFirmas from "./components/PanelFirmas";
import PanelFirmasDoc from "./components/PanelFirmasDoc";
import PanelAdjuntos from "./components/PanelAdjuntos";
import PanelCapas from "./components/PanelCapas";
import DialogoFirmar from "./components/DialogoFirmar";
import PanelSellos from "./components/PanelSellos";
import LimiteError from "./components/LimiteError";
import DibujarFirma from "./components/DibujarFirma";
import DialogoMarcaAgua, {
  type MarcaAguaOpts,
} from "./components/DialogoMarcaAgua";
import DialogoEncabezado from "./components/DialogoEncabezado";
import PanelMarcadores from "./components/PanelMarcadores";
import PanelComentarios from "./components/PanelComentarios";
import DialogoExtraer from "./components/DialogoExtraer";
import DialogoPropiedades from "./components/DialogoPropiedades";
import DialogoContrasena from "./components/DialogoContrasena";
import DialogoProteger, {
  type ProtegerDraft,
} from "./components/DialogoProteger";
import DialogoConfirmar from "./components/DialogoConfirmar";
import DialogoExportar from "./components/DialogoExportar";
import DialogoComprimir from "./components/DialogoComprimir";
import DialogoPreferencias from "./components/DialogoPreferencias";
import DialogoImprimir from "./components/DialogoImprimir";
import DialogoReemplazar from "./components/DialogoReemplazar";
import DialogoDividir from "./components/DialogoDividir";
import DialogoCombinar from "./components/DialogoCombinar";
import DialogoImagenes from "./components/DialogoImagenes";
import DialogoAtajos from "./components/DialogoAtajos";
import DialogoWord from "./components/DialogoWord";
import DialogoEtiquetas from "./components/DialogoEtiquetas";
import DialogoBates, { type BatesOpts } from "./components/DialogoBates";
import DialogoInsertar from "./components/DialogoInsertar";
import "./App.css";

const BASE_WIDTH = 900;
/** Los niveles de zoom de Acrobat. ⌘+ y ⌘− saltan de uno al siguiente; el
 *  campo del zoom sigue aceptando cualquier número entre los extremos. */
const NIVELES_ZOOM = [
  0.08, 0.12, 0.25, 0.3333, 0.5, 0.6667, 0.75, 1, 1.25, 1.5, 2, 4, 8, 16, 24,
  64,
];
/** Zoom válido: del 8 % al 6400 %, redondeado al 1 % (por debajo del 1 % el
 *  redondeo se comía los niveles finos, así que a partir del 100 % se
 *  redondea al entero). El 8 % y el 12 % son los de Acrobat y son los que
 *  dejan ver un cartel A0 entero de una vez. */
function recortaZoom(z: number): number {
  const tope = NIVELES_ZOOM[NIVELES_ZOOM.length - 1];
  return Math.min(tope, Math.max(NIVELES_ZOOM[0], Math.round(z * 100) / 100));
}
/** El nivel siguiente (o el anterior) al zoom de ahora. En un plano el
 *  400 % se queda corto y los saltos de 0,25 tardan una eternidad. */
function nivelZoom(actual: number, delta: 1 | -1): number {
  const eps = 0.001;
  if (delta === 1)
    return NIVELES_ZOOM.find((n) => n > actual + eps) ?? recortaZoom(actual);
  return (
    [...NIVELES_ZOOM].reverse().find((n) => n < actual - eps) ??
    recortaZoom(actual)
  );
}
/** Los cuatro modos de presentación en el segmentado de la píldora. */
const MODOS_PILDORA: [ModoPagina, string, string][] = [
  ["una", "pageOne", "Una sola página"],
  ["continuo", "pageScroll", "Desplazamiento continuo"],
  ["dos", "pageTwo", "Dos páginas"],
  ["dos-continuo", "pageTwoScroll", "Dos páginas con desplazamiento continuo"],
];

/** Separación vertical entre páginas y padding superior del visor (px). */
const PAGE_GAP = 24;
const VIEWER_PAD_TOP = 28;

/** «Se quitarán: autor y título, 2 adjuntos y 1 script», con lo que hay. */
function resumenSaneado(r: SanitizeReport): string {
  const partes: string[] = [];
  if (r.metadatos > 0) partes.push("el autor, el título y demás datos del documento");
  if (r.scripts > 0) partes.push(plural(r.scripts, "script", "scripts"));
  if (r.adjuntos > 0) partes.push(plural(r.adjuntos, "adjunto", "adjuntos"));
  if (r.capas > 0) partes.push(plural(r.capas, "capa oculta", "capas ocultas"));
  if (r.formularios > 0) {
    partes.push(plural(r.formularios, "campo de formulario", "campos de formulario"));
  }
  if (partes.length === 0) return "Este documento no lleva información oculta que quitar.";
  const ultima = partes.pop()!;
  const lista = partes.length > 0 ? `${partes.join(", ")} y ${ultima}` : ultima;
  return `Se quitarán ${lista}. No se puede deshacer guardando, pero ${MOD}Z lo devuelve mientras el documento siga abierto.`;
}

/** «2 bloques de texto y 1 imagen», con lo que haya: nombrar lo que vale
 *  cero («y 0 imágenes») es contar algo que no ocurre. */
function resumenRedaccion(r: RedactReport): string {
  const partes: string[] = [];
  if (r.textos > 0) partes.push(plural(r.textos, "bloque de texto", "bloques de texto"));
  if (r.imagenes > 0) partes.push(plural(r.imagenes, "imagen", "imágenes"));
  if (partes.length === 0) return "el contenido que haya dentro";
  return partes.join(" y ");
}

/** Un documento firmado por varias personas cuya última firma cubre el
 *  fichero entero está **intacto**, y la anterior no tiene que avalar la
 *  revisión que la sigue: firmar detrás de otro es lo normal, no una
 *  manipulación. Lo dice el backend (`documento_intacto`), que es quien ha
 *  mirado los bytes. */
function documentoIntacto(firmas: FirmaInfo[]): boolean {
  const ultima = firmas[firmas.length - 1];
  return (
    firmas.length > 1 &&
    ultima?.documento_intacto === true &&
    firmas.every(
      (f) => estadoDeFirma(f).nivel !== "mal" && f.estado !== "desconocido",
    )
  );
}

/** La banda de firmas: una sola línea, sin jerga, y con el peor de los tres
 *  estados —una firma que no se ha podido comprobar no se pinta en rojo—. */
function estadoBanda(firmas: FirmaInfo[]): NivelFirma {
  const niveles = firmas.map((f) => estadoDeFirma(f).nivel);
  if (niveles.includes("mal")) return "mal";
  if (documentoIntacto(firmas)) return "ok";
  if (niveles.includes("duda")) return "duda";
  return "ok";
}


function resumenFirmas(firmas: FirmaInfo[]): string {
  // la que no está bien manda: se dice quién firmó y qué pasa con ella, sin
  // dar por buena la primera solo porque sea la primera
  const intacto = documentoIntacto(firmas);
  const dudosa = intacto
    ? undefined
    : firmas.find((f) => estadoDeFirma(f).nivel !== "ok");
  if (dudosa) {
    const suyo = dudosa.name || dudosa.cert_subject || "";
    const texto = estadoDeFirma(dudosa).texto;
    // solo baja la PRIMERA letra: `toLowerCase()` sobre la frase entera se
    // llevaba por delante la mayúscula de «Vitela»
    const seguido = texto.charAt(0).toLowerCase() + texto.slice(1);
    return `${suyo ? `Firmado por ${suyo} · ${seguido}` : texto}`;
  }
  const quien = firmas[0].name || firmas[0].cert_subject || "";
  const cuando = firmas[0].signed_at ? ` el ${fechaLarga(firmas[0].signed_at)}` : "";
  // certificar no es firmar: dice que ESTA es la versión buena y qué se
  // puede cambiar sin romper el sello, así que la banda lo nombra y lo
  // cuenta —sin eso la función es invisible en cuanto se ha usado—
  const certifica = firmas[0].certifica;
  const cabecera =
    firmas.length > 1
      ? `Firmado por ${firmas.length} personas`
      : `${certifica ? "Certificado" : "Firmado"}${quien ? ` por ${quien}` : ""}${cuando}${
          certifica ? ` · ${permisosCertificacion(certifica)}` : ""
        }`;
  // con varias firmas, lo que importa es que nadie haya tocado el documento
  // detrás de la última: la anterior no tiene que avalar la revisión que la
  // sigue, que es exactamente lo que hace Acrobat
  if (intacto) {
    const sinRaizTodas = firmas.some((f) => f.confianza !== "raiz_conocida");
    return `${plural(firmas.length, "firma válida", "firmas válidas")} · el documento no ha cambiado desde la última${
      sinRaizTodas ? " · no se ha comprobado quién emitió los certificados" : ""
    }`;
  }
  // la confianza NO cambia el color de la banda —es del certificado, no del
  // documento—, pero sí se dice: quien venga de Acrobat lee el verde como
  // «esto es de fiar» y aquí el verde solo promete que nadie lo ha tocado
  const sinRaiz = firmas.some((f) => f.confianza !== "raiz_conocida");
  return `${cabecera} · el documento no ha cambiado desde la firma${
    sinRaiz ? " · no se ha comprobado quién emitió el certificado" : ""
  }`;
}

/** Punto de lectura al que vuelve ⌥←: página, scroll y zoom. */
type Vista = { page: number; scrollTop: number; zoom: Zoom };

/** Un documento abierto. Todo lo demás —tamaños de página, miniaturas,
 *  marcadores, comentarios, adjuntos, capas, firmas— se relee solo, porque
 *  cuelga de `workPath` y de `docVersion`: lo que hay que guardar es lo que
 *  nadie puede volver a calcular, empezando por dónde se estaba leyendo. */
type Pestana = {
  id: number;
  workPath: string;
  originalPath: string | null;
  nombreProvisional: string | null;
  pageCount: number;
  modified: boolean;
  hadPassword: boolean;
  docPassword: string | null;
  protegido: boolean;
  protPendiente: boolean;
  escalaMm: number;
  /** La vista: la página, el zoom, el scroll y el panel abierto. */
  pageIndex: number;
  zoom: Zoom;
  scrollTop: number;
  viewRotation: number;
  sidebarTab: PestanaSidebar;
  vistasAtras: Vista[];
  vistasAdelante: Vista[];
};

type PestanaSidebar =
  | "paginas"
  | "marcadores"
  | "comentarios"
  | "firmas"
  | "adjuntos"
  | "capas";

/** Reenvía una pulsación ⌘/Ctrl+tecla a los listeners globales (entradas
 *  del menú nativo cuyo atajo captura el sistema antes que el webview).
 *  Devuelve **si alguien la ha atendido**: los listeners que actúan llaman
 *  a `preventDefault`, así que el valor de `dispatchEvent` distingue «lo ha
 *  hecho la página» de «el gesto se ha perdido», que es lo que antes pasaba
 *  en silencio con la entrada del menú viva. */
function reenviaTecla(key: string): boolean {
  return !window.dispatchEvent(
    new KeyboardEvent("keydown", {
      key,
      metaKey: ES_MAC,
      ctrlKey: !ES_MAC,
      bubbles: true,
      cancelable: true,
    }),
  );
}

/** El campo de texto que tiene el foco, si lo hay: es donde está mirando el
 *  usuario y donde tienen que actuar Copiar y Seleccionar todo. */
function campoConFoco(): HTMLInputElement | HTMLTextAreaElement | null {
  const el = document.activeElement;
  return el instanceof HTMLInputElement || el instanceof HTMLTextAreaElement
    ? el
    : null;
}

/** Lo que hay seleccionado dentro de un campo. Un `input type="number"` no
 *  deja leer el rango (el navegador lanza), así que se pregunta con red. */
function seleccionDeCampo(
  campo: HTMLInputElement | HTMLTextAreaElement,
): string {
  try {
    return campo.value.slice(
      campo.selectionStart ?? 0,
      campo.selectionEnd ?? 0,
    );
  } catch {
    return "";
  }
}

function App() {
  const [originalPath, setOriginalPath] = useState<string | null>(null);
  // documento sin ruta (los PDF unidos al soltarlos): la barra lo nombra y
  // ⌘S se comporta como Guardar como, para no machacar ningún original
  const [nombreProvisional, setNombreProvisional] = useState<string | null>(
    null,
  );
  const [workPath, setWorkPath] = useState<string | null>(null);
  const [modified, setModified] = useState(false);
  // el original iba cifrado: la copia de trabajo está en claro y al guardar
  // hay que decidir si se vuelve a proteger (la contraseña solo vive en memoria)
  const [hadPassword, setHadPassword] = useState(false);
  const [docPassword, setDocPassword] = useState<string | null>(null);
  // lo que se contestó la primera vez en este documento: el segundo ⌘S no
  // vuelve a preguntar lo mismo (`null` = todavía no se ha preguntado)
  const [mantenerClave, setMantenerClave] = useState<boolean | null>(null);
  const [saveAsk, setSaveAsk] = useState<{
    dest: string;
    resolve: (ok: boolean) => void;
  } | null>(null);
  // acción pendiente (abrir otro PDF, cerrar) hasta decidir qué hacer con
  // los cambios sin guardar
  const [unsavedAsk, setUnsavedAsk] = useState<(() => void) | null>(null);
  const [docVersion, setDocVersion] = useState(0);
  const [pageCount, setPageCount] = useState(0);
  const [pageSizes, setPageSizes] = useState<PageSize[]>([]);
  const [pageIndex, setPageIndex] = useState(0);
  // la píldora de navegación pasa a campo mientras se teclea una página
  const [pageDraft, setPageDraft] = useState<string | null>(null);
  // igual que la página, el porcentaje de zoom se puede teclear
  const [zoomDraft, setZoomDraft] = useState<string | null>(null);
  // tres modos de zoom, como en Acrobat: número = porcentaje fijo,
  // "ajuste" = al ancho de la ventana, "pagina" = la hoja entera a la vista
  const [zoom, setZoom] = useState<Zoom>("ajuste");
  const [viewerW, setViewerW] = useState<number | null>(null);
  const [viewerH, setViewerH] = useState<number | null>(null);
  const viewerRef = useRef<HTMLElement | null>(null);
  // herramienta Mano: la barra espaciadora mantenida convierte el cursor en
  // mano y arrastrar desplaza, como en Acrobat. Sin botón propio en la fila
  // de modos: es un gesto, y está en los atajos
  const mano = useMano(viewerRef, pageCount > 0);
  const [sidebarVisible, setSidebarVisible] = useState(
    window.innerWidth >= 900,
  );
  const { thumbs, setThumbs, refreshThumb } = useMiniaturas(
    workPath,
    pageCount,
    docVersion,
  );
  const [error, setError] = useState<string | null>(null);
  const [menuOpen, setMenuOpen] = useState(false);

  const { requestRender, evictPage, evictAll } = useRenderCache(
    workPath,
    docVersion,
  );
  const pageElsRef = useRef<Map<number, HTMLDivElement>>(new Map());
  const scrollRafRef = useRef<number | null>(null);
  // ancla del scroll: qué punto de qué página debe quedar a qué altura del
  // visor cuando cambia el zoom. Al desplazarse es el borde superior de la
  // página visible (offset 0); con ⌘+rueda, el punto bajo el cursor, que es
  // lo que hace Acrobat. `fracX`/`offsetX` solo los pone la rueda.
  const scrollAnchorRef = useRef<{
    page: number;
    frac: number;
    offset: number;
    fracX?: number;
    offsetX?: number;
  } | null>(null);
  // Esc quita primero las coincidencias de búsqueda y solo después sale de
  // la herramienta: el efecto del modo lo consulta sin re-registrarse
  const hayCoincidenciasRef = useRef(false);

  const [mode, setMode] = useState<Mode>("select");
  const [annotVersion, setAnnotVersion] = useState(0);
  const [pageVersions, setPageVersions] = useState<number[]>([]);
  const [selOwner, setSelOwner] = useState<number | null>(null);
  // el texto que hay seleccionado en el visor: lo dice la página que tiene
  // la selección y lo usa ⌘B para titular el marcador, como Acrobat. En un
  // ref porque no pinta nada: guardarlo en estado repintaría el visor entero
  // en cada carácter arrastrado
  const textoSelRef = useRef("");
  const recibeSeleccion = useCallback((texto: string) => {
    textoSelRef.current = texto;
  }, []);
  // deshacer/rehacer general (instantáneas en el backend); tras restaurar
  // hace falta el refresco completo porque puede cambiar hasta el recuento
  // El punto del historial en el que el documento está **como se abrió o
  // como se guardó**, por copia de trabajo. Si ⌘Z devuelve ahí, no queda
  // nada que perder: el «•» de la pestaña se apaga y cerrar no pregunta por
  // unos cambios que el propio usuario ya ha deshecho.
  const puntosLimpiosRef = useRef(new Map<string, number>());
  const marcaPuntoLimpio = useCallback((work: string) => {
    historyState(work)
      .then((e) => puntosLimpiosRef.current.set(work, e.undo))
      .catch(() => {});
  }, []);
  const historial = useHistorial({
    workPath,
    onRestaurado: (n, pasos) => {
      afterMutation(n);
      const limpio = puntosLimpiosRef.current.get(workPath ?? "");
      if (limpio !== undefined) setModified(pasos !== limpio);
    },
    onError: (e) => setError(String(e)),
  });
  const refrescarHistorial = historial.refrescar;
  // etiquetas de página (`/PageLabels`): cómo se llama cada hoja —«ii»,
  // «A-3»— en la píldora, en las miniaturas y en «Ir a la página». Sin
  // `/PageLabels` la lista viene vacía y todo se llama por su número
  const [etiquetas, setEtiquetas] = useState<RangoEtiquetas[]>([]);
  const [etiquetasOpen, setEtiquetasOpen] = useState(false);
  const [batesOpen, setBatesOpen] = useState(false);
  // «Insertar PDF…»: el fichero elegido, a la espera de decir dónde entra
  const [insertarAsk, setInsertarAsk] = useState<{
    path: string;
    paginas: number | null;
  } | null>(null);
  const [wmOpen, setWmOpen] = useState(false);
  // cuántas cosas va a quitar «Quitar fondo…», contadas por el ensayo previo
  const [fondoAsk, setFondoAsk] = useState<number | null>(null);
  const [marginalAsk, setMarginalAsk] = useState<{
    zona: "watermark" | "header" | "footer";
    textos: number;
  } | null>(null);
  const [hfOpen, setHfOpen] = useState(false);
  const [sidebarTab, setSidebarTab] = useState<PestanaSidebar>("paginas");
  // lo que el documento lleva dentro y hasta ahora solo se sabía borrar: sus
  // pestañas salen únicamente cuando hay algo que enseñar (con seis fijas a
  // 200 px no cabe ninguna)
  const [adjuntos, setAdjuntos] = useState<Adjunto[]>([]);
  // adjunto que se va a quitar, a la espera de la confirmación
  const [adjuntoAsk, setAdjuntoAsk] = useState<{
    index: number;
    adjunto: Adjunto;
  } | null>(null);
  const [capas, setCapas] = useState<Capa[]>([]);
  // firmas del fichero abierto y la banda que las resume, que se cierra y
  // no vuelve hasta el documento siguiente
  const [firmasDoc, setFirmasDoc] = useState<FirmaInfo[]>([]);
  const [bandaFirmas, setBandaFirmas] = useState(false);
  // recuadro dibujado para la firma con certificado, y su diálogo
  const [firmaRect, setFirmaRect] = useState<{
    page: number;
    rect: { x: number; y: number; w: number; h: number };
  } | null>(null);
  const [firmaDraft, setFirmaDraft] = useState<FirmaDraft>(FIRMA_VACIA);
  // certificar es firmar diciendo además qué se puede tocar después, así que
  // comparte el recuadro, el diálogo y el destino: solo cambia el comando
  const [certificando, setCertificando] = useState(false);
  // la galería de sellos: se abre al entrar en el modo (como la biblioteca
  // de firmas) y al pulsar «Sellos…» en la fila contextual
  const [galeriaSellos, setGaleriaSellos] = useState(false);
  // el atajo de una pestaña la abre Y le lleva el foco: subir el contador es
  // la señal para el panel (un booleano no distinguiría dos peticiones)
  const [focoComentarios, setFocoComentarios] = useState(0);
  // peticiones de la fila contextual a la página actual (texto nuevo, imagen)
  const [pedirTextoNuevo, setPedirTextoNuevo] = useState(0);
  const [pedirImagen, setPedirImagen] = useState(0);
  const [comentarios, setComentarios] = useState<AnotacionDoc[]>([]);
  const [filtroComentarios, setFiltroComentarios] =
    useState<FiltroComentarios>("todos");
  const [filtroAutor, setFiltroAutor] = useState("todos");
  const [filtroEstado, setFiltroEstado] = useState("todos");
  // páginas marcadas en el panel para actuar en lote
  const [paginasSel, setPaginasSel] = useState<Set<number>>(new Set());
  const [extraerOpen, setExtraerOpen] = useState(false);
  const [reemplazarOpen, setReemplazarOpen] = useState(false);
  const [dividirOpen, setDividirOpen] = useState(false);
  const [combinarOpen, setCombinarOpen] = useState(false);
  // giro SOLO de la vista (⇧⌘+ / ⇧⌘−): no toca el fichero y se pierde al
  // cerrar, como en Acrobat
  const [viewRotation, setViewRotation] = useState(0);
  // presentación de página (las cuatro de Acrobat) y portada suelta en las
  // vistas de dos: estados de la vista, recordados entre sesiones
  const [vista, setVista] = useState(cargaVista);
  // preferencias vivas: de aquí sale el modo nocturno del documento
  const [prefs, setPrefs] = useState<Preferencias>(cargaPreferencias);
  const [pantallaCompleta, setPantallaCompleta] = useState(false);
  // «Modo lectura»: el documento con su píldora y nada más, sin salir de la
  // ventana (esa es la diferencia con la pantalla completa)
  const [modoLectura, setModoLectura] = useState(false);
  // herramienta Mano: la barra espaciadora mantenida, como en Acrobat
  // en presentación la píldora asoma al acercar el ratón al borde inferior
  const [pildoraVisible, setPildoraVisible] = useState(false);
  // historial de vistas (⌥← / ⌥→): el modelo del navegador, dos pilas
  const [vistasAtras, setVistasAtras] = useState<Vista[]>([]);
  const [vistasAdelante, setVistasAdelante] = useState<Vista[]>([]);
  // «Resaltar campos existentes» de Acrobat: encendido por defecto
  const [resaltarCampos, setResaltarCampos] = useState(cargaResaltarCampos);
  const [hayFormularios, setHayFormularios] = useState(false);
  // el aviso de «se puede rellenar» sale una vez por documento
  const avisoFormRef = useRef<string | null>(null);
  // comentario elegido en el panel: la página lo abre en su popover
  const [annotSel, setAnnotSel] = useState<{
    page: number;
    index: number;
  } | null>(null);
  const [pwdDraft, setPwdDraft] = useState<{
    path: string;
    password: string;
  } | null>(null);
  const [protectDraft, setProtectDraft] = useState<ProtegerDraft | null>(null);
  // el FICHERO en disco está cifrado (se abrió con contraseña, o ya se ha
  // guardado con la protección puesta): solo entonces la barra pone el candado
  const [protegido, setProtegido] = useState(false);
  // protección anotada pero todavía sin aplicar: `encrypt_pdf` sin `destPath`
  // no cifra nada, lo hace `save_pdf`. Hasta guardar, el fichero sigue en claro
  const [protPendiente, setProtPendiente] = useState(false);
  const [quitarProtAsk, setQuitarProtAsk] = useState(false);
  const [flattenAsk, setFlattenAsk] = useState(false);
  // zonas marcadas para censurar: propuestas revisables, no censuras
  const [marcasRedact, setMarcasRedact] = useState<Redaccion[]>([]);
  // «Reconocer campos…»: lo que la detección propone, todavía sin escribir
  // nada en el PDF, y cuál se está revisando
  const [propuestas, setPropuestas] = useState<CampoPropuesto[]>([]);
  const [propuestaActual, setPropuestaActual] = useState<number | null>(null);
  // «Exportar comentarios…»: el formato se pregunta antes de pedir destino
  const [comentariosAsk, setComentariosAsk] = useState(false);
  // Documentos abiertos. La fila de pestañas solo sale con más de uno; con
  // uno, la app se ve exactamente igual que antes de que existieran. El
  // estado del documento ACTIVO vive en los `useState` de siempre: aquí se
  // guarda el de los demás, y el del activo se vuelca al cambiar de pestaña
  const [pestanas, setPestanas] = useState<Pestana[]>([]);
  const [pestanaActiva, setPestanaActiva] = useState<number | null>(null);
  const proximaPestanaRef = useRef(1);
  const [redactAsk, setRedactAsk] = useState<RedactReport | null>(null);
  const [sanitizeAsk, setSanitizeAsk] = useState<SanitizeReport | null>(null);
  // enlace externo pendiente de confirmar (los URI del PDF no son de fiar)
  const [linkAsk, setLinkAsk] = useState<string | null>(null);
  // páginas rasterizadas listas para el diálogo del sistema; `anchoIn` solo
  // lo llevan las escalas que no son «ajustar al papel»
  const [printPages, setPrintPages] = useState<
    { src: string; anchoIn?: number }[] | null
  >(null);
  const [printOpen, setPrintOpen] = useState(false);
  const [printOpts, setPrintOpts] = useState<OpcionesImprimir>(
    IMPRIMIR_POR_DEFECTO,
  );
  // el bucle de rasterizado mira esta bandera entre página y página
  const printCancelRef = useRef<{ cancelado: boolean } | null>(null);
  const [exportOpen, setExportOpen] = useState(false);
  const [exportFmt, setExportFmt] = useState<"png" | "jpeg">("png");
  const [exportDpi, setExportDpi] = useState(150);
  const [compressOpen, setCompressOpen] = useState(false);
  const [compressQuality, setCompressQuality] = useState(75);
  const [compressDpi, setCompressDpi] = useState(150);
  const [notice, setNoticeTexto] = useState<string | null>(null);
  // los avisos de progreso («Comprimiendo…») no se van solos: mientras dura
  // el trabajo son el único indicio de que la app no está colgada
  const [noticePersistente, setNoticePersistente] = useState(false);
  // contador honesto del progreso («12 / 200»), en Fragment Mono
  const [noticeDato, setNoticeDato] = useState<string | null>(null);
  // salida del trabajo largo: la banda lleva su propio Cancelar
  const [noticeAccion, setNoticeAccion] = useState<{
    texto: string;
    onClick: () => void;
  } | null>(null);
  const [recientes, setRecientes] = useState<Reciente[]>([]);
  // ficha de cada reciente (páginas, tamaño y si va cifrado), pedida sin
  // abrir el documento: es lo que deja poner el candado antes de pinchar
  const [fichasRecientes, setFichasRecientes] = useState<
    Record<string, PdfInfo | null>
  >({});
  // ficheros soltados de golpe: abrir el primero o unirlos
  const [dropAsk, setDropAsk] = useState<string[] | null>(null);
  const [arrastrando, setArrastrando] = useState(false);
  // el usuario ha intentado cerrar la ventana con cambios sin guardar
  const [cerrarAsk, setCerrarAsk] = useState(false);
  const [noticeSaliendo, setNoticeSaliendo] = useState(false);
  const [outline, setOutlineState] = useState<OutlineNode[]>([]);
  const [propsDraft, setPropsDraft] = useState<Metadata | null>(null);
  // la ficha de solo lectura que acompaña a los metadatos: tamaño, versión,
  // fuentes y qué deja hacer la protección. Puede llegar después (o no
  // llegar), y el diálogo se abre igual
  const [propsFicha, setPropsFicha] = useState<DocumentInfo | null>(null);
  // con qué cara se abre el documento: la única parte de las propiedades que
  // además se escribe (`/OpenAction` y `/PageLayout`)
  const [propsVista, setPropsVista] = useState<VistaInicial | null>(null);
  const [prefsAbiertas, setPrefsAbiertas] = useState(false);
  // sesiones que quedaron a medias en un cierre inesperado: una banda de una
  // línea, no un modal, que es como Vitela cuenta todo lo demás. Son varias
  // porque pueden ser varios los documentos que estaban abiertos con cambios
  const [sesionesRotas, setSesionesRotas] = useState<Sesion[]>([]);
  // en cuanto se abre otro documento la banda se pliega a un botón discreto
  // de la barra: obligaba a decidir sobre trabajo perdido en el peor momento
  // y ocupaba una fila sobre el documento durante toda la sesión
  const [sesionPlegada, setSesionPlegada] = useState(false);
  // la lista de una fila por documento, cuando hay varios que recuperar
  const [sesionesAbiertas, setSesionesAbiertas] = useState(false);
  const [descartarAsk, setDescartarAsk] = useState<Sesion[] | null>(null);
  // «Exportar a Word»: el aviso de lo que no sale va ANTES de elegir destino
  const [wordAsk, setWordAsk] = useState(false);
  // «Crear PDF desde imágenes…»: se ofrece también sin documento, que es
  // donde está el usuario cuando todavía no tiene ninguno
  const [imagenesOpen, setImagenesOpen] = useState(false);
  // la imagen que el backend no ha podido leer: se marca en su fila para
  // quitarla y seguir con las demás, en vez de perder la lista entera
  const [imagenesFallos, setImagenesFallos] = useState<string[]>([]);
  // «Ayuda ▸ Atajos de teclado»: el único sitio donde están todos escritos
  const [atajosAbiertos, setAtajosAbiertos] = useState(false);
  const {
    firmas,
    iniciales,
    sellos,
    activeSig,
    setActiveSig,
    drawingSig,
    setDrawingSig,
    pickSignature,
    uploadSignature,
    saveDrawnSignature,
    removeSignature,
    onSigStamped,
  } = useFirmas({
    mode,
    setMode,
    onError: (e) => setError(String(e)),
  });
  // la escala de medida es del documento, no de la herramienta: se guarda
  // por ruta y se recupera al abrirlo (un plano no cambia de escala)
  const [escalaMm, setEscalaMm] = useState(MM_POR_PUNTO);
  // andamio: reglas (⌘R), guías (⌘;) y cuadrícula (⌘'). No tocan el
  // fichero ni se imprimen; las guías se guardan por ruta, como la escala
  const [reglas, setReglas] = useState(false);
  const [cuadricula, setCuadricula] = useState(false);
  // «ajustar a la cuadrícula» (⇧⌘U): lo que se coloca cae en la cuadrícula
  const [ajustarCuadricula, setAjustarCuadricula] = useState(false);
  const [guiasVisibles, setGuiasVisibles] = useState(true);
  // el andamio tenía tres atajos y ninguna puerta: quien no leía la pantalla
  // de atajos no sabía que existía
  const [menuAndamio, setMenuAndamio] = useState(false);
  const [guias, setGuias] = useState<Guias>(SIN_GUIAS);
  const fijarEscala = useCallback(
    (mm: number) => {
      setEscalaMm(mm);
      guardaEscala(originalPath, mm);
    },
    [originalPath],
  );
  /** Deja una guía nueva. Van por ruta, así que el andamio de un plano
   *  sigue ahí la próxima vez que se abra el documento. */
  /** Deja una guía nueva **en su página**, que es como funcionan en
   *  Acrobat; con ⌥ al soltar vale para todo el documento. */
  const ponGuia = useCallback(
    (eje: "v" | "h", valor: number, pagina: number, todas: boolean) => {
      setGuias((g) => {
        const siguiente: Guias = todas
          ? { ...g, [eje]: [...g[eje], valor] }
          : {
              ...g,
              paginas: {
                ...g.paginas,
                [pagina]: {
                  v: [...(g.paginas[pagina]?.v ?? [])],
                  h: [...(g.paginas[pagina]?.h ?? [])],
                  [eje]: [...(g.paginas[pagina]?.[eje] ?? []), valor],
                },
              },
            };
        guardaGuias(originalPath, siguiente);
        return siguiente;
      });
      setGuiasVisibles(true);
    },
    [originalPath],
  );
  /** Quitar una guía la quita de su página **y** de las de todo el
   *  documento: es la misma raya, la ponga quien la ponga. */
  const quitaGuia = useCallback(
    (eje: "v" | "h", valor: number, pagina: number) => {
      setGuias((g) => {
        const suyas = g.paginas[pagina];
        const siguiente: Guias = {
          ...g,
          [eje]: g[eje].filter((v) => v !== valor),
          paginas: suyas
            ? {
                ...g.paginas,
                [pagina]: {
                  ...suyas,
                  [eje]: suyas[eje].filter((v) => v !== valor),
                },
              }
            : g.paginas,
        };
        guardaGuias(originalPath, siguiente);
        return siguiente;
      });
    },
    [originalPath],
  );
  const herramienta = useHerramienta(activeSig, {
    escalaMm,
    onEscala: fijarEscala,
  });
  const tool = herramienta.tool;
  const setFillMark = herramienta.setFillMark;

  // la marca de rellenar es del modo Firma: al salir se desarma sola, como
  // el resto de borradores
  useEffect(() => {
    if (mode !== "firmar") setFillMark(null);
  }, [mode, setFillMark]);

  /** Abre un PDF en la copia de trabajo; devuelve su `work_path` o null
   *  si no se ha podido abrir. */
  async function openPath(
    path: string,
    password?: string,
    /** Ruta que se considera «el fichero»: al recuperar una sesión se abre
     *  la copia de trabajo pero el original sigue siendo el de verdad, para
     *  que ⌘S no escriba en el temporal. */
    original?: string | null,
  ): Promise<string | null> {
    try {
      // los avisos son del documento que se deja atrás: no deben sobrevivir
      // a la apertura de otro
      setError(null);
      setNotice(null);
      // ya abierto: se trae su pestaña a pantalla en vez de abrirlo dos
      // veces, que es lo que hace Acrobat
      const yaAbierto = pestanas.find(
        (p) => p.id !== pestanaActiva && p.originalPath === path,
      );
      if (yaAbierto) {
        eligePestana(yaAbierto.id);
        return yaAbierto.workPath;
      }
      if (originalPath === path && workPath) return workPath;
      const anterior = workPath;
      const vivo = anterior ? estadoDePestana() : null;
      const info = await invoke<{
        page_count: number;
        work_path: string;
        had_password: boolean;
      }>("open_pdf", { path, password: password ?? null });
      // el documento anterior NO se cierra: se queda en su pestaña, con su
      // copia de trabajo, su historial y el punto por el que se iba
      if (vivo) {
        setPestanas((v) =>
          v.map((p) => (p.id === pestanaActiva ? { ...p, ...vivo } : p)),
        );
      }
      // solo ahora se retira el documento anterior: si la apertura falla
      // (no es un PDF, contraseña cancelada) tiene que seguir intacto
      setThumbs([]);
      setPageSizes([]);
      busqueda.limpiar(true);
      setModified(false);
      setPwdDraft(null);
      setProtegido(info.had_password);
      setProtPendiente(false);
      setHadPassword(info.had_password);
      setDocPassword(info.had_password ? (password ?? null) : null);
      setMantenerClave(null);
      if (info.had_password) {
        setNotice(
          "Documento protegido: al guardar puedes mantener la contraseña o quitarla",
        );
      }
      setPaginasSel(new Set());
      setViewRotation(0);
      setHayFormularios(false);
      // las vistas apiladas son del documento que se deja atrás: con otro
      // de distinto tamaño, «volver» llevaba a un scroll que ya no significa
      // nada. Y la pestaña «Firmas» no existe en un PDF sin firmar
      setVistasAtras([]);
      setVistasAdelante([]);
      setSidebarTab("paginas");
      // la herramienta armada no es del documento nuevo: Redactar sobre un
      // PDF recién abierto es lo último que quiere nadie
      setMode("select");
      setActiveSig(null);
      setFirmasDoc([]);
      setBandaFirmas(false);
      setNombreProvisional(null);
      setEscalaMm(cargaEscala(original !== undefined ? original : path));
      setGuias(cargaGuias(original !== undefined ? original : path));
      setOriginalPath(original !== undefined ? original : path);
      setWorkPath(info.work_path);
      setPageCount(info.page_count);
      setPageIndex(0);
      // «Zoom al abrir» de las preferencias; «el último» es el de la última
      // sesión, no el de esta: el zoom se guarda al cambiarlo
      if (prefs.zoomInicial === "pagina") setZoom("pagina");
      else if (prefs.zoomInicial === "ancho") setZoom("ajuste");
      else if (prefs.zoomInicial === "100") setZoom(1);
      else setZoom(cargaZoom());
      setDocVersion((v) => v + 1);
      // el documento recién abierto está en su punto limpio: si ⌘Z vuelve
      // aquí, no hay cambios que perder
      marcaPuntoLimpio(info.work_path);
      // su pestaña. La fila solo se pinta a partir de la segunda
      const id = proximaPestanaRef.current++;
      const nueva: Pestana = {
        id,
        workPath: info.work_path,
        originalPath: original !== undefined ? original : path,
        nombreProvisional: null,
        pageCount: info.page_count,
        modified: false,
        hadPassword: info.had_password,
        docPassword: info.had_password ? (password ?? null) : null,
        protegido: info.had_password,
        protPendiente: false,
        escalaMm: cargaEscala(original !== undefined ? original : path),
        pageIndex: 0,
        zoom: "ajuste",
        scrollTop: 0,
        viewRotation: 0,
        sidebarTab: "paginas",
        vistasAtras: [],
        vistasAdelante: [],
      };
      setPestanas((v) => [...v, nueva]);
      setPestanaActiva(id);
      // la sesión sin guardar sigue esperando, pero ya no en mitad de la
      // pantalla: se pliega al botón «Recuperar…» de la barra
      setSesionPlegada(true);
      viewerRef.current?.scrollTo({ top: 0 });
      scrollAnchorRef.current = null;
      // el menú nativo se monta una sola vez, en el arranque y sin documento:
      // sin este aviso sus entradas se quedan atenuadas para siempre
      setMenuState(true).catch(() => {});
      // la lista de recientes la lleva la UI: open_pdf no la toca. Una copia
      // de trabajo recuperada no es un reciente: no es un fichero del usuario
      if (original === undefined) {
        touchRecent(path)
          .then(refrescarRecientes)
          .catch(() => {});
      }
      return info.work_path;
    } catch (e) {
      if (String(e) === "PASSWORD_REQUIRED") {
        setPwdDraft({ path, password: "" });
        if (password !== undefined) setError("Contraseña incorrecta");
      } else {
        setError(String(e));
      }
      return null;
    }
  }

  /** Muestra un aviso en la banda superior. Con `persistente` se queda hasta
   *  que otro aviso lo sustituye (progreso); si no, se va solo a los 6 s.
   *  `dato` es el contador («12 / 200»), que se pinta en Fragment Mono, y
   *  `accion` el botón de la propia banda (Cancelar). */
  const setNotice = useCallback(
    (
      texto: string | null,
      opts?: {
        persistente?: boolean;
        dato?: string;
        accion?: { texto: string; onClick: () => void };
      },
    ) => {
      setNoticeTexto(texto);
      setNoticePersistente(!!opts?.persistente);
      setNoticeDato(opts?.dato ?? null);
      setNoticeAccion(opts?.accion ?? null);
      // si algo ha salido bien, la banda roja de antes ya no cuenta: se
      // quedaba en pantalla a través de operaciones correctas
      if (texto) setError(null);
    },
    [],
  );

  const mostrarError = useCallback((e: unknown) => setError(String(e)), []);
  const mostrarAviso = useCallback(
    (texto: string) => setNotice(texto),
    [setNotice],
  );

  const refrescarRecientes = useCallback(() => {
    return listRecent()
      .then((l) => {
        setRecientes(l);
        return l;
      })
      .catch(() => {
        setRecientes([]);
        return [] as Reciente[];
      });
  }, []);

  // al montar y cada vez que se vuelve al estado vacío: la lista se
  // revalida (un reciente puede haber desaparecido del disco entretanto)
  useEffect(() => {
    if (!workPath) refrescarRecientes();
  }, [workPath, refrescarRecientes]);

  // La ficha de cada reciente que sigue en su sitio: `pdf_info` no abre el
  // documento ni pide la contraseña, así que se puede preguntar por los
  // ocho. Sin esto, un PDF protegido no se distinguía hasta pincharlo y
  // encontrarse el diálogo de contraseña.
  useEffect(() => {
    let cancelado = false;
    for (const r of recientes) {
      if (!r.exists || fichasRecientes[r.path] !== undefined) continue;
      pdfInfo(r.path)
        .then((i) => {
          if (!cancelado) setFichasRecientes((m) => ({ ...m, [r.path]: i }));
        })
        .catch(() => {
          if (!cancelado) setFichasRecientes((m) => ({ ...m, [r.path]: null }));
        });
    }
    return () => {
      cancelado = true;
    };
  }, [recientes, fichasRecientes]);

  /** Los documentos de una lista de sesiones, en llano: el nombre de
   *  fichero de cada uno, o «un documento sin fichero» si no tenía ruta.
   *  Con más de tres se cuentan, que es lo que cabe en una línea. */
  function nombresDeSesiones(lista: Sesion[]): string {
    const nombres = lista.map(
      (s) => s.original_path?.split(/[\\/]/).pop() ?? "un documento sin fichero",
    );
    if (nombres.length > 3)
      return `${nombres.length} documentos: ${nombres.slice(0, 3).join(", ")}…`;
    return nombres.join(", ");
  }

  /** Al arrancar: si quedaron sesiones sin guardar, se ofrecen. Las copias
   *  de trabajo ya estaban en temp; lo que faltaba era el apunte. */
  useEffect(() => {
    recoverSession()
      .then((lista) => {
        const vivas = lista.filter((s) => s.modificado);
        if (vivas.length > 0) setSesionesRotas(vivas);
      })
      .catch(() => {});
  }, []);

  /** Abre las copias de trabajo que quedaron, **cada una en su pestaña** y
   *  conservando su fichero original: ⌘S escribe donde el usuario espera y
   *  no en el temporal. Todas quedan marcadas con cambios sin guardar, que
   *  es lo que son: si no, cambiar de pestaña las daría por guardadas. */
  async function recuperarSesiones(lista: Sesion[]) {
    // se quitan de la banda **solo las elegidas**: con varios documentos se
    // puede recuperar uno y seguir decidiendo sobre los demás
    setSesionesRotas((v) => v.filter((s) => !lista.includes(s)));
    const abiertas: string[] = [];
    for (const s of lista) {
      const work = await openPath(s.work_path, undefined, s.original_path);
      if (!work) continue;
      abiertas.push(work);
      if (!s.original_path) setNombreProvisional("Documento recuperado");
    }
    if (abiertas.length === 0) {
      setError("La copia con los cambios ya no está: no se ha podido recuperar");
      return;
    }
    setModified(true);
    setPestanas((v) =>
      v.map((p) =>
        abiertas.includes(p.workPath) ? { ...p, modified: true } : p,
      ),
    );
    const uno = lista.length === 1 ? lista[0] : null;
    setNotice(
      uno
        ? uno.original_path
          ? `Recuperados los cambios sin guardar de ${uno.original_path}`
          : "Recuperado el documento sin guardar"
        : `${plural(abiertas.length, "documento recuperado", "documentos recuperados")}, cada uno en su pestaña`,
    );
    if (abiertas.length < lista.length)
      setError(
        `${plural(lista.length - abiertas.length, "documento no se ha podido recuperar", "documentos no se han podido recuperar")}: su copia con los cambios ya no está`,
      );
  }

  /** No guardar: se borran las copias y los apuntes, y se dice. Es el único
   *  borrado irreversible de trabajo del usuario que hay en la app, así que
   *  pregunta antes —una sola vez, aunque sean varios—, como el diálogo de
   *  cierre, y se llama igual que allí, que es lo que hace que se reconozca. */
  function descartarSesiones(lista: Sesion[]) {
    setSesionesRotas((v) => v.filter((s) => !lista.includes(s)));
    for (const s of lista) {
      invoke("close_document", { workPath: s.work_path }).catch(() => {});
      borraSesion(s.work_path).catch((e) =>
        console.warn("no se ha podido borrar el apunte de sesión:", e),
      );
    }
    setNotice(
      lista.length === 1
        ? "Descartados los cambios sin guardar de la sesión anterior"
        : `Descartados los cambios sin guardar de ${plural(lista.length, "documento", "documentos")}`,
    );
  }

  /** Abre un fichero. Con pestañas no hay nada que preguntar: el documento
   *  que estaba abierto se queda en la suya, con sus cambios; la pregunta
   *  de «cambios sin guardar» es del **cierre**, no de la apertura. */
  function abrirComprobando(path: string) {
    openPath(path);
  }

  /** Abre un reciente. Si ya no está donde decía, se quita de la lista en
   *  vez de dejar la entrada rota invitando a volver a pulsarla. */
  async function abrirReciente(path: string) {
    if (await openPath(path)) return;
    const lista = await listRecent().catch(() => [] as Reciente[]);
    const entrada = lista.find((r) => r.path === path);
    if (!entrada || entrada.exists) return;
    await removeRecent(path).catch(() => {});
    refrescarRecientes();
    setError(`Ya no está en ${path}; lo he quitado de recientes`);
  }

  /** «Archivo ▸ Abrir reciente…» del menú nativo: la lista vive en el menú
   *  «Acciones» cuando hay documento y en el estado vacío cuando no, así que
   *  en vez de un tercer sitio se enseña el que toque. En los dos casos se
   *  revalida antes: lo que se pinta tiene que ser lo que hay en el disco.
   *  Sin documento la entrada no producía nada visible: ahora lleva el foco
   *  a la lista que ya está en pantalla, o dice que no hay ninguna. */
  function abrirRecientes() {
    refrescarRecientes().then((lista) => {
      if (pageCount > 0) {
        setMenuOpen(true);
        return;
      }
      if (lista.length === 0) {
        setNotice("No hay documentos recientes");
        return;
      }
      // el estado vacío ya los enseña: basta con llevar allí el foco
      requestAnimationFrame(() => {
        (
          document.querySelector(".recientes .reciente-abrir") as HTMLElement
        )?.focus();
      });
    });
  }

  function quitarReciente(path: string) {
    removeRecent(path)
      .then(refrescarRecientes)
      .catch((e) => setError(String(e)));
  }

  /** Ficheros soltados sobre la ventana: uno se abre, varios preguntan. */
  function soltarFicheros(paths: string[]) {
    const pdfs = paths.filter((p) => /\.pdf$/i.test(p));
    if (pdfs.length === 0) {
      setError(
        "Eso no se puede abrir: Vitela solo abre ficheros PDF (.pdf)",
      );
      return;
    }
    if (pdfs.length === 1) {
      abrirComprobando(pdfs[0]);
      return;
    }
    setDropAsk(pdfs);
  }

  /** Abre el primero de los soltados y le añade el resto al final. */
  async function abrirYUnir(pdfs: string[]) {
    setDropAsk(null);
    const work = await openPath(pdfs[0]);
    if (!work) return;
    try {
      let count = 0;
      for (const otro of pdfs.slice(1)) {
        count = await invoke<number>("merge_pdf", {
          workPath: work,
          otherPath: otro,
        });
      }
      // un solo paso de deshacer para toda la unión
      if (pdfs.length > 2) await historial.agrupar(pdfs.length - 1);
      afterMutation(count);
      // documento nuevo, como el «Combinar archivos» de Acrobat: sin ruta,
      // así que ⌘S pide destino y ninguno de los originales corre peligro
      setOriginalPath(null);
      setNombreProvisional("Documento combinado");
      setNotice(`${pdfs.length} PDF unidos en un documento nuevo`);
    } catch (e) {
      setError(String(e));
    }
  }

  /** Deja que la ventana se cierre de verdad (el backend frenó el cierre). */
  function confirmarCierre() {
    invoke("confirmar_cierre").catch((e) => setError(String(e)));
  }

  /** Guarda y, solo si el guardado ha ido bien, cierra. */
  /** Guarda el documento que se ve y, si quedan otros con cambios, pasa al
   *  siguiente y vuelve a preguntar: salir no puede llevarse por delante el
   *  trabajo de una pestaña que no se estaba mirando. */
  async function guardarYSalir() {
    const ok = await guardar();
    if (!ok) return;
    const otro = pestanas.find((p) => p.id !== pestanaActiva && p.modified);
    if (otro) {
      eligePestana(otro.id);
      return;
    }
    setCerrarAsk(false);
    confirmarCierre();
  }

  /** Documentos con cambios sin guardar, contando el que está en pantalla:
   *  al salir hay que preguntar por todos, no solo por el que se ve. */
  function sucios(): number {
    return (
      pestanas.filter((p) => p.id !== pestanaActiva && p.modified).length +
      (modified ? 1 : 0)
    );
  }

  const cerrarRef = useRef<() => void>(() => {});
  cerrarRef.current = () => {
    // el evento no dice si ha sido ⌘W, el botón rojo o ⌘Q, así que se trata
    // como lo que puede ser lo más grave: cerrar la app. Se pregunta por
    // TODOS los documentos con cambios, y se sale cuando no queda ninguno
    if (sucios() > 0) setCerrarAsk(true);
    else confirmarCierre();
  };

  useEffect(() => onCerrarSolicitado(() => cerrarRef.current()), []);

  // Fichero abierto desde el Finder o pasado como argumento al arrancar
  const abrirRef = useRef<(path: string) => void>(() => {});
  abrirRef.current = abrirComprobando;
  const soltarRef = useRef<(paths: string[]) => void>(() => {});
  soltarRef.current = soltarFicheros;

  useEffect(() => onAbrirFichero((path) => abrirRef.current(path)), []);

  // Arranque con fichero: en cuanto la UI ya escucha, pregunta si había una
  // ruta esperando (doble clic en el Finder con la app cerrada). Después el
  // camino normal es el evento `abrir-fichero`.
  useEffect(() => {
    uiLista()
      .then((path) => {
        if (path) abrirRef.current(path);
      })
      .catch(() => {});
  }, []);

  useEffect(
    () =>
      onArrastreFicheros({
        onEntra: () => setArrastrando(true),
        onSale: () => setArrastrando(false),
        onSuelta: (paths) => {
          setArrastrando(false);
          soltarRef.current(paths);
        },
      }),
    [],
  );

  /** Ejecuta `continuar` directamente si no hay cambios; si los hay,
   *  pregunta antes (Guardar / Descartar / Cancelar). */
  function conCambiosGuardados(continuar: () => void) {
    if (!modified) {
      continuar();
      return;
    }
    setUnsavedAsk(() => continuar);
  }

  function resolverUnsavedAsk(accion: "guardar" | "descartar" | "cancelar") {
    if (!unsavedAsk) return;
    const continuar = unsavedAsk;
    setUnsavedAsk(null);
    if (accion === "cancelar") return;
    if (accion === "descartar") {
      continuar();
      return;
    }
    guardar().then((ok) => {
      if (ok) continuar();
    });
  }

  /** El estado del documento activo, tal como está ahora mismo: es lo que
   *  se guarda al cambiar de pestaña para que volver a ella devuelva la
   *  vista donde se dejó. */
  function estadoDePestana(): Omit<Pestana, "id" | "workPath"> {
    return {
      originalPath,
      nombreProvisional,
      pageCount,
      modified,
      hadPassword,
      docPassword,
      protegido,
      protPendiente,
      escalaMm,
      pageIndex,
      zoom,
      scrollTop: viewerRef.current?.scrollTop ?? 0,
      viewRotation,
      sidebarTab,
      vistasAtras,
      vistasAdelante,
    };
  }

  /** Deja en pantalla el documento de una pestaña. Lo derivado —tamaños,
   *  miniaturas, marcadores, comentarios, adjuntos, capas, firmas y los
   *  contadores de deshacer— cuelga de `workPath` y de `docVersion`, así
   *  que se relee solo. */
  function aplicaPestana(p: Pestana) {
    setError(null);
    setNotice(null);
    busqueda.limpiar(true);
    setThumbs([]);
    setPageSizes([]);
    setPageVersions([]);
    setPaginasSel(new Set());
    setAnnotSel(null);
    setPropuestas([]);
    setPropuestaActual(null);
    setMode("select");
    setActiveSig(null);
    setBandaFirmas(false);
    setHayFormularios(false);
    setWorkPath(p.workPath);
    setOriginalPath(p.originalPath);
    setNombreProvisional(p.nombreProvisional);
    setPageCount(p.pageCount);
    setModified(p.modified);
    setHadPassword(p.hadPassword);
    setDocPassword(p.docPassword);
    setProtegido(p.protegido);
    setProtPendiente(p.protPendiente);
    setMantenerClave(null);
    setEscalaMm(p.escalaMm);
    setPageIndex(p.pageIndex);
    setZoom(p.zoom);
    setViewRotation(p.viewRotation);
    setSidebarTab(p.sidebarTab);
    setVistasAtras(p.vistasAtras);
    setVistasAdelante(p.vistasAdelante);
    setDocVersion((v) => v + 1);
    // el scroll, cuando el visor ya tiene el alto de este documento
    requestAnimationFrame(() =>
      requestAnimationFrame(() =>
        viewerRef.current?.scrollTo({ top: p.scrollTop }),
      ),
    );
  }

  /** Cambia de documento guardando el punto de lectura del que se deja. */
  function eligePestana(id: number) {
    if (id === pestanaActiva) return;
    const destino = pestanas.find((p) => p.id === id);
    if (!destino) return;
    const vivo = estadoDePestana();
    setPestanas((v) =>
      v.map((p) => (p.id === pestanaActiva ? { ...p, ...vivo } : p)),
    );
    setPestanaActiva(id);
    aplicaPestana(destino);
  }

  /** ⌃Tab y ⇧⌃Tab: la pestaña siguiente y la anterior, en círculo. */
  function rotaPestana(delta: number) {
    if (pestanas.length < 2) return;
    const i = pestanas.findIndex((p) => p.id === pestanaActiva);
    if (i < 0) return;
    const j = (i + delta + pestanas.length) % pestanas.length;
    eligePestana(pestanas[j].id);
  }

  /** Cierra una pestaña que NO es la activa: no hay nada que preguntar de
   *  la que está en pantalla, así que este camino es directo. */
  function cierraPestanaInactiva(id: number) {
    const victima = pestanas.find((p) => p.id === id);
    if (!victima) return;
    setPestanas((v) => v.filter((p) => p.id !== id));
    puntosLimpiosRef.current.delete(victima.workPath);
    invoke("close_document", { workPath: victima.workPath }).catch(() => {});
  }

  /** La «×» de una pestaña: si es la activa, es el cierre de siempre (con
   *  su pregunta de cambios sin guardar); si es otra, se pregunta por ella
   *  solo cuando tiene cambios, y se cierra sin traerla a pantalla. */
  function cierraPestana(id: number) {
    if (id === pestanaActiva) {
      closeDocument();
      return;
    }
    const victima = pestanas.find((p) => p.id === id);
    if (!victima) return;
    if (victima.modified) {
      // la pregunta es sobre un documento que no se está viendo: se trae a
      // pantalla **y entonces** se pregunta, para que nadie descarte a
      // ciegas el trabajo de otra pestaña
      eligePestana(id);
      setUnsavedAsk(() => cerrarDocumento);
      return;
    }
    cierraPestanaInactiva(id);
  }

  /** Cierra el documento (preguntando si hay cambios sin guardar). */
  function closeDocument() {
    if (!workPath) return;
    setMenuOpen(false);
    conCambiosGuardados(cerrarDocumento);
  }

  /** Cierra el documento activo y borra su copia de trabajo. Si quedaba
   *  otro abierto, se pasa a él; si no, se vuelve al estado vacío. */
  function cerrarDocumento() {
    if (!workPath) return;
    const anterior = workPath;
    puntosLimpiosRef.current.delete(anterior);
    const restantes = pestanas.filter((p) => p.id !== pestanaActiva);
    if (restantes.length > 0) {
      setPestanas(restantes);
      setPestanaActiva(restantes[0].id);
      aplicaPestana(restantes[0]);
      borraSesion(anterior).catch((e) =>
        console.warn("no se ha podido borrar el apunte de sesión:", e),
      );
      invoke("close_document", { workPath: anterior }).catch((e) =>
        setError(String(e)),
      );
      return;
    }
    setPestanas([]);
    setPestanaActiva(null);
    setError(null);
    setNotice(null);
    setWorkPath(null);
    setOriginalPath(null);
    setNombreProvisional(null);
    setPageCount(0);
    setPageSizes([]);
    setThumbs([]);
    setPageVersions([]);
    busqueda.limpiar(true);
    setModified(false);
    setHadPassword(false);
    setDocPassword(null);
    setMantenerClave(null);
    setProtegido(false);
    setProtPendiente(false);
    setMode("select");
    setPageIndex(0);
    // el modo lectura es del documento que se estaba leyendo: sin documento
    // dejaría el estado vacío sin barra y sin salida visible
    setModoLectura(false);
    setPropuestas([]);
    setPropuestaActual(null);
    setOutlineState([]);
    setComentarios([]);
    setAnnotSel(null);
    setPaginasSel(new Set());
    setViewRotation(0);
    setHayFormularios(false);
    setVistasAtras([]);
    setVistasAdelante([]);
    setSidebarTab("paginas");
    setFirmasDoc([]);
    setBandaFirmas(false);
    evictAll();
    setDocVersion((v) => v + 1);
    setMenuState(false).catch(() => {});
    borraSesion(anterior).catch((e) =>
      console.warn("no se ha podido borrar el apunte de sesión:", e),
    );
    invoke("close_document", { workPath: anterior }).catch((e) => setError(String(e)));
  }

  async function openFile() {
    // sin preguntar por los cambios: el documento abierto no se va a
    // ninguna parte, se queda en su pestaña
    const selected = await open({
      filters: [{ name: "PDF", extensions: ["pdf"] }],
      multiple: true,
    });
    if (typeof selected === "string") {
      await openPath(selected);
      return;
    }
    // varios a la vez: el mismo trato que soltarlos sobre la ventana
    if (!Array.isArray(selected) || selected.length === 0) return;
    if (selected.length === 1) {
      await openPath(selected[0]);
      return;
    }
    setDropAsk(selected);
  }

  /** Los abre todos, **cada uno en su pestaña**: elegir cinco PDF y
   *  quedarse con uno era perder cuatro gestos. Se abren en el orden en que
   *  se eligieron y manda el primero. */
  async function abrirTodosEnPestanas(pdfs: string[]) {
    setDropAsk(null);
    for (const uno of pdfs) {
      await openPath(uno);
    }
    setNotice(
      `${plural(pdfs.length, "documento abierto", "documentos abiertos")} · ⌃Tab cambia de pestaña`,
    );
  }

  // Tamaños de página del documento: el esqueleto del scroll continuo
  useEffect(() => {
    if (!workPath) {
      setPageSizes([]);
      return;
    }
    let cancelled = false;
    invoke<PageSize[]>("get_page_sizes", { path: workPath })
      .then((s) => {
        if (!cancelled) setPageSizes(s);
      })
      .catch((e) => {
        if (!cancelled) setError(String(e));
      });
    return () => {
      cancelled = true;
    };
  }, [workPath, docVersion]);

  // Firmas del documento: se comprueban al abrirlo y tras cada cambio del
  // fichero (firmar escribe una copia, pero deshacer o guardar sí cambian
  // lo que hay). Un PDF sin firmas no enseña nada.
  useEffect(() => {
    if (!workPath) {
      setFirmasDoc([]);
      return;
    }
    let cancelled = false;
    verifySignatures(workPath)
      .then((f) => {
        if (cancelled) return;
        setFirmasDoc(f);
        if (f.length > 0) setBandaFirmas(true);
      })
      .catch(() => {
        if (!cancelled) setFirmasDoc([]);
      });
    return () => {
      cancelled = true;
    };
  }, [workPath, docVersion]);

  // Zonas marcadas para censurar: viven en el PDF como anotaciones, así que
  // sobreviven a guardar y hay que releerlas con cada cambio del documento
  const refrescarMarcas = useCallback(() => {
    if (!workPath) {
      setMarcasRedact([]);
      return;
    }
    listRedactions(workPath)
      .then(setMarcasRedact)
      .catch(() => setMarcasRedact([]));
  }, [workPath]);

  useEffect(() => {
    refrescarMarcas();
  }, [refrescarMarcas, docVersion]);

  /** Quita una marca por su `annot_index`, que es el que trae
   *  `list_redactions` y el único que entiende el backend: un ordinal entre
   *  las marcas de la página apunta a otra anotación en cuanto hay un
   *  resaltado o un campo delante. */
  async function quitarMarca(page: number, annotIndex: number) {
    if (!workPath) return;
    try {
      await unmarkRedaction(workPath, page, annotIndex);
      refrescarMarcas();
      afterPageMutation(page);
    } catch (e) {
      setError(String(e));
    }
  }

  /** Quita todas las marcas de golpe, en un solo paso de deshacer. El
   *  backend lo hace en UNA mutación y ya recorre las anotaciones de mayor
   *  a menor: la UI no repite ni el bucle ni el orden. */
  async function quitarTodasLasMarcas() {
    if (!workPath || marcasRedact.length === 0) return;
    try {
      const hechas = await unmarkAllRedactions(workPath);
      refrescarMarcas();
      afterMutation(pageCount);
      setNotice(
        `${plural(hechas, "marca quitada", "marcas quitadas")} · ${MOD}Z para deshacer`,
      );
    } catch (e) {
      setError(String(e));
    }
  }

  /** Ensayo previo y confirmación destructiva antes de censurar de verdad. */
  async function pedirAplicarRedaccion() {
    if (!workPath || marcasRedact.length === 0) return;
    try {
      setRedactAsk(await applyRedactions(workPath, true));
    } catch (e) {
      setError(String(e));
    }
  }

  async function aplicarRedaccion() {
    if (!workPath) return;
    try {
      const r = await applyRedactions(workPath, false);
      setRedactAsk(null);
      setMode("select");
      refrescarMarcas();
      afterMutation(pageCount);
      setNotice(
        `Censurado: ${plural(r.textos, "bloque de texto", "bloques de texto")} y ${plural(r.imagenes, "imagen", "imágenes")} eliminados`,
      );
    } catch (e) {
      setError(String(e));
    }
  }

  async function pedirSanear() {
    if (!workPath) return;
    try {
      setSanitizeAsk(await sanitizePdf(workPath, true));
    } catch (e) {
      setError(String(e));
    }
  }

  async function aplicarSanear() {
    if (!workPath) return;
    try {
      await sanitizePdf(workPath, false);
      setSanitizeAsk(null);
      afterMutation(pageCount);
      setNotice(`Información oculta eliminada · ${MOD}Z para deshacer`);
    } catch (e) {
      setError(String(e));
    }
  }

  // Marcadores del documento (pestaña del sidebar)
  useEffect(() => {
    if (!workPath) {
      setOutlineState([]);
      return;
    }
    let cancelled = false;
    getOutline(workPath)
      .then((o) => {
        if (!cancelled) setOutlineState(o);
      })
      .catch(() => {
        if (!cancelled) setOutlineState([]);
      });
    return () => {
      cancelled = true;
    };
  }, [workPath, docVersion]);

  // Etiquetas de página: van con el documento y cambian con cualquier
  // mutación que toque las páginas, así que cuelgan de `docVersion`
  useEffect(() => {
    if (!workPath) {
      setEtiquetas([]);
      return;
    }
    let cancelled = false;
    getPageLabels(workPath)
      .then((r) => {
        if (!cancelled) setEtiquetas(r.rangos);
      })
      .catch(() => {
        if (!cancelled) setEtiquetas([]);
      });
    return () => {
      cancelled = true;
    };
  }, [workPath, docVersion]);

  /** Cómo se llama la página `i`: su etiqueta si el documento la trae, y si
   *  no, su número. */
  const nombreDePagina = useCallback(
    (i: number) => etiquetaDePagina(etiquetas, i),
    [etiquetas],
  );

  /** Escribe la numeración entera («Numerar páginas…»). */
  async function aplicarEtiquetas(rangos: RangoEtiquetas[]) {
    if (!workPath) return;
    setEtiquetasOpen(false);
    try {
      await setPageLabels(workPath, rangos);
      setEtiquetas(rangos);
      setModified(true);
      refrescarHistorial();
      setNotice(
        rangos.length === 0
          ? `Numeración quitada: las páginas vuelven a llamarse 1 a ${pageCount} · ${MOD}Z lo deshace`
          : `Páginas numeradas · ${MOD}Z lo deshace`,
      );
    } catch (e) {
      setError(String(e));
    }
  }

  // Comentarios de todo el documento (pestaña del sidebar): se recargan con
  // la misma versión que invalida el caché de renders y con cada anotación
  useEffect(() => {
    if (!workPath) {
      setComentarios([]);
      return;
    }
    let cancelled = false;
    getDocumentAnnotations(workPath)
      .then((c) => {
        if (!cancelled) setComentarios(c);
      })
      .catch(() => {
        if (!cancelled) setComentarios([]);
      });
    return () => {
      cancelled = true;
    };
  }, [workPath, docVersion, annotVersion]);

  // Adjuntos y capas del documento abierto. Se releen con cada cambio: unir
  // un PDF trae los suyos y sanitizar se los lleva
  useEffect(() => {
    if (!workPath) {
      setAdjuntos([]);
      setCapas([]);
      return;
    }
    let cancelled = false;
    listAttachments(workPath)
      .then((a) => {
        if (!cancelled) setAdjuntos(a);
      })
      .catch(() => {
        if (!cancelled) setAdjuntos([]);
      });
    listLayers(workPath)
      .then((c) => {
        if (!cancelled) setCapas(c);
      })
      .catch(() => {
        if (!cancelled) setCapas([]);
      });
    return () => {
      cancelled = true;
    };
  }, [workPath, docVersion]);

  /** Saca un adjunto al disco, tal cual: los mismos bytes que hay dentro. */
  async function guardarAdjunto(index: number, a: Adjunto) {
    if (!workPath) return;
    const dest = await save({ defaultPath: a.name, title: "Guardar el adjunto" });
    if (!dest) return;
    try {
      await saveAttachment(workPath, index, dest);
      setNotice(`${a.name} guardado en ${dest}`);
    } catch (e) {
      setError(String(e));
    }
  }

  /** «Abrir»: el backend lo saca a un temporal y se lo pasa al visor del
   *  sistema. Es la acción de la fila —un adjunto de una factura es un XML
   *  que se quiere ver, no guardar—. */
  async function abrirAdjunto(index: number, a: Adjunto) {
    if (!workPath) return;
    try {
      // el backend lo deja en el temporal y devuelve la ruta: abrirla es de
      // la UI, que es la que tiene el permiso del opener
      await abrirRuta(await openAttachment(workPath, index));
      setNotice(`Abriendo ${a.name} con el visor del sistema…`);
    } catch (e) {
      setError(String(e));
    }
  }

  /** Quitar un adjunto es borrar trabajo del usuario: se pregunta antes,
   *  con la frase que usa el resto de la app, y ⌘Z lo devuelve. */
  async function borrarAdjunto(index: number, a: Adjunto) {
    if (!workPath) return;
    setAdjuntoAsk(null);
    try {
      await deleteAttachment(workPath, index);
      afterMutation(pageCount);
      setNotice(`${a.name} ya no va dentro del documento · ${MOD}Z para deshacer`);
    } catch (e) {
      setError(String(e));
    }
  }

  /** «Adjuntar fichero…»: hasta ahora solo se llegaba desde la pestaña
   *  «Adjuntos», que solo existe cuando el documento **ya** lleva uno, así
   *  que el primer adjunto no se podía poner. Ahora está también en el
   *  menú, y al terminar abre la pestaña, que es donde queda. */
  async function anadirAdjunto() {
    if (!workPath) return;
    const sel = await open({ multiple: false, title: "Añadir un adjunto" });
    if (typeof sel !== "string") return;
    try {
      await addAttachment(workPath, sel, "");
      afterMutation(pageCount);
      abrirPestana("adjuntos");
      setNotice(`Adjunto añadido · ${MOD}Z para deshacer`);
    } catch (e) {
      setError(String(e));
    }
  }

  /** Apagar una capa escribe en el documento (PDFium respeta el `/OFF` del
   *  fichero al renderizar): por eso deja su paso de deshacer y se dice. */
  async function cambiarCapa(index: number, visible: boolean) {
    if (!workPath) return;
    try {
      await setLayerVisible(workPath, index, visible);
      afterMutation(pageCount);
      setNotice(
        `${visible ? "Capa encendida" : "Capa apagada"} · cambia el documento · ${MOD}Z lo devuelve`,
      );
    } catch (e) {
      setError(String(e));
    }
  }

  /** Abre una pestaña del panel lateral y le lleva el foco: con el teclado
   *  se llega a la lista sin pasar por el ratón. */
  function abrirPestana(
    tab:
      | "paginas"
      | "marcadores"
      | "comentarios"
      | "firmas"
      | "adjuntos"
      | "capas",
  ) {
    setSidebarVisible(true);
    setSidebarTab(tab);
    if (tab === "comentarios") setFocoComentarios((n) => n + 1);
  }

  /** Clic en una fila del panel: a su página y con su popover abierto. */
  function irAComentario(c: AnotacionDoc) {
    saltarA(c.page_index);
    setSelOwner(c.page_index);
    setAnnotSel({ page: c.page_index, index: c.index });
  }

  async function borrarComentario(c: AnotacionDoc) {
    if (!workPath) return;
    try {
      await invoke("remove_annotation", {
        workPath,
        pageIndex: c.page_index,
        annotIndex: c.index,
      });
      setAnnotSel(null);
      afterAnnotate(c.page_index);
      setNotice(`Comentario eliminado · ${MOD}Z para deshacer`);
    } catch (e) {
      setError(String(e));
    }
  }

  /** Responder a un comentario: un hilo `/IRT`, como Acrobat, para que la
   *  conversación se lea también allí. */
  async function responderComentario(c: AnotacionDoc, texto: string) {
    if (!workPath) return;
    try {
      await replyAnnotation(
        workPath,
        c.page_index,
        c.index,
        texto,
        autorComentarios(),
      );
      afterAnnotate(c.page_index);
      setNotice(`Respuesta añadida · ${MOD}Z para deshacer`);
    } catch (e) {
      setError(String(e));
    }
  }

  /** Estado de revisión del comentario (Aceptado, Rechazado, Cancelado,
   *  Completado o ninguno), guardado en el PDF como lo guarda Acrobat. */
  async function estadoComentario(c: AnotacionDoc, estado: EstadoComentario) {
    if (!workPath) return;
    try {
      await setAnnotationState(workPath, c.page_index, c.index, estado, autorComentarios() ?? undefined);
      afterAnnotate(c.page_index);
      setNotice(
        estado
          ? `Comentario marcado como «${nombreEstado(estado)}» · ${MOD}Z para deshacer`
          : `Estado quitado · ${MOD}Z para deshacer`,
      );
    } catch (e) {
      setError(String(e));
    }
  }

  /** «Exportar comentarios…»: el formato se pregunta antes, porque de él
   *  dependen la extensión, las opciones y qué comando se llama. */
  function exportarComentarios() {
    if (!workPath) return;
    if (comentarios.length === 0) {
      setNotice("Este documento no tiene comentarios que exportar");
      return;
    }
    setComentariosAsk(true);
  }

  /** Los tres formatos del resumen de Acrobat: la lista en llano, el
   *  resumen imprimible en PDF —que se abre al terminar, como «Crear PDF
   *  desde imágenes»— y el XFDF con el que se devuelve una revisión. */
  async function aplicarExportarComentarios(opts: {
    formato: FormatoComentarios;
    orden: OrdenComentarios;
  }) {
    if (!workPath) return;
    setComentariosAsk(false);
    const ext = opts.formato;
    const nombres: Record<FormatoComentarios, string> = {
      txt: "Texto",
      pdf: "PDF",
      xfdf: "Comentarios XFDF",
    };
    const dest = await save({
      filters: [{ name: nombres[ext], extensions: [ext] }],
      defaultPath: (originalPath ?? "documento.pdf").replace(
        /\.pdf$/i,
        `-comentarios.${ext}`,
      ),
      title: "Exportar comentarios",
    });
    if (!dest) return;
    const nombreDoc = originalPath?.split(/[\\/]/).pop();
    try {
      setNotice("Exportando los comentarios…", { persistente: true });
      if (ext === "pdf") {
        await exportCommentsPdf(workPath, dest, opts.orden, nombreDoc);
      } else if (ext === "xfdf") {
        await exportCommentsXfdf(workPath, dest);
      } else {
        await exportComments(workPath, dest, nombreDoc);
      }
      setNotice(
        `${plural(comentarios.length, "comentario exportado", "comentarios exportados")} a ${dest}`,
      );
      // el resumen es un documento para leer: se abre, como el PDF de
      // imágenes, que es lo que se quiere hacer con él a continuación
      if (ext === "pdf") await openPath(dest);
    } catch (e) {
      setNotice(null);
      setError(String(e));
    }
  }

  /** «Importar comentarios…»: la revisión que devuelve otro revisor sobre
   *  su copia. **Añade**, no sustituye, y en una sola mutación. */
  async function importarComentarios() {
    if (!workPath) return;
    const sel = await open({
      multiple: false,
      filters: [{ name: "Comentarios", extensions: ["xfdf"] }],
      title: "Importar comentarios",
    });
    if (typeof sel !== "string") return;
    try {
      setNotice("Importando los comentarios…", { persistente: true });
      const cuantos = await importCommentsXfdf(workPath, sel);
      afterMutation(pageCount);
      abrirPestana("comentarios");
      setNotice(
        cuantos === 0
          ? "Ese fichero no traía ningún comentario"
          : `${plural(cuantos, "comentario añadido", "comentarios añadidos")} · ${MOD}Z los quita`,
      );
    } catch (e) {
      setNotice(null);
      setError(String(e));
    }
  }

  /** A qué altura de la página que se está leyendo va el visor, en puntos
   *  de la página: es el `top` del destino fino de un marcador. */
  function altoDeVista(): { top: number | null; zoom: number | null } {
    const el = pageElsRef.current.get(pageIndex);
    const s = pageSizes[pageIndex];
    const visor = viewerRef.current;
    if (!el || !s || !visor) return { top: null, zoom: null };
    const alto = el.getBoundingClientRect().height;
    if (alto <= 0) return { top: null, zoom: null };
    const escala = alto / s.height;
    const dif = visor.getBoundingClientRect().top - el.getBoundingClientRect().top;
    return {
      top: Math.max(0, Math.round((dif / escala) * 10) / 10),
      zoom: Math.round(zoomNumRef.current * 100) / 100,
    };
  }

  /** ⌘B: marcador de este punto de lectura, titulado con lo que haya
   *  seleccionado —como Acrobat— y, si no hay nada, con la página. Se
   *  añade al final del árbol y ⌘Z lo quita. */
  function crearMarcador() {
    if (!workPath || pageCount === 0) return;
    const sel = textoSelRef.current.replace(/\s+/g, " ").trim();
    // un título largo no cabe en el panel de nadie: Acrobat también lo corta
    const title = sel
      ? sel.length > 60
        ? `${sel.slice(0, 60)}…`
        : sel
      : `Página ${pageIndex + 1}`;
    const { top, zoom: z } = altoDeVista();
    persistOutline([
      ...outline,
      { title, page_index: pageIndex, top, zoom: z, children: [] },
    ]);
    setNotice(`Marcador «${title}» creado · ${MOD}Z lo quita`);
  }

  /** Seguir un marcador: su página **y** su punto de vista, que es lo que
   *  distingue un marcador de un número de página. Sigue pasando por
   *  `saltarA`, así que ⌥← vuelve a donde se estaba. */
  function seguirMarcador(n: OutlineNode) {
    if (n.page_index === null) return;
    const destino = n.page_index;
    const top = n.top;
    if (n.zoom && n.zoom > 0) setZoom(n.zoom);
    saltarA(destino);
    // El salto entero espera al relayout del zoom, no solo el `top`: con un
    // zoom distinto del de ahora, el `scrollIntoView` de `saltarA` mide con
    // las alturas viejas y el relayout lo deja en el principio del
    // documento. Aquí se calcula el destino **absoluto** y se hace UN solo
    // `scrollTo`, así que da igual de dónde venga el usuario (AC-072).
    requestAnimationFrame(() =>
      requestAnimationFrame(() => {
        const visor = viewerRef.current;
        const el = pageElsRef.current.get(destino);
        const s = pageSizes[destino];
        if (!visor || !el) return;
        const caja = el.getBoundingClientRect();
        const cajaVisor = visor.getBoundingClientRect();
        const arriba = visor.scrollTop + caja.top - cajaVisor.top;
        const escala = s ? caja.height / s.height : 1;
        visor.scrollTo({ top: arriba + (top ?? 0) * escala });
      }),
    );
  }

  /** «Reconocer campos…»: el backend propone y **no escribe nada**. La
   *  lista se pinta sobre las páginas y se corrige antes de crear nada, que
   *  es lo que Acrobat no hace (allí los campos se crean sin preguntar y
   *  quitar los que sobran cuesta más que dibujarlos). */
  async function reconocerCampos() {
    if (!workPath) return;
    try {
      setNotice("Buscando campos…", { persistente: true });
      const encontrados = await detectFormFields(workPath, null);
      setPropuestaActual(null);
      setPropuestas(encontrados);
      if (encontrados.length === 0) {
        setNotice("No se ha encontrado ningún campo en este documento", {
          accion: {
            texto: "Añadir campo…",
            onClick: () => {
              setNotice(null);
              selectMode("select");
              setMode("form-new");
            },
          },
        });
        return;
      }
      const dudosos = encontrados.filter(
        (c) => c.confianza < CONFIANZA_MINIMA,
      ).length;
      setNotice(
        `${plural(encontrados.length, "campo encontrado", "campos encontrados")}${
          dudosos > 0 ? ` · ${dudosos} sin confirmar: repásalos` : ""
        }`,
      );
    } catch (e) {
      setNotice(null);
      setError(String(e));
    }
  }

  function quitarPropuesta(i: number) {
    setPropuestas((v) => v.filter((_, j) => j !== i));
    setPropuestaActual(null);
  }

  function renombraPropuesta(i: number, name: string) {
    setPropuestas((v) => v.map((c, j) => (j === i ? { ...c, name } : c)));
  }

  /** Texto ↔ casilla sobre la propuesta, con su rectángulo intacto: una
   *  heurística confunde una casilla con una raya de escribir, y hasta
   *  ahora la única salida era borrarla y volver a dibujar el campo. */
  function cambiaTipoPropuesta(i: number) {
    setPropuestas((v) =>
      v.map((c, j) =>
        j === i
          ? { ...c, kind: c.kind === "checkbox" ? "text" : "checkbox" }
          : c,
      ),
    );
  }

  /** «Revisar uno a uno»: lleva la vista a la propuesta siguiente y la
   *  señala, para poder corregirla o quitarla antes de crear nada. */
  function revisarPropuesta() {
    if (propuestas.length === 0) return;
    const siguiente =
      propuestaActual === null ? 0 : (propuestaActual + 1) % propuestas.length;
    setPropuestaActual(siguiente);
    saltarA(propuestas[siguiente].page_index);
  }

  /** «Crear todos»: **una sola cirugía** (`create_form_fields`). Con una
   *  llamada por campo, un fallo en el sexto dejaba cinco escritos y un
   *  error, y un formulario a medias es peor que uno que no se creó. */
  async function crearPropuestas() {
    if (!workPath || propuestas.length === 0) return;
    const lote = propuestas;
    setPropuestas([]);
    setPropuestaActual(null);
    // los campos los acaba de crear el usuario: decirle a continuación que
    // «este documento se puede rellenar» tapaba el recuento con una
    // obviedad, que es lo que vio el QA del ciclo 8
    avisoFormRef.current = workPath;
    try {
      setNotice("Creando los campos…", { persistente: true });
      const hechos = await createFormFields(
        workPath,
        lote.map((c, i) => ({
          page_index: c.page_index,
          kind: c.kind,
          rect: c.rect,
          name: c.name,
          group: "",
          export_value: "",
          options: [],
          props: {
            tooltip: null,
            obligatorio: false,
            solo_lectura: false,
            valor_defecto: null,
            // el orden de tabulación es el de la lista: es el orden en el
            // que se han encontrado, que es el de lectura de la página
            orden_tab: i,
          },
        })),
      );
      afterMutation(pageCount);
      setNotice(
        `${plural(hechos, "campo creado", "campos creados")} · ${MOD}Z los quita`,
      );
    } catch (e) {
      setNotice(null);
      setError(String(e));
      afterMutation(pageCount);
    }
  }

  async function persistOutline(nodes: OutlineNode[]) {
    if (!workPath) return;
    const anterior = outline;
    setOutlineState(nodes);
    try {
      await setOutline(workPath, nodes);
      setModified(true);
      refrescarHistorial();
    } catch (e) {
      setOutlineState(anterior);
      setError(String(e));
    }
  }

  async function openProperties() {
    if (!workPath) return;
    try {
      setPropsFicha(null);
      setPropsVista(null);
      setPropsDraft(await getMetadata(workPath));
      // la ficha va aparte: si falla, las propiedades que se escriben se
      // siguen pudiendo editar
      getDocumentInfo(workPath)
        .then(setPropsFicha)
        .catch(() => setPropsFicha(null));
      // y la vista inicial, por lo mismo: un `/OpenAction` que no se sepa
      // leer no puede llevarse por delante el resto del diálogo
      getOpenAction(workPath)
        .then(setPropsVista)
        .catch(() => setPropsVista(null));
    } catch (e) {
      setError(String(e));
    }
  }

  async function saveProperties(meta: Metadata, vista: VistaInicial | null) {
    if (!workPath) return;
    try {
      await setMetadata(workPath, meta);
      // la vista inicial solo se escribe si se ha tocado: es otro paso de
      // historial y no tiene por qué llevárselo quien solo cambia el título
      const antes = propsVista;
      if (vista && antes && JSON.stringify(vista) !== JSON.stringify(antes)) {
        await setOpenAction(workPath, vista);
        await historial.agrupar(2);
      }
      setPropsDraft(null);
      setModified(true);
      refrescarHistorial();
    } catch (e) {
      setError(String(e));
    }
  }

  // Esc vuelve a Seleccionar desde cualquier herramienta, como en Acrobat;
  // las páginas limpian sus borradores al cambiar el modo. Con un diálogo
  // abierto lo consume el modal, y dentro de un campo manda el borrador que
  // se esté escribiendo (nota, texto nuevo, marcador): el primer Esc lo
  // cancela y el segundo, ya fuera del campo, sale de la herramienta.
  useEffect(() => {
    if (mode === "select") return;
    function onKey(e: KeyboardEvent) {
      if (e.key !== "Escape") return;
      // con un modal o un menú abiertos, la tecla es suya
      if (document.querySelector(".modal-backdrop, .menu-backdrop")) return;
      // con coincidencias pintadas el primer Esc es para la búsqueda
      if (hayCoincidenciasRef.current) return;
      const tag = (e.target as HTMLElement)?.tagName;
      if (tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT") return;
      setMode("select");
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [mode]);

  // Ancho útil del visor para el zoom "ajustar a ventana" (redondeado a
  // múltiplos de 16px y con debounce para no invalidar el caché de renders
  // en cada píxel del arrastre de la ventana)
  useEffect(() => {
    const el = viewerRef.current;
    if (!el) return;
    let timer: number | undefined;
    const mide = () => {
      setViewerW(Math.max(320, Math.round(el.clientWidth / 16) * 16));
      setViewerH(Math.max(240, Math.round(el.clientHeight / 16) * 16));
    };
    const ro = new ResizeObserver(() => {
      window.clearTimeout(timer);
      timer = window.setTimeout(mide, 150);
    });
    ro.observe(el);
    mide();
    return () => {
      ro.disconnect();
      window.clearTimeout(timer);
    };
  }, []);

  // Atajos de teclado: ⌘O abrir, ⌘S guardar, ⌘F buscar, ⌘± zoom, ←/→ páginas
  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      // con un modal o un menú abierto los atajos no actúan sobre el
      // documento de debajo (solo Escape, que es cómo se cierran)
      if (
        e.key !== "Escape" &&
        document.querySelector(".modal-backdrop, .menu-backdrop")
      )
        return;
      const mod = e.metaKey || e.ctrlKey;
      const tag = (e.target as HTMLElement)?.tagName;
      const enCampo = tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT";
      if (e.key === "Escape" && !mod) {
        // la lectura en voz alta es lo primero que Esc calla: es lo que
        // está pasando y lo que molesta si no se para
        if (lectura.leyendo) {
          e.preventDefault();
          lectura.parar();
          return;
        }
        // preparando la impresión, Esc la cancela: es el trabajo largo que
        // hay en marcha, y hasta ahora solo se paraba con el botón de la banda
        if (printCancelRef.current) {
          e.preventDefault();
          printCancelRef.current.cancelado = true;
          return;
        }
        // en pantalla completa la primera salida es la de la presentación
        if (pantallaCompleta) {
          e.preventDefault();
          cambiaPantallaCompleta(false);
          return;
        }
        // y en modo lectura, la de volver a ver las herramientas
        if (modoLectura) {
          e.preventDefault();
          setModoLectura(false);
          return;
        }
        // Acrobat quita los resaltados de coincidencia desde cualquier
        // sitio, no solo con el foco dentro del campo
        if (busqueda.matches.length > 0) busqueda.limpiar(true);
      } else if (mod && !e.shiftKey && (e.key === "l" || e.key === "L") && pageCount > 0) {
        e.preventDefault();
        cambiaPantallaCompleta(!pantallaCompleta);
      } else if (e.ctrlKey && !e.metaKey && !e.altKey && e.key === "Tab") {
        // ⌃Tab y ⇧⌃Tab rotan entre documentos, como las pestañas de
        // Acrobat. ⌘1…⌘9 NO saltan a la pestaña N: ⌘1 y ⌘2 son el zoom de
        // Acrobat y pesan más que un atajo que casi nadie usa
        if (pestanas.length > 1) {
          e.preventDefault();
          rotaPestana(e.shiftKey ? -1 : 1);
        }
      } else if (
        mod &&
        e.shiftKey &&
        (e.key === "h" || e.key === "H") &&
        pageCount > 0
      ) {
        // en Acrobat es ⌘H, que en macOS es «Ocultar Vitela» y se la queda
        // AppKit antes que el webview: aquí lleva ⇧, y está escrito en los
        // atajos como todo lo demás
        e.preventDefault();
        cambiaModoLectura(!modoLectura);
      } else if (mod && e.shiftKey && (e.key === "y" || e.key === "Y") && pageCount > 0) {
        // leer desde la página que se está leyendo hasta el final, y la
        // segunda pulsación calla, como el conmutador de Acrobat
        e.preventDefault();
        if (lectura.leyendo) lectura.parar();
        else lectura.leer(pageIndex, true);
      } else if (
        mod &&
        !e.shiftKey &&
        !enCampo &&
        (e.key === "b" || e.key === "B") &&
        pageCount > 0
      ) {
        // ⌘B: marcador de este punto de lectura, con el texto seleccionado
        // por título, que es como se hace en Acrobat
        e.preventDefault();
        crearMarcador();
      } else if (mod && (e.key === "r" || e.key === "R") && pageCount > 0) {
        // ⌘R: las reglas, como en Acrobat (en un navegador esta tecla
        // recarga, así que el `preventDefault` no es opcional)
        e.preventDefault();
        setReglas((v) => !v);
      } else if (mod && e.key === ";" && pageCount > 0) {
        e.preventDefault();
        setGuiasVisibles((v) => !v);
      } else if (mod && e.shiftKey && (e.key === "u" || e.key === "U") && pageCount > 0) {
        // ⇧⌘U: ajustar a la cuadrícula, el atajo de Acrobat
        e.preventDefault();
        setAjustarCuadricula((v) => !v);
        setNotice(
          ajustarCuadricula
            ? "Lo que coloques ya no se ajusta a la cuadrícula"
            : "Lo que coloques se ajustará a la cuadrícula",
        );
      } else if (
        mod &&
        ((e.key === "u" || e.key === "U") || e.key === "'") &&
        pageCount > 0
      ) {
        // ⌘U es el de Acrobat; ⌘' se queda como alias, que es el que llevaba
        // Vitela desde el ciclo 8
        e.preventDefault();
        setCuadricula((v) => !v);
      } else if (mod && e.shiftKey && (e.key === "l" || e.key === "L")) {
        // el modo nocturno del documento: solo cambia lo que se ve
        e.preventDefault();
        aplicaPrefs({ ...prefs, nocturno: !prefs.nocturno });
      } else if (!mod && e.altKey && e.key === "ArrowLeft" && pageCount > 0) {
        e.preventDefault();
        atrasVista();
      } else if (!mod && e.altKey && e.key === "ArrowRight" && pageCount > 0) {
        e.preventDefault();
        adelanteVista();
      } else if (mod && e.key === "o") {
        e.preventDefault();
        openFile();
      } else if (mod && (e.key === "w" || e.key === "W") && pageCount > 0) {
        // ⌘W cierra la PESTAÑA, como en Acrobat, y lo decide la UI, que es
        // la única que sabe cuántos documentos hay abiertos: el evento
        // `cerrar-solicitado` no dice si ha sido ⌘W, el botón rojo o ⌘Q y
        // sigue significando «cerrar la app». Con la última pestaña se
        // vuelve al estado vacío; la ventana solo la cierra el botón rojo
        e.preventDefault();
        closeDocument();
      } else if (mod && (e.key === "s" || e.key === "S")) {
        e.preventDefault();
        if (e.shiftKey) {
          if (pageCount > 0) saveFileAs();
        } else if (modified) guardar();
      } else if (mod && e.key === ",") {
        e.preventDefault();
        setPrefsAbiertas(true);
      } else if (
        mod &&
        e.altKey &&
        (e.key === "1" || e.code === "Digit1") &&
        pageCount > 0
      ) {
        e.preventDefault();
        setSidebarVisible((v) => !v);
      } else if (
        mod &&
        e.altKey &&
        (e.key === "2" || e.code === "Digit2") &&
        pageCount > 0
      ) {
        e.preventDefault();
        abrirPestana("marcadores");
      } else if (
        mod &&
        e.altKey &&
        (e.key === "3" || e.code === "Digit3") &&
        pageCount > 0
      ) {
        e.preventDefault();
        abrirPestana("comentarios");
      } else if (mod && e.key === "0" && pageCount > 0) {
        // los tres ajustes de Acrobat: ⌘0 página entera, ⌘1 tamaño real,
        // ⌘2 ajustar al ancho
        e.preventDefault();
        setZoom("pagina");
      } else if (mod && e.key === "1" && pageCount > 0) {
        e.preventDefault();
        setZoom(1);
      } else if (mod && e.key === "2" && pageCount > 0) {
        e.preventDefault();
        setZoom("ajuste");
      } else if (mod && e.shiftKey && (e.key === "n" || e.key === "N") && pageCount > 0) {
        e.preventDefault();
        setPageDraft(String(paginaMostrada + 1));
      } else if (mod && e.key === "p" && pageCount > 0) {
        e.preventDefault();
        printDocument();
      } else if (mod && !enCampo && e.key === "d" && pageCount > 0) {
        e.preventDefault();
        openProperties();
      } else if (
        mod &&
        e.shiftKey &&
        (e.key === "f" || e.key === "F") &&
        pageCount > 0
      ) {
        // ⇧⌘F: la búsqueda avanzada de Acrobat, que aquí es el mismo campo
        // con el cajón desplegado y el ámbito puesto en «una carpeta»
        e.preventDefault();
        busquedaCarpeta.setAmbito("carpeta");
        setCajonBusqueda(true);
        (document.querySelector(".search input") as HTMLInputElement)?.focus();
      } else if (mod && e.key === "f" && pageCount > 0) {
        e.preventDefault();
        (document.querySelector(".search input") as HTMLInputElement)?.focus();
      } else if (
        mod &&
        !e.shiftKey &&
        (e.key === "+" || e.key === "=") &&
        pageCount > 0
      ) {
        // sin Shift: ⇧⌘+ y ⇧⌘− quedan reservados para girar la vista
        e.preventDefault();
        setZoom(nivelZoom(zoomNum, 1));
      } else if (mod && !e.shiftKey && e.key === "-" && pageCount > 0) {
        e.preventDefault();
        setZoom(nivelZoom(zoomNum, -1));
      } else if (
        mod &&
        e.shiftKey &&
        (e.key === "+" || e.key === "*" || e.key === "-" || e.key === "_") &&
        pageCount > 0
      ) {
        // girar SOLO la vista: el fichero no cambia y el título no gana el «•»
        e.preventDefault();
        const sentido = e.key === "-" || e.key === "_" ? -90 : 90;
        setViewRotation((r) => (r + sentido + 360) % 360);
      } else if (mod && (e.key === "g" || e.key === "G") && pageCount > 0) {
        e.preventDefault();
        // sin coincidencias en pantalla, ⌘G repite la última búsqueda, como
        // Acrobat; y si nunca se ha buscado, lleva el foco al campo
        if (busqueda.matches.length > 0) busqueda.gotoMatch(e.shiftKey ? -1 : 1);
        else if (!busqueda.repetirUltima())
          (document.querySelector(".search input") as HTMLInputElement)?.focus();
      } else if (mod && e.key === "/") {
        // ⌘/ y F1, las dos teclas con las que se pide ayuda: hasta ahora la
        // pantalla que enseña los atajos era la única sin ninguno
        e.preventDefault();
        setAtajosAbiertos(true);
      } else if (!mod && e.key === "F1") {
        e.preventDefault();
        setAtajosAbiertos(true);
      } else if (mod && !enCampo && (e.key === "z" || e.key === "Z") && pageCount > 0) {
        // sin historial el atajo no hace nada, como en Acrobat: llamar al
        // backend solo dejaba un error rojo de «Nada que deshacer»
        e.preventDefault();
        if (e.shiftKey) {
          if (historial.puedeRehacer) historial.rehacer();
        } else if (historial.puedeDeshacer) historial.deshacer();
      } else if (mod && !enCampo && e.key === "y" && pageCount > 0) {
        e.preventDefault();
        if (historial.puedeRehacer) historial.rehacer();
      } else if (!mod && !e.altKey && !enCampo && e.key === "ArrowRight") {
        gotoPage(paginaVecina(1));
      } else if (!mod && !e.altKey && !enCampo && e.key === "ArrowLeft") {
        gotoPage(paginaVecina(-1));
      }
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  });

  // Ancho de página en pantalla: fijo por zoom numérico, o el ancho útil
  // del visor en modo "ajuste". El ancho de render (px físicos) es también
  // la clave del caché: unifica ambos modos.
  const vistaGirada = viewRotation === 90 || viewRotation === 270;
  const PADDING_VIEWER = 48;
  const VIEWER_PAD_BOTTOM = 72;
  // en pantalla completa manda la presentación de Acrobat: una hoja cada vez
  const modoPagina: ModoPagina = pantallaCompleta ? "una" : vista.modoPagina;
  const dobles = modoPagina === "dos" || modoPagina === "dos-continuo";
  const continuo = modoPagina === "continuo" || modoPagina === "dos-continuo";
  const columnas = dobles ? 2 : 1;
  // filas de pantalla: una página por fila, o de dos en dos con la portada
  // suelta. En las presentaciones no continuas solo se pinta la fila actual
  const filas = useMemo(
    () => filasDePaginas(pageCount, dobles, vista.portadaSola),
    [pageCount, dobles, vista.portadaSola],
  );
  const filaActual = Math.max(
    0,
    filas.findIndex((f) => f.includes(pageIndex)),
  );
  // en las presentaciones de dos, Acrobat identifica el pliego por su página
  // izquierda y ←/→ avanzan el pliego entero: con [0,1] en pantalla, la
  // primera → pasaba a la 2, que ya estaba a la vista, y no movía nada
  const paginaMostrada = dobles ? (filas[filaActual]?.[0] ?? pageIndex) : pageIndex;
  const hayAnterior = dobles ? filaActual > 0 : pageIndex > 0;
  const haySiguiente = dobles
    ? filaActual < filas.length - 1
    : pageIndex < pageCount - 1;
  const filasVisibles = continuo ? filas : filas.slice(filaActual, filaActual + 1);
  const fitWidth = viewerW ? Math.max(320, viewerW - PADDING_VIEWER) : BASE_WIDTH;
  // con dos hojas por fila cada una se queda con la mitad, menos el hueco
  const anchoColumna = (fitWidth - (columnas - 1) * PAGE_GAP) / columnas;
  const fitHeight = viewerH
    ? Math.max(200, viewerH - VIEWER_PAD_TOP - VIEWER_PAD_BOTTOM)
    : BASE_WIDTH;
  // «ajustar»: el ancho de hoja que hace que la página ocupe justo el ancho
  // (o el alto) útil. Se usa la proporción más alta del documento para que el
  // ancho no cambie al desplazarse entre páginas de tamaños distintos, y se
  // cuenta el giro de la vista, que intercambia alto y ancho en pantalla.
  const ratioMax = pageSizes.reduce((m, s) => Math.max(m, s.height / s.width), 0);
  const anchoAjuste =
    vistaGirada && ratioMax > 0 ? anchoColumna / ratioMax : anchoColumna;
  const anchoPagina =
    ratioMax > 0
      ? Math.max(
          160,
          Math.min(anchoAjuste, vistaGirada ? fitHeight : fitHeight / ratioMax),
        )
      : anchoAjuste;
  const displayWidth =
    zoom === "ajuste"
      ? anchoAjuste
      : zoom === "pagina"
        ? anchoPagina
        : BASE_WIDTH * zoom;
  const ocupado = useSyncExternalStore(subscribeBusy, busyCount) > 0;

  // el zoom sobrevive a cerrar la app: es lo que promete «zoom al abrir: el
  // último», que hasta ahora solo valía dentro de la misma sesión
  useEffect(() => {
    guardaZoom(zoom);
  }, [zoom]);

  const zoomNum = typeof zoom === "number" ? zoom : displayWidth / BASE_WIDTH;
  // el listener se registra una sola vez: lee el zoom vivo por referencia
  const zoomNumRef = useRef(zoomNum);
  zoomNumRef.current = zoomNum;

  /** Salta al porcentaje escrito en la píldora (recortado en silencio). */
  function aplicaZoomEscrito() {
    const n = Number.parseInt(zoomDraft ?? "", 10);
    setZoomDraft(null);
    if (!Number.isNaN(n)) setZoom(recortaZoom(n / 100));
  }

  // ⌘+rueda y pinch del trackpad hacen zoom sobre el visor, como en
  // Acrobat (el pinch llega como wheel con ctrlKey). Sin animación: el punto
  // bajo el cursor se queda bajo el cursor porque la rueda deja el ancla del
  // scroll ahí y el efecto de `displayWidth` la restituye.
  useEffect(() => {
    const el = viewerRef.current;
    if (!el) return;
    function onWheel(e: WheelEvent) {
      if (!e.ctrlKey && !e.metaKey) return;
      e.preventDefault();
      const visor = viewerRef.current;
      const hoja = (e.target as HTMLElement | null)?.closest?.(
        ".page-wrap",
      ) as HTMLElement | null;
      if (visor && hoja) {
        const rv = visor.getBoundingClientRect();
        const rh = hoja.getBoundingClientRect();
        scrollAnchorRef.current = {
          page: Number(hoja.dataset.page),
          frac: (e.clientY - rh.top) / rh.height,
          offset: e.clientY - rv.top,
          fracX: (e.clientX - rh.left) / rh.width,
          offsetX: e.clientX - rv.left,
        };
      }
      setZoom(recortaZoom(zoomNumRef.current * Math.exp(-e.deltaY / 300)));
    }
    el.addEventListener("wheel", onWheel, { passive: false });
    return () => el.removeEventListener("wheel", onWheel);
  }, []);

  const registerEl = useCallback((page: number, el: HTMLDivElement | null) => {
    if (el) pageElsRef.current.set(page, el);
    else pageElsRef.current.delete(page);
  }, []);

  /** Alturas en pantalla de cada fila con el ancho de hoja dado: la de la
   *  hoja más alta de la fila. Con la vista girada un cuarto, alto y ancho
   *  se intercambian. */
  function alturasFila(width: number): number[] {
    return filas.map((fila) =>
      Math.max(
        ...fila.map((i) => {
          const s = pageSizes[i];
          if (!s) return width;
          return vistaGirada ? width : (width * s.height) / s.width;
        }),
      ),
    );
  }

  /** Lleva el visor al principio de una página (miniaturas, marcadores,
   *  enlaces internos, flechas y píldora). */
  const gotoPage = useCallback(
    (i: number) => {
      if (pageCount === 0) return;
      const target = Math.max(0, Math.min(i, pageCount - 1));
      setPageIndex(target);
      // en las presentaciones de una fila cada vez, la página nueva sustituye
      // a la anterior: el sitio al que ir es el principio del visor
      if (!continuo) {
        viewerRef.current?.scrollTo({ top: 0 });
        return;
      }
      pageElsRef.current.get(target)?.scrollIntoView({ block: "start" });
    },
    [pageCount, continuo],
  );

  // «Leer en voz alta»: la síntesis del webview sobre el texto de cada
  // página, desde la que se está leyendo. Va detrás de `gotoPage`, que es
  // un `useCallback` y no se iza.
  const lectura = useLectura({
    workPath,
    pageCount,
    onNotice: setNotice,
    onPagina: gotoPage,
  });

  /** La página a la que llevan ← y →: el pliego entero en las presentaciones
   *  de dos, la de al lado en el resto. */
  function paginaVecina(delta: number): number {
    if (!dobles) return pageIndex + delta;
    return filas[filaActual + delta]?.[0] ?? pageIndex;
  }

  /** Salta a la página escrita en la píldora; fuera de rango, gotoPage la
   *  recorta en silencio. Es un salto largo, así que pasa por `saltarA` y
   *  ⌥← devuelve a donde se estaba leyendo. */
  /** «Ir a la página» entiende la etiqueta que ella misma enseña —«xii»— y,
   *  si lo escrito no es ninguna, el número físico, como Acrobat. */
  function irAPaginaEscrita() {
    const destino = paginaEscrita(pageDraft ?? "", etiquetas, pageCount);
    setPageDraft(null);
    if (destino === null) {
      if ((pageDraft ?? "").trim())
        setNotice(`En este documento no hay ninguna página «${pageDraft}»`);
      return;
    }
    saltarA(destino);
  }

  /** El punto de lectura de ahora mismo. */
  function vistaActual(): Vista {
    return {
      page: pageIndex,
      scrollTop: viewerRef.current?.scrollTop ?? 0,
      zoom,
    };
  }

  /** Deja el punto de lectura restaurado: primero el zoom y la página, y
   *  cuando el visor ya tiene su alto nuevo, el scroll. */
  function restaurarVista(v: Vista) {
    setZoom(v.zoom);
    setPageIndex(v.page);
    requestAnimationFrame(() =>
      requestAnimationFrame(() =>
        viewerRef.current?.scrollTo({ top: v.scrollTop }),
      ),
    );
  }

  /** Salto largo (enlace, marcador, comentario o coincidencia): apila de
   *  dónde se viene para que ⌥← devuelva ahí, como en Acrobat.
   *
   *  El punto de partida se lee AQUÍ y no dentro del updater de `setState`:
   *  React ejecuta los updaters al procesar la cola, ya después del
   *  `scrollIntoView` síncrono de `gotoPage`, así que lo que se apilaba era
   *  el destino y ⌥← no volvía a ninguna parte. */
  function saltarA(page: number) {
    const desde = vistaActual();
    // dos Enter seguidos en la búsqueda apilaban dos veces el mismo sitio
    if (desde.page !== page) {
      setVistasAtras((v) => [...v.slice(-49), desde]);
      setVistasAdelante([]);
    }
    gotoPage(page);
  }

  function atrasVista() {
    if (vistasAtras.length === 0) return;
    const desde = vistaActual();
    const v = vistasAtras[vistasAtras.length - 1];
    setVistasAtras((p) => p.slice(0, -1));
    setVistasAdelante((p) => [...p, desde]);
    restaurarVista(v);
  }

  function adelanteVista() {
    if (vistasAdelante.length === 0) return;
    const desde = vistaActual();
    const v = vistasAdelante[vistasAdelante.length - 1];
    setVistasAdelante((p) => p.slice(0, -1));
    setVistasAtras((p) => [...p, desde]);
    restaurarVista(v);
  }

  /** Deja la app en presentación o la saca. Lo llaman ⌘L y el evento de la
   *  ventana (botón verde, ⌃⌘F), para que las dos entradas dejen la misma
   *  app: es el que faltaba, y salir por el botón verde dejaba el chrome
   *  escondido. */
  const aplicaPantallaCompleta = useCallback(
    (valor: boolean) => {
      setPantallaCompleta(valor);
      // la presentación no tiene herramientas (tampoco en Acrobat): así Esc
      // es siempre la salida, sin tener que pulsarlo dos veces
      if (!valor) return;
      setMode("select");
      if (avisoPantallaVisto()) return;
      marcaAvisoPantalla();
      setNotice("Pulsa Esc para salir de la pantalla completa");
    },
    [setNotice],
  );

  /** Presentación a pantalla completa: el chrome desaparece y la hoja se
   *  queda sola. Esc sale, y la primera vez se dice cómo. */
  /** «Modo lectura»: esconde barra, fila contextual y panel y deja el
   *  documento con su píldora, **sin** salir de la ventana. Se avisa la
   *  primera vez de cómo se vuelve, que es lo único que no se ve. */
  function cambiaModoLectura(valor: boolean) {
    setModoLectura(valor);
    if (valor) setNotice(`Modo lectura · Esc o ⇧${MOD}H para volver`);
  }

  function cambiaPantallaCompleta(valor: boolean) {
    aplicaPantallaCompleta(valor);
    ponerPantallaCompleta(valor).catch((e) => setError(String(e)));
  }

  useEffect(
    () => onPantallaCompleta(aplicaPantallaCompleta),
    [aplicaPantallaCompleta],
  );

  // la píldora vuelve al acercar el ratón al borde inferior, como el Dock:
  // el resto del tiempo la presentación es papel y nada más
  useEffect(() => {
    if (!pantallaCompleta) {
      setPildoraVisible(false);
      return;
    }
    function onMove(e: MouseEvent) {
      setPildoraVisible(e.clientY > window.innerHeight - 96);
    }
    window.addEventListener("mousemove", onMove);
    return () => window.removeEventListener("mousemove", onMove);
  }, [pantallaCompleta]);

  function cambiaVista(parte: Partial<typeof vista>) {
    const siguiente = { ...vista, ...parte };
    setVista(siguiente);
    guardaVista(siguiente);
  }

  /** Guarda y aplica al instante una preferencia. */
  const aplicaPrefs = useCallback((p: Preferencias) => {
    guardaPreferencias(p);
    setPrefs(p);
  }, []);

  // tema y color del lienzo: atributos en el <html>, que es donde el CSS
  // los espera (con «automático» no se pone nada y manda el sistema)
  useEffect(() => {
    const raiz = document.documentElement;
    if (prefs.tema === "automatico") delete raiz.dataset.tema;
    else raiz.dataset.tema = prefs.tema;
    raiz.dataset.lienzo = prefs.lienzo;
  }, [prefs.tema, prefs.lienzo]);

  // el cajón de resultados va plegado: quien solo quiere buscar no paga nada
  const [cajonBusqueda, setCajonBusqueda] = useState(false);

  const busqueda = useBusqueda({
    workPath,
    // la pasada de contexto (frase y bloque de cada coincidencia) solo se
    // paga cuando el cajón está desplegado, que es donde se usa
    contexto: cajonBusqueda,
    // los saltos entre coincidencias también se apilan: ⌥← vuelve a donde
    // se estaba leyendo antes de buscar
    gotoPage: saltarA,
    onError: (e) => setError(String(e)),
  });
  // buscar en una carpeta entera: otra búsqueda, con sus resultados de otros
  // ficheros, que no se pintan sobre la página y sobreviven a abrir un PDF
  const busquedaCarpeta = useBusquedaCarpeta({
    onError: (e) => setError(String(e)),
  });
  const limpiarBusqueda = busqueda.limpiar;
  const busquedaTrasMutacion = busqueda.trasMutacion;
  hayCoincidenciasRef.current = busqueda.matches.length > 0;

  /** «Recargar el documento» de la red de seguridad: todo lo derivado
   *  —tamaños, miniaturas, comentarios, firmas— cuelga de `docVersion`, así
   *  que subirlo vuelve a leerlo todo de la copia de trabajo sin abrir nada
   *  ni perder un solo cambio. */
  function recargarDocumento() {
    setDocVersion((v) => v + 1);
    setNotice("Documento recargado desde la copia de trabajo");
  }

  /** Un resultado de la búsqueda en carpeta: el PDF se abre **en una
   *  pestaña nueva** —el documento que se está mirando no se toca— y el
   *  visor salta a la página de la coincidencia en cuanto está montado. */
  async function abrirCoincidenciaDeCarpeta(path: string, pageIndex: number) {
    const work = await openPath(path);
    if (!work) return;
    requestAnimationFrame(() => saltarA(pageIndex));
  }

  // Seguimiento del scroll: la página cuyo centro queda más cerca del centro
  // del visor es la "actual" (píldora y sidebar), sin provocar scroll.
  function onViewerScroll() {
    if (scrollRafRef.current !== null) return;
    scrollRafRef.current = requestAnimationFrame(() => {
      scrollRafRef.current = null;
      const el = viewerRef.current;
      if (!el || pageSizes.length === 0 || filas.length === 0) return;
      // en las presentaciones no continuas no hay páginas que seguir: el
      // ancla del zoom es la única fila que hay en pantalla
      if (!continuo) {
        const alto = alturasFila(displayWidth)[filaActual] ?? 1;
        scrollAnchorRef.current = {
          page: pageIndex,
          frac: Math.max(0, (el.scrollTop - VIEWER_PAD_TOP) / alto),
          offset: 0,
        };
        return;
      }
      const centro = el.scrollTop + el.clientHeight / 2;
      const alturas = alturasFila(displayWidth);
      let y = VIEWER_PAD_TOP;
      let best = 0;
      let bestDist = Infinity;
      let anchor: { page: number; frac: number; offset: number } | null = null;
      for (let i = 0; i < alturas.length; i++) {
        const h = alturas[i];
        if (anchor === null && y + h > el.scrollTop) {
          anchor = {
            page: filas[i][0],
            frac: Math.max(0, (el.scrollTop - y) / h),
            offset: 0,
          };
        }
        const d = Math.abs(y + h / 2 - centro);
        if (d < bestDist) {
          bestDist = d;
          best = i;
        }
        y += h + PAGE_GAP;
      }
      scrollAnchorRef.current = anchor;
      setPageIndex(filas[best]?.[0] ?? 0);
    });
  }

  // Al cambiar el ancho de página (zoom o ajuste) se conserva el punto de
  // lectura: el DOM ya tiene el ancho nuevo, así que se mide dónde ha
  // quedado el punto ancla y se corrige el scroll para devolverlo a su sitio.
  const prevWidthRef = useRef(displayWidth);
  useLayoutEffect(() => {
    if (prevWidthRef.current === displayWidth) return;
    prevWidthRef.current = displayWidth;
    const el = viewerRef.current;
    const a = scrollAnchorRef.current;
    const hoja = a ? pageElsRef.current.get(a.page) : null;
    if (!el || !a || !hoja) return;
    const rv = el.getBoundingClientRect();
    const rh = hoja.getBoundingClientRect();
    el.scrollTop += rh.top - rv.top + a.frac * rh.height - a.offset;
    if (a.fracX !== undefined && a.offsetX !== undefined) {
      el.scrollLeft += rh.left - rv.left + a.fracX * rh.width - a.offsetX;
    }
  }, [displayWidth]);

  // Al cambiar de presentación el visor conserva su scroll y `onViewerScroll`
  // no llega a correr, así que nadie recalcula la página: se quedaba el
  // contador de antes con la hoja de después. Acrobat conserva la página.
  const modoPrevioRef = useRef(modoPagina);
  useEffect(() => {
    if (modoPrevioRef.current === modoPagina) return;
    modoPrevioRef.current = modoPagina;
    gotoPage(pageIndex);
  }, [modoPagina, pageIndex, gotoPage]);

  // Apunte de sesión: no en cada tecla, sino diez segundos después del
  // último cambio. Silencioso —ni insignias ni avisos—, como en Acrobat
  useEffect(() => {
    if (!workPath || !modified) return;
    const t = setTimeout(() => {
      // el fallo no se le cuenta al usuario (es un apunte silencioso), pero
      // tampoco se traga en silencio: un contrato roto aquí dejaba la
      // recuperación sin nada que recuperar y nadie se enteraba
      autosaveState(workPath, originalPath, modified).catch((e) =>
        console.warn("no se ha podido apuntar la sesión:", e),
      );
    }, 10000);
    return () => clearTimeout(t);
  }, [workPath, originalPath, modified, docVersion, annotVersion]);

  /** Tras anotar: invalidar el render de esa página sin recargar todo. */
  const afterAnnotate = useCallback(
    (page: number) => {
      setModified(true);
      refrescarHistorial();
      evictPage(page);
      setAnnotVersion((v) => v + 1);
      refreshThumb(page);
    },
    [refrescarHistorial, refreshThumb, evictPage],
  );

  /** Tras mutar UNA página: re-render y miniatura solo de esa página. */
  const afterPageMutation = useCallback(
    (page: number) => {
      setModified(true);
      refrescarHistorial();
      limpiarBusqueda();
      setPageVersions((v) => {
        const next = [...v];
        next[page] = (next[page] ?? 0) + 1;
        return next;
      });
      evictPage(page);
      refreshThumb(page);
    },
    [refrescarHistorial, refreshThumb, evictPage, limpiarBusqueda],
  );

  /** Tras mutar el documento: refrescar render, miniaturas y limpiar búsqueda. */
  const afterMutation = useCallback((newCount: number, nextPage?: number) => {
    setPageCount(newCount);
    setModified(true);
    setError(null);
    // las cajas de las coincidencias ya no valen, pero la lista sí interesa:
    // se rehace la búsqueda en vez de tirarla (Acrobat mantiene el panel)
    busquedaTrasMutacion();
    setPageIndex((p) => Math.max(0, Math.min(nextPage ?? p, newCount - 1)));
    // el docVersion nuevo deja inservible todo el caché: liberar los blobs
    evictAll();
    setDocVersion((v) => v + 1);
    refrescarHistorial();
  }, [refrescarHistorial, evictAll, busquedaTrasMutacion]);

  const reemplazo = useReemplazo({
    workPath,
    query: busqueda.lastQuery,
    matches: busqueda.matches,
    matchIdx: busqueda.matchIdx,
    pageCount,
    onNotice: setNotice,
    onError: mostrarError,
    afterMutation,
  });

  async function rotatePage(i: number) {
    if (!workPath) return;
    try {
      await invoke("rotate_page", { workPath, pageIndex: i });
      afterMutation(pageCount);
    } catch (e) {
      setError(String(e));
    }
  }

  async function deletePage(i: number) {
    if (!workPath || pageCount <= 1) return;
    try {
      const count = await invoke<number>("delete_page", {
        workPath,
        pageIndex: i,
      });
      afterMutation(count);
    } catch (e) {
      setError(String(e));
    }
  }

  /** Gira las páginas marcadas (o la actual) en una sola mutación. */
  async function girarPaginas(cuartos: number) {
    if (!workPath) return;
    const idx = paginasSel.size > 0 ? [...paginasSel] : [pageIndex];
    try {
      await rotatePages(workPath, idx, cuartos);
      afterMutation(pageCount);
    } catch (e) {
      setError(String(e));
    }
  }

  /** Borra las páginas marcadas de una vez: un solo paso de deshacer. */
  async function borrarPaginas() {
    if (!workPath || paginasSel.size === 0) return;
    const idx = [...paginasSel];
    if (idx.length >= pageCount) return;
    try {
      const count = await deletePages(workPath, idx);
      setPaginasSel(new Set());
      afterMutation(count);
      // sin confirmación: ⌘Z las devuelve, pero el aviso lo dice
      setNotice(
        `${idx.length} ${idx.length === 1 ? "página eliminada" : "páginas eliminadas"} · ${MOD}Z para deshacer`,
      );
    } catch (e) {
      setError(String(e));
    }
  }

  async function movePage(from: number, to: number) {
    if (!workPath || to < 0 || to >= pageCount) return;
    try {
      await invoke("move_page", { workPath, fromIndex: from, toIndex: to });
      afterMutation(pageCount, from === pageIndex ? to : undefined);
    } catch (e) {
      setError(String(e));
    }
  }

  async function blankPageAfter(i: number) {
    if (!workPath) return;
    try {
      const count = await addBlankPage(workPath, i + 1);
      afterMutation(count);
    } catch (e) {
      setError(String(e));
    }
  }

  async function duplicatePageAt(i: number) {
    if (!workPath) return;
    try {
      const count = await duplicatePage(workPath, i);
      afterMutation(count);
    } catch (e) {
      setError(String(e));
    }
  }

  /** «Insertar PDF…»: se elige el fichero y **después** dónde entra, que en
   *  Acrobat es antes o después de una página concreta. Antes iba siempre
   *  detrás de la que se estaba leyendo, y la portada de un informe va
   *  delante. */
  async function insertPdfHere() {
    if (!workPath) return;
    const selected = await open({
      filters: [{ name: "PDF", extensions: ["pdf"] }],
      multiple: false,
      title: "Insertar PDF",
    });
    if (typeof selected !== "string") return;
    // cuántas páginas trae: es un vistazo al fichero, sin abrirlo ni pedir
    // contraseña, y si no se puede leer se pregunta igual
    const info = await pdfInfo(selected).catch(() => null);
    setInsertarAsk({ path: selected, paginas: info?.page_count ?? null });
  }

  async function aplicarInsertar(index: number, pageIndices: number[] | null) {
    const pendiente = insertarAsk;
    setInsertarAsk(null);
    if (!workPath || !pendiente) return;
    try {
      const count = await insertPdfAt(
        workPath,
        pendiente.path,
        index,
        pageIndices,
      );
      afterMutation(count, index);
      setNotice(
        `${
          pageIndices
            ? plural(pageIndices.length, "página insertada", "páginas insertadas")
            : "PDF insertado"
        } en la página ${index + 1} · ${MOD}Z para deshacer`,
      );
    } catch (e) {
      setError(String(e));
    }
  }

  async function askRemoveMarginal(zona: "watermark" | "header" | "footer") {
    if (!workPath) return;
    try {
      const r = await removeMarginalText(workPath, zona, true);
      if (r.textos === 0) {
        setNotice(
          zona === "watermark"
            ? "No hay ninguna marca de agua que quitar."
            : "No hay encabezados o pies que quitar.",
        );
        return;
      }
      setMarginalAsk({ zona, textos: r.textos });
    } catch (e) {
      setError(String(e));
    }
  }

  /** «Quitar fondo…»: primero el ensayo, que cuenta lo que hay —el color, la
   *  imagen y el texto que se pusieron como fondo o marca de agua— y solo
   *  entonces se pregunta. Sin esto, un fondo de imagen se quedaba dentro
   *  para siempre en cuanto ⌘Z dejaba de alcanzar. */
  async function askRemoveBackground() {
    if (!workPath) return;
    try {
      const r = await removeBackground(workPath, true);
      const cuantos = (r.objetos ?? 0) + (r.textos ?? 0);
      if (cuantos === 0) {
        setNotice("Este documento no lleva ningún fondo puesto por Vitela.");
        return;
      }
      setFondoAsk(cuantos);
    } catch (e) {
      setError(String(e));
    }
  }

  async function applyRemoveBackground() {
    if (!workPath) return;
    setFondoAsk(null);
    try {
      await removeBackground(workPath, false);
      afterMutation(pageCount);
      setNotice(`Fondo quitado · ${MOD}Z para deshacer`);
    } catch (e) {
      setError(String(e));
    }
  }

  async function applyRemoveMarginal() {
    if (!workPath || !marginalAsk) return;
    try {
      await removeMarginalText(workPath, marginalAsk.zona, false);
      // encabezado y pie van juntos en la UI: quitar ambas bandas
      if (marginalAsk.zona === "header") {
        await removeMarginalText(workPath, "footer", false);
        // un solo paso de deshacer para las dos bandas
        await historial.agrupar(2);
      }
      setMarginalAsk(null);
      afterMutation(pageCount);
    } catch (e) {
      setError(String(e));
    }
  }

  async function applyWatermark(opts: MarcaAguaOpts) {
    if (!workPath) return;
    try {
      // el fondo no se coloca ni se gira: cubre la página entera y va por su
      // propio comando, que además deja marcado lo que pone para que
      // «Quitar fondo…» sepa qué quitar
      if (opts.tipo === "color" || (opts.tipo === "imagen" && opts.detras)) {
        await addBackground({
          workPath,
          color: opts.tipo === "color" ? hexToRgba(opts.color) : null,
          imagePng: opts.tipo === "imagen" ? opts.imagePng : null,
          opacity: opts.opacity / 100,
          pageIndices: opts.pageIndices,
        });
        setWmOpen(false);
        afterMutation(pageCount);
        setNotice(
          `Fondo añadido en ${plural(opts.pageIndices?.length ?? pageCount, "página", "páginas")} · ${MOD}Z para deshacer`,
        );
        return;
      }
      await addWatermark({
        workPath,
        text: opts.text,
        fontSize: opts.fontSize,
        // la opacidad va en su propio parámetro; el color se manda opaco para
        // no aplicarla dos veces
        color: hexToRgba(opts.color),
        opacity: opts.opacity / 100,
        rotation: opts.rotation,
        diagonal: opts.rotation === 45,
        position: opts.position,
        pageIndices: opts.pageIndices,
        imagePng: opts.imagePng,
        detras: opts.detras,
      });
      setWmOpen(false);
      afterMutation(pageCount);
      setNotice(
        `${opts.detras ? "Fondo añadido" : "Marca de agua añadida"} en ${plural(opts.pageIndices?.length ?? pageCount, "página", "páginas")} · ${MOD}Z para deshacer`,
      );
    } catch (e) {
      setError(String(e));
    }
  }

  /** «Numeración Bates»: el sello corrido de los expedientes, una sola
   *  mutación para todas las páginas. */
  async function aplicarBates(opts: BatesOpts) {
    if (!workPath) return;
    setBatesOpen(false);
    try {
      const cuantas = await addBates({
        workPath,
        prefijo: opts.prefijo,
        sufijo: opts.sufijo,
        digitos: opts.digitos,
        empiezaEn: opts.empiezaEn,
        position: opts.position,
        pageIndices: opts.pageIndices,
      });
      afterMutation(pageCount);
      setNotice(
        `${plural(cuantas, "página numerada", "páginas numeradas")} · ${MOD}Z para deshacer`,
      );
    } catch (e) {
      setError(String(e));
    }
  }

  async function applyHeaderFooter(
    zonas: HeaderFooter,
    fontSize: number,
    pageIndices: number[] | null,
  ) {
    if (!workPath) return;
    try {
      await addHeaderFooter(workPath, zonas, fontSize, pageIndices);
      setHfOpen(false);
      afterMutation(pageCount);
      setNotice(
        `Encabezado y pie añadidos en ${plural(pageIndices?.length ?? pageCount, "página", "páginas")} · ${MOD}Z para deshacer`,
      );
    } catch (e) {
      setError(String(e));
    }
  }

  /** Los permisos solo se restringen si hay contraseña de permisos: sin
   *  ella el fichero no puede sostener ninguna restricción. */
  function permisosDe(d: ProtegerDraft) {
    return d.owner.trim()
      ? { imprimir: d.imprimir, copiar: d.copiar, editar: d.editar }
      : TODO_PERMITIDO;
  }

  /** Anota la protección del documento abierto. `encrypt_pdf` sin `destPath`
   *  NO cifra la copia de trabajo (quedaría ilegible para el resto de
   *  comandos): apunta la contraseña y la aplica `save_pdf` al guardar. Por
   *  eso ⌘Z no la quita —no es un paso del historial—; la quita
   *  «Quitar la contraseña». */
  async function applyProtect() {
    if (!workPath || !protectDraft || !protectDraft.user) return;
    const d = protectDraft;
    try {
      await encryptPdf({
        workPath,
        destPath: null,
        userPassword: d.user,
        ownerPassword: d.owner || null,
        permisos: permisosDe(d),
      });
      setProtectDraft(null);
      // el candado no se pone todavía: el fichero en disco sigue en claro
      setProtPendiente(true);
      // manda la contraseña nueva, no la que traía el fichero al abrirse:
      // guardar ya no tiene que preguntar cuál de las dos
      setHadPassword(false);
      setDocPassword(null);
      setModified(true);
      // la protección no entra en el historial (vive en un mapa por
      // work_path, que no cambia al deshacer): el aviso dice cómo se quita
      setNotice(
        "Se protegerá al guardar · Quitar la contraseña, en Seguridad",
      );
    } catch (e) {
      setError(String(e));
    }
  }

  /** Segunda opción del mismo diálogo: una copia protegida aparte. */
  async function applyProtectCopy() {
    // se valida antes de abrir el diálogo del sistema: nadie elige carpeta
    // para descubrir después que faltaba la contraseña
    if (!workPath || !protectDraft || !protectDraft.user) return;
    const d = protectDraft;
    const dest = await save({
      filters: [{ name: "PDF", extensions: ["pdf"] }],
      defaultPath: (originalPath ?? "documento.pdf").replace(
        /\.pdf$/i,
        "-protegido.pdf",
      ),
      title: "Guardar PDF protegido",
    });
    if (!dest) return;
    try {
      await encryptPdf({
        workPath,
        destPath: dest,
        userPassword: d.user,
        ownerPassword: d.owner || null,
        permisos: permisosDe(d),
      });
      setProtectDraft(null);
      setNotice(`Copia protegida guardada en ${dest}`);
    } catch (e) {
      setError(String(e));
    }
  }

  async function applyQuitarProteccion() {
    if (!workPath) return;
    try {
      await removeEncryption(workPath);
      setQuitarProtAsk(false);
      const eraPendiente = protPendiente;
      setProtegido(false);
      setProtPendiente(false);
      setHadPassword(false);
      setDocPassword(null);
      afterMutation(pageCount);
      setNotice(
        eraPendiente
          ? "Ya no se protegerá al guardar: el fichero se escribirá en claro"
          : "Contraseña quitada: al guardar, el fichero se abrirá sin pedir nada",
      );
    } catch (e) {
      setError(String(e));
    }
  }

  async function applyFlatten() {
    if (!workPath) return;
    try {
      await flattenPdf(workPath);
      setFlattenAsk(false);
      afterMutation(pageCount);
    } catch (e) {
      setError(String(e));
    }
  }

  /** ⌘P: el diálogo propio, antes que el del sistema (como Acrobat). */
  function printDocument() {
    if (!workPath) return;
    setPrintOpen(true);
  }

  /** Páginas que pide el diálogo, ya filtradas por pares/impares. La cuenta
   *  la hace `tipos.ts`, que es donde la comparte con el diálogo. */
  function paginasAImprimir(o: OpcionesImprimir): number[] {
    return paginasImprimibles(o, pageCount, pageIndex);
  }

  /** Las hojas del resumen de comentarios, ya rasterizadas. El PDF del
   *  resumen se escribe junto a la copia de trabajo —en el temporal, donde
   *  el barrido de huérfanos se lo lleva— y se abre para pintarlo, porque
   *  imprimir es pintar y la UI solo sabe pintar lo que le rasteriza el
   *  backend. */
  async function hojasDelResumen(
    orden: OrdenComentarios,
    dpi: number,
  ): Promise<string[]> {
    if (!workPath) return [];
    const dest = `${workPath.replace(/\.pdf$/i, "")}-comentarios.pdf`;
    await exportCommentsPdf(
      workPath,
      dest,
      orden,
      originalPath?.split(/[\\/]/).pop(),
    );
    const info = await invoke<{ page_count: number; work_path: string }>(
      "open_pdf",
      { path: dest, password: null },
    );
    try {
      const sizes = await invoke<PageSize[]>("get_page_sizes", {
        path: info.work_path,
      });
      const hojas: string[] = [];
      for (let i = 0; i < info.page_count; i++) {
        const anchoPt = sizes[i]?.width ?? 595;
        hojas.push(
          await renderPageSrc(
            info.work_path,
            i,
            Math.round((anchoPt * dpi) / 72),
          ),
        );
      }
      return hojas;
    } finally {
      await invoke("close_document", { workPath: info.work_path }).catch(
        () => {},
      );
      // `close_document` atenúa por dentro el menú nativo, y aquí sigue
      // habiendo documento abierto
      setMenuState(true).catch(() => {});
    }
  }

  /** Rasteriza solo el rango pedido y abre el diálogo del sistema. El bucle
   *  se puede cancelar desde la propia banda de progreso; al cancelar se
   *  liberan los blobs y no se abre nada. */
  async function prepararImpresion(o: OpcionesImprimir) {
    if (!workPath) return;
    // el diálogo no deja aceptar sin páginas —ni por el rango ni por el
    // filtro de pares/impares—, así que aquí solo queda la red de seguridad
    const idx = paginasAImprimir(o);
    if (idx.length === 0) return;
    setPrintOpts(o);
    setPrintOpen(false);
    const señal = { cancelado: false };
    printCancelRef.current = señal;
    const cancelar = {
      texto: "Cancelar",
      onClick: () => {
        señal.cancelado = true;
      },
    };
    // pocas páginas se pueden permitir 300 dpi; un documento entero a 300
    // tarda de más y no se nota en papel
    const dpi = idx.length <= 8 ? 300 : 200;
    const listas: { src: string; anchoIn?: number }[] = [];
    const soltar = () => {
      for (const p of listas) URL.revokeObjectURL(p.src);
    };
    try {
      setNotice("Preparando la impresión…", {
        persistente: true,
        dato: `0 / ${idx.length}`,
        accion: cancelar,
      });
      for (let n = 0; n < idx.length; n++) {
        if (señal.cancelado) break;
        const i = idx[n];
        const anchoPt = pageSizes[i]?.width ?? 595;
        const src = await renderPageSrc(
          workPath,
          i,
          Math.round((anchoPt * dpi) / 72),
          { withAnnotations: o.conMarcas },
        );
        listas.push({
          src,
          anchoIn:
            o.escala === "ajustar"
              ? undefined
              : (anchoPt / 72) *
                (o.escala === "real" ? 1 : o.porcentaje / 100),
        });
        setNotice("Preparando la impresión…", {
          persistente: true,
          dato: `${n + 1} / ${idx.length}`,
          accion: cancelar,
        });
      }
      // el resumen de comentarios va detrás del documento, como la casilla
      // de Acrobat: se compone con el mismo comando que lo exporta y se
      // rasteriza igual que las demás hojas
      if (o.resumen && !señal.cancelado) {
        setNotice("Componiendo el resumen de comentarios…", {
          persistente: true,
          accion: cancelar,
        });
        try {
          for (const src of await hojasDelResumen(o.ordenResumen, dpi))
            listas.push({ src });
        } catch (e) {
          // sin resumen se imprime igual el documento: no se tira el trabajo
          // ya rasterizado por una hoja de más
          console.warn("no se ha podido componer el resumen:", e);
          setError(
            "No se ha podido componer el resumen de comentarios; se imprime solo el documento",
          );
        }
      }
      if (señal.cancelado) {
        soltar();
        setNotice("Impresión cancelada");
        return;
      }
      setNotice(null);
      setPrintPages(listas);
    } catch (e) {
      soltar();
      setNotice(null);
      setError(String(e));
    } finally {
      printCancelRef.current = null;
    }
  }

  // Los avisos de éxito se van solos a los 6 s con un desvanecido corto
  // (U-14); los errores se quedan hasta que se cierran a mano, y los de
  // progreso hasta que el trabajo termina y los sustituye su resultado.
  useEffect(() => {
    if (!notice || noticePersistente) return;
    setNoticeSaliendo(false);
    const irse = setTimeout(() => setNoticeSaliendo(true), 6000);
    const quitar = setTimeout(() => setNoticeTexto(null), 6200);
    return () => {
      clearTimeout(irse);
      clearTimeout(quitar);
    };
  }, [notice, noticePersistente]);

  // cuando las páginas de impresión están montadas, abrir el diálogo
  useEffect(() => {
    if (!printPages) return;
    const t = setTimeout(() => {
      try {
        window.print();
      } catch (e) {
        setError(String(e));
      }
      setPrintPages(null);
    }, 200);
    // los blob URLs se liberan en la limpieza, no dentro del temporizador:
    // ahí se los llevaba el `clearTimeout` si llegaba otra preparación antes
    // de los 200 ms, y quedaban vivos hasta cerrar la app
    return () => {
      clearTimeout(t);
      for (const p of printPages) URL.revokeObjectURL(p.src);
    };
  }, [printPages]);

  /** «Exportar datos…»: los valores de los campos en XFDF, que es lo que
   *  entienden Acrobat y los gestores de formularios. */
  async function exportarDatosFormulario() {
    if (!workPath) return;
    const dest = await save({
      defaultPath: (originalPath ?? "formulario.pdf").replace(
        /\.pdf$/i,
        ".xfdf",
      ),
      filters: [{ name: "XFDF", extensions: ["xfdf"] }],
      title: "Exportar los datos del formulario",
    });
    if (!dest) return;
    try {
      const cuantos = await exportFormDataXfdf(workPath, dest);
      setNotice(
        `${plural(cuantos, "campo exportado", "campos exportados")} · ${dest}`,
      );
    } catch (e) {
      setError(String(e));
    }
  }

  /** «Importar datos…»: rellena los campos con un XFDF. No crea campos, así
   *  que se dice cuántos han entrado y cuántos no existían aquí. */
  async function importarDatosFormulario() {
    if (!workPath) return;
    const sel = await open({
      filters: [{ name: "XFDF", extensions: ["xfdf"] }],
      multiple: false,
      title: "Importar datos de formulario",
    });
    if (typeof sel !== "string") return;
    try {
      const r = await importFormDataXfdf(workPath, sel);
      afterMutation(pageCount);
      setNotice(
        r.sin_campo > 0
          ? `${plural(r.rellenados, "campo rellenado", "campos rellenados")} · ${plural(
              r.sin_campo,
              "valor del fichero no existe",
              "valores del fichero no existen",
            )} en este documento · ${MOD}Z lo deshace`
          : `${plural(r.rellenados, "campo rellenado", "campos rellenados")} · ${MOD}Z lo deshace`,
      );
    } catch (e) {
      setError(String(e));
    }
  }

  async function exportImages() {
    if (!workPath) return;
    const dir = await open({
      directory: true,
      multiple: false,
      title: "Carpeta para las imágenes",
    });
    if (typeof dir !== "string") return;
    try {
      setExportOpen(false);
      setNotice("Exportando imágenes…", { persistente: true });
      const rutas = await exportPagesPng(workPath, dir, exportDpi, exportFmt);
      setNotice(
        `${plural(rutas.length, "imagen exportada", "imágenes exportadas")} a ${dir}`,
      );
    } catch (e) {
      setNotice(null);
      setError(String(e));
    }
  }

  async function exportPlainText() {
    if (!workPath) return;
    const dest = await save({
      filters: [{ name: "Texto", extensions: ["txt"] }],
      defaultPath: (originalPath ?? "documento.pdf").replace(
        /\.pdf$/i,
        ".txt",
      ),
      title: "Exportar texto plano",
    });
    if (!dest) return;
    try {
      await exportText(workPath, dest);
      setNotice(`Texto exportado a ${dest}`);
    } catch (e) {
      setError(String(e));
    }
  }

  /** «Word (.docx)…»: exporta lo que `get_text_blocks` sabe —texto e
   *  imágenes, no maquetación—. El aviso de que es una aproximación se da
   *  antes de pedir carpeta y nombre: es una función que promete mucho y da
   *  menos, y descubrirlo con el fichero ya escrito es tarde. */
  async function exportarWord(paginas: number[] | null) {
    if (!workPath) return;
    setWordAsk(false);
    const dest = await save({
      filters: [{ name: "Word", extensions: ["docx"] }],
      defaultPath: (originalPath ?? "documento.pdf").replace(/\.pdf$/i, ".docx"),
      title: "Exportar a Word",
    });
    if (!dest) return;
    const cuantas = paginas?.length ?? pageCount;
    try {
      // `export_docx` es un solo viaje: no manda progreso ni se puede
      // interrumpir a mitad, así que la banda dice lo que hay —cuántas
      // páginas y que no hay vuelta atrás— en vez de fingir un contador o
      // un Cancelar que no harían nada. La ventana sigue respondiendo.
      setNotice("Exportando a Word… no se puede cancelar a mitad", {
        persistente: true,
        dato: plural(cuantas, "página", "páginas"),
      });
      const r = await exportDocx(workPath, dest, paginas);
      const resumen = `${plural(r.parrafos, "párrafo", "párrafos")} y ${plural(
        r.imagenes,
        "imagen",
        "imágenes",
      )} en ${dest}`;
      setNotice(
        r.perdido.length > 0
          ? `${resumen} · fuera: ${r.perdido.join(", ")}`
          : resumen,
      );
    } catch (e) {
      setNotice(null);
      setError(String(e));
    }
  }

  /** «Crear PDF desde imágenes…»: escribe un fichero nuevo (no toca el
   *  documento abierto) y lo abre al terminar, que es lo que se quiere hacer
   *  con él a continuación. Si una imagen no se puede leer, el diálogo sigue
   *  abierto con esa fila marcada: se quita y se vuelve a intentar con las
   *  demás, en vez de perder la lista.
   *
   *  Un lote puede salir a medias —el backend se salta la foto que no se
   *  deja leer en vez de tirar las otras diecinueve— y entonces se dice
   *  cuántas han salido y **cuál falta**: nunca un resultado parcial en
   *  silencio. Con imágenes saltadas el PDF no se abre y el diálogo se queda
   *  donde estaba, que es donde se arregla la lista. */
  async function crearDesdeImagenes(opts: {
    rutas: string[];
    tamano: TamanoImagenes;
  }) {
    const dest = await save({
      filters: [{ name: "PDF", extensions: ["pdf"] }],
      defaultPath: "imagenes.pdf",
      title: "Guardar el PDF de las imágenes",
    });
    if (!dest) return;
    try {
      setNotice("Creando el PDF…", { persistente: true });
      const { paginas, saltadas } = await pdfFromImages(
        opts.rutas,
        dest,
        opts.tamano,
      );
      setImagenesFallos(saltadas);
      if (saltadas.length > 0) {
        // a medias: se cuenta lo que ha salido, se nombran las que no y la
        // lista se queda en pantalla con esas filas marcadas
        const nombres = saltadas.map((r) => r.split(/[\\/]/).pop() ?? r);
        setNotice(
          `${paginas} de ${plural(opts.rutas.length, "página", "páginas")} en ${dest} · ${nombres.join(
            ", ",
          )} ${nombres.length === 1 ? "no se ha" : "no se han"} podido leer`,
        );
        return;
      }
      setImagenesOpen(false);
      await openPath(dest);
      setNotice(`${plural(paginas, "página escrita", "páginas escritas")} en ${dest}`);
    } catch (e) {
      setNotice(null);
      const msg = String(e);
      // no ha salido ninguna: el backend nombra las que no ha podido leer
      setImagenesFallos(opts.rutas.filter((r) => msg.includes(r)));
      setError(msg);
    }
  }

  async function applyCompress() {
    if (!workPath) return;
    try {
      setCompressOpen(false);
      setNotice("Comprimiendo…", { persistente: true });
      const r = await compressPdf(workPath, compressQuality, compressDpi);
      setNotice(
        r.imagenes === 0
          ? "No había imágenes que comprimir."
          : `${plural(r.imagenes, "imagen recomprimida", "imágenes recomprimidas")}: ${tamanoFichero(r.antes)} → ${tamanoFichero(r.despues)}`,
      );
      afterMutation(pageCount);
    } catch (e) {
      setNotice(null);
      // no reducir no es un fallo: el fichero queda intacto y se avisa
      if (String(e).startsWith("No se ha podido reducir")) setNotice(String(e));
      else setError(String(e));
    }
  }

  async function addPdf() {
    if (!workPath) return;
    const selected = await open({
      filters: [{ name: "PDF", extensions: ["pdf"] }],
      multiple: false,
      title: "Añadir páginas de otro PDF",
    });
    if (typeof selected !== "string") return;
    try {
      const count = await invoke<number>("merge_pdf", {
        workPath,
        otherPath: selected,
      });
      afterMutation(count);
    } catch (e) {
      setError(String(e));
    }
  }

  async function aplicarExtraer(opts: {
    rango: string;
    borrar: boolean;
    porPagina: boolean;
  }) {
    if (!workPath) return;
    const idx = parseRango(opts.rango, pageCount);
    if (idx.length === 0) {
      setError(
        `Escribe qué páginas quieres extraer, por ejemplo «1-3, 8» (el documento tiene ${pageCount})`,
      );
      return;
    }
    setExtraerOpen(false);
    try {
      if (opts.porPagina) {
        const dir = await open({
          directory: true,
          multiple: false,
          title: "Carpeta para los PDF extraídos",
        });
        if (typeof dir !== "string") return;
        // una sola operación: escribe todos los ficheros y borra dentro de la
        // misma mutación, así que un fallo a mitad no deja el trabajo hecho a
        // medias (antes era un bucle de N llamadas y un borrado aparte)
        const rutas = await extractEachPage({
          workPath,
          pageIndices: idx,
          destDir: dir,
          deleteAfter: opts.borrar,
        });
        if (opts.borrar) {
          setPaginasSel(new Set());
          afterMutation(pageCount - idx.length);
        }
        setNotice(`${plural(rutas.length, "PDF escrito", "PDF escritos")} en ${dir}`);
        return;
      }
      const dest = await save({
        filters: [{ name: "PDF", extensions: ["pdf"] }],
        defaultPath: (originalPath ?? "documento.pdf").replace(
          /\.pdf$/i,
          "-extraido.pdf",
        ),
        title: "Extraer páginas a un PDF nuevo",
      });
      if (!dest) return;
      await extractPages({
        workPath,
        pageIndices: idx,
        destPath: dest,
        deleteAfter: opts.borrar,
      });
      if (opts.borrar) {
        setPaginasSel(new Set());
        afterMutation(pageCount - idx.length);
      }
      setNotice(
        `${plural(idx.length, "página extraída", "páginas extraídas")} a ${dest}`,
      );
    } catch (e) {
      setError(String(e));
    }
  }

  /** Sustituye un rango por las páginas de otro PDF, en una sola mutación. */
  async function aplicarReemplazo(opts: {
    rango: string;
    otherPath: string;
    rangoOrigen: string;
  }) {
    if (!workPath) return;
    const idx = parseRango(opts.rango, pageCount);
    if (idx.length === 0) {
      setError(
        `Escribe qué páginas quieres sustituir, por ejemplo «1-3, 8» (el documento tiene ${pageCount})`,
      );
      return;
    }
    // el rango del origen se lee sin tope: cuántas páginas tiene el otro PDF
    // lo sabe el backend, que descarta lo que se salga
    const origen = opts.rangoOrigen.trim()
      ? parseRango(opts.rangoOrigen, 100000)
      : null;
    setReemplazarOpen(false);
    try {
      const total = await replacePages({
        workPath,
        pageIndices: idx,
        otherPath: opts.otherPath,
        otherIndices: origen,
      });
      setPaginasSel(new Set());
      afterMutation(total);
      setNotice(
        `${plural(idx.length, "página sustituida", "páginas sustituidas")} · ${MOD}Z para deshacer`,
      );
    } catch (e) {
      setError(String(e));
    }
  }

  /** Parte el documento en varios ficheros; el original no se toca. */
  async function aplicarDivision(opts: {
    modo: "cada" | "marcadores";
    cada: number;
  }) {
    if (!workPath) return;
    setDividirOpen(false);
    const dir = await open({
      directory: true,
      multiple: false,
      title: "Carpeta para los ficheros",
    });
    if (typeof dir !== "string") return;
    try {
      setNotice("Dividiendo…", { persistente: true });
      const rutas = await splitPdf({
        workPath,
        destDir: dir,
        modo: opts.modo,
        cada: opts.modo === "cada" ? opts.cada : null,
      });
      setNotice(
        `${plural(rutas.length, "fichero creado", "ficheros creados")} en ${dir}`,
      );
    } catch (e) {
      setNotice(null);
      setError(String(e));
    }
  }

  /** Añade varios PDF de una vez, en el orden de la rejilla. */
  async function aplicarCombinar(opts: { rutas: string[]; alFinal: boolean }) {
    if (!workPath) return;
    setCombinarOpen(false);
    try {
      const total = await mergeMany({
        workPath,
        others: opts.rutas,
        at: opts.alFinal ? null : pageIndex + 1,
      });
      afterMutation(total);
      setNotice(
        `${plural(opts.rutas.length, "PDF añadido", "PDF añadidos")} · ${MOD}Z para deshacer`,
      );
    } catch (e) {
      setError(String(e));
    }
  }

  /** Escribe la copia de trabajo en `dest`, cifrada con la contraseña
   *  original si `mantener`. Devuelve si se llegó a guardar. */
  async function escribirEn(dest: string, mantener: boolean): Promise<boolean> {
    if (!workPath) return false;
    try {
      if (mantener && docPassword !== null) {
        await encryptPdf({
          workPath,
          destPath: dest,
          userPassword: docPassword,
          ownerPassword: null,
        });
        setProtegido(true);
      } else {
        // con protección anotada, `save_pdf` cifra al escribir: solo aquí el
        // fichero pasa a estar protegido de verdad y el candado dice la verdad
        await invoke("save_pdf", { workPath, destPath: dest });
        if (protPendiente) {
          setProtegido(true);
          setProtPendiente(false);
        } else {
          setProtegido(false);
        }
        // a partir de aquí el fichero de `dest` va en claro: el aviso de
        // «Documento protegido» que se puso al abrirlo ya no es cierto
        if (hadPassword) setNotice(null);
        setHadPassword(false);
        setDocPassword(null);
      }
      setOriginalPath(dest);
      setNombreProvisional(null);
      setModified(false);
      // guardado: este es el punto limpio nuevo, y ya no hay nada que
      // recuperar
      marcaPuntoLimpio(workPath);
      borraSesion(workPath).catch((e) =>
        console.warn("no se ha podido borrar el apunte de sesión:", e),
      );
      setNotice(`Guardado en ${dest}`);
      return true;
    } catch (e) {
      setError(String(e));
      return false;
    }
  }

  /** Guarda en `dest`; si el original iba cifrado, pregunta la primera vez
   *  si se mantiene la contraseña y **recuerda la respuesta para ese
   *  documento**: guardar es un gesto que se repite, y repetir la misma
   *  pregunta en cada ⌘S convierte el aviso en un trámite que se contesta
   *  sin leer. Se vuelve a preguntar al abrir otro documento. */
  function guardarEn(dest: string): Promise<boolean> {
    if (!hadPassword || docPassword === null) return escribirEn(dest, false);
    if (mantenerClave !== null) return escribirEn(dest, mantenerClave);
    return new Promise((resolve) => setSaveAsk({ dest, resolve }));
  }

  function resolverSaveAsk(mantener: boolean | null) {
    if (!saveAsk) return;
    const { dest, resolve } = saveAsk;
    setSaveAsk(null);
    if (mantener === null) resolve(false);
    else {
      setMantenerClave(mantener);
      escribirEn(dest, mantener).then(resolve);
    }
  }

  async function saveFile(): Promise<boolean> {
    if (!workPath || !originalPath) return false;
    return guardarEn(originalPath);
  }

  /** ⌘S y el botón Guardar: un documento sin ruta (el combinado) pide
   *  destino en vez de escribir encima de nada. */
  function guardar(): Promise<boolean> {
    return originalPath ? saveFile() : saveFileAs();
  }

  async function saveFileAs(): Promise<boolean> {
    if (!workPath) return false;
    const dest = await save({
      filters: [{ name: "PDF", extensions: ["pdf"] }],
      defaultPath:
        originalPath ?? (nombreProvisional ? "combinado.pdf" : "documento.pdf"),
      title: "Guardar como",
    });
    if (!dest) return false;
    return guardarEn(dest);
  }

  async function pickSignedDest(): Promise<string | null> {
    return await save({
      filters: [{ name: "PDF", extensions: ["pdf"] }],
      defaultPath: (originalPath ?? "documento.pdf").replace(
        /\.pdf$/i,
        "-firmado.pdf",
      ),
      title: "Guardar PDF firmado",
    });
  }

  /** «Firmar con certificado…»: primero se dibuja el recuadro en la página,
   *  como en Acrobat, y después se pregunta todo de una vez. */
  function empezarFirma() {
    if (!workPath) return;
    setActiveSig(null);
    setCertificando(false);
    setMode("firma-cert");
    setNotice(
      "Arrastra en la página el recuadro donde quieres que se vea la firma",
    );
  }

  /** «Certificar documento…»: el mismo gesto que firmar —primero el
   *  recuadro— porque para el usuario es la misma operación con una
   *  promesa más. Con una firma ya puesta no se ofrece: el `/DocMDP` avala
   *  el documento entero y detrás de otra firma hay bytes que esta no ha
   *  visto, así que se dice el motivo antes y no se falla después. */
  function empezarCertificacion() {
    if (!workPath) return;
    if (firmasDoc.length > 0) {
      setNotice("Ya hay una firma: solo la primera puede certificar");
      return;
    }
    setActiveSig(null);
    setCertificando(true);
    setMode("firma-cert");
    setNotice(
      "Arrastra en la página el recuadro donde quieres que se vea la firma",
    );
  }

  const recibeFirmaRect = useCallback(
    (page: number, rect: { x: number; y: number; w: number; h: number }) => {
      setMode("select");
      setNotice(null);
      setFirmaRect({ page, rect });
    },
    [setNotice],
  );

  /** Firma con lo recogido en el diálogo y escribe una copia firmada. El
   *  destino se pide al final: nadie elige carpeta para descubrir después
   *  que faltaba el certificado (U-13). */
  async function aplicarFirma(d: FirmaDraft) {
    if (!workPath || !firmaRect) return;
    const dest = await pickSignedDest();
    if (!dest) return;
    const png = firmas.find((f) => f.id === d.firmaId)?.png_base64 ?? null;
    const apariencia = {
      rect: firmaRect.rect,
      pageIndex: firmaRect.page,
      signerName: d.signerName.trim() || null,
      signaturePng: png,
    };
    const esP12 = /\.(p12|pfx)$/i.test(d.certPath);
    try {
      if (certificando) {
        setNotice("Certificando…", { persistente: true });
        await certifyPdf({
          workPath,
          destPath: dest,
          nivel: d.nivel,
          certPemPath: esP12 ? null : d.certPath,
          keyPemPath: esP12 ? null : d.keyPath,
          p12Path: esP12 ? d.certPath : null,
          password: esP12 ? d.password : null,
          reason: d.reason.trim() || null,
          ...apariencia,
        });
        setFirmaRect(null);
        setCertificando(false);
        setFirmaDraft({ ...d, password: "" });
        setNotice(
          `Certificado y guardado en ${dest} · ${permisosCertificacion(d.nivel)}`,
        );
        return;
      }
      setNotice("Firmando…", { persistente: true });
      if (esP12) {
        await signPdfP12({
          workPath,
          destPath: dest,
          p12Path: d.certPath,
          password: d.password,
          reason: d.reason.trim() || null,
          ...apariencia,
        });
      } else {
        await signPdf({
          workPath,
          destPath: dest,
          certPemPath: d.certPath,
          keyPemPath: d.keyPath,
          reason: d.reason.trim() || null,
          ...apariencia,
        });
      }
      setFirmaRect(null);
      // la contraseña del certificado no se queda en memoria más de lo justo
      setFirmaDraft({ ...d, password: "" });
      setNotice(`Firmado y guardado en ${dest}`);
    } catch (e) {
      setNotice(null);
      setError(String(e));
    }
  }

  /**
   * Menú nativo de la barra del sistema: es el ESPEJO del menú «Acciones»,
   * no un segundo sitio con cosas distintas. Cada id hace exactamente lo
   * mismo que su botón; los que no aplican los deshabilita el backend, que
   * es quien pinta el menú.
   *
   * Los ids son los de `menu::estructura()` (`src-tauri/src/menu.rs`) y son
   * UNA sola lista: no hay sinónimos ni entradas que sobren, y un test de
   * Rust cruza las dos mitades. Ojo con Copiar y Seleccionar todo: sus
   * entradas llevan acelerador, así que en macOS el sistema se queda con ⌘C
   * y ⌘A antes que el webview y sin estos handlers dejarían de funcionar.
   */
  /** «Editar ▸ Copiar»: copia lo que esté seleccionado, esté donde esté,
   *  que es lo que hace Acrobat. El evento sintético de `reenviaTecla` no
   *  tiene acción por defecto, así que con el foco en un campo el navegador
   *  no copiaba nada y la entrada del menú no hacía absolutamente nada. */
  function copiarDelMenu() {
    const campo = campoConFoco();
    if (campo) {
      const texto = seleccionDeCampo(campo);
      if (texto) {
        copyToClipboard(texto);
        setNotice("Texto copiado");
      } else {
        setNotice("No hay nada seleccionado en este campo");
      }
      return;
    }
    // lo seleccionado en la propia interfaz (un aviso, una tarjeta)
    const enPantalla = window.getSelection()?.toString() ?? "";
    if (enPantalla.trim() !== "") {
      copyToClipboard(enPantalla);
      setNotice("Texto copiado");
      return;
    }
    // la selección del visor la pinta Vitela sobre las cajas de glifos de
    // PDFium y no es del DOM: la copia su propio listener
    if (!reenviaTecla("c")) {
      setNotice("Selecciona antes el texto que quieres copiar");
    }
  }

  /** «Editar ▸ Seleccionar todo»: el campo con el foco, y si no lo hay, el
   *  texto de la página que se está leyendo. */
  function seleccionarTodoDelMenu() {
    const campo = campoConFoco();
    if (campo) {
      campo.select();
      return;
    }
    if (!reenviaTecla("a")) {
      setNotice(
        pageCount === 0
          ? "Abre antes un documento"
          : "Esta página no tiene texto que seleccionar",
      );
    }
  }

  const accionesMenu: Record<string, () => void> = {
    /* Archivo */
    abrir: openFile,
    "abrir-reciente": abrirRecientes,
    guardar: () => {
      if (modified) guardar();
    },
    "guardar-como": () => {
      if (pageCount > 0) saveFileAs();
    },
    "cerrar-documento": closeDocument,
    "anadir-pdf": addPdf,
    "insertar-pdf": insertPdfHere,
    "combinar-ficheros": () => setCombinarOpen(true),
    "reemplazar-paginas": () => setReemplazarOpen(true),
    "extraer-paginas": () => setExtraerOpen(true),
    "dividir-documento": () => setDividirOpen(true),
    imprimir: printDocument,
    /* Editar */
    deshacer: () => {
      if (historial.puedeDeshacer) historial.deshacer();
    },
    rehacer: () => {
      if (historial.puedeRehacer) historial.rehacer();
    },
    buscar: () =>
      (document.querySelector(".search input") as HTMLInputElement)?.focus(),
    "buscar-siguiente": () => busqueda.gotoMatch(1),
    "buscar-anterior": () => busqueda.gotoMatch(-1),
    preferencias: () => setPrefsAbiertas(true),
    /* Ver */
    "zoom-mas": () => setZoom(nivelZoom(zoomNum, 1)),
    "zoom-menos": () =>
      setZoom(nivelZoom(zoomNum, -1)),
    "zoom-pagina": () => setZoom("pagina"),
    "zoom-100": () => setZoom(1),
    "zoom-ancho": () => setZoom("ajuste"),
    "pagina-una": () => cambiaVista({ modoPagina: "una" }),
    "pagina-continua": () => cambiaVista({ modoPagina: "continuo" }),
    "pagina-dos": () => cambiaVista({ modoPagina: "dos" }),
    "pagina-dos-continua": () => cambiaVista({ modoPagina: "dos-continuo" }),
    "girar-vista-derecha": () => setViewRotation((r) => (r + 90) % 360),
    "girar-vista-izquierda": () => setViewRotation((r) => (r + 270) % 360),
    // el conmutador de la lectura: la misma función que el botón de
    // «Acciones» y que ⇧⌘Y, para que no haya dos caminos que se separen
    "leer-en-voz-alta": () => {
      if (lectura.leyendo) lectura.parar();
      else if (pageCount > 0) lectura.leer(pageIndex, true);
    },
    "vista-atras": () => atrasVista(),
    "vista-adelante": () => adelanteVista(),
    "panel-lateral": () => setSidebarVisible((v) => !v),
    "pantalla-completa": () => cambiaPantallaCompleta(!pantallaCompleta),
    "modo-nocturno": () => aplicaPrefs({ ...prefs, nocturno: !prefs.nocturno }),
    // «Ver ▸ Mostrar u ocultar»: el andamio, donde lo pone Acrobat
    "mostrar-reglas": () => {
      if (pageCount > 0) setReglas((v) => !v);
    },
    "mostrar-guias": () => {
      if (pageCount > 0) setGuiasVisibles((v) => !v);
    },
    "mostrar-cuadricula": () => {
      if (pageCount > 0) setCuadricula((v) => !v);
    },
    "ajustar-cuadricula": () => {
      if (pageCount > 0) setAjustarCuadricula((v) => !v);
    },
    /* Documento */
    "organizar-paginas": () => abrirPestana("paginas"),
    "recortar-pagina": () => {
      selectMode("select");
      setMode("crop");
    },
    "marca-de-agua": () => setWmOpen(true),
    "encabezado-pie": () => setHfOpen(true),
    "quitar-marca-de-agua": () => askRemoveMarginal("watermark"),
    "quitar-fondo": askRemoveBackground,
    "quitar-encabezados": () => askRemoveMarginal("header"),
    "reconocer-campos": reconocerCampos,
    "adjuntar-fichero": anadirAdjunto,
    "anadir-campo": () => {
      selectMode("select");
      setMode("form-new");
    },
    "anadir-enlace": () => {
      selectMode("select");
      setMode("link-new");
    },
    firmar: empezarFirma,
    certificar: empezarCertificacion,
    proteger: () =>
      setProtectDraft({ user: "", owner: "", ...TODO_PERMITIDO }),
    "quitar-proteccion": () => {
      if (protegido || protPendiente) setQuitarProtAsk(true);
    },
    aplanar: () => setFlattenAsk(true),
    redactar: () => {
      selectMode("select");
      setMode("redact");
    },
    sanitizar: pedirSanear,
    propiedades: openProperties,
    "exportar-imagenes": () => setExportOpen(true),
    "exportar-texto": exportPlainText,
    "exportar-word": () => setWordAsk(true),
    "exportar-comentarios": exportarComentarios,
    "importar-comentarios": importarComentarios,
    "crear-desde-imagenes": () => setImagenesOpen(true),
    // Copiar y Seleccionar todo: el menú nativo se queda con ⌘C y ⌘A antes
    // que el webview, así que las dos entradas actúan donde esté mirando el
    // usuario —el campo con el foco, si lo hay— y solo si no hay ninguno
    // reenvían la tecla al visor, cuya selección no es del DOM
    copiar: copiarDelMenu,
    "seleccionar-todo": seleccionarTodoDelMenu,
    comprimir: () => setCompressOpen(true),
    /* Ayuda */
    atajos: () => setAtajosAbiertos(true),
  };
  // el listener se registra una vez y lee las acciones vivas por referencia
  const accionesMenuRef = useRef(accionesMenu);
  accionesMenuRef.current = accionesMenu;
  useEffect(
    () =>
      onMenuAccion((id) => {
        accionesMenuRef.current[id]?.();
      }),
    [],
  );

  /** Una página ha cargado sus campos: encender el resaltado y avisar. */
  const onFormularios = useCallback(
    (n: number) => {
      if (n === 0) return;
      setHayFormularios(true);
      if (!workPath || avisoFormRef.current === workPath) return;
      avisoFormRef.current = workPath;
      setNotice("Este documento se puede rellenar");
    },
    [workPath, setNotice],
  );
  const onLinkUri = useCallback((uri: string) => {
    if (!esquemaPermitido(uri)) {
      const e = esquemaDe(uri);
      setError(
        e
          ? `Enlace bloqueado: no se abren enlaces «${e.replace(/:$/, "")}»`
          : "Enlace bloqueado: la dirección no es válida",
      );
      return;
    }
    setLinkAsk(uri.trim());
  }, []);

  function openConfirmedLink() {
    if (!linkAsk) return;
    const uri = linkAsk;
    setLinkAsk(null);
    openUrl(uri).catch((e) => setError(String(e)));
  }

  // separador de ruta multiplataforma (macOS "/" y Windows "\")
  const fileName = originalPath?.split(/[\\/]/).pop() ?? nombreProvisional;

  const MODES: { id: Mode; icon: string; label: string; hint: string }[] = [
    { id: "select", icon: "select", label: "Seleccionar", hint: "Seleccionar texto" },
    { id: "draw", icon: "pen", label: "Dibujar", hint: "Dibujar a mano alzada" },
    { id: "note", icon: "note", label: "Nota", hint: "Añadir una nota (clic en la página)" },
    {
      id: "freetext",
      icon: "textbox",
      label: "Cuadro de texto",
      hint: "Comentario encima del documento (no cambia el texto del PDF)",
    },
    {
      id: "callout",
      icon: "callout",
      label: "Llamada",
      hint: "Señalar algo con una línea y escribir al lado (clic donde señala, arrastra hasta el texto)",
    },
    {
      id: "medir",
      icon: "ruler",
      label: "Medir",
      hint: "Medir distancias, perímetros y áreas sobre la página (no toca el documento)",
    },
    {
      id: "edit",
      icon: "textedit",
      label: "Editar",
      hint: "Cambiar el texto que hay en el PDF",
    },
    {
      id: "image",
      icon: "image",
      label: "Imagen",
      hint: "Insertar y editar las imágenes de la página",
    },
    {
      id: "shape",
      icon: "shapes",
      label: "Formas",
      hint: "Dibujar rectángulos, elipses, líneas y flechas (arrastra en la página)",
    },
    {
      id: "stamp",
      icon: "stamp",
      label: "Sello",
      hint: "Estampar un sello (APROBADO, BORRADOR…) con un clic",
    },
    {
      id: "adjunto",
      icon: "clip",
      label: "Adjuntar",
      hint: "Meter un fichero dentro del PDF, con su chincheta en la página (clic donde va)",
    },
    {
      id: "firmar",
      icon: "sign",
      label: "Firma",
      hint: "Estampar tu firma manuscrita: elige o crea una y haz clic (o arrastra) donde quieras colocarla",
    },
  ];

  /** «Iniciales» de la fila de firmar: arma la última guardada para
   *  estamparla con un clic, sin pasar por la biblioteca en cada página. */
  function armarIniciales() {
    const f = firmas.find((x) => iniciales.includes(x.id));
    if (!f) return;
    setMode("firmar");
    pickSignature(f);
  }

  function selectMode(m: Mode) {
    setMode((cur) => (cur === m ? "select" : m));
    setActiveSig(null);
    // entrar en el modo Sello enseña la galería, como entrar en Firma enseña
    // la biblioteca: el sello se elige viéndolo, no de memoria
    setGaleriaSellos(m === "stamp" && mode !== "stamp");
  }

  // la fila contextual va fija bajo la barra: sin contar las bandas se
  // pintaba encima de la de firmas y la cortaba a media frase
  const bandas =
    (error ? 1 : 0) +
    (bandaFirmas && firmasDoc.length > 0 ? 1 : 0) +
    (sesionesRotas.length > 0 && !sesionPlegada ? 1 : 0) +
    (sesionesRotas.length > 1 && !sesionPlegada && sesionesAbiertas
      ? sesionesRotas.length
      : 0) +
    (propuestas.length > 0 ? 1 : 0) +
    (notice ? 1 : 0);

  return (
    <div
      className={`app${pantallaCompleta ? " presentacion" : ""}${
        pantallaCompleta && pildoraVisible ? " pildora" : ""
      }${modoLectura ? " lectura" : ""}${mano.activa ? " mano" : ""}${
        mano.arrastrando ? " mano-tirando" : ""
      }`}
      style={{ "--bandas": bandas } as CSSProperties}
    >
      <header className="toolbar">
        <div className="toolbar-left">
          <button
            className="btn"
            title={`Abrir un PDF (${MOD}O)`}
            aria-label="Abrir un PDF"
            onClick={openFile}
          >
            <Icon name="open" />
            <span className="btn-etiqueta">Abrir</span>
          </button>
          {pageCount > 0 && (
            <button
              className="btn btn-icon"
              title={`${sidebarVisible ? "Ocultar" : "Mostrar"} el panel lateral (${ATAJO_PANEL})`}
              aria-label={
                sidebarVisible
                  ? "Ocultar el panel lateral"
                  : "Mostrar el panel lateral"
              }
              aria-expanded={sidebarVisible}
              onClick={() => setSidebarVisible((v) => !v)}
            >
              <Icon name="panel" size={14} />
            </button>
          )}
          {fileName && (
            <span className="filename" title={originalPath ?? undefined}>
              {fileName}
              {modified ? " •" : ""}
            </span>
          )}
          {protegido && (
            <span
              className="candado"
              title="Protegido con contraseña: se pedirá al abrirlo"
              aria-label="Documento protegido con contraseña"
            >
              <Icon name="lock" size={13} />
            </span>
          )}
          {!protegido && protPendiente && (
            <span
              className="prot-pendiente"
              title="La contraseña se aplicará al guardar; hasta entonces el fichero sigue en claro"
            >
              se protegerá al guardar
            </span>
          )}
          {sesionesRotas.length > 0 && sesionPlegada && (
            <button
              className="btn recuperar-plegado"
              title={`Tenías cambios sin guardar en ${nombresDeSesiones(
                sesionesRotas,
              )} · ${cuandoLlano(sesionesRotas[0].cuando)}`}
              onClick={() => setSesionPlegada(false)}
            >
              <Icon name="undo" size={13} />
              Recuperar…
            </button>
          )}
          {ocupado && <span className="status dato">trabajando…</span>}
        </div>

        {pageCount > 0 && (
          <div className="segmented">
            {(
              [
                ["select"],
                [
                  "draw",
                  "note",
                  "freetext",
                  "callout",
                  "shape",
                  "stamp",
                  "adjunto",
                ],
                ["medir"],
                ["edit", "image"],
                ["firmar"],
              ] as Mode[][]
            ).map((grupo, gi) => (
              <span key={gi} style={{ display: "contents" }}>
                {gi > 0 && <span className="grupo-sep" />}
                {grupo.map((id) => {
                  const m = MODES.find((x) => x.id === id)!;
                  return (
                    <button
                      key={m.id}
                      className={`btn${mode === m.id ? " on" : ""}`}
                      title={m.hint}
                      aria-pressed={mode === m.id}
                      onClick={() => selectMode(m.id)}
                    >
                      <Icon name={m.icon} size={14} />
                      <span className="btn-etiqueta">{m.label}</span>
                    </button>
                  );
                })}
              </span>
            ))}
          </div>
        )}

        <div className="toolbar-right">
          {pageCount > 0 && (
            <>
              <Busqueda
                query={busqueda.query}
                setQuery={busqueda.setQuery}
                lastQuery={busqueda.lastQuery}
                total={busqueda.matches.length}
                matchIdx={busqueda.matchIdx}
                searched={busqueda.searched}
                runSearch={busqueda.runSearch}
                limpiar={busqueda.limpiar}
                opciones={busqueda.opciones}
                cambiaOpcion={busqueda.cambiaOpcion}
                gotoMatch={busqueda.gotoMatch}
                matches={busqueda.matches}
                irAMatch={busqueda.irAMatch}
                cajonAbierto={cajonBusqueda}
                setCajonAbierto={setCajonBusqueda}
                reemplazo={reemplazo}
                carpeta={busquedaCarpeta}
                onAbrirCoincidencia={abrirCoincidenciaDeCarpeta}
              />
              <button
                className="btn btn-icon"
                title={`Deshacer (${MOD}Z)`}
                aria-label="Deshacer"
                disabled={!historial.puedeDeshacer}
                onClick={historial.deshacer}
              >
                <Icon name="undo" />
              </button>
              <button
                className="btn btn-icon"
                title={`Rehacer (⇧${MOD}Z)`}
                aria-label="Rehacer"
                disabled={!historial.puedeRehacer}
                onClick={historial.rehacer}
              >
                <Icon name="redo" />
              </button>
              <button
                className="btn btn-primary"
                title={
                  !modified
                    ? "Sin cambios que guardar"
                    : originalPath
                      ? `Guardar (${MOD}S)`
                      : `Guardar: este documento todavía no tiene fichero, se pedirá dónde (${MOD}S)`
                }
                aria-label="Guardar"
                disabled={!modified}
                onClick={guardar}
              >
                <Icon name="save" size={14} />
                <span className="btn-etiqueta">Guardar</span>
              </button>
              <MenuAcciones
                recientes={recientes}
                abrirReciente={abrirReciente}
                abierto={menuOpen}
                onToggle={() => {
                  // al desplegarlo se relee la lista: lo que pinta el menú
                  // tiene que ser lo que hay ahora en el disco
                  if (!menuOpen) refrescarRecientes();
                  setMenuOpen((o) => !o);
                }}
                onCerrar={() => setMenuOpen(false)}
                saveFileAs={saveFileAs}
                closeDocument={closeDocument}
                addPdf={addPdf}
                abrirExtraer={() => setExtraerOpen(true)}
                abrirReemplazar={() => setReemplazarOpen(true)}
                abrirDividir={() => setDividirOpen(true)}
                abrirCombinar={() => setCombinarOpen(true)}
                crearDesdeImagenes={() => setImagenesOpen(true)}
                insertPdfHere={insertPdfHere}
                recortarPagina={() => {
                  selectMode("select");
                  setMode("crop");
                }}
                abrirMarcaAgua={() => setWmOpen(true)}
                abrirEncabezado={() => setHfOpen(true)}
                askRemoveMarginal={askRemoveMarginal}
                quitarFondo={askRemoveBackground}
                openProperties={openProperties}
                signPdf={empezarFirma}
                certificar={empezarCertificacion}
                puedeCertificar={firmasDoc.length === 0}
                abrirProteger={() =>
                  setProtectDraft({
                    user: "",
                    owner: "",
                    ...TODO_PERMITIDO,
                  })
                }
                puedeQuitarProteccion={protegido || protPendiente}
                quitarProteccion={() => setQuitarProtAsk(true)}
                abrirAplanar={() => setFlattenAsk(true)}
                sanear={pedirSanear}
                redactar={() => {
                  selectMode("select");
                  setMode("redact");
                }}
                reconocerCampos={reconocerCampos}
                adjuntarFichero={anadirAdjunto}
                nuevoCampo={() => {
                  selectMode("select");
                  setMode("form-new");
                }}
                nuevoEnlace={() => {
                  selectMode("select");
                  setMode("link-new");
                }}
                printDocument={printDocument}
                abrirPreferencias={() => setPrefsAbiertas(true)}
                abrirExportar={() => setExportOpen(true)}
                exportPlainText={exportPlainText}
                exportarWord={() => setWordAsk(true)}
                exportarComentarios={exportarComentarios}
                importarComentarios={importarComentarios}
                abrirComprimir={() => setCompressOpen(true)}
                leerEnVozAlta={() => {
                  if (lectura.leyendo) lectura.parar();
                  else lectura.leer(pageIndex, true);
                }}
                leyendo={lectura.leyendo}
              />
            </>
          )}
        </div>
      </header>

      {/* la fila solo existe a partir del segundo documento: con uno, la app
          se ve exactamente igual que antes de que hubiera pestañas */}
      <Pestanas
        pestanas={pestanas.map((p) =>
          p.id === pestanaActiva
            ? {
                id: p.id,
                nombre: fileName ?? "Documento",
                ruta: originalPath,
                modificado: modified,
              }
            : {
                id: p.id,
                nombre:
                  p.originalPath?.split(/[\\/]/).pop() ??
                  p.nombreProvisional ??
                  "Documento",
                ruta: p.originalPath,
                modificado: p.modified,
              },
        )}
        activa={pestanaActiva}
        onElegir={eligePestana}
        onCerrar={cierraPestana}
      />

      {error && (
        <div className="banner-error">
          <p title={error}>{error}</p>
          <button className="btn btn-icon" aria-label="Cerrar el aviso" onClick={() => setError(null)}>
            <Icon name="close" size={13} />
          </button>
        </div>
      )}
      {bandaFirmas && firmasDoc.length > 0 && (
        <div className={`banner-firmas ${estadoBanda(firmasDoc)}`}>
          <button
            className="banner-firmas-texto"
            title="Ver las firmas del documento"
            onClick={() => {
              abrirPestana("firmas");
              setBandaFirmas(false);
            }}
          >
            <Icon name="lock" size={13} />
            {resumenFirmas(firmasDoc)}
          </button>
          <button
            className="btn btn-icon"
            aria-label="Cerrar el aviso de las firmas"
            onClick={() => setBandaFirmas(false)}
          >
            <Icon name="close" size={13} />
          </button>
        </div>
      )}
      {sesionesRotas.length > 0 && !sesionPlegada && (
        <div className="banner-recuperar">
          <p>
            Tenías cambios sin guardar en{" "}
            <span className="dato">{nombresDeSesiones(sesionesRotas)}</span>
            {" · "}
            {/* nunca se ofrece recuperar sin decir cuándo fue: volver a algo
                de hace tres semanas sin saberlo es peor que no ofrecerlo */}
            <span className="dato">{cuandoLlano(sesionesRotas[0].cuando)}</span>
          </p>
          <button
            className="btn btn-primary"
            onClick={() => recuperarSesiones(sesionesRotas)}
          >
            {sesionesRotas.length > 1 ? "Recuperar todos" : "Recuperar"}
          </button>
          <button className="btn" onClick={() => setDescartarAsk(sesionesRotas)}>
            No guardar
          </button>
          {/* con varios documentos, «todo o nada» obliga a recuperar lo que
              no se quiere para poder cerrarlo: cada uno con su fila */}
          {sesionesRotas.length > 1 && (
            <button
              className="btn"
              aria-expanded={sesionesAbiertas}
              onClick={() => setSesionesAbiertas((v) => !v)}
            >
              {sesionesAbiertas ? "Ocultar la lista" : "Elegir uno a uno"}
            </button>
          )}
        </div>
      )}
      {sesionesRotas.length > 1 && !sesionPlegada && sesionesAbiertas && (
        <div className="banner-recuperar recuperar-lista">
          {sesionesRotas.map((s) => (
            <div className="recuperar-fila" key={s.work_path}>
              <span className="recuperar-nombre" title={s.original_path ?? ""}>
                {s.original_path?.split(/[\\/]/).pop() ??
                  "un documento sin fichero"}
              </span>
              <span className="dato">{cuandoLlano(s.cuando)}</span>
              <button
                className="btn"
                onClick={() => recuperarSesiones([s])}
              >
                Recuperar
              </button>
              <button className="btn" onClick={() => setDescartarAsk([s])}>
                Descartar
              </button>
            </div>
          ))}
        </div>
      )}
      {propuestas.length > 0 && (
        <div className="banner-campos">
          <p>
            {plural(
              propuestas.length,
              "campo encontrado",
              "campos encontrados",
            )}
            {" · "}
            <span className="dato">
              nada se escribe hasta que pulses «Crear todos»
            </span>
          </p>
          <button className="btn" onClick={revisarPropuesta}>
            {propuestaActual === null
              ? "Revisar uno a uno"
              : `Siguiente (${propuestaActual + 1} de ${propuestas.length})`}
          </button>
          <button className="btn btn-primary" onClick={crearPropuestas}>
            Crear todos
          </button>
          <button
            className="btn"
            onClick={() => {
              setPropuestas([]);
              setPropuestaActual(null);
            }}
          >
            Cancelar
          </button>
        </div>
      )}
      {notice && (
        <div className={`banner-notice${noticeSaliendo ? " saliendo" : ""}`}>
          <p title={notice}>{notice}</p>
          {noticeDato && <span className="dato notice-dato">{noticeDato}</span>}
          {noticeAccion && (
            <button className="btn" onClick={noticeAccion.onClick}>
              {noticeAccion.texto}
            </button>
          )}
          <button className="btn btn-icon" aria-label="Cerrar el aviso" onClick={() => setNotice(null)}>
            <Icon name="close" size={13} />
          </button>
        </div>
      )}

      {firmaRect && (
        <DialogoFirmar
          inicial={firmaDraft}
          certificar={certificando}
          pagina={firmaRect.page + 1}
          rect={firmaRect.rect}
          firmas={firmas}
          firmasPrevias={firmasDoc}
          onConfirm={aplicarFirma}
          onClose={() => {
            setFirmaRect(null);
            setCertificando(false);
          }}
        />
      )}

      {mode === "stamp" && galeriaSellos && (
        <PanelSellos
          color={herramienta.stampColor}
          ultimo={herramienta.ultimoSello}
          mios={sellos}
          onElegirTexto={(texto, dinamico) => {
            setActiveSig(null);
            herramienta.eligeSello(texto, dinamico);
            setGaleriaSellos(false);
          }}
          onElegirImagen={(f) => {
            pickSignature(f);
            setGaleriaSellos(false);
          }}
          onSubirImagen={() => uploadSignature("sello")}
          onBorrarImagen={removeSignature}
          onClose={() => setGaleriaSellos(false)}
        />
      )}

      {/* con una marca armada el usuario está rellenando, no eligiendo firma:
          la biblioteca taparía la página */}
      {mode === "firmar" && !activeSig && !drawingSig && !herramienta.fillMark && (
        <PanelFirmas
          firmas={firmas}
          iniciales={iniciales}
          onPick={pickSignature}
          onUpload={uploadSignature}
          onDraw={(ranura) => setDrawingSig(ranura)}
          onDelete={removeSignature}
          onClose={() => selectMode("select")}
        />
      )}
      {drawingSig && (
        <DibujarFirma
          ranura={drawingSig === "iniciales" ? "iniciales" : "firma"}
          onSave={(nombre, png) =>
            saveDrawnSignature(
              nombre,
              png,
              drawingSig === "iniciales" ? "iniciales" : "firma",
            )
          }
          onClose={() => setDrawingSig(false)}
        />
      )}
      {wmOpen && (
        <DialogoMarcaAgua
          pageCount={pageCount}
          paginaActual={pageIndex}
          previaSrc={thumbs[pageIndex] ?? null}
          previaSize={pageSizes[pageIndex]}
          onApply={applyWatermark}
          onClose={() => setWmOpen(false)}
        />
      )}
      {hfOpen && (
        <DialogoEncabezado
          pageCount={pageCount}
          paginaActual={pageIndex}
          previaSrc={thumbs[pageIndex] ?? null}
          previaSize={pageSizes[pageIndex]}
          onApply={applyHeaderFooter}
          onClose={() => setHfOpen(false)}
        />
      )}
      {prefsAbiertas && (
        <DialogoPreferencias
          prefs={prefs}
          onCambio={aplicaPrefs}
          onClose={() => setPrefsAbiertas(false)}
        />
      )}
      {atajosAbiertos && (
        <DialogoAtajos onClose={() => setAtajosAbiertos(false)} />
      )}
      {propsDraft && (
        <DialogoPropiedades
          initial={propsDraft}
          ficha={propsFicha}
          vista={propsVista}
          pageCount={pageCount}
          onSave={saveProperties}
          onClose={() => setPropsDraft(null)}
        />
      )}
      {pwdDraft && (
        <DialogoContrasena
          titulo="Documento protegido"
          fichero={pwdDraft.path}
          valor={pwdDraft.password}
          onChange={(v) => setPwdDraft({ ...pwdDraft, password: v })}
          onConfirm={() => openPath(pwdDraft.path, pwdDraft.password)}
          onClose={() => setPwdDraft(null)}
          etiqueta="Abrir"
          placeholder="Contraseña del documento"
        />
      )}
      {protectDraft && (
        <DialogoProteger
          valor={protectDraft}
          onChange={setProtectDraft}
          onConfirm={applyProtect}
          onCopia={applyProtectCopy}
          onClose={() => setProtectDraft(null)}
        />
      )}
      {quitarProtAsk && (
        <DialogoConfirmar
          titulo={
            protPendiente && !protegido
              ? "No proteger al guardar"
              : "Quitar la contraseña"
          }
          cuerpo={
            <p className="modal-file" style={{ whiteSpace: "normal" }}>
              {protPendiente && !protegido
                ? "Se olvidará la contraseña que ibas a poner: al guardar, el fichero se escribirá en claro."
                : "El documento dejará de estar cifrado en cuanto lo guardes: cualquiera podrá abrir el fichero y hacer con él lo que quiera."}
            </p>
          }
          textoConfirmar="Quitar la contraseña"
          peligro
          onConfirm={applyQuitarProteccion}
          onClose={() => setQuitarProtAsk(false)}
        />
      )}
      {fondoAsk !== null && (
        <DialogoConfirmar
          titulo="Quitar el fondo"
          cuerpo={
            <p className="modal-file" style={{ whiteSpace: "normal" }}>
              Se quitarán {plural(fondoAsk, "elemento", "elementos")} puestos
              como fondo o marca de agua —el color, la imagen y el texto— en
              todo el documento. El contenido de las páginas no se toca.
            </p>
          }
          textoConfirmar="Quitar"
          peligro
          onConfirm={applyRemoveBackground}
          onClose={() => setFondoAsk(null)}
        />
      )}
      {marginalAsk && (
        <DialogoConfirmar
          titulo={
            marginalAsk.zona === "watermark"
              ? "Quitar la marca de agua"
              : "Quitar encabezados y pies"
          }
          cuerpo={
            <p className="modal-file" style={{ whiteSpace: "normal" }}>
              Se eliminarán {plural(marginalAsk.textos, "texto", "textos")}
              {marginalAsk.zona === "watermark"
                ? " de marca de agua"
                : " de los márgenes superior e inferior"}{" "}
              en todo el documento.
            </p>
          }
          textoConfirmar="Quitar"
          peligro
          onConfirm={applyRemoveMarginal}
          onClose={() => setMarginalAsk(null)}
        />
      )}
      {descartarAsk && (
        <DialogoConfirmar
          titulo="No guardar los cambios"
          cuerpo={
            <p className="modal-file" style={{ whiteSpace: "normal" }}>
              {descartarAsk.length === 1
                ? `Se borrará la copia con los cambios sin guardar de ${
                    descartarAsk[0].original_path ?? "un documento sin fichero"
                  }.`
                : `Se borrarán las copias con los cambios sin guardar de los ${descartarAsk.length} documentos: ${nombresDeSesiones(descartarAsk)}.`}{" "}
              Es lo único que queda de ese trabajo y no se podrá recuperar.
            </p>
          }
          textoConfirmar="No guardar"
          peligro
          onConfirm={() => {
            const lista = descartarAsk;
            setDescartarAsk(null);
            descartarSesiones(lista);
          }}
          onClose={() => setDescartarAsk(null)}
        />
      )}
      {etiquetasOpen && (
        <DialogoEtiquetas
          pageCount={pageCount}
          pageIndex={pageIndex}
          rangos={etiquetas}
          onConfirm={aplicarEtiquetas}
          onQuitar={() => aplicarEtiquetas([])}
          onClose={() => setEtiquetasOpen(false)}
        />
      )}
      {insertarAsk && (
        <DialogoInsertar
          nombre={insertarAsk.path.split(/[\\/]/).pop() ?? insertarAsk.path}
          paginasQueEntran={insertarAsk.paginas}
          pageCount={pageCount}
          paginaActual={pageIndex}
          onConfirm={aplicarInsertar}
          onClose={() => setInsertarAsk(null)}
        />
      )}
      {batesOpen && (
        <DialogoBates
          pageCount={pageCount}
          onConfirm={aplicarBates}
          onClose={() => setBatesOpen(false)}
        />
      )}
      {wordAsk && (
        <DialogoWord
          pageCount={pageCount}
          onConfirm={exportarWord}
          onClose={() => setWordAsk(false)}
        />
      )}
      {adjuntoAsk && (
        <DialogoConfirmar
          titulo="Quitar el adjunto"
          cuerpo={
            <p className="modal-file" style={{ whiteSpace: "normal" }}>
              {adjuntoAsk.adjunto.name} dejará de ir dentro del documento.
              {MOD}Z lo devuelve mientras el documento siga abierto.
            </p>
          }
          textoConfirmar="Quitar el adjunto"
          peligro
          onConfirm={() =>
            borrarAdjunto(adjuntoAsk.index, adjuntoAsk.adjunto)
          }
          onClose={() => setAdjuntoAsk(null)}
        />
      )}
      {redactAsk && (
        <DialogoConfirmar
          titulo="Aplicar la redacción"
          cuerpo={
            <p className="modal-file" style={{ whiteSpace: "normal" }}>
              En {plural(marcasRedact.length, "zona marcada", "zonas marcadas")}{" "}
              se eliminarán {resumenRedaccion(redactAsk)}, y quedará una caja
              negra encima. El contenido se elimina y no se podrá recuperar
              guardando; {MOD}Z lo devuelve mientras el documento siga abierto.
            </p>
          }
          textoConfirmar="Aplicar la redacción"
          peligro
          onConfirm={aplicarRedaccion}
          onClose={() => setRedactAsk(null)}
        />
      )}
      {sanitizeAsk && (
        <DialogoConfirmar
          titulo="Quitar la información oculta"
          cuerpo={
            <p className="modal-file" style={{ whiteSpace: "normal" }}>
              {resumenSaneado(sanitizeAsk)}
            </p>
          }
          textoConfirmar="Quitar"
          peligro
          onConfirm={aplicarSanear}
          onClose={() => setSanitizeAsk(null)}
        />
      )}
      {flattenAsk && (
        <DialogoConfirmar
          titulo="Fijar las anotaciones en la página"
          cuerpo={
            <p className="modal-file" style={{ whiteSpace: "normal" }}>
              Los resaltados, subrayados, notas, sellos, formas, trazos y
              campos rellenados pasan a ser contenido fijo de la página: ya no
              se podrán editar ni borrar.
            </p>
          }
          textoConfirmar="Fijar"
          onConfirm={applyFlatten}
          onClose={() => setFlattenAsk(false)}
        />
      )}
      {cerrarAsk && (
        <DialogoConfirmar
          titulo="Guardar los cambios"
          cuerpo={
            <p className="modal-file" style={{ whiteSpace: "normal" }}>
              ¿Quieres guardar los cambios en {fileName ?? "el documento"}{" "}
              antes de salir? Si no los guardas se pierden.
              {sucios() > 1 &&
                ` Hay ${sucios()} documentos abiertos con cambios sin guardar; se pregunta por cada uno.`}
            </p>
          }
          textoConfirmar="Guardar y salir"
          secundario={{
            texto: "Salir sin guardar",
            onClick: () => {
              setCerrarAsk(false);
              confirmarCierre();
            },
          }}
          onConfirm={guardarYSalir}
          onClose={() => setCerrarAsk(false)}
        />
      )}
      {unsavedAsk && (
        <DialogoConfirmar
          titulo="Cambios sin guardar"
          cuerpo={
            <p className="modal-file" style={{ whiteSpace: "normal" }}>
              {fileName ?? "El documento"} tiene cambios sin guardar. ¿Quieres
              guardarlos antes de continuar?
            </p>
          }
          textoConfirmar="Guardar"
          secundario={{
            texto: "No guardar",
            onClick: () => resolverUnsavedAsk("descartar"),
          }}
          onConfirm={() => resolverUnsavedAsk("guardar")}
          onClose={() => resolverUnsavedAsk("cancelar")}
        />
      )}
      {saveAsk && (
        <DialogoConfirmar
          titulo="Guardar un documento protegido"
          cuerpo={
            <p className="modal-file" style={{ whiteSpace: "normal" }}>
              Este documento se abrió con contraseña. Puedes guardarlo
              manteniéndola o quitarla y dejarlo en claro.
            </p>
          }
          textoConfirmar="Mantener la contraseña"
          secundario={{
            texto: "Guardar sin contraseña",
            onClick: () => resolverSaveAsk(false),
          }}
          onConfirm={() => resolverSaveAsk(true)}
          onClose={() => resolverSaveAsk(null)}
        />
      )}
      {dropAsk && (
        <DialogoConfirmar
          titulo={`Has elegido ${dropAsk.length} PDF`}
          cuerpo={
            <p className="modal-file" style={{ whiteSpace: "normal" }}>
              Puedes abrirlos todos, cada uno en su pestaña, o unirlos en un
              documento nuevo, en el orden en que los has elegido.
            </p>
          }
          textoConfirmar="Unirlos en uno"
          secundario={{
            texto: "Abrirlos todos",
            onClick: () => abrirTodosEnPestanas(dropAsk),
          }}
          onConfirm={() => abrirYUnir(dropAsk)}
          onClose={() => setDropAsk(null)}
        />
      )}
      {linkAsk && (
        <DialogoConfirmar
          titulo="Abrir enlace externo"
          cuerpo={
            <>
              <p className="modal-file">{destinoDe(linkAsk)}</p>
              <p className="modal-file" style={{ whiteSpace: "normal" }}>
                {linkAsk}
              </p>
            </>
          }
          textoConfirmar="Abrir"
          onConfirm={openConfirmedLink}
          onClose={() => setLinkAsk(null)}
        />
      )}
      {mode === "freetext" && (
        <div className="sign-hint">
          Comentario encima del documento (no cambia el texto del PDF) ·
          arrastra el rectángulo y escribe dentro · {MOD}Enter lo añade · Esc
          cancela
        </div>
      )}
      {mode === "note" && (
        <div className="sign-hint">
          Haz clic donde quieras la nota · Enter salta de línea, {MOD}Enter la
          guarda · Esc cancela
        </div>
      )}
      {mode === "firma-cert" && (
        <div className="sign-hint">
          Arrastra el recuadro donde quieras que se vea la firma · Esc cancela
        </div>
      )}
      {mode === "redact" && (
        <div className="sign-hint">
          Arrastra sobre las zonas a censurar: quedan marcadas en rojo y no se
          borra nada hasta que pulses «Aplicar redacción» · Esc sale
        </div>
      )}
      {mode === "form-new" && (
        <div className="sign-hint">
          Arrastra donde quieras el campo de formulario · Esc cancela
        </div>
      )}
      {mode === "link-new" && (
        <div className="sign-hint">
          Arrastra sobre la zona que será clicable · Esc cancela
        </div>
      )}
      {extraerOpen && (
        <DialogoExtraer
          inicial={
            paginasSel.size > 0
              ? formateaRango([...paginasSel])
              : String(pageIndex + 1)
          }
          pageCount={pageCount}
          onConfirm={aplicarExtraer}
          onClose={() => setExtraerOpen(false)}
        />
      )}
      {reemplazarOpen && (
        <DialogoReemplazar
          inicial={
            paginasSel.size > 0
              ? formateaRango([...paginasSel])
              : String(pageIndex + 1)
          }
          pageCount={pageCount}
          onConfirm={aplicarReemplazo}
          onClose={() => setReemplazarOpen(false)}
        />
      )}
      {dividirOpen && (
        <DialogoDividir
          pageCount={pageCount}
          hayMarcadores={outline.length > 0}
          onConfirm={aplicarDivision}
          onClose={() => setDividirOpen(false)}
        />
      )}
      {combinarOpen && (
        <DialogoCombinar
          paginaActual={pageIndex}
          onConfirm={aplicarCombinar}
          onClose={() => setCombinarOpen(false)}
        />
      )}
      {comentariosAsk && (
        <DialogoComentarios
          cuantos={comentarios.length}
          onConfirm={aplicarExportarComentarios}
          onClose={() => setComentariosAsk(false)}
        />
      )}
      {imagenesOpen && (
        <DialogoImagenes
          fallos={imagenesFallos}
          onConfirm={crearDesdeImagenes}
          onClose={() => {
            setImagenesOpen(false);
            setImagenesFallos([]);
          }}
        />
      )}
      {exportOpen && (
        <DialogoExportar
          fmt={exportFmt}
          setFmt={setExportFmt}
          dpi={exportDpi}
          setDpi={setExportDpi}
          onConfirm={exportImages}
          onClose={() => setExportOpen(false)}
        />
      )}
      {compressOpen && (
        <DialogoComprimir
          quality={compressQuality}
          setQuality={setCompressQuality}
          dpi={compressDpi}
          setDpi={setCompressDpi}
          onConfirm={applyCompress}
          onClose={() => setCompressOpen(false)}
        />
      )}
      {printOpen && (
        <DialogoImprimir
          inicial={printOpts}
          pageCount={pageCount}
          paginaActual={pageIndex}
          onConfirm={prepararImpresion}
          onClose={() => setPrintOpen(false)}
        />
      )}
      {/* Fuera de `.app` a propósito: el `@media print` esconde `.app`
          entera, y un ancestro en `display:none` saca todo su subárbol de la
          caja de renderizado —ningún `display:block` del descendiente lo
          rescata—. Dentro, la hoja salía en blanco. */}
      {printPages &&
        createPortal(
          <div className="print-pages">
            {printPages.map((p, i) => (
              <img
                key={i}
                src={p.src}
                alt={`Página ${i + 1}`}
                style={p.anchoIn ? { width: `${p.anchoIn}in` } : undefined}
              />
            ))}
          </div>,
          document.body,
        )}
      {mode === "crop" && (
        <div className="sign-hint">
          Arrastra para marcar el área que quieres conservar · Esc cancela
        </div>
      )}
      {mode === "firmar" && !activeSig && herramienta.fillMark && (
        <div className="sign-hint">
          Haz clic donde quieras la marca · se mueve y se borra después como
          cualquier comentario · Esc sale
        </div>
      )}
      {mode === "firmar" && activeSig && (
        <div className="sign-hint">
          Haz clic donde quieras la firma (o arrastra para elegir el tamaño) ·
          Esc cancela
        </div>
      )}
      <OpcionesHerramienta
        mode={mode}
        drawColor={herramienta.drawColor}
        drawWidth={herramienta.drawWidth}
        setDrawWidth={herramienta.setDrawWidth}
        goma={herramienta.goma}
        setGoma={herramienta.setGoma}
        gomaAncho={herramienta.gomaAncho}
        setGomaAncho={herramienta.setGomaAncho}
        leyendo={lectura.leyendo}
        pausada={lectura.pausada}
        paginaLeida={lectura.paginaLeida}
        onPausarLectura={lectura.pausar}
        onPararLectura={lectura.parar}
        hayIniciales={iniciales.length > 0}
        onIniciales={armarIniciales}
        medidaTipo={herramienta.medidaTipo}
        setMedidaTipo={herramienta.setMedidaTipo}
        medidaDejar={herramienta.medidaDejar}
        setMedidaDejar={herramienta.setMedidaDejar}
        calibrando={herramienta.calibrando}
        setCalibrando={herramienta.setCalibrando}
        escalaMm={escalaMm}
        shapeKind={herramienta.shapeKind}
        setShapeKind={herramienta.setShapeKind}
        shapeColor={herramienta.shapeColor}
        shapeFill={herramienta.shapeFill}
        setShapeFill={herramienta.setShapeFill}
        shapeWidth={herramienta.shapeWidth}
        setShapeWidth={herramienta.setShapeWidth}
        stampText={herramienta.stampText}
        setStampText={herramienta.setStampText}
        stampDinamico={herramienta.stampDinamico}
        abrirSellos={() => setGaleriaSellos(true)}
        stampColor={herramienta.stampColor}
        hayFormularios={hayFormularios}
        onExportarDatos={exportarDatosFormulario}
        onImportarDatos={importarDatosFormulario}
        resaltarCampos={resaltarCampos}
        setResaltarCampos={(v) => {
          guardaResaltarCampos(v);
          setResaltarCampos(v);
        }}
        freeTextColor={herramienta.freeTextColor}
        freeTextSize={herramienta.freeTextSize}
        setFreeTextSize={herramienta.setFreeTextSize}
        freeTextBorder={herramienta.freeTextBorder}
        setFreeTextBorder={herramienta.setFreeTextBorder}
        textColor={herramienta.textColor}
        textColorBloque={herramienta.textColorBloque}
        textAlign={herramienta.textAlign}
        textLineHeight={herramienta.textLineHeight}
        setTextLineHeight={herramienta.setTextLineHeight}
        textCharSpacing={herramienta.textCharSpacing}
        setTextCharSpacing={herramienta.setTextCharSpacing}
        setTextAlign={herramienta.setTextAlign}
        fillMark={herramienta.fillMark}
        setFillMark={herramienta.setFillMark}
        fillColor={herramienta.fillColor}
        cambiaColorAccion={herramienta.cambiaColorAccion}
        anadirTexto={() => setPedirTextoNuevo((n) => n + 1)}
        insertarImagen={() => setPedirImagen((n) => n + 1)}
        escribirEncima={() => setMode("freetext")}
        marcasRedact={marcasRedact.length}
        aplicarRedaccion={pedirAplicarRedaccion}
        quitarMarcasRedact={quitarTodasLasMarcas}
      />

      <div className="body">
        {pageCount > 0 && sidebarVisible && (
          <aside className="sidebar">
            <div className="sidebar-tabs">
              <button
                className={`btn${sidebarTab === "paginas" ? " on" : ""}`}
                title="Páginas"
                aria-pressed={sidebarTab === "paginas"}
                onClick={() => setSidebarTab("paginas")}
              >
                Páginas
              </button>
              <button
                className={`btn${sidebarTab === "marcadores" ? " on" : ""}`}
                title={`Marcadores (${ATAJO_MARCADORES})`}
                aria-pressed={sidebarTab === "marcadores"}
                onClick={() => abrirPestana("marcadores")}
              >
                Marcadores
              </button>
              <button
                className={`btn${sidebarTab === "comentarios" ? " on" : ""}`}
                title={`Comentarios (${ATAJO_COMENTARIOS})`}
                aria-pressed={sidebarTab === "comentarios"}
                onClick={() => abrirPestana("comentarios")}
              >
                Comentarios
              </button>
              {firmasDoc.length > 0 && (
                <button
                  className={`btn${sidebarTab === "firmas" ? " on" : ""}`}
                  title="Firmas del documento"
                  aria-pressed={sidebarTab === "firmas"}
                  onClick={() => abrirPestana("firmas")}
                >
                  Firmas
                </button>
              )}
              {adjuntos.length > 0 && (
                <button
                  className={`btn${sidebarTab === "adjuntos" ? " on" : ""}`}
                  title="Ficheros que lleva dentro el documento"
                  aria-pressed={sidebarTab === "adjuntos"}
                  onClick={() => abrirPestana("adjuntos")}
                >
                  Adjuntos
                </button>
              )}
              {capas.length > 0 && (
                <button
                  className={`btn${sidebarTab === "capas" ? " on" : ""}`}
                  title="Capas del documento"
                  aria-pressed={sidebarTab === "capas"}
                  onClick={() => abrirPestana("capas")}
                >
                  Capas
                </button>
              )}
            </div>
            {/* cada panel dentro de su red: un fallo leyendo los
                comentarios no puede llevarse el documento por delante */}
            <LimiteError que="este panel" onRecargar={recargarDocumento}>
            {sidebarTab === "firmas" && (
              <PanelFirmasDoc firmas={firmasDoc} onGoto={saltarA} />
            )}
            {sidebarTab === "adjuntos" && (
              <PanelAdjuntos
                adjuntos={adjuntos}
                onAbrir={abrirAdjunto}
                onGuardar={guardarAdjunto}
                onBorrar={(index, adjunto) => setAdjuntoAsk({ index, adjunto })}
                onAnadir={anadirAdjunto}
              />
            )}
            {sidebarTab === "capas" && (
              <PanelCapas capas={capas} onToggle={cambiarCapa} />
            )}
            {sidebarTab === "comentarios" && (
              <PanelComentarios
                comentarios={comentarios}
                filtro={filtroComentarios}
                setFiltro={setFiltroComentarios}
                filtroAutor={filtroAutor}
                setFiltroAutor={setFiltroAutor}
                filtroEstado={filtroEstado}
                setFiltroEstado={setFiltroEstado}
                seleccionada={annotSel}
                focoPedido={focoComentarios}
                onSelect={irAComentario}
                onDelete={borrarComentario}
                onReply={responderComentario}
                onState={estadoComentario}
              />
            )}
            {sidebarTab === "marcadores" && (
              <PanelMarcadores
                outline={outline}
                onGoto={seguirMarcador}
                onChange={persistOutline}
                onAnadir={crearMarcador}
              />
            )}
            {sidebarTab === "paginas" && (
              <PanelPaginas
                thumbs={thumbs}
                pageIndex={pageIndex}
                pageCount={pageCount}
                seleccion={paginasSel}
                setSeleccion={setPaginasSel}
                girarLote={girarPaginas}
                eliminarLote={borrarPaginas}
                extraerLote={() => setExtraerOpen(true)}
                // el clic en una miniatura es un salto largo como cualquier
                // otro: Acrobat lo apila en «Vista anterior» y aquí también
                gotoPage={saltarA}
                movePage={movePage}
                rotatePage={rotatePage}
                duplicatePageAt={duplicatePageAt}
                blankPageAfter={blankPageAfter}
                deletePage={deletePage}
                nombreDePagina={nombreDePagina}
                onNumerar={() => setEtiquetasOpen(true)}
                onBates={() => setBatesOpen(true)}
              />
            )}
            </LimiteError>
          </aside>
        )}

        <div className="viewer-wrap">
          <main
            className={`viewer${arrastrando ? " arrastrando" : ""}${
              prefs.nocturno ? " nocturno" : ""
            }`}
            ref={viewerRef}
            onScroll={onViewerScroll}
            onMouseDownCapture={mano.activa ? mano.empieza : undefined}
            onClick={
              // en presentación el clic avanza, como en Acrobat
              pantallaCompleta ? () => gotoPage(paginaVecina(1)) : undefined
            }
          >
            {!workPath && (
              <div className="placeholder">
                <p className="voz">Nada abierto todavía. El papel espera.</p>
                <button className="btn btn-primary" onClick={openFile}>
                  <Icon name="open" size={14} />
                  Abrir PDF
                </button>
                <p className="placeholder-pista">
                  Arrastra un PDF aquí o pulsa Abrir
                </p>
                {/* sin documento es cuando se quiere hacer uno: la puerta
                    de entrada de Acrobat está justo aquí */}
                <button
                  className="btn placeholder-otra"
                  onClick={() => setImagenesOpen(true)}
                >
                  <Icon name="image" size={14} />
                  Crear PDF desde imágenes…
                </button>
                {recientes.length > 0 && (
                  <div className="recientes">
                    <span className="card-label">Recientes</span>
                    {recientes.map((r) => {
                      const ficha = fichasRecientes[r.path];
                      return (
                      <div
                        key={r.path}
                        className={`reciente${r.exists ? "" : " no-esta"}`}
                      >
                        <button
                          className="reciente-abrir"
                          title={
                            !r.exists
                              ? `Ya no está en ${r.path}`
                              : ficha?.encrypted
                                ? `${r.path} · protegido: pedirá la contraseña`
                                : r.path
                          }
                          disabled={!r.exists}
                          onClick={() => abrirReciente(r.path)}
                        >
                          <span className="reciente-nombre">
                            {ficha?.encrypted && (
                              <Icon name="lock" size={11} />
                            )}
                            {r.name}
                          </span>
                          <span className="reciente-dir">{r.dir}</span>
                          {ficha && (
                            <span className="reciente-ficha dato">
                              {plural(ficha.page_count, "página", "páginas")} ·{" "}
                              {tamanoFichero(ficha.bytes)}
                            </span>
                          )}
                        </button>
                        {!r.exists && (
                          <button
                            className="btn"
                            onClick={() => quitarReciente(r.path)}
                          >
                            Quitar
                          </button>
                        )}
                      </div>
                      );
                    })}
                  </div>
                )}
              </div>
            )}
            {/* red de seguridad: un fallo pintando una página no puede
                dejar la ventana en blanco y sin salida */}
            {workPath && (
              <LimiteError que="este documento" onRecargar={recargarDocumento}>
                {filasVisibles.map((fila) => (
              <div
                className={`fila-paginas${fila.length > 1 ? " doble" : ""}`}
                key={fila[0]}
              >
                {fila
                  .filter((i) => pageSizes[i])
                  .map((i) => (
                <Pagina
                  key={i}
                  index={i}
                  workPath={workPath}
                  size={pageSizes[i]}
                  pageCount={pageCount}
                  displayWidth={displayWidth}
                  viewRotation={viewRotation}
                  devicePixelRatio={window.devicePixelRatio}
                  docVersion={docVersion}
                  annotVersion={annotVersion}
                  pageVersion={pageVersions[i] ?? 0}
                  mode={mode}
                  tool={tool}
                  matches={busqueda.matchesByPage.get(i)}
                  currentGroup={busqueda.matchIdx}
                  esActual={i === pageIndex}
                  selOwner={selOwner}
                  seleccionExterna={
                    annotSel?.page === i ? annotSel.index : null
                  }
                  claimSel={setSelOwner}
                  onSeleccion={recibeSeleccion}
                  requestRender={requestRender}
                  registerEl={registerEl}
                  onAnnotated={afterAnnotate}
                  onPageMutated={afterPageMutation}
                  onDocMutated={afterMutation}
                  onError={mostrarError}
                  onNotice={mostrarAviso}
                  onFormularios={onFormularios}
                  resaltarCampos={resaltarCampos}
                  onModeChange={setMode}
                  onLinkGoto={saltarA}
                  onLinkUri={onLinkUri}
                  onSigStamped={onSigStamped}
                  onFirmaRect={recibeFirmaRect}
                  marcas={marcasRedact
                    .filter((m) => m.page_index === i)
                    .map((m) => ({ annotIndex: m.annot_index, rect: m.rect }))}
                  propuestas={propuestas
                    .map((campo, j) => ({ i: j, campo }))
                    .filter((p) => p.campo.page_index === i)}
                  propuestaActual={
                    propuestas[propuestaActual ?? -1]?.page_index === i
                      ? propuestaActual
                      : null
                  }
                  onPropuestaQuitar={quitarPropuesta}
                  onPropuestaRenombrar={renombraPropuesta}
                  onPropuestaTipo={cambiaTipoPropuesta}
                  reglas={reglas}
                  cuadricula={cuadricula}
                  ajustarCuadricula={ajustarCuadricula}
                  guiasVisibles={guiasVisibles}
                  guias={guiasDePagina(guias, i)}
                  escalaMm={escalaMm}
                  onGuia={ponGuia}
                  onQuitarGuia={quitaGuia}
                  quitarMarca={quitarMarca}
                  onMarcasCambian={refrescarMarcas}
                  pedirTextoNuevo={pedirTextoNuevo}
                  pedirImagen={pedirImagen}
                />
                  ))}
              </div>
                ))}
              </LimiteError>
            )}
          </main>

          {pageCount > 0 && (
            <div className="nav-pill">
              <button
                className="btn btn-icon"
                title={dobles ? "Pliego anterior" : "Página anterior"}
                aria-label={dobles ? "Pliego anterior" : "Página anterior"}
                disabled={!hayAnterior}
                onClick={() => gotoPage(paginaVecina(-1))}
              >
                <Icon name="chevLeft" size={14} />
              </button>
              {pageDraft === null ? (
                <button
                  className="btn pill-boton"
                  title={`Ir a la página (⇧${MOD}N)`}
                  aria-label={`Ir a la página · ahora ${pagineoLlano(
                    etiquetas,
                    paginaMostrada,
                  )} de ${pageCount}`}
                  onClick={() => setPageDraft(String(paginaMostrada + 1))}
                >
                  {/* con `/PageLabels` manda el nombre de la página y el
                      número físico va detrás entre paréntesis, como Acrobat */}
                  {pagineoLlano(etiquetas, paginaMostrada)} / {pageCount}
                </button>
              ) : (
                <input
                  className="pill-input"
                  inputMode="numeric"
                  autoFocus
                  // se admite la etiqueta («xii») además del número físico:
                  // la píldora enseña la etiqueta a un centímetro del campo
                  // y no aceptarla era la interfaz contradiciéndose
                  aria-label={`Ir a la página (1 a ${pageCount}, por su número o por su etiqueta)`}
                  title="Escribe el número de página o su etiqueta («xii»)"
                  value={pageDraft}
                  onFocus={(e) => e.currentTarget.select()}
                  onChange={(e) => setPageDraft(e.target.value)}
                  onBlur={() => setPageDraft(null)}
                  onKeyDown={(e) => {
                    if (e.key === "Enter") irAPaginaEscrita();
                    else if (e.key === "Escape") setPageDraft(null);
                  }}
                />
              )}
              <button
                className="btn btn-icon"
                title={dobles ? "Pliego siguiente" : "Página siguiente"}
                aria-label={dobles ? "Pliego siguiente" : "Página siguiente"}
                disabled={!haySiguiente}
                onClick={() => gotoPage(paginaVecina(1))}
              >
                <Icon name="chevRight" size={14} />
              </button>
              <div className="sep" />
              <button
                className="btn btn-icon"
                title={`Reducir (${MOD}−)`}
                aria-label="Reducir"
                onClick={() =>
                  setZoom(nivelZoom(zoomNum, -1))
                }
              >
                <Icon name="minus" size={14} />
              </button>
              {zoomDraft === null ? (
                <button
                  className="btn pill-boton"
                  title="Escribe un porcentaje de zoom"
                  aria-label="Porcentaje de zoom"
                  onClick={() =>
                    setZoomDraft(String(Math.round(zoomNum * 100)))
                  }
                >
                  {Math.round(zoomNum * 100)}%
                </button>
              ) : (
                <input
                  className="pill-input"
                  inputMode="numeric"
                  autoFocus
                  aria-label="Porcentaje de zoom (8 a 6400)"
                  value={zoomDraft}
                  onFocus={(e) => e.currentTarget.select()}
                  onChange={(e) =>
                    setZoomDraft(e.target.value.replace(/[^0-9]/g, ""))
                  }
                  onBlur={() => setZoomDraft(null)}
                  onKeyDown={(e) => {
                    if (e.key === "Enter") aplicaZoomEscrito();
                    else if (e.key === "Escape") setZoomDraft(null);
                  }}
                />
              )}
              <button
                className="btn btn-icon"
                title={`Ampliar (${MOD}+)`}
                aria-label="Ampliar"
                onClick={() =>
                  setZoom(nivelZoom(zoomNum, 1))
                }
              >
                <Icon name="plus" size={14} />
              </button>
              <div className="sep" />
              <button
                className={`btn${zoom === "pagina" ? " on" : ""}`}
                title={`Ver la página entera (${MOD}0)`}
                aria-pressed={zoom === "pagina"}
                onClick={() => setZoom("pagina")}
              >
                Página
              </button>
              <button
                className={`btn${zoom === "ajuste" ? " on" : ""}`}
                title={`Ajustar al ancho de la ventana (${MOD}2)`}
                aria-pressed={zoom === "ajuste"}
                onClick={() => setZoom("ajuste")}
              >
                Ancho
              </button>
              <button
                className={`btn${zoom === 1 ? " on" : ""}`}
                title={`Tamaño real (${MOD}1)`}
                aria-pressed={zoom === 1}
                onClick={() => setZoom(1)}
              >
                100 %
              </button>
              {/* en presentación manda «una sola página»: el segmentado
                  marcaría un modo que no es el que se ve y pulsarlo no haría
                  nada, así que ni él ni su separador se pintan */}
              {!pantallaCompleta && (
                <>
                  <div className="sep" />
                  {MODOS_PILDORA.map(([id, icono, etiqueta]) => (
                    <button
                      key={id}
                      className={`btn btn-icon${
                        vista.modoPagina === id ? " on" : ""
                      }`}
                      title={etiqueta}
                      aria-label={etiqueta}
                      aria-pressed={vista.modoPagina === id}
                      onClick={() => cambiaVista({ modoPagina: id })}
                    >
                      <Icon name={icono} size={14} />
                    </button>
                  ))}
                </>
              )}
              {dobles && !pantallaCompleta && (
                <button
                  className={`btn${vista.portadaSola ? " on" : ""}`}
                  title="Mostrar la portada sola en la vista de dos páginas"
                  aria-pressed={vista.portadaSola}
                  onClick={() =>
                    cambiaVista({ portadaSola: !vista.portadaSola })
                  }
                >
                  Portada
                </button>
              )}
              {/* el andamio, con su estado a la vista: reglas, guías y
                  cuadrícula tenían tres atajos y ninguna puerta */}
              <div className="menu-wrap">
                <button
                  className={`btn btn-icon${
                    reglas || cuadricula ? " on" : ""
                  }`}
                  title="Reglas, guías y cuadrícula"
                  aria-label="Reglas, guías y cuadrícula"
                  aria-haspopup="menu"
                  aria-expanded={menuAndamio}
                  onClick={() => setMenuAndamio((v) => !v)}
                >
                  <Icon name="ruler" size={14} />
                </button>
                {menuAndamio && (
                  <>
                    <div
                      className="menu-backdrop"
                      onClick={() => setMenuAndamio(false)}
                    />
                    <div className="menu menu-andamio" role="menu">
                      <label className="opt-check">
                        <input
                          type="checkbox"
                          checked={reglas}
                          onChange={() => setReglas((v) => !v)}
                        />
                        <span className="menu-texto">Reglas</span>
                        <span className="menu-atajo dato">{MOD}R</span>
                      </label>
                      <label className="opt-check">
                        <input
                          type="checkbox"
                          checked={guiasVisibles}
                          onChange={() => setGuiasVisibles((v) => !v)}
                        />
                        <span className="menu-texto">Guías</span>
                        <span className="menu-atajo dato">{MOD};</span>
                      </label>
                      <label className="opt-check">
                        <input
                          type="checkbox"
                          checked={cuadricula}
                          onChange={() => setCuadricula((v) => !v)}
                        />
                        <span className="menu-texto">Cuadrícula</span>
                        <span className="menu-atajo dato">{MOD}U</span>
                      </label>
                      <label
                        className={`opt-check${cuadricula ? "" : " disabled"}`}
                        title={
                          cuadricula
                            ? "Lo que coloques cae en la cuadrícula"
                            : "Enciende antes la cuadrícula"
                        }
                      >
                        <input
                          type="checkbox"
                          checked={ajustarCuadricula}
                          disabled={!cuadricula}
                          onChange={() => setAjustarCuadricula((v) => !v)}
                        />
                        <span className="menu-texto">Ajustar a la cuadrícula</span>
                        <span className="menu-atajo dato">⇧{MOD}U</span>
                      </label>
                      <span className="opt-hint bm-hint">
                        Andamio para colocar: no toca el fichero ni se imprime.
                      </span>
                    </div>
                  </>
                )}
              </div>
              <button
                className={`btn btn-icon${modoLectura ? " on" : ""}`}
                title={`Modo lectura: solo el documento (⇧${MOD}H; Esc sale)`}
                aria-label="Modo lectura"
                aria-pressed={modoLectura}
                onClick={() => cambiaModoLectura(!modoLectura)}
              >
                <Icon name="libro" size={14} />
              </button>
              <button
                className={`btn btn-icon${pantallaCompleta ? " on" : ""}`}
                title={`Pantalla completa (${MOD}L; Esc sale)`}
                aria-label="Pantalla completa"
                aria-pressed={pantallaCompleta}
                onClick={() => cambiaPantallaCompleta(!pantallaCompleta)}
              >
                <Icon name="expand" size={14} />
              </button>
              <button
                className={`btn btn-icon${prefs.nocturno ? " on" : ""}`}
                title={`Modo nocturno: solo cambia lo que ves, el fichero no se toca (⇧${MOD}L)`}
                aria-label="Modo nocturno del documento"
                aria-pressed={prefs.nocturno}
                onClick={() =>
                  aplicaPrefs({ ...prefs, nocturno: !prefs.nocturno })
                }
              >
                <Icon name="moon" size={14} />
              </button>
              {(vistasAtras.length > 0 || vistasAdelante.length > 0) && (
                <>
                  <div className="sep" />
                  <button
                    className="btn btn-icon"
                    title="Volver a la vista anterior (⌥←)"
                    aria-label="Volver a la vista anterior"
                    disabled={vistasAtras.length === 0}
                    onClick={atrasVista}
                  >
                    <Icon name="back" size={14} />
                  </button>
                  <button
                    className="btn btn-icon"
                    title="Ir a la vista siguiente (⌥→)"
                    aria-label="Ir a la vista siguiente"
                    disabled={vistasAdelante.length === 0}
                    onClick={adelanteVista}
                  >
                    <Icon name="forward" size={14} />
                  </button>
                </>
              )}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

export default App;
