import { useCallback, useMemo, useState } from "react";
import { cargaColores, guardaColor, type ShapeKind } from "../tipos";
import type { Alineacion } from "../api";
import type { MarcaRellenar, ToolProps } from "../components/Pagina";

export const STAMP_PRESETS = [
  "APROBADO",
  "BORRADOR",
  "CONFIDENCIAL",
  "REVISADO",
  "URGENTE",
];

/**
 * Opciones de las herramientas de anotación (trazo, marcado, formas y
 * sello) y el objeto `tool` memoizado que reciben las Paginas.
 */
export function useHerramienta(
  activeSig: ToolProps["activeSig"],
  /** La escala del documento abierto: la guarda `App` por ruta, porque es
   *  del documento y no de la herramienta. */
  escala: { escalaMm: number; onEscala: (mmPorPunto: number) => void },
) {
  const [drawColor, setDrawColor] = useState(() => cargaColores().dibujo ?? "#c0392b");
  const [drawWidth, setDrawWidth] = useState(2);
  const [markupPending, setMarkupPending] = useState<string | null>(null);
  const [markupColors, setMarkupColors] = useState(() => {
    const c = cargaColores();
    return {
      resaltar: c.resaltar ?? "#f5c400",
      subrayar: c.subrayar ?? "#2ea043",
      tachar: c.tachar ?? "#c0392b",
    };
  });
  const [shapeKind, setShapeKind] = useState<ShapeKind>("rect");
  const [shapeColor, setShapeColor] = useState(() => cargaColores().forma ?? "#c0392b");
  const [shapeFill, setShapeFill] = useState(false);
  const [shapeWidth, setShapeWidth] = useState(2);
  const [stampText, setStampText] = useState(STAMP_PRESETS[0]);
  const [stampCustom, setStampCustom] = useState("");
  const [stampColor, setStampColor] = useState(() => cargaColores().sello ?? "#c0392b");
  const [freeTextColor, setFreeTextColor] = useState(
    () => cargaColores().cuadro ?? "#1d1c18",
  );
  const [freeTextSize, setFreeTextSize] = useState(12);
  const [freeTextBorder, setFreeTextBorder] = useState(true);
  // color y alineación del texto del documento (modo Editar). `null` es
  // «como esté»: editar un párrafo no debe recolorearlo sin querer
  const [textColor, setTextColor] = useState<string | null>(null);
  const [textAlign, setTextAlign] = useState<Alineacion | null>(null);
  // interlineado: la distancia a la que se coloca cada línea (en un PDF las
  // líneas son objetos, no hay párrafos). `null` es «el del documento»
  const [textLineHeight, setTextLineHeight] = useState<number | null>(null);
  // espaciado entre caracteres: ese sí es un operador (`Tc`) y lo escribe el
  // backend con lopdf. Arranca en 0 —«Normal»— como el panel de Acrobat
  const [textCharSpacing, setTextCharSpacing] = useState(0);
  // el color real del bloque que está seleccionado: es lo que pinta el
  // swatch «A» («el que ya tenga»), que hasta ahora era una letra gris
  const [textColorBloque, setTextColorBloque] = useState<string | null>(null);
  // goma de borrar del modo Dibujar: un conmutador, no un modo aparte, con
  // el cursor redondo del tamaño del borrado (Acrobat)
  const [goma, setGoma] = useState(false);
  const [gomaAncho, setGomaAncho] = useState(16);
  // medir: qué se mide, si la medida se deja puesta en el documento y si el
  // arrastre siguiente es el de fijar la escala
  const [medidaTipo, setMedidaTipo] = useState<
    "distancia" | "perimetro" | "area"
  >("distancia");
  const [medidaDejar, setMedidaDejar] = useState(false);
  const [calibrando, setCalibrando] = useState(false);
  // marca de «rellenar y firmar» armada, si la hay
  const [fillMark, setFillMark] = useState<MarcaRellenar | null>(null);
  const [fillColor, setFillColor] = useState(
    () => cargaColores().marca ?? "#1d1c18",
  );

  function cambiaColorAccion(
    accion: "dibujo" | "forma" | "sello" | "cuadro" | "texto" | "marca",
    color: string,
  ) {
    // el texto admite «como esté» (cadena vacía), que no se recuerda: es un
    // no-color, no una preferencia
    if (color) guardaColor(accion, color);
    if (accion === "dibujo") setDrawColor(color);
    else if (accion === "forma") setShapeColor(color);
    else if (accion === "cuadro") setFreeTextColor(color);
    else if (accion === "texto") setTextColor(color || null);
    else if (accion === "marca") setFillColor(color);
    else setStampColor(color);
  }

  const onMarkupUsed = useCallback(
    (kind: "highlight" | "underline" | "strikeout", color: string) => {
      const accion =
        kind === "highlight"
          ? "resaltar"
          : kind === "underline"
            ? "subrayar"
            : "tachar";
      guardaColor(accion, color);
      setMarkupColors((c) => ({ ...c, [accion]: color }));
      setMarkupPending(null);
    },
    [],
  );

  // memoizado para que Pagina (React.memo) no re-renderice todas las páginas
  // en cada cambio de estado de App
  const tool: ToolProps = useMemo(
    () => ({
      drawColor,
      drawWidth,
      markupPending,
      onMarkupPending: setMarkupPending,
      markupColors,
      onMarkupUsed,
      shapeKind,
      shapeColor,
      shapeFill,
      shapeWidth,
      stampText,
      stampCustom,
      stampColor,
      freeTextColor,
      freeTextSize,
      freeTextBorder,
      textColor,
      textAlign,
      textLineHeight,
      textCharSpacing,
      goma,
      gomaAncho,
      medidaTipo,
      medidaDejar,
      calibrando,
      escalaMm: escala.escalaMm,
      onEscala: escala.onEscala,
      onCalibrado: () => setCalibrando(false),
      onTextBlockPicked: setTextColorBloque,
      fillMark,
      fillColor,
      activeSig,
    }),
    [
      drawColor,
      drawWidth,
      markupPending,
      markupColors,
      onMarkupUsed,
      shapeKind,
      shapeColor,
      shapeFill,
      shapeWidth,
      stampText,
      stampCustom,
      stampColor,
      freeTextColor,
      freeTextSize,
      freeTextBorder,
      textColor,
      textAlign,
      textLineHeight,
      textCharSpacing,
      goma,
      gomaAncho,
      medidaTipo,
      medidaDejar,
      calibrando,
      // por miembros y no por el objeto: `App` lo compone en cada render y
      // el `tool` memoizado dejaría de serlo (y con él, todas las Paginas)
      escala.escalaMm,
      escala.onEscala,
      fillMark,
      fillColor,
      activeSig,
    ],
  );

  return {
    drawColor,
    drawWidth,
    setDrawWidth,
    shapeKind,
    setShapeKind,
    shapeColor,
    shapeFill,
    setShapeFill,
    shapeWidth,
    setShapeWidth,
    stampText,
    setStampText,
    stampCustom,
    setStampCustom,
    stampColor,
    freeTextColor,
    freeTextSize,
    setFreeTextSize,
    freeTextBorder,
    setFreeTextBorder,
    textColor,
    setTextColor,
    textAlign,
    setTextAlign,
    textLineHeight,
    setTextLineHeight,
    textCharSpacing,
    setTextCharSpacing,
    goma,
    setGoma,
    gomaAncho,
    setGomaAncho,
    medidaTipo,
    setMedidaTipo,
    medidaDejar,
    setMedidaDejar,
    calibrando,
    setCalibrando,
    textColorBloque,
    fillMark,
    setFillMark,
    fillColor,
    cambiaColorAccion,
    tool,
  };
}
