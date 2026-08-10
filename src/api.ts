import { invoke } from "@tauri-apps/api/core";

/** Firma manuscrita guardada en la biblioteca del usuario. */
export type FirmaGuardada = {
  id: string;
  name: string;
  png_base64: string;
};

export function listStoredSignatures(): Promise<FirmaGuardada[]> {
  return invoke("list_stored_signatures");
}

export function saveStoredSignature(
  name: string,
  pngBase64: string,
): Promise<FirmaGuardada> {
  return invoke("save_stored_signature", { name, pngBase64 });
}

export function importSignatureFile(imagePath: string): Promise<FirmaGuardada> {
  return invoke("import_signature_file", { imagePath });
}

export function deleteStoredSignature(id: string): Promise<void> {
  return invoke("delete_stored_signature", { id });
}

export function stampSignature(args: {
  workPath: string;
  pageIndex: number;
  pngBase64: string;
  x: number;
  y: number;
  w: number;
  h: number;
}): Promise<void> {
  return invoke("stamp_signature", { ...args });
}
