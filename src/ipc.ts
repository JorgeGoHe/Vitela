/**
 * IPC con el backend: dentro de Tauri delega en su invoke; en un navegador
 * normal (sesión de QA contra el dev server de Vite) habla con el puente
 * HTTP de desarrollo (src-tauri/src/puente_dev.rs) en el puerto 1422.
 */
import { invoke as tauriInvoke } from "@tauri-apps/api/core";

export const hayTauri = "__TAURI_INTERNALS__" in window;

const PUENTE = "http://localhost:1422";

export async function invoke<T>(
  cmd: string,
  args?: Record<string, unknown>,
): Promise<T> {
  if (hayTauri) return tauriInvoke<T>(cmd, args);
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
}
