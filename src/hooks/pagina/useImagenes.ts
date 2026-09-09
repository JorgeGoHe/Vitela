import { useEffect, useRef, useState, type MouseEvent, type RefObject } from "react";
import { invoke } from "../../ipc";
import { open } from "../../dialogos";
import { getImageData } from "../../api";
import type { ImageInfo, ImgAction, Mode, PageSize, Rect, ResizeHandle } from "../../tipos";
import { puntoAPagina, puntoEnCapa, rectAPagina, rectAVista } from "./geometria";

/**
 * Imágenes de la página (modo imagen): insertar, mover/redimensionar por
 * arrastre (con parche que tapa la copia quemada en el bitmap), reemplazar
 * y borrar. Los handlers de ratón viven en `Pagina`.
 */
export function useImagenes(ctx: {
  workPath: string;
  index: number;
  visible: boolean;
  docVersion: number;
  pageVersion: number;
  mode: Mode;
  scale: number;
  size: PageSize;
  viewRotation: number;
  wrapRef: RefObject<HTMLDivElement | null>;
  onPageMutated: (page: number) => void;
  onError: (e: unknown) => void;
}) {
  const {
    workPath,
    index,
    visible,
    docVersion,
    pageVersion,
    mode,
    scale,
    size,
    viewRotation,
    wrapRef,
    onPageMutated,
    onError,
  } = ctx;
  const [images, setImages] = useState<ImageInfo[]>([]);
  const [imgPreviews, setImgPreviews] = useState<Record<number, string>>({});
  const [imgDraft, setImgDraft] = useState<ImageInfo | null>(null);
  // espejo del borrador: en un arrastre en un solo frame el estado de React
  // aún no se ha re-renderizado al llegar el mouseup y imgDraft iría un
  // frame por detrás (como annotLiveRef y el resto de *LiveRef)
  const imgLiveRef = useRef<ImageInfo | null>(null);
  const [imagePopover, setImagePopover] = useState<ImageInfo | null>(null);
  const imgActionRef = useRef<ImgAction | null>(null);
  // parche que tapa la copia original (quemada en el bitmap) durante un
  // arrastre de imagen; se limpia cuando llega el bitmap actualizado
  const [imgPatch, setImgPatch] = useState<{ rect: Rect; color: string } | null>(null);

  // Al cambiar de modo: fuera borradores y estado transitorio
  useEffect(() => {
    setImagePopover(null);
    setImgDraft(null);
    imgLiveRef.current = null;
    imgActionRef.current = null;
    setImgPatch(null);
  }, [mode]);

  // Imágenes de la página (solo en modo imagen). Se precarga también su
  // contenido para que al arrastrar se mueva la imagen, no solo el recuadro.
  useEffect(() => {
    setImagePopover(null);
    imgActionRef.current = null;
    setImgPreviews({});
    if (!workPath || !visible || mode !== "image") {
      setImages([]);
      setImgDraft(null);
      setImgPatch(null);
      return;
    }
    let cancelled = false;
    invoke<ImageInfo[]>("get_images", { path: workPath, pageIndex: index })
      .then((lista) => {
        if (cancelled) return;
        // igual que el texto, las imágenes vienen en el espacio propio de
        // la página y el overlay las necesita en el de la vista
        const list = lista.map((im) => ({ ...im, ...rectAVista(im, size) }));
        setImages(list);
        // el borrador y el parche del arrastre aguantan hasta que llegan los
        // datos frescos: así no reaparece la copia vieja mientras se re-renderiza
        setImgDraft(null);
        setImgPatch(null);
        for (const im of list) {
          getImageData(workPath, index, im.object_index)
            .then((b64) => {
              if (!cancelled)
                setImgPreviews((p) => ({ ...p, [im.object_index]: b64 }));
            })
            .catch(() => {
              // sin vista previa: al arrastrar se verá solo el recuadro
            });
        }
      })
      .catch((e) => {
        if (!cancelled) onError(e);
      });
    return () => {
      cancelled = true;
    };
  }, [workPath, index, visible, docVersion, mode, pageVersion, size, onError]);

  async function insertImageAt(x: number, y: number) {
    if (!workPath) return;
    const sel = await open({
      filters: [
        {
          name: "Imagen",
          extensions: ["png", "jpg", "jpeg", "webp", "gif", "bmp"],
        },
      ],
      multiple: false,
      title: "Insertar imagen",
    });
    if (typeof sel !== "string") return;
    const p = puntoAPagina({ x, y }, size);
    try {
      await invoke("add_image", {
        workPath,
        pageIndex: index,
        imagePath: sel,
        x: p.x,
        y: p.y,
      });
      onPageMutated(index);
    } catch (e) {
      onError(e);
    }
  }

  async function commitImage(objectIndex: number, b: ImageInfo) {
    if (!workPath) return;
    const pr = rectAPagina(b, size);
    try {
      await invoke("transform_image", {
        workPath,
        pageIndex: index,
        objectIndex,
        x: pr.x,
        y: pr.y,
        w: pr.w,
        h: pr.h,
      });
      onPageMutated(index);
    } catch (e) {
      setImgDraft(null);
      setImgPatch(null);
      onError(e);
    }
  }

  /** Color medio del perímetro de un rect en el bitmap de la página, para
   *  tapar la copia original mientras se arrastra (fallback: blanco papel). */
  function sampleAround(r: Rect): string {
    try {
      const el = wrapRef.current?.querySelector("img.page") as HTMLImageElement | null;
      if (!el || !el.naturalWidth) return "#ffffff";
      const cw = Math.min(800, el.naturalWidth);
      const k = cw / size.width;
      const cv = document.createElement("canvas");
      cv.width = cw;
      cv.height = Math.round(el.naturalHeight * (cw / el.naturalWidth));
      const ctx = cv.getContext("2d", { willReadFrequently: true });
      if (!ctx) return "#ffffff";
      ctx.drawImage(el, 0, 0, cv.width, cv.height);
      const pad = 3 * k;
      const x0 = r.x * k - pad;
      const x1 = (r.x + r.w) * k + pad;
      const y0 = r.y * k - pad;
      const y1 = (r.y + r.h) * k + pad;
      const cx = (x0 + x1) / 2;
      const cy = (y0 + y1) / 2;
      let cr = 0;
      let cg = 0;
      let cb = 0;
      let n = 0;
      for (const [px, py] of [
        [cx, y0],
        [cx, y1],
        [x0, cy],
        [x1, cy],
        [x0, y0],
        [x1, y0],
        [x0, y1],
        [x1, y1],
      ]) {
        const xx = Math.round(Math.min(Math.max(px, 0), cv.width - 1));
        const yy = Math.round(Math.min(Math.max(py, 0), cv.height - 1));
        const d = ctx.getImageData(xx, yy, 1, 1).data;
        cr += d[0];
        cg += d[1];
        cb += d[2];
        n++;
      }
      return `rgb(${Math.round(cr / n)}, ${Math.round(cg / n)}, ${Math.round(cb / n)})`;
    } catch {
      return "#ffffff";
    }
  }

  async function replaceImagePick(im: ImageInfo) {
    if (!workPath) return;
    const sel = await open({
      filters: [
        {
          name: "Imagen",
          extensions: ["png", "jpg", "jpeg", "webp", "gif", "bmp"],
        },
      ],
      multiple: false,
      title: "Imagen de reemplazo",
    });
    if (typeof sel !== "string") return;
    try {
      await invoke("replace_image", {
        workPath,
        pageIndex: index,
        objectIndex: im.object_index,
        imagePath: sel,
      });
      setImagePopover(null);
      onPageMutated(index);
    } catch (e) {
      onError(e);
    }
  }

  async function deleteImage(im: ImageInfo) {
    if (!workPath) return;
    try {
      await invoke("delete_image", {
        workPath,
        pageIndex: index,
        objectIndex: im.object_index,
      });
      setImagePopover(null);
      onPageMutated(index);
    } catch (e) {
      onError(e);
    }
  }

  function startImgAction(
    e: MouseEvent<HTMLDivElement>,
    im: ImageInfo,
    kind: ImgAction["kind"],
    handle: ResizeHandle = "se",
  ) {
    e.stopPropagation();
    if (e.button !== 0) return;
    const p = puntoEnCapa(e, scale, viewRotation);
    imgActionRef.current = {
      kind,
      handle,
      startX: p.x,
      startY: p.y,
      orig: im,
      moved: false,
    };
    setImagePopover(null);
    imgLiveRef.current = im;
    setImgDraft(im);
    setImgPatch({
      rect: { x: im.x, y: im.y, w: im.w, h: im.h },
      color: sampleAround(im),
    });
  }

  return {
    images,
    imgPreviews,
    imgDraft,
    setImgDraft,
    imgLiveRef,
    imagePopover,
    setImagePopover,
    imgActionRef,
    imgPatch,
    setImgPatch,
    insertImageAt,
    commitImage,
    sampleAround,
    replaceImagePick,
    deleteImage,
    startImgAction,
  };
}

export type Imagenes = ReturnType<typeof useImagenes>;
