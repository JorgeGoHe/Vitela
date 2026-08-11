/**
 * Diálogos del sistema: dentro de Tauri delegan en los plugins; en una
 * sesión de QA en navegador piden al puente la siguiente respuesta encolada
 * (POST http://localhost:1422/qa/dialogo con {"value": ...} la encola;
 * cola vacía → null = el usuario canceló).
 */
import {
  open as tauriOpen,
  save as tauriSave,
  type OpenDialogOptions,
  type SaveDialogOptions,
} from "@tauri-apps/plugin-dialog";
import { openUrl as tauriOpenUrl } from "@tauri-apps/plugin-opener";
import { hayTauri } from "./ipc";

async function siguienteRespuesta(): Promise<string | string[] | null> {
  const res = await fetch("http://localhost:1422/qa/dialogo/siguiente");
  const data = await res.json();
  return data.value ?? null;
}

export async function open(
  options?: OpenDialogOptions,
): Promise<string | string[] | null> {
  if (hayTauri) return tauriOpen(options) as Promise<string | string[] | null>;
  return siguienteRespuesta();
}

export async function save(
  options?: SaveDialogOptions,
): Promise<string | null> {
  if (hayTauri) return tauriSave(options);
  const v = await siguienteRespuesta();
  return Array.isArray(v) ? (v[0] ?? null) : v;
}

export async function openUrl(url: string): Promise<void> {
  if (hayTauri) return tauriOpenUrl(url);
  console.log(`[qa] openUrl: ${url}`);
}
