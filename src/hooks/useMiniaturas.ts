import { useCallback, useEffect, useState } from "react";
import { renderPageSrc } from "../api";

const THUMB_WIDTH = 240;

/**
 * Miniaturas de la barra lateral: se renderizan en secuencia y en segundo
 * plano al abrir o mutar el documento; `refreshThumb` rehace solo una.
 */
export function useMiniaturas(
  workPath: string | null,
  pageCount: number,
  docVersion: number,
) {
  const [thumbs, setThumbs] = useState<(string | null)[]>([]);

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
  const refreshThumb = useCallback(async (page: number) => {
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
  }, [workPath]);

  return { thumbs, setThumbs, refreshThumb };
}
