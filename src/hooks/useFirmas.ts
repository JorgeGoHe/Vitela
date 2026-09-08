import { useCallback, useEffect, useRef, useState } from "react";
import { open } from "../dialogos";
import {
  deleteStoredSignature,
  importSignatureFile,
  listStoredSignatures,
  saveStoredSignature,
  type FirmaGuardada,
} from "../api";
import type { Mode } from "../tipos";

/**
 * Biblioteca de firmas manuscritas y la firma activa lista para estampar.
 * La biblioteca se carga al entrar en modo firma; Esc cancela el estampado.
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
  const [drawingSig, setDrawingSig] = useState(false);
  const optsRef = useRef(opts);
  optsRef.current = opts;

  // Biblioteca de firmas al entrar en modo firma; Esc cancela el estampado
  useEffect(() => {
    if (opts.mode !== "firmar") return;
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
      opts.onError(e);
    }
  }

  async function saveDrawnSignature(name: string, png: string) {
    try {
      const f = await saveStoredSignature(name, png);
      setDrawingSig(false);
      setFirmas((l) => [f, ...l]);
      pickSignature(f);
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
  };
}
