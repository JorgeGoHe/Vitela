import {
  ANNOT_COLORS,
  NOMBRE_COLOR,
  plural,
  type Mode,
  type ShapeKind,
  formateaLongitud,
} from "../tipos";
import type { Alineacion } from "../api";
import type { MarcaRellenar } from "./Pagina";
import { STAMP_PRESETS } from "../hooks/useHerramienta";
import Icon from "./Icon";

const SHAPE_COLORS = ANNOT_COLORS;

/** Interlineado: los cuatro de un procesador de textos. En un PDF no es un
 *  operador: es la distancia a la que se coloca la línea siguiente. */
const INTERLINEADOS: [number, string][] = [
  [1, "Sencillo"],
  [1.15, "1,15"],
  [1.5, "1,5"],
  [2, "Doble"],
];

/** Espaciado entre caracteres en puntos (operador `Tc`). Acrobat parte de 0
 *  y lo llama «Normal»: separar las letras es la excepción, no el estado. */
const ESPACIADOS: [number, string][] = [
  [0, "Normal"],
  [0.5, "0,5 pt"],
  [1, "1 pt"],
  [2, "2 pt"],
];

/** Las tres herramientas de medida de Acrobat. La distancia va de dos
 *  puntos; el perímetro y el área se ponen por vértices. */
const MEDIDAS: ["distancia" | "perimetro" | "area", string, string][] = [
  ["distancia", "Distancia", "Entre dos puntos: arrastra de uno a otro"],
  ["perimetro", "Perímetro", "El contorno: un clic por vértice"],
  ["area", "Área", "La superficie del polígono: un clic por vértice"],
];

/** Los tres botones de alineación, con la etiqueta que entiende el backend. */
const ALINEACIONES: [Alineacion, string, string][] = [
  ["izq", "Izq.", "Alinear a la izquierda"],
  ["centro", "Centro", "Centrar"],
  ["der", "Der.", "Alinear a la derecha"],
];

/** La fila de «rellenar y firmar»: se colocan con un clic y se mueven
 *  después, como cualquier otra marca. */
const MARCAS: [MarcaRellenar, string, string][] = [
  ["check", "check", "Marca de verificación"],
  ["cross", "close", "Aspa"],
  ["dot", "dot", "Punto"],
  ["line", "minus", "Línea"],
];

/** Fila contextual de opciones de la herramienta activa (trazo, formas y
 *  sello); no pinta nada en el resto de modos. */
export default function OpcionesHerramienta({
  mode,
  drawColor,
  drawWidth,
  setDrawWidth,
  goma,
  setGoma,
  gomaAncho,
  setGomaAncho,
  leyendo,
  pausada,
  paginaLeida,
  onPausarLectura,
  onPararLectura,
  medidaTipo,
  setMedidaTipo,
  medidaDejar,
  setMedidaDejar,
  calibrando,
  setCalibrando,
  escalaMm,
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
  hayFormularios,
  resaltarCampos,
  setResaltarCampos,
  freeTextColor,
  freeTextSize,
  setFreeTextSize,
  freeTextBorder,
  setFreeTextBorder,
  textColor,
  textColorBloque,
  textAlign,
  setTextAlign,
  textLineHeight,
  setTextLineHeight,
  textCharSpacing,
  setTextCharSpacing,
  fillMark,
  setFillMark,
  fillColor,
  cambiaColorAccion,
  anadirTexto,
  insertarImagen,
  escribirEncima,
  marcasRedact,
  aplicarRedaccion,
  quitarMarcasRedact,
}: {
  mode: Mode;
  drawColor: string;
  drawWidth: number;
  setDrawWidth: (w: number) => void;
  /** Goma de borrar armada dentro del modo Dibujar. */
  goma: boolean;
  setGoma: (v: boolean) => void;
  gomaAncho: number;
  setGomaAncho: (v: number) => void;
  /** «Leer en voz alta»: mientras suena, la fila es la de la lectura. */
  leyendo: boolean;
  pausada: boolean;
  paginaLeida: number | null;
  onPausarLectura: () => void;
  onPararLectura: () => void;
  /** Modo «Medir». */
  medidaTipo: "distancia" | "perimetro" | "area";
  setMedidaTipo: (v: "distancia" | "perimetro" | "area") => void;
  medidaDejar: boolean;
  setMedidaDejar: (v: boolean) => void;
  calibrando: boolean;
  setCalibrando: (v: boolean) => void;
  /** Milímetros por punto de página del documento abierto. */
  escalaMm: number;
  shapeKind: ShapeKind;
  setShapeKind: (k: ShapeKind) => void;
  shapeColor: string;
  shapeFill: boolean;
  setShapeFill: (f: boolean) => void;
  shapeWidth: number;
  setShapeWidth: (w: number) => void;
  stampText: string;
  setStampText: (t: string) => void;
  stampCustom: string;
  setStampCustom: (t: string) => void;
  stampColor: string;
  /** El documento tiene campos: solo entonces sale la fila de Seleccionar. */
  hayFormularios: boolean;
  resaltarCampos: boolean;
  setResaltarCampos: (v: boolean) => void;
  freeTextColor: string;
  freeTextSize: number;
  setFreeTextSize: (s: number) => void;
  freeTextBorder: boolean;
  setFreeTextBorder: (b: boolean) => void;
  /** Color del texto del documento; `null` = el que ya tenga. */
  textColor: string | null;
  /** El color real del bloque señalado, para pintar «el que ya tenga». */
  textColorBloque: string | null;
  textAlign: Alineacion | null;
  setTextAlign: (a: Alineacion | null) => void;
  textLineHeight: number | null;
  setTextLineHeight: (v: number | null) => void;
  textCharSpacing: number;
  setTextCharSpacing: (v: number) => void;
  fillMark: MarcaRellenar | null;
  setFillMark: (m: MarcaRellenar | null) => void;
  fillColor: string;
  cambiaColorAccion: (
    accion: "dibujo" | "forma" | "sello" | "cuadro" | "texto" | "marca",
    color: string,
  ) => void;
  /** Abre el borrador de texto nuevo en la página actual (modo Editar). */
  anadirTexto: () => void;
  /** Pide una imagen y la coloca en la página actual (modo Imagen). */
  insertarImagen: () => void;
  /** Pasa al cuadro de texto: es el «Texto» de rellenar y firmar. */
  escribirEncima: () => void;
  /** Zonas marcadas para censurar en todo el documento. */
  marcasRedact: number;
  aplicarRedaccion: () => void;
  quitarMarcasRedact: () => void;
}) {
  // mientras se lee en voz alta la fila es la de la lectura: es lo que el
  // usuario está haciendo, y son los controles que va a querer a mano
  if (leyendo) {
    return (
      <div className="tool-options">
        <span>Leyendo en voz alta</span>
        {paginaLeida !== null && (
          <span className="dato">pág. {paginaLeida + 1}</span>
        )}
        <button className="btn" onClick={onPausarLectura}>
          {pausada ? "Seguir" : "Pausa"}
        </button>
        <button className="btn" onClick={onPararLectura}>
          Parar
        </button>
        <span className="opt-hint">Esc también para</span>
      </div>
    );
  }

  return (
    <>
      {mode === "select" && hayFormularios && (
        <div className="tool-options">
          <span>Formulario</span>
          <label className="opt-check">
            <input
              type="checkbox"
              checked={resaltarCampos}
              onChange={(e) => setResaltarCampos(e.target.checked)}
            />
            Resaltar campos
          </label>
          <span className="opt-hint">
            Clic en un campo para rellenarlo · Tab salta al siguiente
          </span>
        </div>
      )}
      {mode === "redact" && (
        <div className="tool-options">
          <span>Redactar</span>
          <button
            className="btn btn-danger"
            disabled={marcasRedact === 0}
            title={
              marcasRedact === 0
                ? "Marca antes las zonas que quieras censurar"
                : "Elimina el contenido de las zonas marcadas"
            }
            onClick={aplicarRedaccion}
          >
            <Icon name="redact" size={14} />
            Aplicar redacción ({plural(marcasRedact, "zona", "zonas")})
          </button>
          <button
            className="btn"
            disabled={marcasRedact === 0}
            onClick={quitarMarcasRedact}
          >
            Quitar todas las marcas
          </button>
          <span className="opt-hint">
            Marcar no borra: se revisa y se aplica al final
          </span>
        </div>
      )}
      {mode === "edit" && (
        <div className="tool-options">
          <span>Texto</span>
          <button className="btn" onClick={anadirTexto}>
            <Icon name="textedit" size={14} />
            Añadir texto
          </button>
          {/* color y alineación, donde el usuario ya está mirando cuando los
              necesita. «Como está» es el defecto: editar un párrafo no debe
              recolorearlo sin querer */}
          <div className="swatches" role="group" aria-label="Color del texto">
            {/* con un bloque señalado la letra va de su color real, como la
                barra de propiedades de Acrobat: «el que ya tenga» deja de ser
                una letra gris con un tooltip */}
            <button
              className={`swatch swatch-auto${textColor === null ? " on" : ""}`}
              style={
                textColorBloque ? { color: textColorBloque } : undefined
              }
              title={
                textColorBloque
                  ? "El color que ya tenga (el del texto señalado)"
                  : "El color que ya tenga"
              }
              aria-label="El color que ya tenga"
              aria-pressed={textColor === null}
              onClick={() => cambiaColorAccion("texto", "")}
            >
              A
            </button>
            {SHAPE_COLORS.map((c) => (
              <button
                key={c}
                className={`swatch${textColor === c ? " on" : ""}`}
                style={{ background: c }}
                title={NOMBRE_COLOR[c] ?? c}
                aria-label={NOMBRE_COLOR[c] ?? c}
                aria-pressed={textColor === c}
                onClick={() => cambiaColorAccion("texto", c)}
              />
            ))}
          </div>
          <div className="segmented">
            {ALINEACIONES.map(([a, corta, etiqueta]) => (
              <button
                key={a}
                className={`btn${textAlign === a ? " on" : ""}`}
                title={etiqueta}
                aria-label={etiqueta}
                aria-pressed={textAlign === a}
                onClick={() => setTextAlign(textAlign === a ? null : a)}
              >
                {corta}
              </button>
            ))}
          </div>
          {/* interlineado y espaciado entre caracteres: los dos que Acrobat
              pone en la misma fila. El interlineado se resuelve colocando
              el objeto de la línea siguiente; el espaciado sí es un
              operador del PDF (`Tc`) y lo escribe el backend */}
          <select
            className="size-select"
            title="Interlineado"
            aria-label="Interlineado"
            value={textLineHeight ?? ""}
            onChange={(e) =>
              setTextLineHeight(e.target.value ? Number(e.target.value) : null)
            }
          >
            <option value="">Interlineado del documento</option>
            {INTERLINEADOS.map(([v, etiqueta]) => (
              <option key={v} value={v}>
                {etiqueta}
              </option>
            ))}
          </select>
          <select
            className="size-select"
            title="Espaciado entre caracteres: separa las letras entre sí; 0 es lo normal"
            aria-label="Espaciado entre caracteres"
            value={textCharSpacing}
            onChange={(e) => setTextCharSpacing(Number(e.target.value))}
          >
            {ESPACIADOS.map(([v, etiqueta]) => (
              <option key={v} value={v}>
                Espaciado: {etiqueta}
              </option>
            ))}
          </select>
          <span className="opt-hint">
            Clic en un texto del PDF para corregirlo · clic en una zona libre
            para escribir uno nuevo
          </span>
        </div>
      )}
      {mode === "firmar" && (
        <div className="tool-options">
          <span>Rellenar</span>
          {MARCAS.map(([m, icono, etiqueta]) => (
            <button
              key={m}
              className={`btn btn-icon${fillMark === m ? " on" : ""}`}
              title={`${etiqueta} · clic en la página para colocarla`}
              aria-label={etiqueta}
              aria-pressed={fillMark === m}
              onClick={() => setFillMark(fillMark === m ? null : m)}
            >
              <Icon name={icono} size={14} />
            </button>
          ))}
          <button
            className="btn"
            title="Escribir encima del documento (cuadro de texto)"
            onClick={escribirEncima}
          >
            <Icon name="textbox" size={14} />
            Texto
          </button>
          <div className="swatches" role="group" aria-label="Color de la marca">
            {SHAPE_COLORS.map((c) => (
              <button
                key={c}
                className={`swatch${fillColor === c ? " on" : ""}`}
                style={{ background: c }}
                title={NOMBRE_COLOR[c] ?? c}
                aria-label={NOMBRE_COLOR[c] ?? c}
                aria-pressed={fillColor === c}
                onClick={() => cambiaColorAccion("marca", c)}
              />
            ))}
          </div>
          <span className="opt-hint">
            {fillMark
              ? "Clic en la página para colocarla · se mueve después"
              : "Marcas para rellenar un formulario que no es interactivo"}
          </span>
        </div>
      )}
      {mode === "image" && (
        <div className="tool-options">
          <span>Imagen</span>
          <button className="btn" onClick={insertarImagen}>
            <Icon name="image" size={14} />
            Insertar imagen…
          </button>
          <span className="opt-hint">
            Arrastrar mueve · los tiradores redimensionan · clic abre las
            opciones de la imagen
          </span>
        </div>
      )}
      {mode === "draw" && (
        <div className="tool-options">
          <span>Trazo</span>
          <div className="swatches">
            {SHAPE_COLORS.map((c) => (
              <button
                key={c}
                className={`swatch${drawColor === c ? " on" : ""}`}
                style={{ background: c }}
                title={NOMBRE_COLOR[c] ?? c}
                aria-label={NOMBRE_COLOR[c] ?? c}
                aria-pressed={drawColor === c}
                onClick={() => cambiaColorAccion("dibujo", c)}
              />
            ))}
            <label
              className={`swatch swatch-custom${
                !SHAPE_COLORS.includes(drawColor) ? " on" : ""
              }`}
              title="Color personalizado"
            >
              <input
                type="color"
                value={drawColor}
                onChange={(e) => cambiaColorAccion("dibujo", e.target.value)}
              />
              <Icon name="plus" size={10} />
            </label>
          </div>
          <select
            className="size-select"
            title="Grosor"
            value={drawWidth}
            onChange={(e) => setDrawWidth(Number(e.target.value))}
          >
            {[1, 2, 3, 5, 8].map((w) => (
              <option key={w} value={w}>
                {w} pt
              </option>
            ))}
          </select>
          {/* la goma es un conmutador del propio modo Dibujar, no un modo
              nuevo: se enciende, se borra lo que sobra y se sigue dibujando */}
          <span className="grupo-sep" />
          <button
            className={`btn${goma ? " on" : ""}`}
            title="Goma de borrar: quita el trozo del trazo que tapa, no el trazo entero"
            aria-pressed={goma}
            onClick={() => setGoma(!goma)}
          >
            <Icon name="close" size={13} />
            Goma
          </button>
          {goma && (
            <select
              className="size-select"
              title="Tamaño de la goma"
              aria-label="Tamaño de la goma"
              value={gomaAncho}
              onChange={(e) => setGomaAncho(Number(e.target.value))}
            >
              {[8, 16, 28, 48].map((w) => (
                <option key={w} value={w}>
                  {w} pt
                </option>
              ))}
            </select>
          )}
        </div>
      )}
      {mode === "medir" && (
        <div className="tool-options">
          {/* las tres de Acrobat: la distancia es un arrastre y las otras
              dos se ponen por vértices, que es como se mide una parcela o
              una planta */}
          <div className="segmented">
            {MEDIDAS.map(([v, etiqueta, ayuda]) => (
              <button
                key={v}
                className={`btn${medidaTipo === v ? " on" : ""}`}
                title={ayuda}
                aria-pressed={medidaTipo === v}
                onClick={() => setMedidaTipo(v)}
              >
                {etiqueta}
              </button>
            ))}
          </div>
          {/* la escala se pide una vez por documento y se guarda por ruta;
              sin fijarla se mide el papel, como Acrobat cuando el PDF no
              trae `/Measure` */}
          <span className="dato" title="Lo que mide de verdad un centímetro del papel">
            1 cm de papel = {formateaLongitud(28.3465, escalaMm)}
          </span>
          <button
            className={`btn${calibrando ? " on" : ""}`}
            title="Arrastra sobre algo cuya medida conozcas y dime cuánto mide"
            aria-pressed={calibrando}
            onClick={() => setCalibrando(!calibrando)}
          >
            Fijar la escala…
          </button>
          <label
            className="opt-check"
            title="La medida queda como comentario: sale en el panel y se borra desde ahí"
          >
            <input
              type="checkbox"
              checked={medidaDejar}
              onChange={(e) => setMedidaDejar(e.target.checked)}
            />
            Dejar la medida puesta
          </label>
          <span className="opt-hint">
            {medidaTipo === "distancia"
              ? "Arrastra sobre la página · Esc sale"
              : "Un clic por vértice · doble clic o Enter cierra · Retroceso quita el último · Esc cancela"}
          </span>
        </div>
      )}
      {mode === "callout" && (
        <div className="tool-options">
          <span>Llamada</span>
          <div className="swatches">
            {SHAPE_COLORS.map((c) => (
              <button
                key={c}
                className={`swatch${freeTextColor === c ? " on" : ""}`}
                style={{ background: c }}
                title={NOMBRE_COLOR[c] ?? c}
                aria-label={NOMBRE_COLOR[c] ?? c}
                aria-pressed={freeTextColor === c}
                onClick={() => cambiaColorAccion("cuadro", c)}
              />
            ))}
          </div>
          <select
            className="size-select"
            title="Tamaño de la letra"
            aria-label="Tamaño de la letra"
            value={freeTextSize}
            onChange={(e) => setFreeTextSize(Number(e.target.value))}
          >
            {[9, 10, 12, 14, 18, 24].map((t) => (
              <option key={t} value={t}>
                {t} pt
              </option>
            ))}
          </select>
          <span className="opt-hint">
            Clic donde quieres que señale y arrastra hasta donde va el texto ·
            Esc cancela
          </span>
        </div>
      )}
      {mode === "shape" && (
        <div className="tool-options">
          <div className="segmented">
            {(
              [
                ["rect", "Rectángulo"],
                ["ellipse", "Elipse"],
                ["line", "Línea"],
                ["arrow", "Flecha"],
              ] as [ShapeKind, string][]
            ).map(([k, label]) => (
              <button
                key={k}
                className={`btn${shapeKind === k ? " on" : ""}`}
                onClick={() => setShapeKind(k)}
              >
                {label}
              </button>
            ))}
          </div>
          <div className="swatches">
            {SHAPE_COLORS.map((c) => (
              <button
                key={c}
                className={`swatch${shapeColor === c ? " on" : ""}`}
                style={{ background: c }}
                title={NOMBRE_COLOR[c] ?? c}
                aria-label={NOMBRE_COLOR[c] ?? c}
                aria-pressed={shapeColor === c}
                onClick={() => cambiaColorAccion("forma", c)}
              />
            ))}
            <label
              className={`swatch swatch-custom${
                !SHAPE_COLORS.includes(shapeColor) ? " on" : ""
              }`}
              title="Color personalizado"
            >
              <input
                type="color"
                value={shapeColor}
                onChange={(e) => cambiaColorAccion("forma", e.target.value)}
              />
              <Icon name="plus" size={10} />
            </label>
          </div>
          <label
            className={`opt-check${
              shapeKind === "line" || shapeKind === "arrow" ? " disabled" : ""
            }`}
          >
            <input
              type="checkbox"
              checked={shapeFill}
              disabled={shapeKind === "line" || shapeKind === "arrow"}
              onChange={(e) => setShapeFill(e.target.checked)}
            />
            Relleno
          </label>
          <select
            className="size-select"
            title="Grosor"
            value={shapeWidth}
            onChange={(e) => setShapeWidth(Number(e.target.value))}
          >
            {[1, 2, 3, 5].map((s) => (
              <option key={s} value={s}>
                {s} pt
              </option>
            ))}
          </select>
        </div>
      )}
      {mode === "freetext" && (
        <div className="tool-options">
          <span>Cuadro</span>
          <div className="swatches">
            {SHAPE_COLORS.map((c) => (
              <button
                key={c}
                className={`swatch${freeTextColor === c ? " on" : ""}`}
                style={{ background: c }}
                title={NOMBRE_COLOR[c] ?? c}
                aria-label={NOMBRE_COLOR[c] ?? c}
                aria-pressed={freeTextColor === c}
                onClick={() => cambiaColorAccion("cuadro", c)}
              />
            ))}
            <label
              className={`swatch swatch-custom${
                !SHAPE_COLORS.includes(freeTextColor) ? " on" : ""
              }`}
              title="Color personalizado"
            >
              <input
                type="color"
                value={freeTextColor}
                onChange={(e) => cambiaColorAccion("cuadro", e.target.value)}
              />
              <Icon name="plus" size={10} />
            </label>
          </div>
          <select
            className="size-select"
            title="Tamaño de letra"
            aria-label="Tamaño de letra"
            value={freeTextSize}
            onChange={(e) => setFreeTextSize(Number(e.target.value))}
          >
            {[8, 10, 12, 14, 18, 24, 32].map((t) => (
              <option key={t} value={t}>
                {t} pt
              </option>
            ))}
          </select>
          <label className="opt-check">
            <input
              type="checkbox"
              checked={freeTextBorder}
              onChange={(e) => setFreeTextBorder(e.target.checked)}
            />
            Con borde
          </label>
          <span className="opt-hint">
            Arrastra el rectángulo y escribe dentro
          </span>
        </div>
      )}
      {mode === "stamp" && (
        <div className="tool-options">
          <select
            className="size-select"
            value={stampText}
            onChange={(e) => setStampText(e.target.value)}
          >
            {STAMP_PRESETS.map((s) => (
              <option key={s} value={s}>
                {s}
              </option>
            ))}
            <option value="custom">Personalizado…</option>
          </select>
          {stampText === "custom" && (
            <input
              type="text"
              className="stamp-input"
              placeholder="Texto del sello"
              value={stampCustom}
              onChange={(e) => setStampCustom(e.target.value.toUpperCase())}
            />
          )}
          <div className="swatches">
            {["#c0392b", "#2743c0", "#2ea043", "#1d1c18"].map((c) => (
              <button
                key={c}
                className={`swatch${stampColor === c ? " on" : ""}`}
                style={{ background: c }}
                title={NOMBRE_COLOR[c] ?? c}
                aria-label={NOMBRE_COLOR[c] ?? c}
                aria-pressed={stampColor === c}
                onClick={() => cambiaColorAccion("sello", c)}
              />
            ))}
            <label
              className={`swatch swatch-custom${
                !["#c0392b", "#2743c0", "#2ea043", "#1d1c18"].includes(
                  stampColor,
                )
                  ? " on"
                  : ""
              }`}
              title="Color personalizado"
            >
              <input
                type="color"
                value={stampColor}
                onChange={(e) => cambiaColorAccion("sello", e.target.value)}
              />
              <Icon name="plus" size={10} />
            </label>
          </div>
          <span
            className="sello-preview"
            style={{ color: stampColor, borderColor: stampColor }}
          >
            {(stampText === "custom" ? stampCustom || "SELLO" : stampText)}
          </span>
          <span className="opt-hint">Clic en la página para colocarlo</span>
        </div>
      )}
    </>
  );
}
