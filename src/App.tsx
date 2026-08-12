import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  useSyncExternalStore,
} from "react";
import { busyCount, invoke, subscribeBusy } from "./ipc";
import { open, save, openUrl } from "./dialogos";
import {
  addBlankPage,
  removeMarginalText,
  addHeaderFooter,
  addWatermark,
  deleteStoredSignature,
  duplicatePage,
  importSignatureFile,
  insertPdfAt,
  listStoredSignatures,
  renderPageSrc,
  saveStoredSignature,
  type FirmaGuardada,
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
import {
  ANNOT_COLORS,
  hexToRgba,
  NOMBRE_COLOR,
  type AnnotationInfo,
  type Mode,
  type PageSize,
  type SearchMatch,
  type ShapeKind,
  cargaColores,
  guardaColor,
} from "./tipos";
import Icon from "./components/Icon";
import Pagina, { type PageMatch, type ToolProps } from "./components/Pagina";
import PanelFirmas from "./components/PanelFirmas";
import DibujarFirma from "./components/DibujarFirma";
import DialogoMarcaAgua from "./components/DialogoMarcaAgua";
import DialogoEncabezado from "./components/DialogoEncabezado";
import PanelMarcadores from "./components/PanelMarcadores";
import DialogoPropiedades from "./components/DialogoPropiedades";
import "./App.css";

const BASE_WIDTH = 900;
const THUMB_WIDTH = 240;
/** Separación vertical entre páginas y padding superior del visor (px). */
const PAGE_GAP = 24;
const VIEWER_PAD_TOP = 28;

const SHAPE_COLORS = ANNOT_COLORS;
const STAMP_PRESETS = [
  "APROBADO",
  "BORRADOR",
  "CONFIDENCIAL",
  "REVISADO",
  "URGENTE",
];

type UndoEntry = { page: number };

function App() {
  const [originalPath, setOriginalPath] = useState<string | null>(null);
  const [workPath, setWorkPath] = useState<string | null>(null);
  const [modified, setModified] = useState(false);
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
  const [thumbs, setThumbs] = useState<(string | null)[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [menuOpen, setMenuOpen] = useState(false);

  const pageCacheRef = useRef<Map<string, string>>(new Map());
  const inFlightRef = useRef<Map<string, Promise<string>>>(new Map());
  const pageElsRef = useRef<Map<number, HTMLDivElement>>(new Map());
  const scrollRafRef = useRef<number | null>(null);
  // ancla del scroll (página superior visible y fracción ya desplazada)
  // para conservar el punto de lectura al cambiar el zoom
  const scrollAnchorRef = useRef<{ page: number; frac: number } | null>(null);

  const [mode, setMode] = useState<Mode>("select");
  const [annotVersion, setAnnotVersion] = useState(0);
  const [pageVersions, setPageVersions] = useState<number[]>([]);
  const [selOwner, setSelOwner] = useState<number | null>(null);
  const undoStackRef = useRef<UndoEntry[]>([]);
  const [p12Draft, setP12Draft] = useState<{
    path: string;
    password: string;
  } | null>(null);
  const [drawColor, setDrawColor] = useState(() => cargaColores().dibujo ?? "#c0392b");
  const [drawWidth, setDrawWidth] = useState(2);
  const [markupPending, setMarkupPending] = useState<string | null>(null);
  const [markupColors, setMarkupColors] = useState(() => {
    const c = cargaColores();
    return {
      resaltar: c.resaltar ?? "#f5c400",
      subrayar: c.subrayar ?? "#2ea043",
      tachar: c.tachar ?? "#c0392b",
    };
  });
  const [shapeKind, setShapeKind] = useState<ShapeKind>("rect");
  const [shapeColor, setShapeColor] = useState(() => cargaColores().forma ?? "#c0392b");
  const [shapeFill, setShapeFill] = useState(false);
  const [shapeWidth, setShapeWidth] = useState(2);
  const [stampText, setStampText] = useState(STAMP_PRESETS[0]);
  const [stampCustom, setStampCustom] = useState("");
  const [stampColor, setStampColor] = useState(() => cargaColores().sello ?? "#c0392b");
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
  const [firmas, setFirmas] = useState<FirmaGuardada[]>([]);
  const [activeSig, setActiveSig] = useState<{
    png: string;
    ratio: number;
  } | null>(null);
  const [drawingSig, setDrawingSig] = useState(false);

  const [query, setQuery] = useState("");
  const [lastQuery, setLastQuery] = useState("");
  const [matches, setMatches] = useState<SearchMatch[]>([]);
  const [matchIdx, setMatchIdx] = useState(0);
  const [searched, setSearched] = useState(false);

  async function openPath(path: string, password?: string) {
    try {
      setError(null);
      setThumbs([]);
      setPageSizes([]);
      setMatches([]);
      setSearched(false);
      setQuery("");
      setModified(false);
      const info = await invoke<{ page_count: number; work_path: string }>(
        "open_pdf",
        { path, password: password ?? null },
      );
      setPwdDraft(null);
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

  // Biblioteca de firmas al entrar en modo firma; Esc cancela el estampado
  useEffect(() => {
    if (mode !== "firmar") return;
    listStoredSignatures()
      .then(setFirmas)
      .catch((e) => setError(String(e)));
    function onKey(e: KeyboardEvent) {
      if (e.key === "Escape") {
        setActiveSig(null);
        setMode("select");
      }
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
      } else if (!mod && !enCampo && e.key === "ArrowRight") {
        gotoPage(pageIndex + 1);
      } else if (!mod && !enCampo && e.key === "ArrowLeft") {
        gotoPage(pageIndex - 1);
      }
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  });

  /** Saca una entrada del caché revocando su blob URL (no-op para data:). */
  function cacheEvict(key: string) {
    const src = pageCacheRef.current.get(key);
    if (src) URL.revokeObjectURL(src);
    pageCacheRef.current.delete(key);
  }

  /** Guarda un render en el caché de páginas, con tope de entradas. El get
   *  de requestRender refresca la posición: evicción LRU de verdad, para que
   *  volver a un nivel de zoom anterior siga acertando. */
  function cachePut(key: string, src: string) {
    const cache = pageCacheRef.current;
    cache.set(key, src);
    if (cache.size > 60) {
      const oldest = cache.keys().next().value;
      if (oldest) cacheEvict(oldest);
    }
  }

  // Ancho de página en pantalla: fijo por zoom numérico, o el ancho útil
  // del visor en modo "ajuste". El ancho de render (px físicos) es también
  // la clave del caché: unifica ambos modos.
  const PADDING_VIEWER = 48;
  const fitWidth = viewerW ? Math.max(320, viewerW - PADDING_VIEWER) : BASE_WIDTH;
  const displayWidth = zoom === "ajuste" ? fitWidth : BASE_WIDTH * zoom;
  const ocupado = useSyncExternalStore(subscribeBusy, busyCount) > 0;

  function cambiaColorAccion(
    accion: "dibujo" | "forma" | "sello",
    color: string,
  ) {
    guardaColor(accion, color);
    if (accion === "dibujo") setDrawColor(color);
    else if (accion === "forma") setShapeColor(color);
    else setStampColor(color);
  }

  const onMarkupUsed = useCallback(
    (kind: "highlight" | "underline" | "strikeout", color: string) => {
      const accion =
        kind === "highlight"
          ? "resaltar"
          : kind === "underline"
            ? "subrayar"
            : "tachar";
      guardaColor(accion, color);
      setMarkupColors((c) => ({ ...c, [accion]: color }));
      setMarkupPending(null);
    },
    [],
  );
  const zoomNum = zoom === "ajuste" ? displayWidth / BASE_WIDTH : zoom;

  // el estado del documento en un ref para que requestRender sea estable
  const docRef = useRef({ workPath, docVersion });
  docRef.current = { workPath, docVersion };

  /** Render de una página vía el caché global, con deduplicación de las
   *  peticiones en vuelo. Lo consumen las Paginas visibles. */
  const requestRender = useCallback(
    (page: number, width: number, pv: number): Promise<string> => {
      const { workPath, docVersion } = docRef.current;
      if (!workPath) return Promise.reject("Sin documento");
      const key = `${docVersion}:${pv}:${page}:${width}`;
      const cached = pageCacheRef.current.get(key);
      if (cached) {
        // refrescar la posición en el Map (LRU)
        pageCacheRef.current.delete(key);
        pageCacheRef.current.set(key, cached);
        return Promise.resolve(cached);
      }
      const enVuelo = inFlightRef.current.get(key);
      if (enVuelo) return enVuelo;
      const p = renderPageSrc(workPath, page, width)
        .then((src) => {
          cachePut(key, src);
          return src;
        })
        .finally(() => {
          inFlightRef.current.delete(key);
        });
      inFlightRef.current.set(key, p);
      return p;
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [],
  );

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
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [displayWidth]);

  // Miniaturas de la barra lateral (secuencial, en segundo plano)
  useEffect(() => {
    if (!workPath || pageCount === 0) return;
    let cancelled = false;
    // conservar las miniaturas viejas mientras llegan las nuevas (sin
    // parpadeo a placeholders); solo la primera carga parte de null
    setThumbs((t) => {
      const next = t.slice(0, pageCount);
      while (next.length < pageCount) next.push(null);
      return next;
    });
    (async () => {
      for (let i = 0; i < pageCount; i++) {
        if (cancelled) return;
        try {
          const src = await renderPageSrc(workPath, i, THUMB_WIDTH, {
            background: true,
          });
          if (cancelled) {
            URL.revokeObjectURL(src);
            return;
          }
          setThumbs((t) => {
            const next = [...t];
            const previa = next[i];
            if (previa) URL.revokeObjectURL(previa);
            next[i] = src;
            return next;
          });
        } catch {
          // miniatura fallida: se queda el placeholder
        }
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [workPath, pageCount, docVersion]);

  /** Refresca solo la miniatura de una página (tras anotar). */
  async function refreshThumb(page: number) {
    if (!workPath) return;
    try {
      const src = await renderPageSrc(workPath, page, THUMB_WIDTH, {
        background: true,
      });
      setThumbs((t) => {
        const next = [...t];
        const previa = next[page];
        if (previa) URL.revokeObjectURL(previa);
        next[page] = src;
        return next;
      });
    } catch {
      // la miniatura vieja sigue siendo razonable
    }
  }

  /** Tras anotar: invalidar el render de esa página sin recargar todo. */
  const afterAnnotate = useCallback(
    (page: number, pushUndo = true) => {
      if (pushUndo) undoStackRef.current.push({ page });
      setModified(true);
      for (const key of [...pageCacheRef.current.keys()]) {
        if (key.split(":")[2] === String(page)) cacheEvict(key);
      }
      setAnnotVersion((v) => v + 1);
      refreshThumb(page);
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [workPath],
  );

  async function undoAnnotation() {
    const action = undoStackRef.current.pop();
    if (!action || !workPath) return;
    try {
      const list = await invoke<AnnotationInfo[]>("get_annotations", {
        path: workPath,
        pageIndex: action.page,
      });
      if (list.length === 0) return;
      await invoke("remove_annotation", {
        workPath,
        pageIndex: action.page,
        annotIndex: list[list.length - 1].index,
      });
      afterAnnotate(action.page, false);
    } catch (e) {
      setError(String(e));
    }
  }

  /** Tras mutar UNA página: re-render y miniatura solo de esa página. */
  const afterPageMutation = useCallback(
    (page: number) => {
      setModified(true);
      setMatches([]);
      setSearched(false);
      setLastQuery("");
      setPageVersions((v) => {
        const next = [...v];
        next[page] = (next[page] ?? 0) + 1;
        return next;
      });
      for (const key of [...pageCacheRef.current.keys()]) {
        if (key.split(":")[2] === String(page)) cacheEvict(key);
      }
      refreshThumb(page);
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [workPath],
  );

  /** Tras mutar el documento: refrescar render, miniaturas y limpiar búsqueda. */
  const afterMutation = useCallback((newCount: number, nextPage?: number) => {
    setPageCount(newCount);
    setModified(true);
    setMatches([]);
    setSearched(false);
    setLastQuery("");
    setPageIndex((p) => Math.max(0, Math.min(nextPage ?? p, newCount - 1)));
    // el docVersion nuevo deja inservible todo el caché: liberar los blobs
    for (const key of [...pageCacheRef.current.keys()]) cacheEvict(key);
    setDocVersion((v) => v + 1);
  }, []);

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

  async function saveFile() {
    if (!workPath || !originalPath) return;
    try {
      await invoke("save_pdf", { workPath, destPath: originalPath });
      setModified(false);
    } catch (e) {
      setError(String(e));
    }
  }

  async function saveFileAs() {
    if (!workPath) return;
    const dest = await save({
      filters: [{ name: "PDF", extensions: ["pdf"] }],
      defaultPath: originalPath ?? "documento.pdf",
      title: "Guardar como",
    });
    if (!dest) return;
    try {
      await invoke("save_pdf", { workPath, destPath: dest });
      setOriginalPath(dest);
      setModified(false);
    } catch (e) {
      setError(String(e));
    }
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

  /** Activa una firma para estamparla (guarda su relación de aspecto). */
  function pickSignature(f: FirmaGuardada) {
    const img = new Image();
    img.onload = () =>
      setActiveSig({
        png: f.png_base64,
        ratio: img.height / Math.max(1, img.width),
      });
    img.src = `data:image/png;base64,${f.png_base64}`;
  }

  async function uploadSignature() {
    const sel = await open({
      filters: [
        {
          name: "Imagen",
          extensions: ["png", "jpg", "jpeg", "webp", "gif", "bmp"],
        },
      ],
      multiple: false,
      title: "Imagen de tu firma (PNG con transparencia funciona mejor)",
    });
    if (typeof sel !== "string") return;
    try {
      const f = await importSignatureFile(sel);
      setFirmas((l) => [f, ...l]);
      pickSignature(f);
    } catch (e) {
      setError(String(e));
    }
  }

  async function saveDrawnSignature(name: string, png: string) {
    try {
      const f = await saveStoredSignature(name, png);
      setDrawingSig(false);
      setFirmas((l) => [f, ...l]);
      pickSignature(f);
    } catch (e) {
      setError(String(e));
    }
  }

  async function removeSignature(id: string) {
    try {
      await deleteStoredSignature(id);
      setFirmas((l) => l.filter((f) => f.id !== id));
    } catch (e) {
      setError(String(e));
    }
  }

  async function runSearch() {
    if (!workPath) return;
    if (!query.trim()) {
      setMatches([]);
      setSearched(false);
      setLastQuery("");
      return;
    }
    try {
      const res = await invoke<SearchMatch[]>("search_pdf", {
        path: workPath,
        query,
      });
      setMatches(res);
      setMatchIdx(0);
      setSearched(true);
      setLastQuery(query);
      if (res.length > 0) gotoPage(res[0].page_index);
    } catch (e) {
      setError(String(e));
    }
  }

  function gotoMatch(delta: number) {
    if (matches.length === 0) return;
    const next = (matchIdx + delta + matches.length) % matches.length;
    setMatchIdx(next);
    gotoPage(matches[next].page_index);
  }

  // Coincidencias agrupadas por página, con su índice global para saber
  // cuál es la actual
  const matchesByPage = useMemo(() => {
    const m = new Map<number, PageMatch[]>();
    matches.forEach((match, i) => {
      const list = m.get(match.page_index) ?? [];
      list.push({ rects: match.rects, groupIndex: i });
      m.set(match.page_index, list);
    });
    return m;
  }, [matches]);

  const mostrarError = useCallback((e: unknown) => setError(String(e)), []);
  const onLinkUri = useCallback(
    (uri: string) => {
      openUrl(uri).catch((e) => setError(String(e)));
    },
    [],
  );
  const onSigStamped = useCallback(() => {
    setActiveSig(null);
    // la firma estampada es una imagen: el modo imagen permite retocarla
    setMode("image");
  }, []);

  // memoizado para que Pagina (React.memo) no re-renderice todas las páginas
  // en cada cambio de estado de App
  const tool: ToolProps = useMemo(
    () => ({
      drawColor,
      drawWidth,
      markupPending,
      onMarkupPending: setMarkupPending,
      markupColors,
      onMarkupUsed,
      shapeKind,
      shapeColor,
      shapeFill,
      shapeWidth,
      stampText,
      stampCustom,
      stampColor,
      activeSig,
    }),
    [
      drawColor,
      drawWidth,
      markupPending,
      markupColors,
      onMarkupUsed,
      shapeKind,
      shapeColor,
      shapeFill,
      shapeWidth,
      stampText,
      stampCustom,
      stampColor,
      activeSig,
    ],
  );

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
              <div className="search">
                <Icon name="search" size={13} />
                <input
                  type="text"
                  placeholder="Buscar"
                  value={query}
                  onChange={(e) => setQuery(e.target.value)}
                  onKeyDown={(e) => {
                    if (e.key !== "Enter") return;
                    if (searched && matches.length > 0 && query === lastQuery) {
                      gotoMatch(e.shiftKey ? -1 : 1);
                    } else {
                      runSearch();
                    }
                  }}
                />
                {searched && (
                  <>
                    <span className="match-count">
                      {matches.length > 0
                        ? `${matchIdx + 1}/${matches.length}`
                        : "0"}
                    </span>
                    {matches.length > 0 && (
                      <>
                        <button className="btn btn-icon" onClick={() => gotoMatch(-1)}>
                          <Icon name="up" size={13} />
                        </button>
                        <button className="btn btn-icon" onClick={() => gotoMatch(1)}>
                          <Icon name="down" size={13} />
                        </button>
                      </>
                    )}
                  </>
                )}
              </div>
              <button
                className="btn btn-icon"
                title="Deshacer la última anotación"
                onClick={undoAnnotation}
              >
                <Icon name="undo" />
              </button>
              <button
                className="btn btn-primary"
                disabled={!modified}
                onClick={saveFile}
              >
                <Icon name="save" size={14} />
                <span className="btn-etiqueta">Guardar</span>
              </button>
              <div className="menu-wrap">
                <button
                  className="btn btn-icon"
                  title="Más acciones"
                  onClick={() => setMenuOpen((o) => !o)}
                >
                  ⋯
                </button>
                {menuOpen && (
                  <>
                    <div
                      className="menu-backdrop"
                      onClick={() => setMenuOpen(false)}
                    />
                    <div className="menu">
                      <div className="menu-titulo">Archivo</div>
                      <button
                        className="btn"
                        onClick={() => {
                          setMenuOpen(false);
                          saveFileAs();
                        }}
                      >
                        <Icon name="save" size={14} />
                        Guardar como…
                      </button>
                      <button
                        className="btn"
                        onClick={() => {
                          setMenuOpen(false);
                          addPdf();
                        }}
                      >
                        <Icon name="merge" size={14} />
                        Añadir PDF…
                      </button>
                      <button
                        className="btn"
                        onClick={() => {
                          setMenuOpen(false);
                          extractCurrentPage();
                        }}
                      >
                        <Icon name="extract" size={14} />
                        Extraer página…
                      </button>
                      <button
                        className="btn"
                        onClick={() => {
                          setMenuOpen(false);
                          insertPdfHere();
                        }}
                      >
                        <Icon name="merge" size={14} />
                        Insertar PDF aquí…
                      </button>
                      <div className="menu-titulo">Documento</div>
                      <button
                        className="btn"
                        onClick={() => {
                          setMenuOpen(false);
                          selectMode("select");
                          setMode("crop");
                        }}
                      >
                        <Icon name="crop" size={14} />
                        Recortar página…
                      </button>
                      <button
                        className="btn"
                        onClick={() => {
                          setMenuOpen(false);
                          setWmOpen(true);
                        }}
                      >
                        <Icon name="water" size={14} />
                        Marca de agua…
                      </button>
                      <button
                        className="btn"
                        onClick={() => {
                          setMenuOpen(false);
                          setHfOpen(true);
                        }}
                      >
                        <Icon name="hf" size={14} />
                        Encabezado, pie y numeración…
                      </button>
                      <button
                        className="btn"
                        onClick={() => {
                          setMenuOpen(false);
                          askRemoveMarginal("watermark");
                        }}
                      >
                        <Icon name="water" size={14} />
                        Quitar marca de agua…
                      </button>
                      <button
                        className="btn"
                        onClick={() => {
                          setMenuOpen(false);
                          askRemoveMarginal("header");
                        }}
                      >
                        <Icon name="hf" size={14} />
                        Quitar encabezados y pies…
                      </button>
                      <button
                        className="btn"
                        onClick={() => {
                          setMenuOpen(false);
                          openProperties();
                        }}
                      >
                        <Icon name="doc" size={14} />
                        Propiedades del documento…
                      </button>
                      <div className="menu-titulo">Seguridad</div>
                      <button
                        className="btn"
                        onClick={() => {
                          setMenuOpen(false);
                          signPdf();
                        }}
                      >
                        <Icon name="sign" size={14} />
                        Firma digital (certificado)…
                      </button>
                      <button
                        className="btn"
                        onClick={() => {
                          setMenuOpen(false);
                          setProtectDraft({ user: "", owner: "" });
                        }}
                      >
                        <Icon name="lock" size={14} />
                        Proteger con contraseña…
                      </button>
                      <button
                        className="btn"
                        onClick={() => {
                          setMenuOpen(false);
                          setFlattenAsk(true);
                        }}
                      >
                        <Icon name="flatten" size={14} />
                        Aplanar anotaciones…
                      </button>
                      <button
                        className="btn"
                        onClick={() => {
                          setMenuOpen(false);
                          selectMode("select");
                          setMode("redact");
                        }}
                      >
                        <Icon name="redact" size={14} />
                        Redactar (censurar)…
                      </button>
                      <div className="menu-titulo">Insertar</div>
                      <button
                        className="btn"
                        onClick={() => {
                          setMenuOpen(false);
                          selectMode("select");
                          setMode("form-new");
                        }}
                      >
                        <Icon name="field" size={14} />
                        Añadir campo de formulario…
                      </button>
                      <button
                        className="btn"
                        onClick={() => {
                          setMenuOpen(false);
                          selectMode("select");
                          setMode("link-new");
                        }}
                      >
                        <Icon name="link" size={14} />
                        Añadir enlace…
                      </button>
                      <div className="menu-titulo">Salida</div>
                      <button
                        className="btn"
                        onClick={() => {
                          setMenuOpen(false);
                          printDocument();
                        }}
                      >
                        <Icon name="printer" size={14} />
                        Imprimir…
                      </button>
                      <button
                        className="btn"
                        onClick={() => {
                          setMenuOpen(false);
                          setExportOpen(true);
                        }}
                      >
                        <Icon name="image" size={14} />
                        Exportar como imágenes…
                      </button>
                      <button
                        className="btn"
                        onClick={() => {
                          setMenuOpen(false);
                          exportPlainText();
                        }}
                      >
                        <Icon name="extract" size={14} />
                        Exportar texto…
                      </button>
                      <button
                        className="btn"
                        onClick={() => {
                          setMenuOpen(false);
                          setCompressOpen(true);
                        }}
                      >
                        <Icon name="shrink" size={14} />
                        Reducir tamaño…
                      </button>
                    </div>
                  </>
                )}
              </div>
            </>
          )}
        </div>
      </header>

      {error && (
        <div className="banner-error">
          <p title={error}>{error}</p>
          <button className="btn btn-icon" onClick={() => setError(null)}>
            <Icon name="close" size={13} />
          </button>
        </div>
      )}
      {notice && (
        <div className="banner-notice">
          <p title={notice}>{notice}</p>
          <button className="btn btn-icon" onClick={() => setNotice(null)}>
            <Icon name="close" size={13} />
          </button>
        </div>
      )}

      {p12Draft && (
        <div className="modal-backdrop" onClick={() => setP12Draft(null)}>
          <div className="modal" onClick={(e) => e.stopPropagation()}>
            <h3>Contraseña del .p12</h3>
            <p className="modal-file">{p12Draft.path.split(/[\\/]/).pop()}</p>
            <input
              type="password"
              autoFocus
              placeholder="Contraseña"
              value={p12Draft.password}
              onChange={(e) =>
                setP12Draft({ ...p12Draft, password: e.target.value })
              }
              onKeyDown={(e) => {
                if (e.key === "Enter") signWithP12();
                if (e.key === "Escape") setP12Draft(null);
              }}
            />
            <div className="card-actions">
              <button className="btn" onClick={() => setP12Draft(null)}>
                Cancelar
              </button>
              <button className="btn btn-primary" onClick={signWithP12}>
                Firmar
              </button>
            </div>
          </div>
        </div>
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
        <div className="modal-backdrop" onClick={() => setPwdDraft(null)}>
          <div className="modal" onClick={(e) => e.stopPropagation()}>
            <h3>Documento protegido</h3>
            <p className="modal-file">{pwdDraft.path.split(/[\\/]/).pop()}</p>
            <input
              type="password"
              autoFocus
              placeholder="Contraseña del documento"
              value={pwdDraft.password}
              onChange={(e) =>
                setPwdDraft({ ...pwdDraft, password: e.target.value })
              }
              onKeyDown={(e) => {
                if (e.key === "Enter")
                  openPath(pwdDraft.path, pwdDraft.password);
                if (e.key === "Escape") setPwdDraft(null);
              }}
            />
            <div className="card-actions">
              <button className="btn" onClick={() => setPwdDraft(null)}>
                Cancelar
              </button>
              <button
                className="btn btn-primary"
                onClick={() => openPath(pwdDraft.path, pwdDraft.password)}
              >
                Abrir
              </button>
            </div>
          </div>
        </div>
      )}
      {protectDraft && (
        <div className="modal-backdrop" onClick={() => setProtectDraft(null)}>
          <div className="modal" onClick={(e) => e.stopPropagation()}>
            <h3>Proteger con contraseña</h3>
            <input
              type="password"
              autoFocus
              placeholder="Contraseña (necesaria para abrir)"
              value={protectDraft.user}
              onChange={(e) =>
                setProtectDraft({ ...protectDraft, user: e.target.value })
              }
            />
            <input
              type="password"
              placeholder="Contraseña de propietario (opcional)"
              value={protectDraft.owner}
              onChange={(e) =>
                setProtectDraft({ ...protectDraft, owner: e.target.value })
              }
            />
            <p className="modal-file">
              Cifrado AES-256. Se guarda como una copia protegida; si el
              documento va a llevar firma digital, fírmalo por separado.
            </p>
            <div className="card-actions">
              <button className="btn" onClick={() => setProtectDraft(null)}>
                Cancelar
              </button>
              <button
                className="btn btn-primary"
                disabled={!protectDraft.user}
                onClick={applyProtect}
              >
                Proteger…
              </button>
            </div>
          </div>
        </div>
      )}
      {marginalAsk && (
        <div className="modal-backdrop" onClick={() => setMarginalAsk(null)}>
          <div className="modal" onClick={(e) => e.stopPropagation()}>
            <h3>
              {marginalAsk.zona === "watermark"
                ? "Quitar la marca de agua"
                : "Quitar encabezados y pies"}
            </h3>
            <p className="modal-file" style={{ whiteSpace: "normal" }}>
              Se eliminarán {marginalAsk.textos} texto(s)
              {marginalAsk.zona === "watermark"
                ? " de marca de agua"
                : " de los márgenes superior e inferior"}{" "}
              en todo el documento.
            </p>
            <div className="card-actions">
              <button className="btn" onClick={() => setMarginalAsk(null)}>
                Cancelar
              </button>
              <button className="btn btn-danger" onClick={applyRemoveMarginal}>
                Quitar
              </button>
            </div>
          </div>
        </div>
      )}
      {flattenAsk && (
        <div className="modal-backdrop" onClick={() => setFlattenAsk(false)}>
          <div className="modal" onClick={(e) => e.stopPropagation()}>
            <h3>Aplanar anotaciones y formularios</h3>
            <p className="modal-file" style={{ whiteSpace: "normal" }}>
              Los sellos, formas, trazos y campos rellenados pasan a ser
              contenido fijo de la página (ya no se podrán editar ni borrar).
              Ojo: los resaltados, subrayados y notas creados con esta app se
              perderán al aplanar.
            </p>
            <div className="card-actions">
              <button className="btn" onClick={() => setFlattenAsk(false)}>
                Cancelar
              </button>
              <button className="btn btn-primary" onClick={applyFlatten}>
                Aplanar
              </button>
            </div>
          </div>
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
      {exportOpen && (
        <div className="modal-backdrop" onClick={() => setExportOpen(false)}>
          <div className="modal" onClick={(e) => e.stopPropagation()}>
            <h3>Exportar como imágenes</h3>
            <div className="card-row">
              <select
                className="size-select"
                value={exportFmt}
                onChange={(e) => setExportFmt(e.target.value as "png" | "jpeg")}
              >
                <option value="png">PNG</option>
                <option value="jpeg">JPEG</option>
              </select>
              <select
                className="size-select"
                value={exportDpi}
                onChange={(e) => setExportDpi(Number(e.target.value))}
              >
                {[96, 150, 200, 300].map((d) => (
                  <option key={d} value={d}>
                    {d} ppp
                  </option>
                ))}
              </select>
            </div>
            <p className="modal-file">Una imagen por página del documento.</p>
            <div className="card-actions">
              <button className="btn" onClick={() => setExportOpen(false)}>
                Cancelar
              </button>
              <button className="btn btn-primary" onClick={exportImages}>
                Elegir carpeta…
              </button>
            </div>
          </div>
        </div>
      )}
      {compressOpen && (
        <div className="modal-backdrop" onClick={() => setCompressOpen(false)}>
          <div className="modal" onClick={(e) => e.stopPropagation()}>
            <h3>Reducir tamaño del PDF</h3>
            <div className="card-row">
              <select
                className="size-select"
                title="Calidad JPEG"
                value={compressQuality}
                onChange={(e) => setCompressQuality(Number(e.target.value))}
              >
                <option value={60}>Calidad baja (más pequeño)</option>
                <option value={75}>Calidad media</option>
                <option value={85}>Calidad alta</option>
              </select>
              <select
                className="size-select"
                title="Resolución máxima"
                value={compressDpi}
                onChange={(e) => setCompressDpi(Number(e.target.value))}
              >
                {[110, 150, 200, 300].map((d) => (
                  <option key={d} value={d}>
                    {d} ppp máx.
                  </option>
                ))}
              </select>
            </div>
            <p className="modal-file" style={{ whiteSpace: "normal" }}>
              Recomprime las imágenes del documento (las que tienen
              transparencia se conservan tal cual). El texto no se toca.
            </p>
            <div className="card-actions">
              <button className="btn" onClick={() => setCompressOpen(false)}>
                Cancelar
              </button>
              <button className="btn btn-primary" onClick={applyCompress}>
                Comprimir
              </button>
            </div>
          </div>
        </div>
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
      {mode === "draw" && (
        <div className="tool-options">
          <span>Trazo</span>
          <div className="swatches">
            {SHAPE_COLORS.map((c) => (
              <button
                key={c}
                className={`swatch${drawColor === c ? " on" : ""}`}
                style={{ background: c }}
                title={NOMBRE_COLOR[c] ?? c}
                onClick={() => cambiaColorAccion("dibujo", c)}
              />
            ))}
            <label
              className={`swatch swatch-custom${
                !SHAPE_COLORS.includes(drawColor) ? " on" : ""
              }`}
              title="Color personalizado"
            >
              <input
                type="color"
                value={drawColor}
                onChange={(e) => cambiaColorAccion("dibujo", e.target.value)}
              />
            </label>
          </div>
          <select
            className="size-select"
            title="Grosor"
            value={drawWidth}
            onChange={(e) => setDrawWidth(Number(e.target.value))}
          >
            {[1, 2, 3, 5, 8].map((w) => (
              <option key={w} value={w}>
                {w} pt
              </option>
            ))}
          </select>
        </div>
      )}
      {mode === "shape" && (
        <div className="tool-options">
          <div className="segmented">
            {(
              [
                ["rect", "Rectángulo"],
                ["ellipse", "Elipse"],
                ["line", "Línea"],
                ["arrow", "Flecha"],
              ] as [ShapeKind, string][]
            ).map(([k, label]) => (
              <button
                key={k}
                className={`btn${shapeKind === k ? " on" : ""}`}
                onClick={() => setShapeKind(k)}
              >
                {label}
              </button>
            ))}
          </div>
          <div className="swatches">
            {SHAPE_COLORS.map((c) => (
              <button
                key={c}
                className={`swatch${shapeColor === c ? " on" : ""}`}
                style={{ background: c }}
                title={NOMBRE_COLOR[c] ?? c}
                onClick={() => cambiaColorAccion("forma", c)}
              />
            ))}
            <label
              className={`swatch swatch-custom${
                !SHAPE_COLORS.includes(shapeColor) ? " on" : ""
              }`}
              title="Color personalizado"
            >
              <input
                type="color"
                value={shapeColor}
                onChange={(e) => cambiaColorAccion("forma", e.target.value)}
              />
            </label>
          </div>
          <label
            className={`opt-check${
              shapeKind === "line" || shapeKind === "arrow" ? " disabled" : ""
            }`}
          >
            <input
              type="checkbox"
              checked={shapeFill}
              disabled={shapeKind === "line" || shapeKind === "arrow"}
              onChange={(e) => setShapeFill(e.target.checked)}
            />
            Relleno
          </label>
          <select
            className="size-select"
            title="Grosor"
            value={shapeWidth}
            onChange={(e) => setShapeWidth(Number(e.target.value))}
          >
            {[1, 2, 3, 5].map((s) => (
              <option key={s} value={s}>
                {s} pt
              </option>
            ))}
          </select>
        </div>
      )}
      {mode === "stamp" && (
        <div className="tool-options">
          <select
            className="size-select"
            value={stampText}
            onChange={(e) => setStampText(e.target.value)}
          >
            {STAMP_PRESETS.map((s) => (
              <option key={s} value={s}>
                {s}
              </option>
            ))}
            <option value="custom">Personalizado…</option>
          </select>
          {stampText === "custom" && (
            <input
              type="text"
              className="stamp-input"
              placeholder="Texto del sello"
              value={stampCustom}
              onChange={(e) => setStampCustom(e.target.value.toUpperCase())}
            />
          )}
          <div className="swatches">
            {["#c0392b", "#2743c0", "#2ea043", "#1d1c18"].map((c) => (
              <button
                key={c}
                className={`swatch${stampColor === c ? " on" : ""}`}
                style={{ background: c }}
                title={NOMBRE_COLOR[c] ?? c}
                onClick={() => cambiaColorAccion("sello", c)}
              />
            ))}
            <label
              className={`swatch swatch-custom${
                !["#c0392b", "#2743c0", "#2ea043", "#1d1c18"].includes(
                  stampColor,
                )
                  ? " on"
                  : ""
              }`}
              title="Color personalizado"
            >
              <input
                type="color"
                value={stampColor}
                onChange={(e) => cambiaColorAccion("sello", e.target.value)}
              />
            </label>
          </div>
          <span
            className="sello-preview"
            style={{ color: stampColor, borderColor: stampColor }}
          >
            {(stampText === "custom" ? stampCustom || "SELLO" : stampText)}
          </span>
          <span className="opt-hint">Clic en la página para colocarlo</span>
        </div>
      )}

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
            {sidebarTab === "paginas" &&
              thumbs.map((src, i) => (
              <div
                key={i}
                className={`thumb${i === pageIndex ? " active" : ""}`}
                onClick={() => gotoPage(i)}
              >
                {src ? (
                  <img
                    src={src}
                    draggable={false}
                    decoding="async"
                    alt={`Página ${i + 1}`}
                  />
                ) : (
                  <div className="thumb-placeholder" />
                )}
                <span className="thumb-num">{i + 1}</span>
                <div className="thumb-actions">
                  <button
                    title="Subir"
                    disabled={i === 0}
                    onClick={(e) => {
                      e.stopPropagation();
                      movePage(i, i - 1);
                    }}
                  >
                    <Icon name="up" size={13} />
                  </button>
                  <button
                    title="Bajar"
                    disabled={i === pageCount - 1}
                    onClick={(e) => {
                      e.stopPropagation();
                      movePage(i, i + 1);
                    }}
                  >
                    <Icon name="down" size={13} />
                  </button>
                  <button
                    title="Rotar 90°"
                    onClick={(e) => {
                      e.stopPropagation();
                      rotatePage(i);
                    }}
                  >
                    <Icon name="rotate" size={13} />
                  </button>
                  <button
                    title="Duplicar página"
                    onClick={(e) => {
                      e.stopPropagation();
                      duplicatePageAt(i);
                    }}
                  >
                    <Icon name="copy" size={13} />
                  </button>
                  <button
                    title="Página en blanco después"
                    onClick={(e) => {
                      e.stopPropagation();
                      blankPageAfter(i);
                    }}
                  >
                    <Icon name="plus" size={13} />
                  </button>
                  <button
                    title="Eliminar página"
                    disabled={pageCount <= 1}
                    onClick={(e) => {
                      e.stopPropagation();
                      deletePage(i);
                    }}
                  >
                    <Icon name="trash" size={13} />
                  </button>
                </div>
              </div>
            ))}
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
                  matches={matchesByPage.get(i)}
                  currentGroup={matchIdx}
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
                disabled={pageIndex >= pageCount - 1}
                onClick={() => gotoPage(pageIndex + 1)}
              >
                <Icon name="chevRight" size={14} />
              </button>
              <div className="sep" />
              <button
                className="btn btn-icon"
                onClick={() =>
                  setZoom(Math.max(0.5, Math.round((zoomNum - 0.25) * 4) / 4))
                }
              >
                <Icon name="minus" size={14} />
              </button>
              <span>{Math.round(zoomNum * 100)}%</span>
              <button
                className="btn btn-icon"
                onClick={() =>
                  setZoom(Math.min(4, Math.round((zoomNum + 0.25) * 4) / 4))
                }
              >
                <Icon name="plus" size={14} />
              </button>
              <button
                className={`btn${zoom === "ajuste" ? " on" : ""}`}
                title="Ajustar la página al ancho de la ventana"
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
