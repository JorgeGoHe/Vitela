/**
 * Enlaces que vienen dentro del PDF: contenido no confiable. Solo se abren
 * esquemas web y correo, y siempre tras confirmar mostrando el dominio.
 */
const ESQUEMAS_PERMITIDOS = new Set(["http:", "https:", "mailto:"]);

export function esquemaDe(uri: string): string | null {
  try {
    return new URL(uri.trim()).protocol;
  } catch {
    return null;
  }
}

export function esquemaPermitido(uri: string): boolean {
  const e = esquemaDe(uri);
  return e !== null && ESQUEMAS_PERMITIDOS.has(e);
}

/** Lo que se enseña al usuario antes de abrir: dominio o dirección de correo. */
export function destinoDe(uri: string): string {
  try {
    const u = new URL(uri.trim());
    if (u.protocol === "mailto:") return u.pathname;
    return u.host || uri.trim();
  } catch {
    return uri.trim();
  }
}
