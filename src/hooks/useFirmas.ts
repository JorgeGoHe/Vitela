import { useCallback, useEffect, useRef, useState } from "react";
import { open } from "../dialogos";
import {
  deleteStoredSignature,
  importSignatureFile,
  listStoredSignatures,
  saveStoredSignature,
  type FirmaGuardada,
} from "../api";
import { cargaIniciales, guardaIniciales, type Mode } from "../tipos";

/**
 * Biblioteca de firmas manuscritas y la firma activa lista para estampar.
 * La biblioteca se carga al entrar en modo firma; Esc cancela el estampado.
 *
 * Hay **dos ranuras**, como en Acrobat: la firma y las iniciales. La
 * biblioteca del backend guarda imágenes por nombre y no sabe de ranuras,
 * así que cuál es cuál se recuerda aquí (`localStorage`), como el resto de
 * la memoria de la interfaz. Las iniciales son lo que se estampa **en cada
 * página** de un contrato.
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
  const [iniciales, setIniciales] = useState<string[]>(() => cargaIniciales());

  /** Marca o desmarca una entrada de la biblioteca como «iniciales». */
  const marcaIniciales = useCallback((id: string, esInicial: boolean) => {
    setIniciales((v) => {
      const next = esInicial
        ? [...new Set([...v, id])]
        : v.filter((x) => x !== id);
      guardaIniciales(next);
      return next;
    });
  }, []);
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

  async function uploadSignature(ranura: "firma" | "iniciales" = "firma") {
    const sel = await open({
      filters: [
        {
          name: "Imagen",
          extensions: ["png", "jpg", "jpeg", "webp", "gif", "bmp"],
        },
      ],
      multiple: false,
      title:
        ranura === "iniciales"
          ? "Imagen de tus iniciales (PNG con transparencia funciona mejor)"
          : "Imagen de tu firma (PNG con transparencia funciona mejor)",
    });
    if (typeof sel !== "string") return;
    try {
      const f = await importSignatureFile(sel);
      setFirmas((l) => [f, ...l]);
      if (ranura === "iniciales") marcaIniciales(f.id, true);
      pickSignature(f);
    } catch (e) {
      opts.onError(e);
    }
  }

  async function saveDrawnSignature(
    name: string,
    png: string,
    ranura: "firma" | "iniciales" = "firma",
  ) {
    try {
      const f = await saveStoredSignature(name, png);
      setDrawingSig(false);
      setFirmas((l) => [f, ...l]);
      if (ranura === "iniciales") marcaIniciales(f.id, true);
      pickSignature(f);
    } catch (e) {
      opts.onError(e);
    }
  }

  async function removeSignature(id: string) {
    try {
      await deleteStoredSignature(id);
      setFirmas((l) => l.filter((f) => f.id !== id));
      marcaIniciales(id, false);
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
    /** Ids de la biblioteca que son iniciales, no la firma entera. */
    iniciales,
    marcaIniciales,
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
