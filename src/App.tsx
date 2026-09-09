import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  useSyncExternalStore,
} from "react";
import { busyCount, invoke, subscribeBusy } from "./ipc";
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
  renderPageSrc,
  type HeaderFooter,
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
import { hexToRgba, type Mode, type PageSize } from "./tipos";
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
import DialogoPropiedades from "./components/DialogoPropiedades";
import DialogoContrasena from "./components/DialogoContrasena";
import DialogoProteger from "./components/DialogoProteger";
import DialogoConfirmar from "./components/DialogoConfirmar";
import DialogoExportar from "./components/DialogoExportar";
import DialogoComprimir from "./components/DialogoComprimir";
import "./App.css";

const BASE_WIDTH = 900;
/** Separación vertical entre páginas y padding superior del visor (px). */
const PAGE_GAP = 24;
const VIEWER_PAD_TOP = 28;

/** Tecla modificadora en los tooltips de atajos. */
const MOD = navigator.platform.startsWith("Mac") ? "⌘" : "Ctrl+";

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
  const [docVersion, setDocVersion] = useState(0);
  const [pageCount, setPageCount] = useState(0);
  const [pageSizes, setPageSizes] = useState<PageSize[]>([]);
  const [pageIndex, setPageIndex] = useState(0);
  const [zoom, setZoom] = useState<number | "ajuste">("ajuste");
  const [viewerW, setViewerW] = useState<number | null>(null);
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
  // ancla del scroll (página superior visible y fracción ya desplazada)
  // para conservar el punto de lectura al cambiar el zoom
  const scrollAnchorRef = useRef<{ page: number; frac: number } | null>(null);

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
  const [sidebarTab, setSidebarTab] = useState<"paginas" | "marcadores">(
    "paginas",
  );
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
  const [notice, setNotice] = useState<string | null>(null);
  const [outline, setOutlineState] = useState<OutlineNode[]>([]);
  const [propsDraft, setPropsDraft] = useState<Metadata | null>(null);
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

  async function openPath(path: string, password?: string) {
    try {
      setError(null);
      setThumbs([]);
      setPageSizes([]);
      busqueda.limpiar(true);
      setModified(false);
      const anterior = workPath;
      const info = await invoke<{
        page_count: number;
        work_path: string;
        had_password: boolean;
      }>("open_pdf", { path, password: password ?? null });
      // la copia de trabajo del documento anterior ya no sirve: borrarla
      if (anterior) invoke("close_document", { workPath: anterior }).catch(() => {});
      setPwdDraft(null);
      setHadPassword(info.had_password);
      setDocPassword(info.had_password ? (password ?? null) : null);
      if (info.had_password) {
        setNotice(
          "Documento protegido: al guardar puedes mantener la contraseña o quitarla",
        );
      }
      setOriginalPath(path);
      setWorkPath(info.work_path);
      setPageCount(info.page_count);
      setPageIndex(0);
      setDocVersion((v) => v + 1);
      viewerRef.current?.scrollTo({ top: 0 });
      scrollAnchorRef.current = null;
    } catch (e) {
      if (String(e) === "PASSWORD_REQUIRED") {
        setPwdDraft({ path, password: "" });
        if (password !== undefined) setError("Contraseña incorrecta");
      } else {
        setError(String(e));
      }
    }
  }

  /** Cierra el documento: vuelve al estado vacío y borra la copia de trabajo. */
  function closeDocument() {
    if (!workPath) return;
    const anterior = workPath;
    setMenuOpen(false);
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
    evictAll();
    setDocVersion((v) => v + 1);
    invoke("close_document", { workPath: anterior }).catch((e) => setError(String(e)));
  }

  async function openFile() {
    const selected = await open({
      filters: [{ name: "PDF", extensions: ["pdf"] }],
      multiple: false,
    });
    if (typeof selected !== "string") return;
    await openPath(selected);
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

  // Esc sale de los modos de área (recorte, redacción, campo y enlace);
  // las páginas limpian sus borradores al cambiar el modo
  useEffect(() => {
    if (
      mode !== "crop" &&
      mode !== "redact" &&
      mode !== "form-new" &&
      mode !== "link-new"
    )
      return;
    function onKey(e: KeyboardEvent) {
      if (e.key === "Escape") setMode("select");
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
    const mide = () =>
      setViewerW(Math.max(320, Math.round(el.clientWidth / 16) * 16));
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
      const mod = e.metaKey || e.ctrlKey;
      const tag = (e.target as HTMLElement)?.tagName;
      const enCampo = tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT";
      if (mod && e.key === "o") {
        e.preventDefault();
        openFile();
      } else if (mod && e.key === "s") {
        e.preventDefault();
        if (modified) saveFile();
      } else if (mod && e.key === "f" && pageCount > 0) {
        e.preventDefault();
        (document.querySelector(".search input") as HTMLInputElement)?.focus();
      } else if (mod && (e.key === "+" || e.key === "=") && pageCount > 0) {
        e.preventDefault();
        setZoom(Math.min(4, Math.round((zoomNum + 0.25) * 4) / 4));
      } else if (mod && e.key === "-" && pageCount > 0) {
        e.preventDefault();
        setZoom(Math.max(0.5, Math.round((zoomNum - 0.25) * 4) / 4));
      } else if (mod && !enCampo && (e.key === "z" || e.key === "Z") && pageCount > 0) {
        e.preventDefault();
        if (e.shiftKey) historial.rehacer();
        else historial.deshacer();
      } else if (mod && !enCampo && e.key === "y" && pageCount > 0) {
        e.preventDefault();
        historial.rehacer();
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
  const PADDING_VIEWER = 48;
  const fitWidth = viewerW ? Math.max(320, viewerW - PADDING_VIEWER) : BASE_WIDTH;
  const displayWidth = zoom === "ajuste" ? fitWidth : BASE_WIDTH * zoom;
  const ocupado = useSyncExternalStore(subscribeBusy, busyCount) > 0;

  const zoomNum = zoom === "ajuste" ? displayWidth / BASE_WIDTH : zoom;

  const registerEl = useCallback((page: number, el: HTMLDivElement | null) => {
    if (el) pageElsRef.current.set(page, el);
    else pageElsRef.current.delete(page);
  }, []);

  /** Alturas en pantalla de cada página con el ancho dado. */
  function alturasPagina(width: number): number[] {
    return pageSizes.map((s) => (width * s.height) / s.width);
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

  const busqueda = useBusqueda({
    workPath,
    gotoPage,
    onError: (e) => setError(String(e)),
  });
  const limpiarBusqueda = busqueda.limpiar;

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
      let anchor: { page: number; frac: number } | null = null;
      for (let i = 0; i < alturas.length; i++) {
        const h = alturas[i];
        if (anchor === null && y + h > el.scrollTop) {
          anchor = { page: i, frac: Math.max(0, (el.scrollTop - y) / h) };
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
  // lectura: misma página superior y misma fracción desplazada.
  const prevWidthRef = useRef(displayWidth);
  useLayoutEffect(() => {
    if (prevWidthRef.current === displayWidth) return;
    prevWidthRef.current = displayWidth;
    const el = viewerRef.current;
    const a = scrollAnchorRef.current;
    if (!el || !a || pageSizes.length === 0) return;
    const alturas = alturasPagina(displayWidth);
    let y = VIEWER_PAD_TOP;
    for (let i = 0; i < a.page && i < alturas.length; i++) {
      y += alturas[i] + PAGE_GAP;
    }
    el.scrollTop = y + a.frac * (alturas[a.page] ?? 0);
    // solo debe correr cuando cambia el ancho (zoom): alturasPagina y
    // pageSizes se leen del render actual a propósito
    // eslint-disable-next-line react-hooks/exhaustive-deps
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
      setNotice("Preparando la impresión…");
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
      setNotice("Exportando imágenes…");
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
      setNotice("Comprimiendo…");
      const r = await compressPdf(workPath, compressQuality, compressDpi);
      const mb = (n: number) => (n / 1024 / 1024).toFixed(2);
      setNotice(
        r.imagenes === 0
          ? "No había imágenes que comprimir."
          : `${r.imagenes} imagen(es) recomprimidas: ${mb(r.antes)} MB → ${mb(r.despues)} MB`,
      );
      afterMutation(pageCount);
    } catch (e) {
      setNotice(null);
      setError(String(e));
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

  async function extractCurrentPage() {
    if (!workPath) return;
    const dest = await save({
      filters: [{ name: "PDF", extensions: ["pdf"] }],
      defaultPath: `pagina-${pageIndex + 1}.pdf`,
      title: "Extraer página a un PDF nuevo",
    });
    if (!dest) return;
    try {
      await invoke("extract_pages", {
        workPath,
        pageIndices: [pageIndex],
        destPath: dest,
      });
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
        // a partir de aquí el fichero de `dest` va en claro
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
      id: "edit",
      icon: "textedit",
      label: "Editar",
      hint: "Editar un bloque de texto o añadir texto nuevo (clic en zona libre)",
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
          <button className="btn" onClick={openFile}>
            <Icon name="open" />
            <span className="btn-etiqueta">Abrir</span>
          </button>
          {pageCount > 0 && (
            <button
              className="btn btn-icon"
              title={sidebarVisible ? "Ocultar el panel lateral" : "Mostrar el panel lateral"}
              aria-label={sidebarVisible ? "Ocultar el panel lateral" : "Mostrar el panel lateral"}
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
                ["draw", "note", "shape", "stamp"],
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
                disabled={!modified}
                onClick={saveFile}
              >
                <Icon name="save" size={14} />
                <span className="btn-etiqueta">Guardar</span>
              </button>
              <MenuAcciones
                abierto={menuOpen}
                onToggle={() => setMenuOpen((o) => !o)}
                onCerrar={() => setMenuOpen(false)}
                saveFileAs={saveFileAs}
                closeDocument={closeDocument}
                addPdf={addPdf}
                extractCurrentPage={extractCurrentPage}
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
        <div className="banner-notice">
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
          titulo="Aplanar anotaciones y formularios"
          cuerpo={
            <p className="modal-file" style={{ whiteSpace: "normal" }}>
              Los sellos, formas, trazos y campos rellenados pasan a ser
              contenido fijo de la página (ya no se podrán editar ni borrar).
              Ojo: los resaltados, subrayados y notas creados con esta app se
              perderán al aplanar.
            </p>
          }
          textoConfirmar="Aplanar"
          onConfirm={applyFlatten}
          onClose={() => setFlattenAsk(false)}
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
        cambiaColorAccion={herramienta.cambiaColorAccion}
      />

      <div className="body">
        {pageCount > 0 && sidebarVisible && (
          <aside className="sidebar">
            <div className="sidebar-tabs">
              <button
                className={`btn${sidebarTab === "paginas" ? " on" : ""}`}
                onClick={() => setSidebarTab("paginas")}
              >
                Páginas
              </button>
              <button
                className={`btn${sidebarTab === "marcadores" ? " on" : ""}`}
                onClick={() => setSidebarTab("marcadores")}
              >
                Marcadores
              </button>
            </div>
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
          <main className="viewer" ref={viewerRef} onScroll={onViewerScroll}>
            {!workPath && (
              <div className="placeholder">
                <p className="voz">Nada abierto todavía. El papel espera.</p>
                <button className="btn btn-primary" onClick={openFile}>
                  <Icon name="open" size={14} />
                  Abrir PDF
                </button>
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
                  devicePixelRatio={window.devicePixelRatio}
                  docVersion={docVersion}
                  annotVersion={annotVersion}
                  pageVersion={pageVersions[i] ?? 0}
                  mode={mode}
                  tool={tool}
                  matches={busqueda.matchesByPage.get(i)}
                  currentGroup={busqueda.matchIdx}
                  selOwner={selOwner}
                  claimSel={setSelOwner}
                  requestRender={requestRender}
                  registerEl={registerEl}
                  onAnnotated={afterAnnotate}
                  onPageMutated={afterPageMutation}
                  onDocMutated={afterMutation}
                  onError={mostrarError}
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
              <span>
                {pageIndex + 1} / {pageCount}
              </span>
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
                  setZoom(Math.max(0.5, Math.round((zoomNum - 0.25) * 4) / 4))
                }
              >
                <Icon name="minus" size={14} />
              </button>
              <span>{Math.round(zoomNum * 100)}%</span>
              <button
                className="btn btn-icon"
                title={`Ampliar (${MOD}+)`}
                aria-label="Ampliar"
                onClick={() =>
                  setZoom(Math.min(4, Math.round((zoomNum + 0.25) * 4) / 4))
                }
              >
                <Icon name="plus" size={14} />
              </button>
              <button
                className={`btn${zoom === "ajuste" ? " on" : ""}`}
                title="Ajustar la página al ancho de la ventana"
                aria-pressed={zoom === "ajuste"}
                onClick={() =>
                  setZoom((z) => (z === "ajuste" ? 1 : "ajuste"))
                }
              >
                Ajustar
              </button>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

export default App;
