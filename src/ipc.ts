/**
 * IPC con el backend: dentro de Tauri delega en su invoke; en un navegador
 * normal (sesión de QA contra el dev server de Vite) habla con el puente
 * HTTP de desarrollo (src-tauri/src/puente_dev.rs) en el puerto 1422.
 */
import { invoke as tauriInvoke } from "@tauri-apps/api/core";

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
