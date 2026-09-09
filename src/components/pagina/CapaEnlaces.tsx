/** Zonas de enlace clicables (modo selección) con su popover
 *  (Abrir / Ir a la página / Eliminar) y tarjeta del enlace nuevo. */
import { KIND_LABELS, type Mode } from "../../tipos";
import { clampCardLeft } from "../../hooks/pagina/geometria";
import type { Enlaces } from "../../hooks/pagina/useEnlaces";
import Icon from "../Icon";

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
    linkPopover,
    setLinkPopover,
    deleteLink,
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
              setLinkPopover((p) => (p === l ? null : l));
            }}
          />
        ))}
      {mode === "select" && linkPopover && (
        <div
          className="card link-card"
          style={{
            left: clampCardLeft(linkPopover.x * scale, displayWidth),
            top: (linkPopover.y + linkPopover.h) * scale + 6,
          }}
          onMouseDown={(e) => e.stopPropagation()}
        >
          <p>
            {KIND_LABELS.Link}:{" "}
            {linkPopover.uri ?? `página ${(linkPopover.dest_page ?? 0) + 1}`}
          </p>
          <div className="card-actions">
            <button
              className="btn btn-primary"
              onClick={() => onLinkClick(linkPopover)}
            >
              {linkPopover.uri
                ? "Abrir"
                : `Ir a la página ${(linkPopover.dest_page ?? 0) + 1}`}
            </button>
            <button
              className="btn btn-danger"
              onClick={() => deleteLink(linkPopover)}
            >
              <Icon name="trash" size={13} />
              Eliminar
            </button>
            <button className="btn" onClick={() => setLinkPopover(null)}>
              Cerrar
            </button>
          </div>
        </div>
      )}
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
