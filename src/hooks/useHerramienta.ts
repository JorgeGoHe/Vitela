import { useCallback, useMemo, useState } from "react";
import { cargaColores, guardaColor, type ShapeKind } from "../tipos";
import type { ToolProps } from "../components/Pagina";

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
export function useHerramienta(activeSig: ToolProps["activeSig"]) {
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

  function cambiaColorAccion(
    accion: "dibujo" | "forma" | "sello",
    color: string,
  ) {
    guardaColor(accion, color);
    if (accion === "dibujo") setDrawColor(color);
    else if (accion === "forma") setShapeColor(color);
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
    cambiaColorAccion,
    tool,
  };
}
