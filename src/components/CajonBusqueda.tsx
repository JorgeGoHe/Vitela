import { useState } from "react";
import { plural, type SearchMatch } from "../tipos";
import type { Reemplazador } from "../hooks/useReemplazo";
import type { BusquedaCarpeta } from "../hooks/useBusquedaCarpeta";
import Icon from "./Icon";

/**
 * El cajón que se despliega del campo de búsqueda: la lista de coincidencias
 * con la frase de alrededor y, debajo, «Reemplazar» y «Reemplazar todo». Sale
 * del campo que el usuario ya está usando, no de un modo nuevo, y va plegado
 * por defecto: quien solo quiere buscar no paga ni un paso.
 */
export default function CajonBusqueda({
  matches,
  matchIdx,
  query,
  termino,
  hayDocumento,
  irAMatch,
  reemplazo,
  carpeta,
  onBuscarCarpeta,
  onAbrirCoincidencia,
}: {
  matches: SearchMatch[];
  matchIdx: number;
  /** Lo escrito en el campo: es lo que se buscaría en la carpeta. */
  termino: string;
  /** Hay un documento abierto. Sin él solo se puede buscar en una carpeta,
   *  que es justo lo que se hace ANTES de saber qué fichero se quiere. */
  hayDocumento: boolean;
  /** El término que se buscó, que es el que va resaltado en cada frase. */
  query: string;
  irAMatch: (i: number) => void;
  reemplazo: Reemplazador;
  /** La búsqueda en carpeta: el otro ámbito del segmentado de arriba. */
  carpeta: BusquedaCarpeta;
  /** Lanza la búsqueda en la carpeta con lo que hay escrito. */
  onBuscarCarpeta: () => void;
  /** Abre ese PDF en una pestaña nueva y salta a la coincidencia. */
  onAbrirCoincidencia: (path: string, pageIndex: number) => void;
}) {
  const paginas = new Set(matches.map((m) => m.page_index)).size;
  // qué grupos están plegados: se abren todos y se pliega lo que estorbe
  const [plegados, setPlegados] = useState<Set<string>>(new Set());
  const enCarpeta = carpeta.ambito === "carpeta" || !hayDocumento;

  return (
    <div className="search-cajon" onMouseDown={(e) => e.stopPropagation()}>
      {/* dónde se busca, como la búsqueda avanzada de Acrobat: en el
          documento de delante o en una carpeta entera */}
      <div className="segmented search-ambito" role="tablist">
        <button
          role="tab"
          className={`btn${enCarpeta ? "" : " on"}`}
          aria-selected={!enCarpeta}
          disabled={!hayDocumento}
          title={
            hayDocumento
              ? "Buscar en el documento de delante"
              : "No hay ningún documento abierto"
          }
          onClick={() => carpeta.setAmbito("documento")}
        >
          Este documento
        </button>
        <button
          role="tab"
          className={`btn${enCarpeta ? " on" : ""}`}
          aria-selected={enCarpeta}
          onClick={() => carpeta.setAmbito("carpeta")}
        >
          Una carpeta…
        </button>
      </div>
      {enCarpeta ? (
        <>
          <div className="card-row">
            <button
              className="btn"
              onClick={() => void carpeta.elegirCarpeta()}
            >
              <Icon name="doc" size={14} />
              {carpeta.carpeta ? "Cambiar de carpeta…" : "Elegir carpeta…"}
            </button>
            <span className="dato opt-hint" title={carpeta.carpeta ?? ""}>
              {carpeta.carpeta
                ? (carpeta.carpeta.split(/[\\/]/).pop() ?? carpeta.carpeta)
                : "ninguna elegida"}
            </span>
          </div>
          <div className="card-row search-lanzar">
            <label className="opt-check">
              <input
                type="checkbox"
                checked={carpeta.recursivo}
                onChange={(e) => carpeta.setRecursivo(e.target.checked)}
              />
              Incluir las subcarpetas
            </label>
            {/* hasta ahora la única forma de lanzarla era Enter, y no lo
                decía nadie */}
            <button
              className="btn btn-primary"
              disabled={carpeta.buscando || !termino.trim()}
              title={
                termino.trim()
                  ? carpeta.carpeta
                    ? `Buscar «${termino}» en ${carpeta.carpeta}`
                    : "Se pedirá la carpeta"
                  : "Escribe primero qué buscar"
              }
              onClick={onBuscarCarpeta}
            >
              Buscar en la carpeta
            </button>
          </div>
          {carpeta.buscando && (
            <div className="card-row">
              <span className="dato search-resumen">
                Buscando… {carpeta.progreso?.hechos ?? 0} /{" "}
                {carpeta.progreso?.total ?? "…"}
                {carpeta.progreso?.fichero
                  ? ` · ${carpeta.progreso.fichero}`
                  : ""}
              </span>
              <button className="btn" onClick={() => void carpeta.cancelar()}>
                Cancelar
              </button>
            </div>
          )}
          {!carpeta.buscando && carpeta.hecho && (
            <span className="dato search-resumen">
              {/* parada a medias: decir «sin coincidencias» sería contestar
                  a una pregunta que no se ha llegado a hacer entera */}
              {carpeta.parada &&
                `Parada en ${carpeta.parada.hechos} de ${plural(
                  carpeta.parada.total,
                  "fichero",
                  "ficheros",
                )} · `}
              {carpeta.grupos.length === 0
                ? carpeta.parada
                  ? "sin coincidencias hasta ahí"
                  : `Sin coincidencias en ${plural(carpeta.mirados, "fichero", "ficheros")}`
                : `${plural(
                    carpeta.grupos.reduce(
                      (n, g) => n + g.coincidencias.length,
                      0,
                    ),
                    "coincidencia",
                    "coincidencias",
                  )} en ${plural(carpeta.grupos.length, "fichero", "ficheros")}`}
            </span>
          )}
          <div className="search-resultados" role="tree">
            {carpeta.grupos.map((g) => {
              const plegado = plegados.has(g.path);
              return (
                <div key={g.path} className="search-grupo">
                  <button
                    className="search-grupo-cab"
                    aria-expanded={!plegado}
                    title={g.path}
                    onClick={() =>
                      setPlegados((v) => {
                        const n = new Set(v);
                        if (plegado) n.delete(g.path);
                        else n.add(g.path);
                        return n;
                      })
                    }
                  >
                    <Icon name={plegado ? "chevRight" : "down"} size={12} />
                    <span className="search-grupo-nombre">{g.nombre}</span>
                    <span className="dato">
                      {plural(
                        g.coincidencias.length,
                        "coincidencia",
                        "coincidencias",
                      )}
                    </span>
                  </button>
                  {!plegado &&
                    g.coincidencias.map((m, i) => (
                      <button
                        key={`${g.path}-${i}`}
                        className="search-resultado"
                        onClick={() =>
                          onAbrirCoincidencia(g.path, m.page_index)
                        }
                      >
                        <span className="dato search-pagina">
                          pág. {m.page_index + 1}
                        </span>
                        <span className="search-frase">
                          {m.before ?? ""}
                          <mark>{carpeta.termino}</mark>
                          {m.after ?? ""}
                        </span>
                      </button>
                    ))}
                </div>
              );
            })}
          </div>
          {/* un PDF con contraseña no rompe la búsqueda, pero callarlo sería
              decir que en esos ficheros no hay nada */}
          {!carpeta.buscando && carpeta.ilegibles.length > 0 && (
            <p className="search-aviso">
              {carpeta.ilegibles.length} de {carpeta.mirados} no se han podido
              abrir (tienen contraseña o están rotos).
            </p>
          )}
        </>
      ) : (
        <>
      <span className="dato search-resumen">
        {plural(matches.length, "coincidencia", "coincidencias")} en{" "}
        {plural(paginas, "página", "páginas")}
      </span>
      <div
        className="search-resultados"
        role="listbox"
        aria-label="Resultados de la búsqueda"
      >
        {matches.map((m, i) => (
          <button
            key={`${m.page_index}-${i}`}
            role="option"
            aria-selected={i === matchIdx}
            className={`search-resultado${i === matchIdx ? " on" : ""}`}
            onClick={() => irAMatch(i)}
          >
            <span className="dato search-pagina">pág. {m.page_index + 1}</span>
            <span className="search-frase">
              {m.before ?? ""}
              <mark>{query}</mark>
              {m.after ?? ""}
            </span>
          </button>
        ))}
      </div>
      {/* lo que no se va a poder hacer se dice ANTES de pulsar, como el
          diálogo de combinar: después ya no hay decisión que tomar */}
      {reemplazo.previo && reemplazo.previo.fuera > 0 && (
        <p className="search-aviso">
          {reemplazo.previo.tratables === 0
            ? `Ninguna de las ${reemplazo.previo.total} está en un texto que se pueda reescribir.`
            : `${reemplazo.previo.fuera} de las ${reemplazo.previo.total} están en un texto que no se puede reescribir; se ${
                reemplazo.previo.tratables === 1
                  ? "reemplazará 1"
                  : `reemplazarán ${reemplazo.previo.tratables}`
              }.`}
        </p>
      )}
      <div className="search-reemplazo">
        <input
          type="text"
          placeholder="Reemplazar con…"
          aria-label="Reemplazar con"
          value={reemplazo.texto}
          onChange={(e) => reemplazo.setTexto(e.target.value)}
        />
        <button
          className="btn"
          title="Reemplaza la coincidencia señalada (y las de su misma línea)"
          disabled={reemplazo.trabajando}
          onClick={() => reemplazo.reemplazar(false)}
        >
          Reemplazar
        </button>
        {/* sin confirmación, como el resto de Vitela: se hace, se dice cuántas
            y ⌘Z lo devuelve entero */}
        <button
          className="btn"
          title="Reemplaza todas las coincidencias del documento"
          disabled={reemplazo.trabajando}
          onClick={() => reemplazo.reemplazar(true)}
        >
          Reemplazar todo
        </button>
      </div>
        </>
      )}
    </div>
  );
}
