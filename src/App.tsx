import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  useSyncExternalStore,
} from "react";
import {
  busyCount,
  invoke,
  onAbrirFichero,
  onArrastreFicheros,
  onCerrarSolicitado,
  onMenuAccion,
  ponerPantallaCompleta,
  subscribeBusy,
} from "./ipc";
import { useHistorial } from "./hooks/useHistorial";
import { useRenderCache } from "./hooks/useRenderCache";
import { useMiniaturas } from "./hooks/useMiniaturas";
import { useBusqueda } from "./hooks/useBusqueda";
import { useFirmas } from "./hooks/useFirmas";
import { useHerramienta } from "./hooks/useHerramienta";
import { destinoDe, esquemaDe, esquemaPermitido } from "./enlaces";
import { open, save, openUrl } from "./dialogos";
import {
  addBlankPage,
  removeMarginalText,
  addHeaderFooter,
  addWatermark,
  duplicatePage,
  insertPdfAt,
  deletePages,
  extractPages,
  getDocumentAnnotations,
  listRecent,
  listRedactions,
  applyRedactions,
  mergeMany,
  replacePages,
  splitPdf,
  sanitizePdf,
  unmarkRedaction,
  rotatePages,
  removeRecent,
  renderPageSrc,
  signPdf,
  signPdfP12,
  touchRecent,
  uiLista,
  verifySignatures,
  type AnotacionDoc,
  type FirmaInfo,
  type HeaderFooter,
  type Redaccion,
  type RedactReport,
  type SanitizeReport,
  type Reciente,
} from "./api";
import {
  compressPdf,
  encryptPdf,
  exportPagesPng,
  exportText,
  flattenPdf,
  getMetadata,
  removeEncryption,
  TODO_PERMITIDO,
  getOutline,
  setMetadata,
  setOutline,
  type Metadata,
  type OutlineNode,
} from "./api";
import {
  ATAJO_COMENTARIOS,
  ATAJO_MARCADORES,
  ATAJO_PANEL,
  cargaPreferencias,
  cargaVista,
  estadoDeFirma,
  fechaLarga,
  filasDePaginas,
  FIRMA_VACIA,
  type FirmaDraft,
  guardaPreferencias,
  guardaVista,
  IMPRIMIR_POR_DEFECTO,
  type ModoPagina,
  type OpcionesImprimir,
  cargaResaltarCampos,
  formateaRango,
  hexToRgba,
  guardaResaltarCampos,
  MOD,
  parseRango,
  plural,
  type FiltroComentarios,
  type Mode,
  type PageSize,
  type Preferencias,
} from "./tipos";
import Icon from "./components/Icon";
import Busqueda from "./components/Busqueda";
import OpcionesHerramienta from "./components/OpcionesHerramienta";
import MenuAcciones from "./components/MenuAcciones";
import PanelPaginas from "./components/PanelPaginas";
import Pagina from "./components/Pagina";
import PanelFirmas from "./components/PanelFirmas";
import PanelFirmasDoc from "./components/PanelFirmasDoc";
import DialogoFirmar from "./components/DialogoFirmar";
import DibujarFirma from "./components/DibujarFirma";
import DialogoMarcaAgua from "./components/DialogoMarcaAgua";
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
import "./App.css";

const BASE_WIDTH = 900;
/** Zoom válido: del 50 % al 400 %, redondeado al 1 %. */
function recortaZoom(z: number): number {
  return Math.min(4, Math.max(0.5, Math.round(z * 100) / 100));
}
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

/** La banda de firmas, en una línea y sin jerga. */
function resumenFirmas(firmas: FirmaInfo[]): string {
  const mala = firmas.find((f) => !estadoDeFirma(f).ok);
  if (mala) return estadoDeFirma(mala).texto;
  const quien = firmas[0].name || firmas[0].cert_subject || "";
  const cuando = firmas[0].signed_at ? ` el ${fechaLarga(firmas[0].signed_at)}` : "";
  const cabecera =
    firmas.length > 1
      ? `Firmado por ${firmas.length} personas`
      : `Firmado${quien ? ` por ${quien}` : ""}${cuando}`;
  return `${cabecera} · el documento no ha cambiado desde la firma`;
}

/** Punto de lectura al que vuelve ⌥←: página, scroll y zoom. */
type Vista = { page: number; scrollTop: number; zoom: number | "ajuste" | "pagina" };

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
  const [zoom, setZoom] = useState<number | "ajuste" | "pagina">("ajuste");
  const [viewerW, setViewerW] = useState<number | null>(null);
  const [viewerH, setViewerH] = useState<number | null>(null);
  const viewerRef = useRef<HTMLElement | null>(null);
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
  // deshacer/rehacer general (instantáneas en el backend); tras restaurar
  // hace falta el refresco completo porque puede cambiar hasta el recuento
  const historial = useHistorial({
    workPath,
    onRestaurado: (n) => afterMutation(n),
    onError: (e) => setError(String(e)),
  });
  const refrescarHistorial = historial.refrescar;
  const [wmOpen, setWmOpen] = useState(false);
  const [marginalAsk, setMarginalAsk] = useState<{
    zona: "watermark" | "header" | "footer";
    textos: number;
  } | null>(null);
  const [hfOpen, setHfOpen] = useState(false);
  const [sidebarTab, setSidebarTab] = useState<
    "paginas" | "marcadores" | "comentarios" | "firmas"
  >("paginas");
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
  // el aviso de cómo salir sale una sola vez por sesión, como en Acrobat
  const avisoPantallaRef = useRef(false);
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
  // ficheros soltados de golpe: abrir el primero o unirlos
  const [dropAsk, setDropAsk] = useState<string[] | null>(null);
  const [arrastrando, setArrastrando] = useState(false);
  // el usuario ha intentado cerrar la ventana con cambios sin guardar
  const [cerrarAsk, setCerrarAsk] = useState(false);
  const [noticeSaliendo, setNoticeSaliendo] = useState(false);
  const [outline, setOutlineState] = useState<OutlineNode[]>([]);
  const [propsDraft, setPropsDraft] = useState<Metadata | null>(null);
  const [prefsAbiertas, setPrefsAbiertas] = useState(false);
  const {
    firmas,
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
  const herramienta = useHerramienta(activeSig);
  const tool = herramienta.tool;

  /** Abre un PDF en la copia de trabajo; devuelve su `work_path` o null
   *  si no se ha podido abrir. */
  async function openPath(
    path: string,
    password?: string,
  ): Promise<string | null> {
    try {
      // los avisos son del documento que se deja atrás: no deben sobrevivir
      // a la apertura de otro
      setError(null);
      setNotice(null);
      const anterior = workPath;
      const info = await invoke<{
        page_count: number;
        work_path: string;
        had_password: boolean;
      }>("open_pdf", { path, password: password ?? null });
      // la copia de trabajo del documento anterior ya no sirve: borrarla
      if (anterior) invoke("close_document", { workPath: anterior }).catch(() => {});
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
      if (info.had_password) {
        setNotice(
          "Documento protegido: al guardar puedes mantener la contraseña o quitarla",
        );
      }
      setPaginasSel(new Set());
      setViewRotation(0);
      setHayFormularios(false);
      // la herramienta armada no es del documento nuevo: Redactar sobre un
      // PDF recién abierto es lo último que quiere nadie
      setMode("select");
      setActiveSig(null);
      setFirmasDoc([]);
      setBandaFirmas(false);
      setNombreProvisional(null);
      setOriginalPath(path);
      setWorkPath(info.work_path);
      setPageCount(info.page_count);
      setPageIndex(0);
      // «Zoom al abrir» de las preferencias; «el último» no toca nada
      if (prefs.zoomInicial === "pagina") setZoom("pagina");
      else if (prefs.zoomInicial === "ancho") setZoom("ajuste");
      else if (prefs.zoomInicial === "100") setZoom(1);
      setDocVersion((v) => v + 1);
      viewerRef.current?.scrollTo({ top: 0 });
      scrollAnchorRef.current = null;
      // la lista de recientes la lleva la UI: open_pdf no la toca
      touchRecent(path)
        .then(refrescarRecientes)
        .catch(() => {});
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

  const refrescarRecientes = useCallback(() => {
    listRecent()
      .then(setRecientes)
      .catch(() => setRecientes([]));
  }, []);

  // al montar y cada vez que se vuelve al estado vacío: la lista se
  // revalida (un reciente puede haber desaparecido del disco entretanto)
  useEffect(() => {
    if (!workPath) refrescarRecientes();
  }, [workPath, refrescarRecientes]);

  /** Abre un fichero comprobando antes los cambios sin guardar. */
  function abrirComprobando(path: string) {
    conCambiosGuardados(() => {
      openPath(path);
    });
  }

  /** Abre un reciente. Si ya no está donde decía, se quita de la lista en
   *  vez de dejar la entrada rota invitando a volver a pulsarla. */
  function abrirReciente(path: string) {
    conCambiosGuardados(async () => {
      if (await openPath(path)) return;
      const lista = await listRecent().catch(() => [] as Reciente[]);
      const entrada = lista.find((r) => r.path === path);
      if (!entrada || entrada.exists) return;
      await removeRecent(path).catch(() => {});
      refrescarRecientes();
      setError(`Ya no está en ${path}; lo he quitado de recientes`);
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
    conCambiosGuardados(async () => {
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
    });
  }

  /** Deja que la ventana se cierre de verdad (el backend frenó el cierre). */
  function confirmarCierre() {
    invoke("confirmar_cierre").catch((e) => setError(String(e)));
  }

  /** Guarda y, solo si el guardado ha ido bien, cierra. */
  async function guardarYSalir() {
    const ok = await guardar();
    if (!ok) return;
    setCerrarAsk(false);
    confirmarCierre();
  }

  const cerrarRef = useRef<() => void>(() => {});
  cerrarRef.current = () => {
    if (modified) setCerrarAsk(true);
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

  /** Cierra el documento (preguntando si hay cambios sin guardar). */
  function closeDocument() {
    if (!workPath) return;
    setMenuOpen(false);
    conCambiosGuardados(cerrarDocumento);
  }

  /** Vuelve al estado vacío y borra la copia de trabajo. */
  function cerrarDocumento() {
    if (!workPath) return;
    const anterior = workPath;
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
    setProtegido(false);
    setProtPendiente(false);
    setMode("select");
    setPageIndex(0);
    setOutlineState([]);
    setComentarios([]);
    setAnnotSel(null);
    setPaginasSel(new Set());
    setViewRotation(0);
    setHayFormularios(false);
    setFirmasDoc([]);
    setBandaFirmas(false);
    evictAll();
    setDocVersion((v) => v + 1);
    invoke("close_document", { workPath: anterior }).catch((e) => setError(String(e)));
  }

  function openFile() {
    // el diálogo nativo se abre solo después de decidir qué hacer con los
    // cambios pendientes
    conCambiosGuardados(async () => {
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
    });
  }

  /** Abre el primero de varios y deja el resto a mano en recientes, en vez
   *  de descartarlos en silencio. */
  async function abrirPrimeroYRecordar(pdfs: string[]) {
    setDropAsk(null);
    // primero los demás: así el que se abre queda arriba de la lista
    for (const otro of pdfs.slice(1)) {
      await touchRecent(otro).catch(() => {});
    }
    abrirComprobando(pdfs[0]);
    setNotice(
      `${pdfs.length - 1} PDF más en Recientes, dentro de «Acciones»`,
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

  /** El índice que entiende `unmark_redaction` es la posición de la marca
   *  DENTRO de su página; `list_redactions` las devuelve en ese orden. */
  const marcasIndexadas = useMemo(() => {
    const cuenta = new Map<number, number>();
    return marcasRedact.map((m) => {
      const n = cuenta.get(m.page_index) ?? 0;
      cuenta.set(m.page_index, n + 1);
      return { ...m, markIndex: n };
    });
  }, [marcasRedact]);

  /** Quita una marca (no toca el contenido: solo la propuesta). */
  async function quitarMarca(page: number, markIndex: number) {
    if (!workPath) return;
    try {
      await unmarkRedaction(workPath, page, markIndex);
      refrescarMarcas();
      afterPageMutation(page);
    } catch (e) {
      setError(String(e));
    }
  }

  /** Quita todas las marcas de golpe, en un solo paso de deshacer. */
  async function quitarTodasLasMarcas() {
    if (!workPath || marcasIndexadas.length === 0) return;
    // de mayor a menor: quitar una corre los índices de las siguientes
    const orden = [...marcasIndexadas].sort(
      (a, b) => b.page_index - a.page_index || b.markIndex - a.markIndex,
    );
    try {
      let hechas = 0;
      for (const m of orden) {
        await unmarkRedaction(workPath, m.page_index, m.markIndex);
        hechas++;
      }
      if (hechas > 1) await historial.agrupar(hechas);
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

  /** Abre una pestaña del panel lateral y le lleva el foco: con el teclado
   *  se llega a la lista sin pasar por el ratón. */
  function abrirPestana(
    tab: "paginas" | "marcadores" | "comentarios" | "firmas",
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
      setPropsDraft(await getMetadata(workPath));
    } catch (e) {
      setError(String(e));
    }
  }

  async function saveProperties(meta: Metadata) {
    if (!workPath) return;
    try {
      await setMetadata(workPath, meta);
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
        // en pantalla completa la primera salida es la de la presentación
        if (pantallaCompleta) {
          e.preventDefault();
          cambiaPantallaCompleta(false);
          return;
        }
        // Acrobat quita los resaltados de coincidencia desde cualquier
        // sitio, no solo con el foco dentro del campo
        if (busqueda.matches.length > 0) busqueda.limpiar(true);
      } else if (mod && !e.shiftKey && (e.key === "l" || e.key === "L") && pageCount > 0) {
        e.preventDefault();
        cambiaPantallaCompleta(!pantallaCompleta);
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
        setPageDraft(String(pageIndex + 1));
      } else if (mod && e.key === "p" && pageCount > 0) {
        e.preventDefault();
        printDocument();
      } else if (mod && !enCampo && e.key === "d" && pageCount > 0) {
        e.preventDefault();
        openProperties();
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
        setZoom(recortaZoom(Math.round((zoomNum + 0.25) * 4) / 4));
      } else if (mod && !e.shiftKey && e.key === "-" && pageCount > 0) {
        e.preventDefault();
        setZoom(recortaZoom(Math.round((zoomNum - 0.25) * 4) / 4));
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
        busqueda.gotoMatch(e.shiftKey ? -1 : 1);
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
        gotoPage(pageIndex + 1);
      } else if (!mod && !e.altKey && !enCampo && e.key === "ArrowLeft") {
        gotoPage(pageIndex - 1);
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

  /** Salta a la página escrita en la píldora; fuera de rango, gotoPage la
   *  recorta en silencio. */
  function irAPaginaEscrita() {
    const n = Number.parseInt(pageDraft ?? "", 10);
    setPageDraft(null);
    if (!Number.isNaN(n)) gotoPage(n - 1);
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
   *  dónde se viene para que ⌥← devuelva ahí, como en Acrobat. */
  function saltarA(page: number) {
    setVistasAtras((v) => [...v.slice(-49), vistaActual()]);
    setVistasAdelante([]);
    gotoPage(page);
  }

  function atrasVista() {
    if (vistasAtras.length === 0) return;
    const v = vistasAtras[vistasAtras.length - 1];
    setVistasAtras((p) => p.slice(0, -1));
    setVistasAdelante((p) => [...p, vistaActual()]);
    restaurarVista(v);
  }

  function adelanteVista() {
    if (vistasAdelante.length === 0) return;
    const v = vistasAdelante[vistasAdelante.length - 1];
    setVistasAdelante((p) => p.slice(0, -1));
    setVistasAtras((p) => [...p, vistaActual()]);
    restaurarVista(v);
  }

  /** Presentación a pantalla completa: el chrome desaparece y la hoja se
   *  queda sola. Esc sale, y la primera vez se dice cómo. */
  function cambiaPantallaCompleta(valor: boolean) {
    setPantallaCompleta(valor);
    // la presentación no tiene herramientas (tampoco en Acrobat): así Esc
    // es siempre la salida, sin tener que pulsarlo dos veces
    if (valor) setMode("select");
    ponerPantallaCompleta(valor).catch((e) => setError(String(e)));
    if (valor && !avisoPantallaRef.current) {
      avisoPantallaRef.current = true;
      setNotice("Pulsa Esc para salir de la pantalla completa");
    }
  }

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

  const busqueda = useBusqueda({
    workPath,
    // los saltos entre coincidencias también se apilan: ⌥← vuelve a donde
    // se estaba leyendo antes de buscar
    gotoPage: saltarA,
    onError: (e) => setError(String(e)),
  });
  const limpiarBusqueda = busqueda.limpiar;
  hayCoincidenciasRef.current = busqueda.matches.length > 0;

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
    limpiarBusqueda();
    setPageIndex((p) => Math.max(0, Math.min(nextPage ?? p, newCount - 1)));
    // el docVersion nuevo deja inservible todo el caché: liberar los blobs
    evictAll();
    setDocVersion((v) => v + 1);
    refrescarHistorial();
  }, [refrescarHistorial, evictAll, limpiarBusqueda]);

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

  async function insertPdfHere() {
    if (!workPath) return;
    const selected = await open({
      filters: [{ name: "PDF", extensions: ["pdf"] }],
      multiple: false,
      title: "Insertar PDF después de la página actual",
    });
    if (typeof selected !== "string") return;
    try {
      const count = await insertPdfAt(workPath, selected, pageIndex + 1);
      afterMutation(count);
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

  async function applyWatermark(opts: {
    text: string;
    fontSize: number;
    color: string;
    opacity: number;
    diagonal: boolean;
    position: string;
  }) {
    if (!workPath) return;
    try {
      await addWatermark({
        workPath,
        text: opts.text,
        fontSize: opts.fontSize,
        color: hexToRgba(opts.color, Math.round((opts.opacity / 100) * 255)),
        diagonal: opts.diagonal,
        position: opts.position,
      });
      setWmOpen(false);
      afterMutation(pageCount);
    } catch (e) {
      setError(String(e));
    }
  }

  async function applyHeaderFooter(zonas: HeaderFooter, fontSize: number) {
    if (!workPath) return;
    try {
      await addHeaderFooter(workPath, zonas, fontSize);
      setHfOpen(false);
      afterMutation(pageCount);
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

  /** Páginas que pide el diálogo, ya filtradas por pares/impares. */
  function paginasAImprimir(o: OpcionesImprimir): number[] {
    const base =
      o.ambito === "todas"
        ? Array.from({ length: pageCount }, (_, i) => i)
        : o.ambito === "actual"
          ? [pageIndex]
          : parseRango(o.rango, pageCount);
    if (o.subconjunto === "todas") return base;
    // «pares» e «impares» van por el número que ve el usuario, no por índice
    const quiereImpar = o.subconjunto === "impares";
    return base.filter((i) => (i + 1) % 2 === (quiereImpar ? 1 : 0));
  }

  /** Rasteriza solo el rango pedido y abre el diálogo del sistema. El bucle
   *  se puede cancelar desde la propia banda de progreso; al cancelar se
   *  liberan los blobs y no se abre nada. */
  async function prepararImpresion(o: OpcionesImprimir) {
    if (!workPath) return;
    const idx = paginasAImprimir(o);
    if (idx.length === 0) {
      setError(
        `Escribe qué páginas quieres imprimir, por ejemplo «1-3, 8» (el documento tiene ${pageCount})`,
      );
      return;
    }
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
      // liberar los blob URLs de las páginas ya impresas
      for (const p of printPages) URL.revokeObjectURL(p.src);
      setPrintPages(null);
    }, 200);
    return () => clearTimeout(t);
  }, [printPages]);

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

  async function applyCompress() {
    if (!workPath) return;
    try {
      setCompressOpen(false);
      setNotice("Comprimiendo…", { persistente: true });
      const r = await compressPdf(workPath, compressQuality, compressDpi);
      // por debajo de 1 MB dos decimales de MB no distinguen nada: KB
      const tam = (n: number) =>
        n < 1024 * 1024
          ? `${Math.round(n / 1024)} KB`
          : `${(n / 1024 / 1024).toFixed(2)} MB`;
      setNotice(
        r.imagenes === 0
          ? "No había imágenes que comprimir."
          : `${plural(r.imagenes, "imagen recomprimida", "imágenes recomprimidas")}: ${tam(r.antes)} → ${tam(r.despues)}`,
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
        const sep = dir.includes("\\") ? "\\" : "/";
        for (const i of idx) {
          await extractPages({
            workPath,
            pageIndices: [i],
            destPath: `${dir}${sep}pagina-${i + 1}.pdf`,
            deleteAfter: false,
          });
        }
        if (opts.borrar) {
          const count = await deletePages(workPath, idx);
          setPaginasSel(new Set());
          afterMutation(count);
        }
        setNotice(`${plural(idx.length, "PDF escrito", "PDF escritos")} en ${dir}`);
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
      setNotice(`Guardado en ${dest}`);
      return true;
    } catch (e) {
      setError(String(e));
      return false;
    }
  }

  /** Guarda en `dest`; si el original iba cifrado, pregunta antes si se
   *  mantiene la contraseña. */
  function guardarEn(dest: string): Promise<boolean> {
    if (!hadPassword || docPassword === null) return escribirEn(dest, false);
    return new Promise((resolve) => setSaveAsk({ dest, resolve }));
  }

  function resolverSaveAsk(mantener: boolean | null) {
    if (!saveAsk) return;
    const { dest, resolve } = saveAsk;
    setSaveAsk(null);
    if (mantener === null) resolve(false);
    else escribirEn(dest, mantener).then(resolve);
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
    try {
      setNotice("Firmando…", { persistente: true });
      if (/\.(p12|pfx)$/i.test(d.certPath)) {
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
   * es quien pinta el menú. Copiar, cortar y pegar son entradas nativas de
   * Tauri y no llegan por aquí.
   */
  const accionesMenu: Record<string, () => void> = {
    abrir: openFile,
    guardar: () => {
      if (modified) guardar();
    },
    "guardar-como": () => {
      if (pageCount > 0) saveFileAs();
    },
    "cerrar-documento": closeDocument,
    imprimir: printDocument,
    "anadir-pdf": addPdf,
    extraer: () => setExtraerOpen(true),
    reemplazar: () => setReemplazarOpen(true),
    dividir: () => setDividirOpen(true),
    combinar: () => setCombinarOpen(true),
    "insertar-pdf": insertPdfHere,
    deshacer: () => {
      if (historial.puedeDeshacer) historial.deshacer();
    },
    rehacer: () => {
      if (historial.puedeRehacer) historial.rehacer();
    },
    buscar: () =>
      (document.querySelector(".search input") as HTMLInputElement)?.focus(),
    preferencias: () => setPrefsAbiertas(true),
    "zoom-pagina": () => setZoom("pagina"),
    "zoom-ancho": () => setZoom("ajuste"),
    "zoom-100": () => setZoom(1),
    ampliar: () => setZoom(recortaZoom(Math.round((zoomNum + 0.25) * 4) / 4)),
    reducir: () => setZoom(recortaZoom(Math.round((zoomNum - 0.25) * 4) / 4)),
    "girar-vista-derecha": () => setViewRotation((r) => (r + 90) % 360),
    "girar-vista-izquierda": () => setViewRotation((r) => (r + 270) % 360),
    "pantalla-completa": () => cambiaPantallaCompleta(!pantallaCompleta),
    nocturno: () => aplicaPrefs({ ...prefs, nocturno: !prefs.nocturno }),
    "panel-lateral": () => setSidebarVisible((v) => !v),
    "pagina-una": () => cambiaVista({ modoPagina: "una" }),
    "pagina-continuo": () => cambiaVista({ modoPagina: "continuo" }),
    "pagina-dos": () => cambiaVista({ modoPagina: "dos" }),
    "pagina-dos-continuo": () => cambiaVista({ modoPagina: "dos-continuo" }),
    "vista-atras": atrasVista,
    "vista-adelante": adelanteVista,
    recortar: () => setMode("crop"),
    "marca-agua": () => setWmOpen(true),
    encabezado: () => setHfOpen(true),
    "quitar-marca-agua": () => askRemoveMarginal("watermark"),
    "quitar-encabezados": () => askRemoveMarginal("header"),
    propiedades: openProperties,
    proteger: () =>
      setProtectDraft({ user: "", owner: "", ...TODO_PERMITIDO }),
    "quitar-proteccion": () => {
      if (protegido || protPendiente) setQuitarProtAsk(true);
    },
    firmar: empezarFirma,
    aplanar: () => setFlattenAsk(true),
    redactar: () => setMode("redact"),
    sanear: pedirSanear,
    "campo-nuevo": () => setMode("form-new"),
    "enlace-nuevo": () => setMode("link-new"),
    "exportar-imagenes": () => setExportOpen(true),
    "exportar-texto": exportPlainText,
    comprimir: () => setCompressOpen(true),
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

  const mostrarError = useCallback((e: unknown) => setError(String(e)), []);
  const mostrarAviso = useCallback(
    (texto: string) => setNotice(texto),
    [setNotice],
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
      id: "firmar",
      icon: "sign",
      label: "Firma",
      hint: "Estampar tu firma manuscrita: elige o crea una y haz clic (o arrastra) donde quieras colocarla",
    },
  ];

  function selectMode(m: Mode) {
    setMode((cur) => (cur === m ? "select" : m));
    setActiveSig(null);
  }

  return (
    <div className={`app${pantallaCompleta ? " presentacion" : ""}`}>
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
          {ocupado && <span className="status dato">trabajando…</span>}
        </div>

        {pageCount > 0 && (
          <div className="segmented">
            {(
              [
                ["select"],
                ["draw", "note", "freetext", "shape", "stamp"],
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
                insertPdfHere={insertPdfHere}
                recortarPagina={() => {
                  selectMode("select");
                  setMode("crop");
                }}
                abrirMarcaAgua={() => setWmOpen(true)}
                abrirEncabezado={() => setHfOpen(true)}
                askRemoveMarginal={askRemoveMarginal}
                openProperties={openProperties}
                signPdf={empezarFirma}
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
                abrirComprimir={() => setCompressOpen(true)}
              />
            </>
          )}
        </div>
      </header>

      {error && (
        <div className="banner-error">
          <p title={error}>{error}</p>
          <button className="btn btn-icon" aria-label="Cerrar el aviso" onClick={() => setError(null)}>
            <Icon name="close" size={13} />
          </button>
        </div>
      )}
      {bandaFirmas && firmasDoc.length > 0 && (
        <div
          className={`banner-firmas${
            firmasDoc.every((f) => estadoDeFirma(f).ok) ? "" : " mal"
          }`}
        >
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
          pagina={firmaRect.page + 1}
          firmas={firmas}
          onConfirm={aplicarFirma}
          onClose={() => setFirmaRect(null)}
        />
      )}

      {mode === "firmar" && !activeSig && !drawingSig && (
        <PanelFirmas
          firmas={firmas}
          onPick={pickSignature}
          onUpload={uploadSignature}
          onDraw={() => setDrawingSig(true)}
          onDelete={removeSignature}
          onClose={() => selectMode("select")}
        />
      )}
      {drawingSig && (
        <DibujarFirma
          onSave={saveDrawnSignature}
          onClose={() => setDrawingSig(false)}
        />
      )}
      {wmOpen && (
        <DialogoMarcaAgua onApply={applyWatermark} onClose={() => setWmOpen(false)} />
      )}
      {hfOpen && (
        <DialogoEncabezado
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
      {propsDraft && (
        <DialogoPropiedades
          initial={propsDraft}
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
      {redactAsk && (
        <DialogoConfirmar
          titulo="Aplicar la redacción"
          cuerpo={
            <p className="modal-file" style={{ whiteSpace: "normal" }}>
              En {plural(marcasRedact.length, "zona marcada", "zonas marcadas")}{" "}
              se eliminarán{" "}
              {plural(redactAsk.textos, "bloque de texto", "bloques de texto")} y{" "}
              {plural(redactAsk.imagenes, "imagen", "imágenes")}, y quedará una
              caja negra encima. El contenido se elimina y no se podrá
              recuperar guardando; {MOD}Z lo devuelve mientras el documento
              siga abierto.
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
            texto: "Descartar",
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
          textoConfirmar="Guardar sin contraseña"
          secundario={{
            texto: "Mantener contraseña",
            onClick: () => resolverSaveAsk(true),
          }}
          onConfirm={() => resolverSaveAsk(false)}
          onClose={() => resolverSaveAsk(null)}
        />
      )}
      {dropAsk && (
        <DialogoConfirmar
          titulo={`Has elegido ${dropAsk.length} PDF`}
          cuerpo={
            <p className="modal-file" style={{ whiteSpace: "normal" }}>
              Puedes abrir solo el primero ({dropAsk[0].split(/[\\/]/).pop()})
              —los demás quedan en Recientes— o unirlos todos en un documento
              nuevo, en el orden en que los has elegido.
            </p>
          }
          textoConfirmar="Unirlos en uno"
          secundario={{
            texto: "Abrir el primero",
            onClick: () => abrirPrimeroYRecordar(dropAsk),
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
      {printPages && (
        <div className="print-pages">
          {printPages.map((p, i) => (
            <img
              key={i}
              src={p.src}
              alt={`Página ${i + 1}`}
              style={p.anchoIn ? { width: `${p.anchoIn}in` } : undefined}
            />
          ))}
        </div>
      )}
      {mode === "crop" && (
        <div className="sign-hint">
          Arrastra para marcar el área que quieres conservar · Esc cancela
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
        shapeKind={herramienta.shapeKind}
        setShapeKind={herramienta.setShapeKind}
        shapeColor={herramienta.shapeColor}
        shapeFill={herramienta.shapeFill}
        setShapeFill={herramienta.setShapeFill}
        shapeWidth={herramienta.shapeWidth}
        setShapeWidth={herramienta.setShapeWidth}
        stampText={herramienta.stampText}
        setStampText={herramienta.setStampText}
        stampCustom={herramienta.stampCustom}
        setStampCustom={herramienta.setStampCustom}
        stampColor={herramienta.stampColor}
        hayFormularios={hayFormularios}
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
        cambiaColorAccion={herramienta.cambiaColorAccion}
        anadirTexto={() => setPedirTextoNuevo((n) => n + 1)}
        insertarImagen={() => setPedirImagen((n) => n + 1)}
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
            </div>
            {sidebarTab === "firmas" && (
              <PanelFirmasDoc firmas={firmasDoc} onGoto={saltarA} />
            )}
            {sidebarTab === "comentarios" && (
              <PanelComentarios
                comentarios={comentarios}
                filtro={filtroComentarios}
                setFiltro={setFiltroComentarios}
                filtroAutor={filtroAutor}
                setFiltroAutor={setFiltroAutor}
                seleccionada={annotSel}
                focoPedido={focoComentarios}
                onSelect={irAComentario}
                onDelete={borrarComentario}
              />
            )}
            {sidebarTab === "marcadores" && (
              <PanelMarcadores
                outline={outline}
                currentPage={pageIndex}
                onGoto={saltarA}
                onChange={persistOutline}
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
                gotoPage={gotoPage}
                movePage={movePage}
                rotatePage={rotatePage}
                duplicatePageAt={duplicatePageAt}
                blankPageAfter={blankPageAfter}
                deletePage={deletePage}
              />
            )}
          </aside>
        )}

        <div className="viewer-wrap">
          <main
            className={`viewer${arrastrando ? " arrastrando" : ""}${
              prefs.nocturno ? " nocturno" : ""
            }`}
            ref={viewerRef}
            onScroll={onViewerScroll}
            onClick={
              // en presentación el clic avanza, como en Acrobat
              pantallaCompleta ? () => gotoPage(pageIndex + 1) : undefined
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
                {recientes.length > 0 && (
                  <div className="recientes">
                    <span className="card-label">Recientes</span>
                    {recientes.map((r) => (
                      <div
                        key={r.path}
                        className={`reciente${r.exists ? "" : " no-esta"}`}
                      >
                        <button
                          className="reciente-abrir"
                          title={r.exists ? r.path : `Ya no está en ${r.path}`}
                          disabled={!r.exists}
                          onClick={() => abrirReciente(r.path)}
                        >
                          <span className="reciente-nombre">{r.name}</span>
                          <span className="reciente-dir">{r.dir}</span>
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
                    ))}
                  </div>
                )}
              </div>
            )}
            {workPath &&
              filasVisibles.map((fila) => (
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
                  marcas={marcasIndexadas
                    .filter((m) => m.page_index === i)
                    .map((m) => ({ markIndex: m.markIndex, rect: m.rect }))}
                  quitarMarca={quitarMarca}
                  onMarcasCambian={refrescarMarcas}
                  pedirTextoNuevo={pedirTextoNuevo}
                  pedirImagen={pedirImagen}
                />
                  ))}
              </div>
              ))}
          </main>

          {pageCount > 0 && (
            <div className="nav-pill">
              <button
                className="btn btn-icon"
                title="Página anterior"
                aria-label="Página anterior"
                disabled={pageIndex === 0}
                onClick={() => gotoPage(pageIndex - 1)}
              >
                <Icon name="chevLeft" size={14} />
              </button>
              {pageDraft === null ? (
                <button
                  className="btn pill-boton"
                  title={`Ir a la página (⇧${MOD}N)`}
                  aria-label="Ir a la página"
                  onClick={() => setPageDraft(String(pageIndex + 1))}
                >
                  {pageIndex + 1} / {pageCount}
                </button>
              ) : (
                <input
                  className="pill-input"
                  inputMode="numeric"
                  autoFocus
                  aria-label={`Ir a la página (1 a ${pageCount})`}
                  value={pageDraft}
                  onFocus={(e) => e.currentTarget.select()}
                  onChange={(e) =>
                    setPageDraft(e.target.value.replace(/[^0-9]/g, ""))
                  }
                  onBlur={() => setPageDraft(null)}
                  onKeyDown={(e) => {
                    if (e.key === "Enter") irAPaginaEscrita();
                    else if (e.key === "Escape") setPageDraft(null);
                  }}
                />
              )}
              <button
                className="btn btn-icon"
                title="Página siguiente"
                aria-label="Página siguiente"
                disabled={pageIndex >= pageCount - 1}
                onClick={() => gotoPage(pageIndex + 1)}
              >
                <Icon name="chevRight" size={14} />
              </button>
              <div className="sep" />
              <button
                className="btn btn-icon"
                title={`Reducir (${MOD}−)`}
                aria-label="Reducir"
                onClick={() =>
                  setZoom(recortaZoom(Math.round((zoomNum - 0.25) * 4) / 4))
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
                  aria-label="Porcentaje de zoom (50 a 400)"
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
                  setZoom(recortaZoom(Math.round((zoomNum + 0.25) * 4) / 4))
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
              <div className="sep" />
              {(
                [
                  ["una", "pageOne", "Una sola página"],
                  ["continuo", "pageScroll", "Desplazamiento continuo"],
                  ["dos", "pageTwo", "Dos páginas"],
                  [
                    "dos-continuo",
                    "pageTwoScroll",
                    "Dos páginas con desplazamiento continuo",
                  ],
                ] as [ModoPagina, string, string][]
              ).map(([id, icono, etiqueta]) => (
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
