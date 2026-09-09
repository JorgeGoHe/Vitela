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
    orientaImagen,
    ordenaImagen,
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
          {/* girar, voltear y ordenar: lo que Acrobat pone en la barra de
              propiedades de una imagen seleccionada, sin submenús */}
          <div className="card-row">
            <button
              className="btn btn-icon"
              title="Girar 90° a la izquierda"
              aria-label="Girar 90° a la izquierda"
              onClick={() => orientaImagen(imagePopover, { rotate: -90 })}
            >
              <Icon name="rotate" size={13} espejo />
            </button>
            <button
              className="btn btn-icon"
              title="Girar 90° a la derecha"
              aria-label="Girar 90° a la derecha"
              onClick={() => orientaImagen(imagePopover, { rotate: 90 })}
            >
              <Icon name="rotate" size={13} />
            </button>
            <button
              className="btn btn-icon"
              title="Voltear en horizontal"
              aria-label="Voltear en horizontal"
              onClick={() => orientaImagen(imagePopover, { flipH: true })}
            >
              <Icon name="flipH" size={13} />
            </button>
            <button
              className="btn btn-icon"
              title="Voltear en vertical"
              aria-label="Voltear en vertical"
              onClick={() => orientaImagen(imagePopover, { flipV: true })}
            >
              <Icon name="flipV" size={13} />
            </button>
            <button
              className="btn btn-icon"
              title="Traer al frente"
              aria-label="Traer al frente"
              onClick={() => ordenaImagen(imagePopover, true)}
            >
              <Icon name="up" size={13} />
            </button>
            <button
              className="btn btn-icon"
              title="Enviar al fondo"
              aria-label="Enviar al fondo"
              onClick={() => ordenaImagen(imagePopover, false)}
            >
              <Icon name="down" size={13} />
            </button>
          </div>
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
