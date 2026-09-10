/** Modo imagen: parche del arrastre, cajas de imagen con tiradores,
 *  popover y la capa de recorte. */
import type { MouseEvent } from "react";
import type { Mode, Rect } from "../../tipos";
import { cardTop, clampCardLeft } from "../../hooks/pagina/geometria";
import type { Imagenes } from "../../hooks/pagina/useImagenes";
import Icon from "../Icon";

type Props = {
  mode: Mode;
  imagenes: Imagenes;
  scale: number;
  displayWidth: number;
  displayHeight: number;
};

export default function CapaImagenes({
  mode,
  imagenes,
  scale,
  displayWidth,
  displayHeight,
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
    cropOf,
    setCropOf,
    cropRect,
    setCropRect,
    recortaImagen,
    guardarImagen,
    deleteImage,
    startImgAction,
  } = imagenes;

  /** El rectángulo que se dibuja encima de la imagen al recortar, en puntos
   *  de página: la capa se coloca sobre la caja de la imagen, así que el
   *  origen del gesto es esa esquina. */
  function puntoDeCorte(e: MouseEvent<HTMLDivElement>): Rect | null {
    if (!cropOf) return null;
    const caja = e.currentTarget.getBoundingClientRect();
    return {
      x: cropOf.x + (e.clientX - caja.left) / scale,
      y: cropOf.y + (e.clientY - caja.top) / scale,
      w: 0,
      h: 0,
    };
  }
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
      {mode === "image" && cropOf && (
        <div
          className="crop-capa"
          style={{
            left: cropOf.x * scale,
            top: cropOf.y * scale,
            width: cropOf.w * scale,
            height: cropOf.h * scale,
          }}
          onMouseDown={(e) => {
            e.stopPropagation();
            if (e.button !== 0) return;
            setCropRect(puntoDeCorte(e));
          }}
          onMouseMove={(e) => {
            if (!cropRect || !(e.buttons & 1)) return;
            e.stopPropagation();
            const p = puntoDeCorte(e);
            if (!p) return;
            setCropRect({
              x: Math.min(cropRect.x, p.x),
              y: Math.min(cropRect.y, p.y),
              w: Math.abs(p.x - cropRect.x),
              h: Math.abs(p.y - cropRect.y),
            });
          }}
          onMouseUp={(e) => {
            e.stopPropagation();
            if (cropRect && cropRect.w > 4 && cropRect.h > 4)
              recortaImagen(cropOf, cropRect);
            else {
              setCropOf(null);
              setCropRect(null);
            }
          }}
        >
          {cropRect && (
            <div
              className="crop-marco"
              style={{
                left: (cropRect.x - cropOf.x) * scale,
                top: (cropRect.y - cropOf.y) * scale,
                width: cropRect.w * scale,
                height: cropRect.h * scale,
              }}
            />
          )}
        </div>
      )}
      {imagePopover && (
        <div
          className="card"
          style={{
            left: clampCardLeft(imagePopover.x * scale, displayWidth),
            top: cardTop(
              imagePopover.y * scale,
              (imagePopover.y + imagePopover.h) * scale,
              displayHeight,
              150,
            ),
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
              title="Arrastra sobre la imagen la parte que quieres conservar"
              onClick={() => {
                setCropOf(imagePopover);
                setCropRect(null);
                setImagePopover(null);
              }}
            >
              <Icon name="crop" size={13} />
              Recortar
            </button>
            <button
              className="btn"
              onClick={() => replaceImagePick(imagePopover)}
            >
              <Icon name="image" size={13} />
              Reemplazar…
            </button>
            <button
              className="btn"
              onClick={() => guardarImagen(imagePopover)}
            >
              <Icon name="save" size={13} />
              Guardar como…
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
