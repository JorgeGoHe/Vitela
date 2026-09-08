/** Modo imagen: parche del arrastre, cajas de imagen con tiradores y popover. */
import type { Mode } from "../../tipos";
import { clampCardLeft } from "../../hooks/pagina/geometria";
import type { Imagenes } from "../../hooks/pagina/useImagenes";
import Icon from "../Icon";

type Props = {
  mode: Mode;
  imagenes: Imagenes;
  scale: number;
  displayWidth: number;
};

export default function CapaImagenes({
  mode,
  imagenes,
  scale,
  displayWidth,
}: Props) {
  const {
    images,
    imgPreviews,
    imgDraft,
    imagePopover,
    setImagePopover,
    imgPatch,
    replaceImagePick,
    deleteImage,
    startImgAction,
  } = imagenes;
  return (
    <>
      {mode === "image" && imgPatch && (
        <div
          className="img-patch"
          style={{
            left: imgPatch.rect.x * scale,
            top: imgPatch.rect.y * scale,
            width: imgPatch.rect.w * scale,
            height: imgPatch.rect.h * scale,
            background: imgPatch.color,
          }}
        />
      )}
      {mode === "image" &&
        images.map((im) => {
          const isDragging =
            imgDraft !== null && imgDraft.object_index === im.object_index;
          const b = isDragging ? imgDraft : im;
          const preview = imgPreviews[im.object_index];
          return (
            <div
              key={`im${im.object_index}`}
              className="image-box"
              style={{
                left: b.x * scale,
                top: b.y * scale,
                width: b.w * scale,
                height: b.h * scale,
              }}
              onMouseDown={(e) => startImgAction(e, im, "move")}
            >
              {isDragging && preview && (
                <img
                  className="image-preview"
                  src={`data:image/png;base64,${preview}`}
                  draggable={false}
                  alt=""
                />
              )}
              {(["nw", "n", "ne", "e", "se", "s", "sw", "w"] as const).map(
                (hd) => (
                  <div
                    key={hd}
                    className={`image-handle h-${hd}`}
                    title="Redimensionar (Shift: libre en esquinas)"
                    onMouseDown={(e) => startImgAction(e, im, "resize", hd)}
                  />
                ),
              )}
            </div>
          );
        })}
      {imagePopover && (
        <div
          className="card"
          style={{
            left: clampCardLeft(imagePopover.x * scale, displayWidth),
            top: (imagePopover.y + imagePopover.h) * scale + 6,
          }}
          onMouseDown={(e) => e.stopPropagation()}
        >
          <div className="card-actions">
            <button
              className="btn"
              onClick={() => replaceImagePick(imagePopover)}
            >
              <Icon name="image" size={13} />
              Reemplazar…
            </button>
            <button
              className="btn btn-danger"
              onClick={() => deleteImage(imagePopover)}
            >
              <Icon name="trash" size={13} />
              Eliminar
            </button>
            <button className="btn" onClick={() => setImagePopover(null)}>
              Cerrar
            </button>
          </div>
        </div>
      )}
    </>
  );
}
