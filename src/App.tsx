import {
  useCallback,
  useEffect,
  useLayoutEffect,
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
  rotatePages,
  removeRecent,
  renderPageSrc,
  touchRecent,
  type AnotacionDoc,
  type HeaderFooter,
  type Reciente,
} from "./api";
import {
  compressPdf,
  encryptPdf,
  exportPagesPng,
  exportText,
  flattenPdf,
  getMetadata,
  getOutline,
  setMetadata,
  setOutline,
  type Metadata,
  type OutlineNode,
} from "./api";
import {
  ATAJO_PANEL,
  cargaPreferencias,
  cargaResaltarCampos,
  formateaRango,
  hexToRgba,
  guardaResaltarCampos,
  MOD,
  parseRango,
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
import DibujarFirma from "./components/DibujarFirma";
import DialogoMarcaAgua from "./components/DialogoMarcaAgua";
import DialogoEncabezado from "./components/DialogoEncabezado";
import PanelMarcadores from "./components/PanelMarcadores";
import PanelComentarios from "./components/PanelComentarios";
import DialogoExtraer from "./components/DialogoExtraer";
import DialogoPropiedades from "./components/DialogoPropiedades";
import DialogoContrasena from "./components/DialogoContrasena";
import DialogoProteger from "./components/DialogoProteger";
import DialogoConfirmar from "./components/DialogoConfirmar";
import DialogoExportar from "./components/DialogoExportar";
import DialogoComprimir from "./components/DialogoComprimir";
import DialogoPreferencias from "./components/DialogoPreferencias";
import "./App.css";

const BASE_WIDTH = 900;
/** Zoom válido: del 50 % al 400 %, redondeado al 1 %. */
function recortaZoom(z: number): number {
  return Math.min(4, Math.max(0.5, Math.round(z * 100) / 100));
}
/** Separación vertical entre páginas y padding superior del visor (px). */
const PAGE_GAP = 24;
const VIEWER_PAD_TOP = 28;

function App() {
  const [originalPath, setOriginalPath] = useState<string | null>(null);
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
  const [p12Draft, setP12Draft] = useState<{
    path: string;
    password: string;
  } | null>(null);
  const [wmOpen, setWmOpen] = useState(false);
  const [marginalAsk, setMarginalAsk] = useState<{
    zona: "watermark" | "header" | "footer";
    textos: number;
  } | null>(null);
  const [hfOpen, setHfOpen] = useState(false);
  const [sidebarTab, setSidebarTab] = useState<
    "paginas" | "marcadores" | "comentarios"
  >("paginas");
  const [comentarios, setComentarios] = useState<AnotacionDoc[]>([]);
  const [filtroComentarios, setFiltroComentarios] =
    useState<FiltroComentarios>("todos");
  // páginas marcadas en el panel para actuar en lote
  const [paginasSel, setPaginasSel] = useState<Set<number>>(new Set());
  const [extraerOpen, setExtraerOpen] = useState(false);
  // giro SOLO de la vista (⇧⌘+ / ⇧⌘−): no toca el fichero y se pierde al
  // cerrar, como en Acrobat
  const [viewRotation, setViewRotation] = useState(0);
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
  const [protectDraft, setProtectDraft] = useState<{
    user: string;
    owner: string;
  } | null>(null);
  const [flattenAsk, setFlattenAsk] = useState(false);
  // enlace externo pendiente de confirmar (los URI del PDF no son de fiar)
  const [linkAsk, setLinkAsk] = useState<string | null>(null);
  const [printPages, setPrintPages] = useState<string[] | null>(null);
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
  const [recientes, setRecientes] = useState<Reciente[]>([]);
  // ficheros soltados de golpe: abrir el primero o unirlos
  const [dropAsk, setDropAsk] = useState<string[] | null>(null);
  const [arrastrando, setArrastrando] = useState(false);
  // el usuario ha intentado cerrar la ventana con cambios sin guardar
  const [cerrarAsk, setCerrarAsk] = useState(false);
  const [noticeSaliendo, setNoticeSaliendo] = useState(false);
  const [outline, setOutlineState] = useState<OutlineNode[]>([]);
  const [propsDraft, setPropsDraft] = useState<Metadata | null>(null);
  const [prefsDraft, setPrefsDraft] = useState<Preferencias | null>(null);
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
      setOriginalPath(path);
      setWorkPath(info.work_path);
      setPageCount(info.page_count);
      setPageIndex(0);
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
   *  que otro aviso lo sustituye (progreso); si no, se va solo a los 6 s. */
  const setNotice = useCallback(
    (texto: string | null, opts?: { persistente?: boolean }) => {
      setNoticeTexto(texto);
      setNoticePersistente(!!opts?.persistente);
    },
    [],
  );

  const refrescarRecientes = useCallback(() => {
    listRecent()
      .then(setRecientes)
      .catch(() => setRecientes([]));
  }, []);

  useEffect(() => {
    refrescarRecientes();
  }, [refrescarRecientes]);

  /** Abre un fichero comprobando antes los cambios sin guardar. */
  function abrirComprobando(path: string) {
    conCambiosGuardados(() => {
      openPath(path);
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
        setNotice(
          `${pdfs.length} PDF unidos en uno; usa Guardar como para conservarlo`,
        );
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
    const ok = await (originalPath ? saveFile() : saveFileAs());
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
    (originalPath ? saveFile() : saveFileAs()).then((ok) => {
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
    setPageCount(0);
    setPageSizes([]);
    setThumbs([]);
    setPageVersions([]);
    busqueda.limpiar(true);
    setModified(false);
    setHadPassword(false);
    setDocPassword(null);
    setMode("select");
    setPageIndex(0);
    setOutlineState([]);
    setComentarios([]);
    setAnnotSel(null);
    setPaginasSel(new Set());
    setViewRotation(0);
    setHayFormularios(false);
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
        multiple: false,
      });
      if (typeof selected !== "string") return;
      await openPath(selected);
    });
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

  /** Clic en una fila del panel: a su página y con su popover abierto. */
  function irAComentario(c: AnotacionDoc) {
    gotoPage(c.page_index);
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
      if (document.querySelector(".modal-backdrop")) return;
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
        // Acrobat quita los resaltados de coincidencia desde cualquier
        // sitio, no solo con el foco dentro del campo
        if (busqueda.matches.length > 0) busqueda.limpiar(true);
      } else if (mod && e.key === "o") {
        e.preventDefault();
        openFile();
      } else if (mod && (e.key === "s" || e.key === "S")) {
        e.preventDefault();
        if (e.shiftKey) {
          if (pageCount > 0) saveFileAs();
        } else if (modified) saveFile();
      } else if (mod && e.key === ",") {
        e.preventDefault();
        setPrefsDraft(cargaPreferencias());
      } else if (
        mod &&
        e.altKey &&
        (e.key === "1" || e.code === "Digit1") &&
        pageCount > 0
      ) {
        e.preventDefault();
        setSidebarVisible((v) => !v);
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
      } else if (!mod && !enCampo && e.key === "ArrowRight") {
        gotoPage(pageIndex + 1);
      } else if (!mod && !enCampo && e.key === "ArrowLeft") {
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
  const fitWidth = viewerW ? Math.max(320, viewerW - PADDING_VIEWER) : BASE_WIDTH;
  const fitHeight = viewerH
    ? Math.max(200, viewerH - VIEWER_PAD_TOP - VIEWER_PAD_BOTTOM)
    : BASE_WIDTH;
  // «ajustar»: el ancho de hoja que hace que la página ocupe justo el ancho
  // (o el alto) útil. Se usa la proporción más alta del documento para que el
  // ancho no cambie al desplazarse entre páginas de tamaños distintos, y se
  // cuenta el giro de la vista, que intercambia alto y ancho en pantalla.
  const ratioMax = pageSizes.reduce((m, s) => Math.max(m, s.height / s.width), 0);
  const anchoAjuste =
    vistaGirada && ratioMax > 0 ? fitWidth / ratioMax : fitWidth;
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

  /** Alturas en pantalla de cada página con el ancho de hoja dado. Con la
   *  vista girada un cuarto, alto y ancho se intercambian. */
  function alturasPagina(width: number): number[] {
    return pageSizes.map((s) =>
      vistaGirada ? width : (width * s.height) / s.width,
    );
  }

  /** Lleva el visor al principio de una página (miniaturas, marcadores,
   *  enlaces internos, flechas y píldora). */
  const gotoPage = useCallback(
    (i: number) => {
      if (pageCount === 0) return;
      const target = Math.max(0, Math.min(i, pageCount - 1));
      setPageIndex(target);
      pageElsRef.current.get(target)?.scrollIntoView({ block: "start" });
    },
    [pageCount],
  );

  /** Salta a la página escrita en la píldora; fuera de rango, gotoPage la
   *  recorta en silencio. */
  function irAPaginaEscrita() {
    const n = Number.parseInt(pageDraft ?? "", 10);
    setPageDraft(null);
    if (!Number.isNaN(n)) gotoPage(n - 1);
  }

  const busqueda = useBusqueda({
    workPath,
    gotoPage,
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
      if (!el || pageSizes.length === 0) return;
      const centro = el.scrollTop + el.clientHeight / 2;
      const alturas = alturasPagina(displayWidth);
      let y = VIEWER_PAD_TOP;
      let best = 0;
      let bestDist = Infinity;
      let anchor: { page: number; frac: number; offset: number } | null = null;
      for (let i = 0; i < alturas.length; i++) {
        const h = alturas[i];
        if (anchor === null && y + h > el.scrollTop) {
          anchor = {
            page: i,
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
      setPageIndex(best);
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

  async function applyProtect() {
    if (!workPath || !protectDraft || !protectDraft.user) return;
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
        userPassword: protectDraft.user,
        ownerPassword: protectDraft.owner || null,
      });
      setProtectDraft(null);
      setNotice(`Copia protegida guardada en ${dest}`);
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

  async function printDocument() {
    if (!workPath) return;
    try {
      setNotice("Preparando la impresión…", { persistente: true });
      const pages: string[] = [];
      for (let i = 0; i < pageCount; i++) {
        const width = Math.round(((pageSizes[i]?.width ?? 595) * 200) / 72);
        pages.push(await renderPageSrc(workPath, i, width));
      }
      setNotice(null);
      setPrintPages(pages);
    } catch (e) {
      setNotice(null);
      setError(String(e));
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
      for (const src of printPages) URL.revokeObjectURL(src);
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
      setNotice(`${rutas.length} imagen(es) exportadas a ${dir}`);
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
          : `${r.imagenes} imagen(es) recomprimidas: ${tam(r.antes)} → ${tam(r.despues)}`,
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
        setNotice(`${idx.length} PDF escritos en ${dir}`);
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
      setNotice(`${idx.length} página(s) extraídas a ${dest}`);
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
      } else {
        await invoke("save_pdf", { workPath, destPath: dest });
        // a partir de aquí el fichero de `dest` va en claro: el aviso de
        // «Documento protegido» que se puso al abrirlo ya no es cierto
        if (hadPassword) setNotice(null);
        setHadPassword(false);
        setDocPassword(null);
      }
      setOriginalPath(dest);
      setModified(false);
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

  async function saveFileAs(): Promise<boolean> {
    if (!workPath) return false;
    const dest = await save({
      filters: [{ name: "PDF", extensions: ["pdf"] }],
      defaultPath: originalPath ?? "documento.pdf",
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

  async function signPdf() {
    if (!workPath) return;
    const certPath = await open({
      filters: [
        {
          name: "Certificado (PEM o PKCS#12)",
          extensions: ["pem", "crt", "cer", "p12", "pfx"],
        },
      ],
      multiple: false,
      title: "Certificado (PEM) o contenedor .p12/.pfx",
    });
    if (typeof certPath !== "string") return;
    if (/\.(p12|pfx)$/i.test(certPath)) {
      setP12Draft({ path: certPath, password: "" });
      return;
    }
    const keyPath = await open({
      filters: [{ name: "Clave privada PEM", extensions: ["pem", "key"] }],
      multiple: false,
      title: "Clave privada (PEM, RSA sin cifrar)",
    });
    if (typeof keyPath !== "string") return;
    const dest = await pickSignedDest();
    if (!dest) return;
    try {
      await invoke("sign_pdf", {
        workPath,
        destPath: dest,
        certPemPath: certPath,
        keyPemPath: keyPath,
        reason: null,
      });
    } catch (e) {
      setError(String(e));
    }
  }

  async function signWithP12() {
    if (!workPath || !p12Draft) return;
    const dest = await pickSignedDest();
    if (!dest) return;
    try {
      await invoke("sign_pdf_p12", {
        workPath,
        destPath: dest,
        p12Path: p12Draft.path,
        password: p12Draft.password,
        reason: null,
      });
      setP12Draft(null);
    } catch (e) {
      setError(String(e));
    }
  }

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
  const fileName = originalPath?.split(/[\\/]/).pop() ?? null;

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
      hint: "Cambia el texto que hay en el PDF",
    },
    {
      id: "image",
      icon: "image",
      label: "Imagen",
      hint: "Insertar imágenes (clic en zona libre) o editar las existentes (arrastrar mueve, tirador redimensiona, clic abre opciones)",
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
    <div className="app">
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
                title={modified ? `Guardar (${MOD}S)` : "Sin cambios que guardar"}
                aria-label="Guardar"
                disabled={!modified}
                onClick={saveFile}
              >
                <Icon name="save" size={14} />
                <span className="btn-etiqueta">Guardar</span>
              </button>
              <MenuAcciones
                recientes={recientes}
                abrirReciente={abrirComprobando}
                abierto={menuOpen}
                onToggle={() => setMenuOpen((o) => !o)}
                onCerrar={() => setMenuOpen(false)}
                saveFileAs={saveFileAs}
                closeDocument={closeDocument}
                addPdf={addPdf}
                abrirExtraer={() => setExtraerOpen(true)}
                insertPdfHere={insertPdfHere}
                recortarPagina={() => {
                  selectMode("select");
                  setMode("crop");
                }}
                abrirMarcaAgua={() => setWmOpen(true)}
                abrirEncabezado={() => setHfOpen(true)}
                askRemoveMarginal={askRemoveMarginal}
                openProperties={openProperties}
                signPdf={signPdf}
                abrirProteger={() => setProtectDraft({ user: "", owner: "" })}
                abrirAplanar={() => setFlattenAsk(true)}
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
                abrirPreferencias={() => setPrefsDraft(cargaPreferencias())}
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
      {notice && (
        <div className={`banner-notice${noticeSaliendo ? " saliendo" : ""}`}>
          <p title={notice}>{notice}</p>
          <button className="btn btn-icon" aria-label="Cerrar el aviso" onClick={() => setNotice(null)}>
            <Icon name="close" size={13} />
          </button>
        </div>
      )}

      {p12Draft && (
        <DialogoContrasena
          titulo="Contraseña del .p12"
          fichero={p12Draft.path}
          valor={p12Draft.password}
          onChange={(v) => setP12Draft({ ...p12Draft, password: v })}
          onConfirm={signWithP12}
          onClose={() => setP12Draft(null)}
          etiqueta="Firmar"
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
      {prefsDraft && (
        <DialogoPreferencias
          initial={prefsDraft}
          onClose={() => setPrefsDraft(null)}
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
          onClose={() => setProtectDraft(null)}
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
              Se eliminarán {marginalAsk.textos} texto(s)
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
          titulo={`Has soltado ${dropAsk.length} PDF`}
          cuerpo={
            <p className="modal-file" style={{ whiteSpace: "normal" }}>
              Puedes abrir solo el primero ({dropAsk[0].split(/[\\/]/).pop()})
              o unirlos todos en un documento nuevo, en el orden en que los
              has soltado.
            </p>
          }
          textoConfirmar="Unirlos en uno"
          secundario={{
            texto: "Abrir el primero",
            onClick: () => {
              const primero = dropAsk[0];
              setDropAsk(null);
              abrirComprobando(primero);
            },
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
      {mode === "redact" && (
        <div className="sign-hint">
          Arrastra sobre el área a censurar: el contenido se ELIMINA de verdad
          · Esc cancela
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
      {printPages && (
        <div className="print-pages">
          {printPages.map((src, i) => (
            <img key={i} src={src} alt={`Página ${i + 1}`} />
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
      />

      <div className="body">
        {pageCount > 0 && sidebarVisible && (
          <aside className="sidebar">
            <div className="sidebar-tabs">
              <button
                className={`btn${sidebarTab === "paginas" ? " on" : ""}`}
                title="Páginas"
                onClick={() => setSidebarTab("paginas")}
              >
                Páginas
              </button>
              <button
                className={`btn${sidebarTab === "marcadores" ? " on" : ""}`}
                title="Marcadores"
                onClick={() => setSidebarTab("marcadores")}
              >
                Marcadores
              </button>
              <button
                className={`btn${sidebarTab === "comentarios" ? " on" : ""}`}
                title="Comentarios"
                onClick={() => setSidebarTab("comentarios")}
              >
                Comentarios
              </button>
            </div>
            {sidebarTab === "comentarios" && (
              <PanelComentarios
                comentarios={comentarios}
                filtro={filtroComentarios}
                setFiltro={setFiltroComentarios}
                seleccionada={annotSel}
                onSelect={irAComentario}
                onDelete={borrarComentario}
              />
            )}
            {sidebarTab === "marcadores" && (
              <PanelMarcadores
                outline={outline}
                currentPage={pageIndex}
                onGoto={gotoPage}
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
            className={`viewer${arrastrando ? " arrastrando" : ""}`}
            ref={viewerRef}
            onScroll={onViewerScroll}
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
                          onClick={() => abrirComprobando(r.path)}
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
              pageSizes.slice(0, pageCount).map((size, i) => (
                <Pagina
                  key={i}
                  index={i}
                  workPath={workPath}
                  size={size}
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
                  onLinkGoto={gotoPage}
                  onLinkUri={onLinkUri}
                  onSigStamped={onSigStamped}
                />
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
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

export default App;
