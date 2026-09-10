import { useState } from "react";
import { useModal } from "../hooks/useModal";
import type { ModoComposicion, OrdenComentarios } from "../api";
import {
  caraDeFolleto,
  hojasDeComposicion,
  paginasImprimibles,
  parseRango,
  plural,
  type OpcionesImprimir,
} from "../tipos";

/** Cuántas columnas tiene una hoja con N páginas encima. */
function columnasDe(porHoja: number): number {
  if (porHoja <= 2) return 2;
  if (porHoja <= 4) return 2;
  if (porHoja <= 6) return 3;
  if (porHoja <= 9) return 3;
  return 4;
}

/**
 * La hoja, dibujada: sin ver la composición, «folleto» es magia negra —el
 * orden 8-1, 2-7 no se le ocurre a nadie— y «6 por hoja» no dice si van en
 * filas o en columnas. Se dibuja aquí, con los números de las páginas tal
 * como van a caer en el papel.
 */
function PreviaComposicion({
  modo,
  porHoja,
  orden,
  borde,
  escala,
  paginas,
}: {
  modo: ModoComposicion;
  porHoja: number;
  orden: "horizontal" | "vertical";
  borde: boolean;
  escala: number;
  paginas: number;
}) {
  if (modo === "folleto") {
    const [izq, der] = caraDeFolleto(paginas);
    return (
      <div className="previa-hoja apaisada">
        <span className="previa-celda">{izq}</span>
        <span className="previa-celda">{der}</span>
      </div>
    );
  }
  if (modo === "poster") {
    const trozos = Math.max(1, Math.ceil(escala / 100));
    return (
      <div
        className="previa-hoja"
        style={{ gridTemplateColumns: `repeat(${trozos}, 1fr)` }}
      >
        {Array.from({ length: trozos * trozos }, (_, i) => (
          <span key={i} className="previa-celda dato">
            1
          </span>
        ))}
      </div>
    );
  }
  const columnas = columnasDe(porHoja);
  const filas = Math.ceil(porHoja / columnas);
  // «vertical» recorre por columnas: la página 2 va debajo de la 1
  const numero = (i: number) => {
    const fila = Math.floor(i / columnas);
    const col = i % columnas;
    return orden === "horizontal" ? i + 1 : col * filas + fila + 1;
  };
  return (
    <div
      className={`previa-hoja${borde ? " con-borde" : ""}`}
      style={{ gridTemplateColumns: `repeat(${columnas}, 1fr)` }}
    >
      {Array.from({ length: porHoja }, (_, i) => (
        <span key={i} className="previa-celda">
          {numero(i)}
        </span>
      ))}
    </div>
  );
}

/**
 * Diálogo de impresión propio, el que Acrobat abre antes del diálogo del
 * sistema: rango, escala y qué se pinta encima del documento.
 */
export default function DialogoImprimir({
  inicial,
  pageCount,
  paginaActual,
  onConfirm,
  onClose,
}: {
  inicial: OpcionesImprimir;
  pageCount: number;
  /** Página que enseña la píldora, para «Página actual». */
  paginaActual: number;
  onConfirm: (opts: OpcionesImprimir) => void;
  onClose: () => void;
}) {
  const [o, setO] = useState<OpcionesImprimir>(inicial);
  // el rango vacío se dice AQUÍ, junto al campo: la banda roja global queda
  // detrás del velo del modal, que es donde nadie la lee
  const rangoVacio =
    o.ambito === "rango" && parseRango(o.rango, pageCount).length === 0;
  // y lo mismo con el filtro de pares/impares: si no deja ninguna página se
  // dice aquí, con las opciones delante y sin cerrar nada
  const hojas = paginasImprimibles(o, pageCount, paginaActual).length;
  const sinPaginas = !rangoVacio && hojas === 0;
  const confirmar = () => {
    if (!rangoVacio && !sinPaginas) onConfirm(o);
  };
  const { ref, onKeyDown } = useModal({ onClose, onConfirm: confirmar });
  const cambia = (parte: Partial<OpcionesImprimir>) =>
    setO((v) => ({ ...v, ...parte }));

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div
        className="modal"
        role="dialog"
        aria-modal="true"
        aria-label="Imprimir"
        onClick={(e) => e.stopPropagation()}
        onKeyDown={onKeyDown}
        ref={ref}
        tabIndex={-1}
      >
        <h3>Imprimir</h3>

        <span className="card-label">Páginas</span>
        <label className="opt-check">
          <input
            type="radio"
            name="ambito"
            checked={o.ambito === "todas"}
            onChange={() => cambia({ ambito: "todas" })}
          />
          Todas <span className="dato">({pageCount})</span>
        </label>
        <label className="opt-check">
          <input
            type="radio"
            name="ambito"
            checked={o.ambito === "actual"}
            onChange={() => cambia({ ambito: "actual" })}
          />
          Página actual <span className="dato">({paginaActual + 1})</span>
        </label>
        <label className="opt-check">
          <input
            type="radio"
            name="ambito"
            checked={o.ambito === "rango"}
            onChange={() => cambia({ ambito: "rango" })}
          />
          Páginas
        </label>
        <input
          type="text"
          placeholder="1-3, 8"
          aria-label={`Páginas a imprimir, de 1 a ${pageCount}`}
          value={o.rango}
          onFocus={() => cambia({ ambito: "rango" })}
          onChange={(e) => cambia({ rango: e.target.value, ambito: "rango" })}
        />
        {rangoVacio && (
          <p className="modal-error" role="alert">
            Escribe qué páginas quieres imprimir, por ejemplo «1-3, 8» (el
            documento tiene {pageCount}).
          </p>
        )}
        <select
          className="size-select"
          aria-label="Imprimir solo las pares o las impares"
          value={o.subconjunto}
          onChange={(e) =>
            cambia({
              subconjunto: e.target.value as OpcionesImprimir["subconjunto"],
            })
          }
        >
          <option value="todas">Todas las del rango</option>
          <option value="pares">Solo las pares</option>
          <option value="impares">Solo las impares</option>
        </select>
        {/* cuántas hojas van a salir, antes de pulsar: es el dato que hace
            falta para decidir el rango, y sale una hoja por página */}
        {!rangoVacio && !sinPaginas && (
          <span className="dato">
            {o.composicion === "ninguna"
              ? `${plural(hojas, "hoja", "hojas")} de ${pageCount}`
              : `${plural(hojas, "página", "páginas")} de ${pageCount}`}
            {o.resumen && " · más el resumen de comentarios"}
          </span>
        )}
        {sinPaginas && (
          <p className="modal-error" role="alert">
            Ninguna de las páginas elegidas es{" "}
            {o.subconjunto === "pares" ? "par" : "impar"}: cambia el filtro o
            las páginas.
          </p>
        )}

        <span className="card-label">Tamaño</span>
        <select
          className="size-select"
          aria-label="Tamaño de la página impresa"
          value={o.escala}
          onChange={(e) =>
            cambia({ escala: e.target.value as OpcionesImprimir["escala"] })
          }
        >
          <option value="ajustar">Ajustar al papel</option>
          <option value="real">Tamaño real</option>
          <option value="personalizada">Escala personalizada</option>
        </select>
        {o.escala === "personalizada" && (
          <label className="prop-field">
            <span className="card-label">Porcentaje</span>
            <input
              type="number"
              min={10}
              max={400}
              aria-label="Porcentaje de escala (10 a 400)"
              value={o.porcentaje}
              onChange={(e) =>
                cambia({
                  porcentaje: Math.min(
                    400,
                    Math.max(10, Number(e.target.value) || 100),
                  ),
                })
              }
            />
          </label>
        )}

        <span className="card-label">Composición</span>
        <select
          className="size-select"
          aria-label="Cómo se montan las páginas en el papel"
          value={o.composicion}
          onChange={(e) =>
            cambia({ composicion: e.target.value as ModoComposicion })
          }
        >
          <option value="ninguna">Una página por hoja</option>
          <option value="nup">Varias páginas por hoja</option>
          <option value="folleto">Folleto (grapado por el centro)</option>
          <option value="poster">Póster (una página en varias hojas)</option>
        </select>
        {o.composicion === "nup" && (
          <div className="card-row">
            <select
              className="size-select"
              aria-label="Cuántas páginas por hoja"
              value={o.comp.por_hoja}
              onChange={(e) =>
                cambia({ comp: { ...o.comp, por_hoja: Number(e.target.value) } })
              }
            >
              {[2, 4, 6, 9, 16].map((n) => (
                <option key={n} value={n}>
                  {n} por hoja
                </option>
              ))}
            </select>
            <select
              className="size-select"
              aria-label="En qué orden se colocan"
              value={o.comp.orden}
              onChange={(e) =>
                cambia({
                  comp: {
                    ...o.comp,
                    orden: e.target.value as "horizontal" | "vertical",
                  },
                })
              }
            >
              <option value="horizontal">En filas</option>
              <option value="vertical">En columnas</option>
            </select>
            <label className="opt-check">
              <input
                type="checkbox"
                checked={o.comp.borde}
                onChange={(e) =>
                  cambia({ comp: { ...o.comp, borde: e.target.checked } })
                }
              />
              Imprimir el borde de cada página
            </label>
          </div>
        )}
        {o.composicion === "folleto" && (
          <div className="card-row">
            <select
              className="size-select"
              aria-label="Por dónde se encuaderna"
              value={o.comp.encuadernacion}
              onChange={(e) =>
                cambia({
                  comp: {
                    ...o.comp,
                    encuadernacion: e.target.value as "izquierda" | "derecha",
                  },
                })
              }
            >
              <option value="izquierda">Encuadernado a la izquierda</option>
              <option value="derecha">Encuadernado a la derecha</option>
            </select>
            <select
              className="size-select"
              aria-label="Qué caras se imprimen"
              value={o.comp.caras}
              onChange={(e) =>
                cambia({
                  comp: {
                    ...o.comp,
                    caras: e.target.value as "ambas" | "anverso" | "reverso",
                  },
                })
              }
            >
              <option value="ambas">Ambas caras</option>
              <option value="anverso">Solo el anverso</option>
              <option value="reverso">Solo el reverso</option>
            </select>
          </div>
        )}
        {o.composicion === "poster" && (
          <div className="card-row">
            <label className="opt-check">
              Ampliar al
              <input
                type="number"
                className="stamp-input"
                style={{ width: 80 }}
                min={100}
                max={1000}
                step={25}
                aria-label="Porcentaje de ampliación del póster"
                value={o.comp.escala_por_ciento}
                onChange={(e) =>
                  cambia({
                    comp: {
                      ...o.comp,
                      escala_por_ciento: Math.min(
                        1000,
                        Math.max(100, Number(e.target.value) || 100),
                      ),
                    },
                  })
                }
              />
              %
            </label>
            <label className="opt-check">
              Solape
              <input
                type="number"
                className="stamp-input"
                style={{ width: 72 }}
                min={0}
                max={50}
                aria-label="Solape entre hojas, en milímetros"
                value={o.comp.solape_mm}
                onChange={(e) =>
                  cambia({
                    comp: {
                      ...o.comp,
                      solape_mm: Math.min(
                        50,
                        Math.max(0, Number(e.target.value) || 0),
                      ),
                    },
                  })
                }
              />
              mm
            </label>
            <label className="opt-check">
              <input
                type="checkbox"
                checked={o.comp.marcas}
                onChange={(e) =>
                  cambia({ comp: { ...o.comp, marcas: e.target.checked } })
                }
              />
              Marcas de corte
            </label>
          </div>
        )}
        {o.composicion !== "ninguna" && (
          <div className="card-row previa-fila">
            <PreviaComposicion
              modo={o.composicion}
              porHoja={o.comp.por_hoja}
              orden={o.comp.orden}
              borde={o.comp.borde}
              escala={o.comp.escala_por_ciento}
              paginas={hojas}
            />
            <span className="dato">
              {plural(hojas, "página", "páginas")} →{" "}
              {plural(
                hojasDeComposicion(o.composicion, o.comp, hojas),
                "hoja",
                "hojas",
              )}
              {o.composicion === "folleto" &&
                ` · orden ${caraDeFolleto(hojas)[0]}-${caraDeFolleto(hojas)[1]}, 2-${caraDeFolleto(hojas)[0] - 1}…`}
              {/* las hojas son las mismas: lo que cambia es que se imprimen
                  por un lado, y eso hay que decirlo */}
              {o.composicion === "folleto" &&
                o.comp.caras !== "ambas" &&
                ` · solo el ${o.comp.caras === "anverso" ? "anverso" : "reverso"}`}
            </span>
          </div>
        )}

        <span className="card-label">Comentarios y formularios</span>
        <select
          className="size-select"
          aria-label="Qué se imprime encima del documento"
          value={o.conMarcas ? "marcas" : "solo"}
          onChange={(e) => cambia({ conMarcas: e.target.value === "marcas" })}
        >
          <option value="marcas">Documento y marcas</option>
          <option value="solo">Solo el documento</option>
        </select>
        {/* la casilla de Acrobat: detrás del documento, una hoja con una
            fila por comentario, que es lo que se lleva a una reunión */}
        <label className="opt-check">
          <input
            type="checkbox"
            checked={o.resumen}
            onChange={(e) => cambia({ resumen: e.target.checked })}
          />
          Imprimir el resumen de comentarios
        </label>
        {o.resumen && (
          <select
            className="size-select"
            aria-label="Orden del resumen de comentarios"
            value={o.ordenResumen}
            onChange={(e) =>
              cambia({ ordenResumen: e.target.value as OrdenComentarios })
            }
          >
            <option value="pagina">Por página</option>
            <option value="autor">Por autor</option>
            <option value="fecha">Por fecha</option>
            <option value="tipo">Por tipo</option>
          </select>
        )}

        <div className="card-actions">
          <button className="btn" onClick={onClose}>
            Cancelar
          </button>
          <button
            className="btn btn-primary"
            disabled={rangoVacio || sinPaginas}
            onClick={confirmar}
          >
            Imprimir
          </button>
        </div>
      </div>
    </div>
  );
}
