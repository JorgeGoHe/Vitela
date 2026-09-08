/** Zonas de enlace clicables (modo selección) y tarjeta del enlace nuevo. */
import type { Mode } from "../../tipos";
import { clampCardLeft } from "../../hooks/pagina/geometria";
import type { Enlaces } from "../../hooks/pagina/useEnlaces";

type Props = {
  mode: Mode;
  enlaces: Enlaces;
  scale: number;
  displayWidth: number;
  pageCount: number;
};

export default function CapaEnlaces({
  mode,
  enlaces,
  scale,
  displayWidth,
  pageCount,
}: Props) {
  const {
    links,
    linkDraft,
    setLinkDraft,
    linkTipo,
    setLinkTipo,
    linkValor,
    setLinkValor,
    applyLink,
    onLinkClick,
  } = enlaces;
  return (
    <>
      {mode === "select" &&
        links.map((l, i) => (
          <div
            key={`lk${i}`}
            className="link-zone"
            title={l.uri ?? `Ir a la página ${(l.dest_page ?? 0) + 1}`}
            style={{
              left: l.x * scale,
              top: l.y * scale,
              width: l.w * scale,
              height: l.h * scale,
            }}
            onMouseDown={(e) => e.stopPropagation()}
            onClick={(e) => {
              e.stopPropagation();
              onLinkClick(l);
            }}
          />
        ))}
      {mode === "link-new" && linkDraft && (
        <>
          <div
            className="crop-rect"
            style={{
              left: linkDraft.x * scale,
              top: linkDraft.y * scale,
              width: linkDraft.w * scale,
              height: linkDraft.h * scale,
            }}
          />
          <div
            className="card crop-actions"
            style={{
              left: clampCardLeft(linkDraft.x * scale, displayWidth, 300),
              top: (linkDraft.y + linkDraft.h) * scale + 8,
            }}
            onMouseDown={(e) => e.stopPropagation()}
          >
            <div className="card-row">
              <select
                className="size-select"
                value={linkTipo}
                onChange={(e) => {
                  setLinkTipo(e.target.value as "url" | "pagina");
                  setLinkValor("");
                }}
              >
                <option value="url">URL</option>
                <option value="pagina">Página</option>
              </select>
              <input
                type={linkTipo === "url" ? "text" : "number"}
                className="stamp-input"
                autoFocus
                placeholder={
                  linkTipo === "url" ? "https://…" : `1-${pageCount}`
                }
                value={linkValor}
                onChange={(e) => setLinkValor(e.target.value)}
              />
            </div>
            <div className="card-actions">
              <button
                className="btn btn-primary"
                disabled={!linkValor.trim()}
                onClick={applyLink}
              >
                Crear enlace
              </button>
              <button className="btn" onClick={() => setLinkDraft(null)}>
                Cancelar
              </button>
            </div>
          </div>
        </>
      )}
    </>
  );
}
