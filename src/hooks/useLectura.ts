import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "../ipc";

/** El texto de una página tal como lo devuelve `get_page_text`. */
type PageText = { chars: { ch: string }[] };

/** ¿Hay síntesis de voz en este webview? En el navegador de QA y en algunas
 *  builds de WebKit no la hay, y se dice en vez de callarse. */
const HAY_VOZ =
  typeof window !== "undefined" && "speechSynthesis" in window;

/** La primera voz en español que tenga instalada el sistema. */
function vozEspanola(): SpeechSynthesisVoice | null {
  if (!HAY_VOZ) return null;
  const voces = window.speechSynthesis.getVoices();
  return voces.find((v) => v.lang.toLowerCase().startsWith("es")) ?? null;
}

/**
 * «Leer en voz alta» (H5): la síntesis del propio webview sobre el texto que
 * ya sabe extraer `get_page_text`, en su orden de lectura.
 *
 * Los tres controles de Acrobat —leer esta página, leer hasta el final y
 * parar— más pausa, que es lo que se quiere de verdad cuando suena el
 * teléfono. Lee **desde la página que se está leyendo**, no desde la
 * primera. Si el sistema no tiene ninguna voz en español se dice en una
 * línea, en vez de leer un documento español con acento inglés.
 */
export function useLectura(ctx: {
  workPath: string | null;
  pageCount: number;
  onNotice: (texto: string) => void;
  onPagina: (page: number) => void;
}) {
  const { workPath, pageCount, onNotice, onPagina } = ctx;
  const [leyendo, setLeyendo] = useState(false);
  const [pausada, setPausada] = useState(false);
  const [paginaLeida, setPaginaLeida] = useState<number | null>(null);
  // el lote de páginas que queda por leer; un ref porque lo consume la
  // cadena de `onend`, fuera del ciclo de render
  const colaRef = useRef<number[]>([]);
  const vivoRef = useRef(false);

  const parar = useCallback(() => {
    vivoRef.current = false;
    colaRef.current = [];
    if (HAY_VOZ) window.speechSynthesis.cancel();
    setLeyendo(false);
    setPausada(false);
    setPaginaLeida(null);
  }, []);

  /** Lee la página que toque y encadena la siguiente al terminar. */
  const siguiente = useCallback(async () => {
    if (!workPath || !vivoRef.current) return;
    const page = colaRef.current.shift();
    if (page === undefined) {
      parar();
      return;
    }
    setPaginaLeida(page);
    onPagina(page);
    let texto: string;
    try {
      const t = await invoke<PageText>("get_page_text", {
        path: workPath,
        pageIndex: page,
      });
      texto = t.chars.map((c) => c.ch).join("").trim();
    } catch {
      texto = "";
    }
    if (!vivoRef.current) return;
    if (!texto) {
      // una página sin texto (un escaneo) no interrumpe la lectura
      siguiente();
      return;
    }
    const frase = new SpeechSynthesisUtterance(texto);
    frase.lang = "es-ES";
    const voz = vozEspanola();
    if (voz) frase.voice = voz;
    frase.onend = () => {
      if (vivoRef.current) siguiente();
    };
    frase.onerror = () => parar();
    window.speechSynthesis.speak(frase);
  }, [workPath, onPagina, parar]);

  /** Empieza a leer desde `desde`; con `hastaElFinal`, sigue documento
   *  abajo, y si no, se para al acabar esa página. */
  const leer = useCallback(
    (desde: number, hastaElFinal: boolean) => {
      if (!workPath || pageCount === 0) return;
      if (!HAY_VOZ) {
        onNotice("Este sistema no trae voz: no se puede leer en voz alta");
        return;
      }
      if (!vozEspanola()) {
        onNotice(
          "No hay ninguna voz en español instalada; se leerá con la del sistema",
        );
      }
      window.speechSynthesis.cancel();
      colaRef.current = hastaElFinal
        ? Array.from({ length: pageCount - desde }, (_, i) => desde + i)
        : [desde];
      vivoRef.current = true;
      setLeyendo(true);
      setPausada(false);
      siguiente();
    },
    [workPath, pageCount, onNotice, siguiente],
  );

  const pausar = useCallback(() => {
    if (!HAY_VOZ || !vivoRef.current) return;
    if (window.speechSynthesis.paused) {
      window.speechSynthesis.resume();
      setPausada(false);
    } else {
      window.speechSynthesis.pause();
      setPausada(true);
    }
  }, []);

  // la lectura es del documento: cerrarlo o cambiarlo la calla
  useEffect(() => {
    parar();
  }, [workPath, parar]);

  // al salir de la app, que no se quede una voz hablando sola
  useEffect(() => () => parar(), [parar]);

  // ganchos de QA: la sesión del navegador no tiene menú nativo ni voz
  useEffect(() => {
    const w = window as unknown as Record<string, unknown>;
    w.__vitelaLeer = (desde: number, hastaElFinal: boolean) =>
      leer(desde, !!hastaElFinal);
    w.__vitelaPararLectura = () => parar();
    return () => {
      delete w.__vitelaLeer;
      delete w.__vitelaPararLectura;
    };
  }, [leer, parar]);

  return { leyendo, pausada, paginaLeida, leer, pausar, parar, hayVoz: HAY_VOZ };
}

export type Lectura = ReturnType<typeof useLectura>;
