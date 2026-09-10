import { useCallback, useEffect, useRef, useState } from "react";
import { cancelSearch, searchFolder, type GrupoCarpeta } from "../api";
import { open } from "../dialogos";
import { onBuscandoCarpeta, type ProgresoCarpeta } from "../ipc";
import {
  cargaCarpetaBusqueda,
  guardaCarpetaBusqueda,
  type OpcionesBusqueda,
} from "../tipos";

/** Dónde se busca: en el documento de delante o en una carpeta entera. */
export type AmbitoBusqueda = "documento" | "carpeta";

/**
 * Buscar en todos los PDF de una carpeta, que es la búsqueda avanzada de
 * Acrobat (⇧⌘F). Vive aparte de `useBusqueda` porque no es la misma
 * búsqueda: sus resultados son de otros ficheros, no se pintan sobre la
 * página y sobreviven a abrir un documento.
 *
 * Nada de modal: se busca desde el mismo campo de siempre, con el contador
 * honesto de DESIGN.md mientras tanto y con Cancelar, que **deja en pantalla
 * lo encontrado hasta ahí**.
 */
export function useBusquedaCarpeta(opts: { onError: (e: unknown) => void }) {
  const [ambito, setAmbito] = useState<AmbitoBusqueda>("documento");
  const [carpeta, setCarpeta] = useState<string | null>(() =>
    cargaCarpetaBusqueda(),
  );
  const [recursivo, setRecursivo] = useState(false);
  const [buscando, setBuscando] = useState(false);
  const [progreso, setProgreso] = useState<ProgresoCarpeta | null>(null);
  const [grupos, setGrupos] = useState<GrupoCarpeta[]>([]);
  const [termino, setTermino] = useState("");
  const [hecho, setHecho] = useState(false);
  const onErrorRef = useRef(opts.onError);
  onErrorRef.current = opts.onError;

  useEffect(() => onBuscandoCarpeta(setProgreso), []);

  /** Elige la carpeta con el diálogo del sistema y la recuerda. */
  const elegirCarpeta = useCallback(async (): Promise<string | null> => {
    const sel = await open({
      directory: true,
      multiple: false,
      title: "Buscar en esta carpeta",
    });
    if (typeof sel !== "string") return null;
    setCarpeta(sel);
    guardaCarpetaBusqueda(sel);
    return sel;
  }, []);

  /** Lanza la búsqueda; si todavía no hay carpeta elegida, la pide. */
  async function buscar(query: string, opciones: OpcionesBusqueda) {
    if (!query.trim()) return;
    const dir = carpeta ?? (await elegirCarpeta());
    if (!dir) return;
    setBuscando(true);
    setHecho(false);
    setGrupos([]);
    setProgreso(null);
    setTermino(query);
    try {
      // el contexto se pide siempre: la lista de una búsqueda en carpeta es
      // lo único que se ve de esos ficheros, y sin la frase no dice nada
      const res = await searchFolder(
        dir,
        query,
        opciones.matchCase,
        opciones.wholeWord,
        true,
        recursivo,
      );
      setGrupos(res);
      setHecho(true);
    } catch (e) {
      onErrorRef.current(e);
    } finally {
      setBuscando(false);
      setProgreso(null);
    }
  }

  /** Parar: lo encontrado hasta ahora se queda: cancelar no tira el trabajo. */
  async function cancelar() {
    try {
      await cancelSearch();
    } catch (e) {
      onErrorRef.current(e);
    }
  }

  const conCoincidencias = grupos.filter((g) => g.coincidencias.length > 0);
  const ilegibles = grupos.filter((g) => !!g.error);

  return {
    ambito,
    setAmbito,
    carpeta,
    elegirCarpeta,
    recursivo,
    setRecursivo,
    buscando,
    progreso,
    /** Solo los ficheros con algo que enseñar. */
    grupos: conCoincidencias,
    /** Los que no se han podido abrir (cifrados o rotos). */
    ilegibles,
    /** Cuántos ficheros se han mirado en total. */
    mirados: grupos.length,
    termino,
    hecho,
    buscar,
    cancelar,
  };
}

export type BusquedaCarpeta = ReturnType<typeof useBusquedaCarpeta>;
