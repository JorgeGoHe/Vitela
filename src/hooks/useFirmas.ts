import { useCallback, useEffect, useRef, useState } from "react";
import { open } from "../dialogos";
import {
  deleteStoredSignature,
  importSignatureFile,
  listStoredSignatures,
  saveStoredSignature,
  setSignatureSlot,
  type FirmaGuardada,
  type RanuraImagen,
} from "../api";
import type { Mode } from "../tipos";

/**
 * Biblioteca de firmas manuscritas y la firma activa lista para estampar.
 * La biblioteca se carga al entrar en modo firma; Esc cancela el estampado.
 *
 * Hay **tres ranuras**: la firma, las iniciales —lo que se estampa en cada
 * página de un contrato— y los sellos propios de la galería. Cuál es cuál
 * lo dice el backend (`ranura`), que es donde vive la biblioteca: en
 * `localStorage` se quedaba en esta máquina y no la conocía nadie más.
 */
export function useFirmas(opts: {
  mode: Mode;
  setMode: (m: Mode) => void;
  onError: (e: unknown) => void;
}) {
  const [firmas, setFirmas] = useState<FirmaGuardada[]>([]);
  const [activeSig, setActiveSig] = useState<{
    png: string;
    ratio: number;
  } | null>(null);
  const [drawingSig, setDrawingSig] = useState<false | "firma" | "iniciales">(
    false,
  );
  const optsRef = useRef(opts);
  optsRef.current = opts;

  /** Vuelve a leer la biblioteca del backend. La lista no es de esta
   *  sesión: se dibuja una firma, se importa una imagen o se cambia de
   *  ranura, y quien tenga Vitela abierta desde antes seguía viendo la de
   *  entonces —«todavía no hay ninguna guardada» con una guardada—. */
  const recargar = useCallback(() => {
    listStoredSignatures()
      .then(setFirmas)
      .catch((e) => optsRef.current.onError(e));
  }, []);

  // La biblioteca se carga al entrar en los dos modos que la usan —la firma
  // manuscrita y la galería de sellos—; Esc cancela el estampado
  useEffect(() => {
    if (opts.mode !== "firmar" && opts.mode !== "stamp") return;
    listStoredSignatures()
      .then(setFirmas)
      .catch((e) => optsRef.current.onError(e));
    function onKey(e: KeyboardEvent) {
      if (e.key === "Escape") {
        setActiveSig(null);
        optsRef.current.setMode("select");
      }
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [opts.mode]);

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

  const TITULOS: Record<RanuraImagen, string> = {
    firma: "Imagen de tu firma (PNG con transparencia funciona mejor)",
    iniciales: "Imagen de tus iniciales (PNG con transparencia funciona mejor)",
    sello: "Imagen del sello (PNG con transparencia funciona mejor)",
  };

  async function uploadSignature(ranura: RanuraImagen = "firma") {
    const sel = await open({
      filters: [
        {
          name: "Imagen",
          extensions: ["png", "jpg", "jpeg", "webp", "gif", "bmp"],
        },
      ],
      multiple: false,
      title: TITULOS[ranura],
    });
    if (typeof sel !== "string") return;
    try {
      const f = await importSignatureFile(sel, ranura);
      setFirmas((l) => [f, ...l]);
      pickSignature(f);
    } catch (e) {
      opts.onError(e);
    }
  }

  async function saveDrawnSignature(
    name: string,
    png: string,
    ranura: RanuraImagen = "firma",
  ) {
    try {
      const f = await saveStoredSignature(name, png, ranura);
      setDrawingSig(false);
      setFirmas((l) => [f, ...l]);
      pickSignature(f);
    } catch (e) {
      opts.onError(e);
    }
  }

  /** Mueve una imagen ya guardada a otra ranura. Hasta ahora la ranura se
   *  fijaba al guardarla y equivocarse obligaba a borrarla y repetir. */
  async function cambiarRanura(id: string, ranura: RanuraImagen) {
    try {
      await setSignatureSlot(id, ranura);
      setFirmas((l) => l.map((f) => (f.id === id ? { ...f, ranura } : f)));
    } catch (e) {
      opts.onError(e);
    }
  }

  async function removeSignature(id: string) {
    try {
      await deleteStoredSignature(id);
      setFirmas((l) => l.filter((f) => f.id !== id));
    } catch (e) {
      opts.onError(e);
    }
  }

  const onSigStamped = useCallback(() => {
    setActiveSig(null);
    // la firma estampada es una imagen: el modo imagen permite retocarla
    optsRef.current.setMode("image");
  }, []);

  return {
    /** La biblioteca de firmar: la firma entera y las iniciales. */
    firmas: firmas.filter((f) => f.ranura !== "sello"),
    /** Ids de la biblioteca que son iniciales, no la firma entera. */
    iniciales: firmas.filter((f) => f.ranura === "iniciales").map((f) => f.id),
    /** Los sellos propios de la galería («Mis sellos»). */
    sellos: firmas.filter((f) => f.ranura === "sello"),
    /** La biblioteca entera, sin filtrar por ranura. */
    biblioteca: firmas,
    activeSig,
    setActiveSig,
    drawingSig,
    setDrawingSig,
    pickSignature,
    uploadSignature,
    saveDrawnSignature,
    cambiarRanura,
    removeSignature,
    recargar,
    onSigStamped,
  };
}
