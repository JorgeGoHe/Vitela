import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "../../ipc";
import { createLink, getLinks, type LinkInfo } from "../../api";
import type { Mode, Rect } from "../../tipos";

/**
 * Enlaces de la página: las zonas clicables existentes (modo selección,
 * con su popover Abrir / Eliminar) y el borrador del enlace nuevo (modo
 * link-new) con su tarjeta URL/página.
 */
export function useEnlaces(ctx: {
  workPath: string;
  index: number;
  visible: boolean;
  docVersion: number;
  annotVersion: number;
  pageVersion: number;
  mode: Mode;
  pageCount: number;
  onAnnotated: (page: number) => void;
  onPageMutated: (page: number) => void;
  onError: (e: unknown) => void;
  onModeChange: (m: Mode) => void;
  onLinkGoto: (page: number) => void;
  onLinkUri: (uri: string) => void;
}) {
  const {
    workPath,
    index,
    visible,
    docVersion,
    annotVersion,
    pageVersion,
    mode,
    pageCount,
    onAnnotated,
    onPageMutated,
    onError,
    onModeChange,
    onLinkGoto,
    onLinkUri,
  } = ctx;
  const [links, setLinks] = useState<LinkInfo[]>([]);
  const [linkPopover, setLinkPopover] = useState<LinkInfo | null>(null);
  const [linkDraft, setLinkDraft] = useState<Rect | null>(null);
  const linkStartRef = useRef<{ x: number; y: number } | null>(null);
  const linkLiveRef = useRef<Rect | null>(null);
  const [linkTipo, setLinkTipo] = useState<"url" | "pagina">("url");
  const [linkValor, setLinkValor] = useState("");

  // Al cambiar de modo: fuera el borrador y el popover
  useEffect(() => {
    setLinkDraft(null);
    setLinkPopover(null);
    linkStartRef.current = null;
    linkLiveRef.current = null;
  }, [mode]);

  // El popover se cierra con Esc o con un clic fuera de su tarjeta
  useEffect(() => {
    if (!linkPopover) return;
    function onKey(e: KeyboardEvent) {
      if (e.key === "Escape") setLinkPopover(null);
    }
    function onDown(e: globalThis.MouseEvent) {
      const t = e.target as Element | null;
      if (t?.closest?.(".link-card")) return;
      setLinkPopover(null);
    }
    window.addEventListener("keydown", onKey);
    document.addEventListener("mousedown", onDown);
    return () => {
      window.removeEventListener("keydown", onKey);
      document.removeEventListener("mousedown", onDown);
    };
  }, [linkPopover]);

  // Enlaces de la página (zonas clicables en modo selección)
  useEffect(() => {
    setLinkPopover(null);
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
  }, [workPath, index, visible, docVersion, annotVersion, pageVersion]);

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

  /** Sigue el enlace (URI externa, previa confirmación en App, o página). */
  function onLinkClick(l: LinkInfo) {
    setLinkPopover(null);
    if (l.uri) {
      onLinkUri(l.uri);
    } else if (l.dest_page !== null) {
      onLinkGoto(l.dest_page);
    }
  }

  const deleteLink = useCallback(
    async (l: LinkInfo) => {
      if (!workPath) return;
      try {
        await invoke("remove_annotation", {
          workPath,
          pageIndex: index,
          annotIndex: l.annot_index,
        });
        setLinkPopover(null);
        onAnnotated(index);
      } catch (e) {
        onError(e);
      }
    },
    [workPath, index, onAnnotated, onError],
  );

  // Supr o Retroceso borran el enlace seleccionado
  useEffect(() => {
    const elegido = linkPopover;
    if (!elegido) return;
    function onKey(e: KeyboardEvent) {
      if (e.key !== "Delete" && e.key !== "Backspace") return;
      const tag = (e.target as HTMLElement)?.tagName;
      if (tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT") return;
      if (document.querySelector(".modal-backdrop")) return;
      e.preventDefault();
      if (elegido) deleteLink(elegido);
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [linkPopover, deleteLink]);

  return {
    links,
    linkPopover,
    setLinkPopover,
    deleteLink,
    linkDraft,
    setLinkDraft,
    linkStartRef,
    linkLiveRef,
    linkTipo,
    setLinkTipo,
    linkValor,
    setLinkValor,
    applyLink,
    onLinkClick,
  };
}

export type Enlaces = ReturnType<typeof useEnlaces>;
