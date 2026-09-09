/**
 * IPC con el backend: dentro de Tauri delega en su invoke; en un navegador
 * normal (sesión de QA contra el dev server de Vite) habla con el puente
 * HTTP de desarrollo (src-tauri/src/puente_dev.rs) en el puerto 1422.
 */
import { invoke as tauriInvoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";

export const hayTauri = "__TAURI_INTERNALS__" in window;

const PUENTE = "http://localhost:1422";

// Contador de comandos en vuelo para el indicador global de actividad.
// Los invokes marcados {background: true} (miniaturas, prefetch) no cuentan.
let enVuelo = 0;
const oyentes = new Set<() => void>();

export function subscribeBusy(cb: () => void): () => void {
  oyentes.add(cb);
  return () => {
    oyentes.delete(cb);
  };
}

export function busyCount(): number {
  return enVuelo;
}

function cambia(delta: number) {
  enVuelo += delta;
  oyentes.forEach((cb) => cb());
}

/**
 * Fichero que llega de fuera: doble clic en el Finder o el Explorador y
 * argumento de arranque (el backend emite `abrir-fichero`). En el navegador
 * de QA no hay eventos de Tauri: no hace nada.
 */
export function onAbrirFichero(cb: (path: string) => void): () => void {
  if (!hayTauri) return () => {};
  const pendiente = listen<{ path: string }>("abrir-fichero", (e) =>
    cb(e.payload.path),
  );
  return () => {
    pendiente.then((quitar) => quitar()).catch(() => {});
  };
}

/** Ventana de la app en el navegador de QA: no hay evento de cierre, se
 *  dispara a mano con `window.__vitelaCerrar()`. */
type VentanaQa = Window & { __vitelaCerrar?: () => void };

/**
 * El usuario intenta cerrar la ventana o salir de la app: el backend frena
 * el cierre y emite `cerrar-solicitado`; cuando la UI decide que se puede
 * cerrar llama al comando `confirmar_cierre`.
 */
export function onCerrarSolicitado(cb: () => void): () => void {
  if (!hayTauri) {
    (window as VentanaQa).__vitelaCerrar = cb;
    return () => {
      delete (window as VentanaQa).__vitelaCerrar;
    };
  }
  const pendiente = listen("cerrar-solicitado", () => cb());
  return () => {
    pendiente.then((quitar) => quitar()).catch(() => {});
  };
}

/**
 * Arrastrar y soltar ficheros sobre la ventana. Va por los eventos nativos
 * de Tauri porque el `drop` de HTML5 no trae la ruta del fichero: en el
 * navegador de QA el gesto no existe.
 */
export function onArrastreFicheros(h: {
  onEntra: () => void;
  onSale: () => void;
  onSuelta: (paths: string[]) => void;
}): () => void {
  if (!hayTauri) return () => {};
  const pendientes = [
    listen("tauri://drag-enter", () => h.onEntra()),
    listen("tauri://drag-leave", () => h.onSale()),
    listen<{ paths: string[] }>("tauri://drag-drop", (e) => {
      h.onSale();
      h.onSuelta(e.payload?.paths ?? []);
    }),
  ];
  return () => {
    for (const p of pendientes) p.then((quitar) => quitar()).catch(() => {});
  };
}

/**
 * Pantalla completa de la ventana (⌘L). En el navegador de QA no hay
 * ventana que agrandar: la app esconde igual su chrome, que es lo que se
 * puede probar ahí.
 */
export async function ponerPantallaCompleta(valor: boolean): Promise<void> {
  if (!hayTauri) return;
  await getCurrentWindow().setFullscreen(valor);
}

export async function invoke<T>(
  cmd: string,
  args?: Record<string, unknown>,
  opts?: { background?: boolean },
): Promise<T> {
  const cuenta = !opts?.background;
  if (cuenta) cambia(1);
  try {
    if (hayTauri) return await tauriInvoke<T>(cmd, args);
    const res = await fetch(`${PUENTE}/invoke/${cmd}`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(args ?? {}),
    });
    const data = await res.json();
    if (!res.ok) {
      // los catch de la app hacen String(e): lanzar el mensaje tal cual
      throw data.error ?? `Error ${res.status} en ${cmd}`;
    }
    return data as T;
  } finally {
    if (cuenta) cambia(-1);
  }
}
