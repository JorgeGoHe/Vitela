import type { FirmaGuardada, RanuraImagen } from "../api";

/** Las tres ranuras, con el nombre que lee el usuario en la app —no el que
 *  tienen en el fichero—. */
const RANURAS: [RanuraImagen, string][] = [
  ["firma", "Tu firma"],
  ["iniciales", "Tus iniciales"],
  ["sello", "Mis sellos"],
];

/**
 * «Cambiar la ranura» de una imagen de la biblioteca: para qué sirve —la
 * firma entera, las iniciales o un sello—, que hasta ahora se decidía al
 * guardarla y no se podía corregir después.
 *
 * Es un desplegable y no tres botones porque las ranuras son excluyentes:
 * la imagen está en una, y elegir otra la mueve.
 */
export default function SelectorRanura({
  firma,
  onCambiar,
}: {
  firma: FirmaGuardada;
  onCambiar: (id: string, ranura: RanuraImagen) => void;
}) {
  return (
    <select
      className="size-select sign-ranura"
      value={firma.ranura}
      aria-label={`Ranura de «${firma.name}»`}
      title="Para qué sirve esta imagen: se mueve a la ranura que elijas"
      onChange={(e) => onCambiar(firma.id, e.target.value as RanuraImagen)}
    >
      {RANURAS.map(([valor, etiqueta]) => (
        <option key={valor} value={valor}>
          {etiqueta}
        </option>
      ))}
    </select>
  );
}
