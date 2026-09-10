const ICONS: Record<string, string[]> = {
  open: [
    "M4 20h16a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13c0 1.1.9 2 2 2Z",
  ],
  save: [
    "M19 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h11l5 5v11a2 2 0 0 1-2 2Z",
    "M17 21v-8H7v8",
    "M7 3v5h8",
  ],
  chevLeft: ["m15 18-6-6 6-6"],
  chevRight: ["m9 18 6-6-6-6"],
  minus: ["M5 12h14"],
  check: ["m4 13 5 5L20 6"],
  dot: ["M12 5a7 7 0 1 0 0 14 7 7 0 0 0 0-14Z"],
  flipH: ["M12 3v18", "M8 7 3 12l5 5V7Z", "m16 7 5 5-5 5V7Z"],
  flipV: ["M3 12h18", "M7 8l5-5 5 5H7Z", "m7 16 5 5 5-5H7Z"],
  plus: ["M12 5v14", "M5 12h14"],
  search: ["M11 3a8 8 0 1 0 0 16 8 8 0 0 0 0-16Z", "m21 21-4.35-4.35"],
  select: ["m3 3 7.07 16.97 2.51-7.39 7.39-2.51L3 3Z"],
  pen: ["M17 3a2.85 2.83 0 1 1 4 4L7.5 20.5 2 22l1.5-5.5L17 3Z"],
  note: [
    "M15.5 3H5a2 2 0 0 0-2 2v14c0 1.1.9 2 2 2h14a2 2 0 0 0 2-2V8.5L15.5 3Z",
    "M15 3v6h6",
  ],
  textedit: ["M4 7V5h16v2", "M9 20h6", "M12 5v15"],
  undo: ["M3 7v6h6", "M21 17a9 9 0 0 0-15-6.7L3 13"],
  redo: ["M21 7v6h-6", "M3 17a9 9 0 0 1 15-6.7L21 13"],
  highlight: [
    "m9 11-6 6v3h9l3-3",
    "m22 12-4.6 4.6a2 2 0 0 1-2.83 0l-5.17-5.17a2 2 0 0 1 0-2.83L14 4",
  ],
  trash: [
    "M3 6h18",
    "M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6",
    "M8 6V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2",
  ],
  rotate: ["M21 3v5h-5", "M21 8a9 9 0 1 0-2.34 8.66"],
  up: ["m18 15-6-6-6 6"],
  down: ["m6 9 6 6 6-6"],
  copy: [
    "M20 8H10a2 2 0 0 0-2 2v10c0 1.1.9 2 2 2h10a2 2 0 0 0 2-2V10a2 2 0 0 0-2-2Z",
    "M4 16c-1.1 0-2-.9-2-2V4c0-1.1.9-2 2-2h10c1.1 0 2 .9 2 2",
  ],
  close: ["M18 6 6 18", "m6 6 12 12"],
  sign: ["M3 17c3-6 6-6 8 0s5 6 8 0", "M3 21h18"],
  merge: ["M14 2H6a2 2 0 0 0-2 2v16c0 1.1.9 2 2 2h12a2 2 0 0 0 2-2V8l-6-6Z", "M12 11v6", "M9 14h6"],
  extract: [
    "M14 2H6a2 2 0 0 0-2 2v16c0 1.1.9 2 2 2h12a2 2 0 0 0 2-2V8l-6-6Z",
    "M14 2v6h6",
    "M12 18v-6",
    "m9 15 3 3 3-3",
  ],
  doc: [
    "M14 2H6a2 2 0 0 0-2 2v16c0 1.1.9 2 2 2h12a2 2 0 0 0 2-2V8l-6-6Z",
    "M14 2v6h6",
  ],
  image: [
    "M19 3H5a2 2 0 0 0-2 2v14c0 1.1.9 2 2 2h14a2 2 0 0 0 2-2V5a2 2 0 0 0-2-2Z",
    "M9 9.5a1.5 1.5 0 1 0 0-3 1.5 1.5 0 0 0 0 3Z",
    "m21 15-3.09-3.09a2 2 0 0 0-2.82 0L6 21",
  ],
  sticky: ["M12 3v10", "M12 13l-3-3", "M12 13l3-3"],
  panel: ["M3 5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5Z", "M9 3v18"],
  shapes: [
    "M8.3 10a.7.7 0 0 1-.626-1.08L11.4 3a.7.7 0 0 1 1.198-.043L16.3 8.9a.7.7 0 0 1-.572 1.1Z",
    "M3 14h7v7H3z",
    "M17.5 21a3.5 3.5 0 1 0 0-7 3.5 3.5 0 0 0 0 7Z",
  ],
  stamp: [
    "M5 22h14",
    "M19.27 13.73A2.5 2.5 0 0 0 17.5 13h-11A2.5 2.5 0 0 0 4 15.5V17a1 1 0 0 0 1 1h14a1 1 0 0 0 1-1v-1.5c0-.66-.26-1.3-.73-1.77Z",
    "M14 13V8.5C14 7 15 7 15 5a3 3 0 0 0-6 0c0 2 1 2 1 3.5V13",
  ],
  textbox: [
    "M3 5h18a1 1 0 0 1 1 1v12a1 1 0 0 1-1 1H3a1 1 0 0 1-1-1V6a1 1 0 0 1 1-1Z",
    "M8 9h8",
    "M12 9v6",
  ],
  underline: ["M6 4v6a6 6 0 0 0 12 0V4", "M4 20h16"],
  strike: ["M16 4H9a3 3 0 0 0-2.83 4", "M14 12a4 4 0 0 1 0 8H6", "M4 12h16"],
  crop: ["M6 2v14a2 2 0 0 0 2 2h14", "M18 22V8a2 2 0 0 0-2-2H2"],
  lock: [
    "M19 11H5a2 2 0 0 0-2 2v7a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-7a2 2 0 0 0-2-2Z",
    "M7 11V7a5 5 0 0 1 10 0v4",
  ],
  flatten: ["M12 3v12", "m8 11 4 4 4-4", "M4 21h16"],
  redact: ["M4 5h16v6H4Z", "M4 15h7", "M4 19h10"],
  field: [
    "M4 7h16a1 1 0 0 1 1 1v8a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1V8a1 1 0 0 1 1-1Z",
    "M7 10v4",
  ],
  link: [
    "M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71",
    "M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71",
  ],
  printer: [
    "M6 9V3h12v6",
    "M6 18H4a2 2 0 0 1-2-2v-5a2 2 0 0 1 2-2h16a2 2 0 0 1 2 2v5a2 2 0 0 1-2 2h-2",
    "M6 14h12v8H6Z",
  ],
  shrink: ["m15 15 6 6", "m15 9 6-6", "M9 21v-6H3", "M3 9h6V3"],
  water: ["M12 22a7 7 0 0 0 7-7c0-2-1-3.9-3-5.5s-3.5-4-4-6.5c-.5 2.5-2 4.9-4 6.5C6 11.1 5 13 5 15a7 7 0 0 0 7 7Z"],
  hf: ["M3 5h18", "M3 19h18", "M7 12h10"],
  sliders: ["M4 7h9", "M17 7h3", "M4 17h3", "M11 17h9", "M15 4.5v5", "M7 14.5v5"],
  /* presentaciones de página (D3): una hoja, scroll continuo, dos hojas y
     dos hojas en scroll continuo */
  pageOne: ["M6 3h12a1 1 0 0 1 1 1v16a1 1 0 0 1-1 1H6a1 1 0 0 1-1-1V4a1 1 0 0 1 1-1Z"],
  pageScroll: [
    "M6 2h12a1 1 0 0 1 1 1v6a1 1 0 0 1-1 1H6a1 1 0 0 1-1-1V3a1 1 0 0 1 1-1Z",
    "M6 14h12a1 1 0 0 1 1 1v6a1 1 0 0 1-1 1H6a1 1 0 0 1-1-1v-6a1 1 0 0 1 1-1Z",
  ],
  pageTwo: [
    "M3 3h8a1 1 0 0 1 1 1v16a1 1 0 0 1-1 1H3a1 1 0 0 1-1-1V4a1 1 0 0 1 1-1Z",
    "M13 3h8a1 1 0 0 1 1 1v16a1 1 0 0 1-1 1h-8a1 1 0 0 1-1-1V4a1 1 0 0 1 1-1Z",
  ],
  pageTwoScroll: [
    "M3 2h8a1 1 0 0 1 1 1v6a1 1 0 0 1-1 1H3a1 1 0 0 1-1-1V3a1 1 0 0 1 1-1Z",
    "M13 2h8a1 1 0 0 1 1 1v6a1 1 0 0 1-1 1h-8a1 1 0 0 1-1-1V3a1 1 0 0 1 1-1Z",
    "M3 14h8a1 1 0 0 1 1 1v6a1 1 0 0 1-1 1H3a1 1 0 0 1-1-1v-6a1 1 0 0 1 1-1Z",
    "M13 14h8a1 1 0 0 1 1 1v6a1 1 0 0 1-1 1h-8a1 1 0 0 1-1-1v-6a1 1 0 0 1 1-1Z",
  ],
  expand: ["M8 3H3v5", "M16 3h5v5", "M3 16v5h5", "M21 16v5h-5"],
  moon: ["M21 12.8A9 9 0 1 1 11.2 3a7 7 0 0 0 9.8 9.8Z"],
  back: ["M19 12H5", "m12 19-7-7 7-7"],
  forward: ["M5 12h14", "m12 5 7 7-7 7"],
  // libro abierto: «Modo lectura»
  libro: [
    "M2 4.5h6a3 3 0 0 1 3 3V20a2.5 2.5 0 0 0-2.5-2H2Z",
    "M22 4.5h-6a3 3 0 0 0-3 3V20a2.5 2.5 0 0 1 2.5-2H22Z",
  ],
  // regla en diagonal con sus marcas: el icono de «Medir», que hasta ahora
  // reusaba el de encabezado y pie
  ruler: [
    "M21.3 15.3a2.4 2.4 0 0 1 0 3.4l-2.6 2.6a2.4 2.4 0 0 1-3.4 0L2.7 8.7a2.41 2.41 0 0 1 0-3.4l2.6-2.6a2.41 2.41 0 0 1 3.4 0Z",
    "m14.5 12.5 2-2",
    "m11.5 9.5 2-2",
    "m8.5 6.5 2-2",
    "m17.5 15.5 2-2",
  ],
  // bocadillo con su rabo: el icono de «Llamada», que reusaba una flecha
  callout: [
    "M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2Z",
    "M8 9h8",
    "M8 13h5",
  ],
  more: [
    "M5 11a1 1 0 1 0 0 2 1 1 0 0 0 0-2Z",
    "M12 11a1 1 0 1 0 0 2 1 1 0 0 0 0-2Z",
    "M19 11a1 1 0 1 0 0 2 1 1 0 0 0 0-2Z",
  ],
};

/** `espejo` voltea el icono en horizontal: es lo que distingue «girar a la
 *  izquierda» de «girar a la derecha» sin dibujar un segundo trazado. */
export default function Icon({
  name,
  size = 16,
  espejo = false,
}: {
  name: string;
  size?: number;
  espejo?: boolean;
}) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      style={espejo ? { transform: "scaleX(-1)" } : undefined}
      fill="none"
      stroke="currentColor"
      strokeWidth={1.8}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden
    >
      {ICONS[name]?.map((d, i) => (
        <path key={i} d={d} />
      ))}
    </svg>
  );
}
