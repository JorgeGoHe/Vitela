import { useCallback, useRef } from "react";
import { renderPageSrc } from "../api";

/**
 * Caché global de renders de página (blob URLs) con evicción LRU y
 * deduplicación de las peticiones en vuelo. Lo consumen las Paginas visibles;
 * App invalida por página o por completo tras cada mutación.
 */
export function useRenderCache(workPath: string | null, docVersion: number) {
  const pageCacheRef = useRef<Map<string, string>>(new Map());
  const inFlightRef = useRef<Map<string, Promise<string>>>(new Map());
  // el estado del documento en un ref para que requestRender sea estable
  const docRef = useRef({ workPath, docVersion });
  docRef.current = { workPath, docVersion };

  /** Saca una entrada del caché revocando su blob URL (no-op para data:). */
  const cacheEvict = useCallback((key: string) => {
    const src = pageCacheRef.current.get(key);
    if (src) URL.revokeObjectURL(src);
    pageCacheRef.current.delete(key);
  }, []);

  /** Guarda un render en el caché de páginas, con tope de entradas. El get
   *  de requestRender refresca la posición: evicción LRU de verdad, para que
   *  volver a un nivel de zoom anterior siga acertando. */
  const cachePut = useCallback(
    (key: string, src: string) => {
      const cache = pageCacheRef.current;
      cache.set(key, src);
      if (cache.size > 60) {
        const oldest = cache.keys().next().value;
        if (oldest) cacheEvict(oldest);
      }
    },
    [cacheEvict],
  );

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
    [cachePut],
  );

  /** Invalida todos los renders de una página (cualquier zoom o versión). */
  const evictPage = useCallback(
    (page: number) => {
      for (const key of [...pageCacheRef.current.keys()]) {
        if (key.split(":")[2] === String(page)) cacheEvict(key);
      }
    },
    [cacheEvict],
  );

  /** Vacía el caché entero liberando los blobs. */
  const evictAll = useCallback(() => {
    for (const key of [...pageCacheRef.current.keys()]) cacheEvict(key);
  }, [cacheEvict]);

  return { requestRender, cacheEvict, evictPage, evictAll };
}
