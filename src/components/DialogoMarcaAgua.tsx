import { useRef, useState } from "react";
import {
  cargaColores,
  guardaColor,
  indicesDeRango,
  type PageSize,
} from "../tipos";
import Icon from "./Icon";
import { useModal } from "../hooks/useModal";
import RangoPaginas from "./RangoPaginas";

const COLORS = ["#c0392b", "#6f6a5c", "#2743c0", "#2ea043"];

export type PosicionMarca = "nw" | "n" | "ne" | "w" | "c" | "e" | "sw" | "s" | "se";

const POSICIONES: PosicionMarca[] = ["nw", "n", "ne", "w", "c", "e", "sw", "s", "se"];

/** Los tres ángulos que ofrece Acrobat, con el suyo por defecto en medio. */
const GIROS: [number, string][] = [
  [0, "Horizontal"],
  [45, "En diagonal"],
  [90, "Vertical"],
];

/** Ancho en píxeles de la vista previa. */
const PREVIA = 190;

/** Qué se pone: texto, imagen o un color sólido a sangre (que solo tiene
 *  sentido como fondo, y por eso enciende «Fondo» y no se apaga). */
export type TipoMarca = "texto" | "imagen" | "color";

export type MarcaAguaOpts = {
  tipo: TipoMarca;
  text: string;
  fontSize: number;
  color: string;
  opacity: number;
  rotation: number;
  position: PosicionMarca;
  /** Índices de página, o null para todas. */
  pageIndices: number[] | null;
  /** PNG en base64 cuando la marca es una imagen; si no, null. */
  imagePng: string | null;
  /** Debajo del contenido de la página, que es el «Fondo» de Acrobat. */
  detras: boolean;
};

/**
 * Marca de agua **y fondo**: texto, imagen o color sólido, con rango de
 * páginas y **vista previa en vivo** sobre la página actual. Sin la previa,
 * poner una marca de agua bien eran tres o cinco ciclos de aplicar, mirar y
 * deshacer.
 *
 * El fondo se llama fondo y está donde se busca: era una casilla escondida
 * dentro de «Marca de agua…» y el color sólido —el caso por defecto de
 * Acrobat— no se podía hacer.
 */
export default function DialogoMarcaAgua({
  pageCount,
  paginaActual,
  previaSrc,
  previaSize,
  error,
  onApply,
  onClose,
}: {
  pageCount: number;
  /** Página que se enseña en la previa (la que está leyendo el usuario). */
  paginaActual: number;
  /** Miniatura ya renderizada de esa página, si la hay. */
  previaSrc: string | null;
  previaSize: PageSize | undefined;
  /** Lo que ha fallado en el último intento. Se pinta dentro, porque el
   *  diálogo sigue abierto y la banda roja quedaría bajo el velo. */
  error: string | null;
  onApply: (opts: MarcaAguaOpts) => void;
  onClose: () => void;
}) {
  const [tipo, setTipo] = useState<TipoMarca>("texto");
  const [text, setText] = useState("BORRADOR");
  const [fontSize, setFontSize] = useState(64);
  const [color, setColor] = useState(() => cargaColores().marcaAgua ?? COLORS[1]);
  // el 30 % de Acrobat es el defecto sensato: se lee debajo
  const [opacity, setOpacity] = useState(30);
  const [rotation, setRotation] = useState(45);
  const [position, setPosition] = useState<PosicionMarca>("c");
  const [todas, setTodas] = useState(true);
  const [rango, setRango] = useState("");
  const [imagen, setImagen] = useState<{ nombre: string; png: string } | null>(
    null,
  );
  // «detrás del contenido» es lo que Acrobat llama Fondo: el mismo trabajo,
  // con el objeto al principio de la página en vez de al final
  const [detras, setDetras] = useState(false);
  // el color sólido tapa la página entera: solo existe como fondo
  const esColor = tipo === "color";
  const alFondo = detras || esColor;
  const ficheroRef = useRef<HTMLInputElement | null>(null);

  const indices = indicesDeRango(todas, rango, pageCount);
  const listo =
    (tipo === "texto" ? !!text.trim() : tipo === "color" ? true : !!imagen) &&
    (todas || (indices?.length ?? 0) > 0);

  function aplicar() {
    if (!listo) return;
    onApply({
      tipo,
      text,
      fontSize,
      color,
      opacity,
      rotation,
      position,
      pageIndices: indices,
      imagePng: tipo === "imagen" ? (imagen?.png ?? null) : null,
      detras: alFondo,
    });
  }

  /** La imagen se lee en el navegador y no por el diálogo nativo: la previa
   *  necesita los bytes de todas formas, y así se enseña al instante. */
  function eligeImagen(f: File | undefined) {
    if (!f) return;
    const lector = new FileReader();
    lector.onload = () => {
      const r = String(lector.result);
      setImagen({ nombre: f.name, png: r.slice(r.indexOf(",") + 1) });
    };
    lector.readAsDataURL(f);
  }

  const { ref, onKeyDown } = useModal({ onClose, onConfirm: aplicar });

  // la previa mide en puntos de la página y se dibuja a escala
  const escala = previaSize ? PREVIA / previaSize.width : 0;
  const altoPrevia = previaSize ? previaSize.height * escala : 0;
  // la celda del grid 3×3 en la que cae la marca
  const ix = POSICIONES.indexOf(position);
  const fila = Math.floor(ix / 3);
  const columna = ix % 3;

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div
        className="modal modal-hf"
        role="dialog"
        aria-modal="true"
        aria-label="Marca de agua y fondo"
        onClick={(e) => e.stopPropagation()}
        onKeyDown={onKeyDown}
        ref={ref}
        tabIndex={-1}
      >
        <h3>Marca de agua y fondo</h3>
        <div className="card-row">
          <label className="opt-check">
            <input
              type="radio"
              name="tipo-marca"
              checked={tipo === "texto"}
              onChange={() => setTipo("texto")}
            />
            Texto
          </label>
          <label className="opt-check">
            <input
              type="radio"
              name="tipo-marca"
              checked={tipo === "imagen"}
              onChange={() => setTipo("imagen")}
            />
            Imagen
          </label>
          <label className="opt-check">
            <input
              type="radio"
              name="tipo-marca"
              checked={esColor}
              onChange={() => setTipo("color")}
            />
            Color sólido
          </label>
        </div>
        {tipo === "texto" ? (
          <input
            type="text"
            placeholder="Texto de la marca de agua"
            aria-label="Texto de la marca de agua"
            value={text}
            onChange={(e) => setText(e.target.value)}
          />
        ) : esColor ? (
          <p className="modal-file" style={{ whiteSpace: "normal" }}>
            El color cubre la página entera, por detrás del contenido: es el
            fondo de Acrobat.
          </p>
        ) : (
          <div className="card-row">
            <button className="btn" onClick={() => ficheroRef.current?.click()}>
              <Icon name="image" size={14} />
              {imagen ? "Cambiar la imagen…" : "Elegir imagen…"}
            </button>
            {imagen && <span className="dato opt-hint">{imagen.nombre}</span>}
            <input
              ref={ficheroRef}
              type="file"
              accept="image/png,image/jpeg,image/webp"
              hidden
              onChange={(e) => eligeImagen(e.target.files?.[0])}
            />
          </div>
        )}
        <div className="card-row">
          {tipo === "texto" && (
            <select
              className="size-select"
              title="Tamaño"
              aria-label="Tamaño del texto"
              value={fontSize}
              onChange={(e) => setFontSize(Number(e.target.value))}
            >
              {[36, 48, 64, 80, 100].map((s) => (
                <option key={s} value={s}>
                  {s} pt
                </option>
              ))}
            </select>
          )}
          <select
            className="size-select"
            title="Opacidad"
            aria-label="Opacidad"
            value={opacity}
            onChange={(e) => setOpacity(Number(e.target.value))}
          >
            {[15, 25, 30, 50, 75].map((o) => (
              <option key={o} value={o}>
                {o} %
              </option>
            ))}
          </select>
          {!esColor && (
            <select
              className="size-select"
              title="Giro"
              aria-label="Giro de la marca"
              value={rotation}
              onChange={(e) => setRotation(Number(e.target.value))}
            >
              {GIROS.map(([g, etiqueta]) => (
                <option key={g} value={g}>
                  {etiqueta}
                </option>
              ))}
            </select>
          )}
        </div>
        {(tipo === "texto" || esColor) && (
          <div className="swatches">
            {COLORS.map((c) => (
              <button
                key={c}
                className={`swatch${color === c ? " on" : ""}`}
                style={{ background: c }}
                aria-label={`Color ${c}`}
                onClick={() => {
                  setColor(c);
                  guardaColor("marcaAgua", c);
                }}
              />
            ))}
            <label
              className={`swatch swatch-custom${
                !COLORS.includes(color) ? " on" : ""
              }`}
              title="Color personalizado"
            >
              <input
                type="color"
                value={color}
                onChange={(e) => {
                  setColor(e.target.value);
                  guardaColor("marcaAgua", e.target.value);
                }}
              />
              <Icon name="plus" size={10} />
            </label>
          </div>
        )}
        <div className="card-row previa-fila">
          {!esColor && (
            <div className="pos-grid" title="Posición en la página">
              {POSICIONES.map((p) => (
                <button
                  key={p}
                  className={`pos-cell${position === p ? " on" : ""}`}
                  aria-label={`Posición ${p}`}
                  aria-pressed={position === p}
                  onClick={() => setPosition(p)}
                />
              ))}
            </div>
          )}
          {previaSrc && previaSize && (
            <div
              className="previa-pagina"
              style={{ width: PREVIA, height: altoPrevia }}
            >
              <img src={previaSrc} alt={`Página ${paginaActual + 1}`} />
              {esColor ? (
                // el fondo va a sangre: la previa lo pinta sobre la hoja
                // entera, multiplicando como se verá en el papel
                <div
                  className="previa-fondo"
                  style={{
                    background: color,
                    opacity: opacity / 100,
                    mixBlendMode: "multiply",
                  }}
                />
              ) : (
              <div
                className="previa-marca"
                style={{
                  left: `${columna * 33.33}%`,
                  top: `${fila * 33.33}%`,
                  opacity: opacity / 100,
                  transform: `rotate(${-rotation}deg)`,
                  // debajo del contenido: en la previa se multiplica, que es
                  // lo que se ve en el papel —el texto de la página encima,
                  // la marca asomando por donde no hay tinta—
                  mixBlendMode: alFondo ? "multiply" : undefined,
                }}
              >
                {tipo === "imagen" && imagen ? (
                  <img
                    src={`data:image/png;base64,${imagen.png}`}
                    alt=""
                    className="previa-marca-img"
                  />
                ) : (
                  <span style={{ color, fontSize: fontSize * escala }}>
                    {text || "BORRADOR"}
                  </span>
                )}
              </div>
              )}
            </div>
          )}
        </div>
        <RangoPaginas
          pageCount={pageCount}
          todas={todas}
          setTodas={setTodas}
          rango={rango}
          setRango={setRango}
        />
        <label
          className={`opt-check${esColor ? " disabled" : ""}`}
          title={
            esColor
              ? "Un color sólido solo puede ir detrás: delante taparía la página"
              : "El fondo de Acrobat: el mismo dibujo, debajo del texto"
          }
        >
          <input
            type="checkbox"
            checked={alFondo}
            disabled={esColor}
            onChange={(e) => setDetras(e.target.checked)}
          />
          Fondo (detrás del contenido)
        </label>
        <p className="modal-file" style={{ whiteSpace: "normal" }}>
          Se añade como contenido del documento; la previa es aproximada.
          {alFondo &&
            " Detrás del contenido queda tapada donde la página lleve una imagen o un fondo opaco."}
          {/* cada uno se quita con SU botón: mandar al equivocado es peor
              que no decir nada */}
          {alFondo
            ? " Se puede quitar después con «Quitar fondo…»."
            : " Se puede quitar después con «Quitar marca de agua…»."}
        </p>
        {error && (
          <p className="modal-error" role="alert">
            {error}
          </p>
        )}
        <div className="card-actions">
          <button className="btn" onClick={onClose}>
            Cancelar
          </button>
          <button className="btn btn-primary" disabled={!listo} onClick={aplicar}>
            Aplicar
          </button>
        </div>
      </div>
    </div>
  );
}
