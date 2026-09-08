import { useEffect, useRef, useState } from "react";
import { createLink, getLinks, type LinkInfo } from "../../api";
import type { Mode, Rect } from "../../tipos";

/**
 * Enlaces de la página: las zonas clicables existentes (modo selección) y
 * el borrador del enlace nuevo (modo link-new) con su tarjeta URL/página.
 */
export function useEnlaces(ctx: {
  workPath: string;
  index: number;
  visible: boolean;
  docVersion: number;
  pageVersion: number;
  mode: Mode;
  pageCount: number;
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
    pageVersion,
    mode,
    pageCount,
    onPageMutated,
    onError,
    onModeChange,
    onLinkGoto,
    onLinkUri,
  } = ctx;
  const [links, setLinks] = useState<LinkInfo[]>([]);
  const [linkDraft, setLinkDraft] = useState<Rect | null>(null);
  const linkStartRef = useRef<{ x: number; y: number } | null>(null);
  const linkLiveRef = useRef<Rect | null>(null);
  const [linkTipo, setLinkTipo] = useState<"url" | "pagina">("url");
  const [linkValor, setLinkValor] = useState("");

  // Al cambiar de modo: fuera el borrador
  useEffect(() => {
    setLinkDraft(null);
    linkStartRef.current = null;
    linkLiveRef.current = null;
  }, [mode]);

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

  function onLinkClick(l: LinkInfo) {
    if (l.uri) {
      onLinkUri(l.uri);
    } else if (l.dest_page !== null) {
      onLinkGoto(l.dest_page);
    }
  }

  return {
    links,
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
