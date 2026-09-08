import Icon from "./Icon";

/** Lista de miniaturas del panel lateral con las acciones por página
 *  (mover, rotar, duplicar, página en blanco y eliminar). */
export default function PanelPaginas({
  thumbs,
  pageIndex,
  pageCount,
  gotoPage,
  movePage,
  rotatePage,
  duplicatePageAt,
  blankPageAfter,
  deletePage,
}: {
  thumbs: (string | null)[];
  pageIndex: number;
  pageCount: number;
  gotoPage: (i: number) => void;
  movePage: (from: number, to: number) => void;
  rotatePage: (i: number) => void;
  duplicatePageAt: (i: number) => void;
  blankPageAfter: (i: number) => void;
  deletePage: (i: number) => void;
}) {
  return (
    <>
      {thumbs.map((src, i) => (
        <div
          key={i}
          className={`thumb${i === pageIndex ? " active" : ""}`}
          onClick={() => gotoPage(i)}
        >
          {src ? (
            <img
              src={src}
              draggable={false}
              decoding="async"
              alt={`Página ${i + 1}`}
            />
          ) : (
            <div className="thumb-placeholder" />
          )}
          <span className="thumb-num">{i + 1}</span>
          <div className="thumb-actions">
            <button
              title="Subir"
              disabled={i === 0}
              onClick={(e) => {
                e.stopPropagation();
                movePage(i, i - 1);
              }}
            >
              <Icon name="up" size={13} />
            </button>
            <button
              title="Bajar"
              disabled={i === pageCount - 1}
              onClick={(e) => {
                e.stopPropagation();
                movePage(i, i + 1);
              }}
            >
              <Icon name="down" size={13} />
            </button>
            <button
              title="Rotar 90°"
              onClick={(e) => {
                e.stopPropagation();
                rotatePage(i);
              }}
            >
              <Icon name="rotate" size={13} />
            </button>
            <button
              title="Duplicar página"
              onClick={(e) => {
                e.stopPropagation();
                duplicatePageAt(i);
              }}
            >
              <Icon name="copy" size={13} />
            </button>
            <button
              title="Página en blanco después"
              onClick={(e) => {
                e.stopPropagation();
                blankPageAfter(i);
              }}
            >
              <Icon name="plus" size={13} />
            </button>
            <button
              title="Eliminar página"
              disabled={pageCount <= 1}
              onClick={(e) => {
                e.stopPropagation();
                deletePage(i);
              }}
            >
              <Icon name="trash" size={13} />
            </button>
          </div>
        </div>
      ))}
    </>
  );
}
