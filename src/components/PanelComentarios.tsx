import { useEffect, useMemo, useRef, useState } from "react";
import type { AnotacionDoc, EstadoComentario } from "../api";
import {
  ESTADOS_COMENTARIO,
  fechaAnotacion,
  firmaAnotacion,
  KIND_ICONS,
  KIND_LABELS,
  KIND_PLURALS,
  MOD,
  nombreEstado,
  plural,
  type FiltroComentarios,
} from "../tipos";
import Icon from "./Icon";

/**
 * Pestaña «Comentarios» del sidebar: todos los comentarios del documento en
 * una sola lista, ordenados por página. Clic (o Enter) lleva a la página y
 * selecciona el comentario; ↑/↓ recorren la lista y Supr borra el que tenga
 * el foco. Se entra con Tab desde el resto de la app o con su atajo, que
 * pide el foco subiendo `focoPedido`.
 */
export default function PanelComentarios({
  comentarios,
  filtro,
  setFiltro,
  filtroAutor,
  setFiltroAutor,
  filtroEstado,
  setFiltroEstado,
  seleccionada,
  focoPedido,
  onSelect,
  onDelete,
  onReply,
  onState,
}: {
  comentarios: AnotacionDoc[];
  filtro: FiltroComentarios;
  setFiltro: (f: FiltroComentarios) => void;
  /** «todos» o el autor exacto: la otra mitad del filtro que pedía C2. */
  filtroAutor: string;
  setFiltroAutor: (a: string) => void;
  /** «todos», «sin» o uno de los cuatro estados de revisión. */
  filtroEstado: string;
  setFiltroEstado: (e: string) => void;
  /** Comentario seleccionado ahora mismo, si está en esta lista. */
  seleccionada: { page: number; index: number } | null;
  /** Sube cada vez que el atajo del panel pide el foco de la lista. */
  focoPedido: number;
  onSelect: (c: AnotacionDoc) => void;
  onDelete: (c: AnotacionDoc) => void;
  onReply: (c: AnotacionDoc, texto: string) => void;
  onState: (c: AnotacionDoc, estado: EstadoComentario) => void;
}) {
  // el comentario al que se está respondiendo, y el borrador de la respuesta
  const [respondiendo, setRespondiendo] = useState<string | null>(null);
  const [borrador, setBorrador] = useState("");
  // los tipos que hay de verdad en el documento: un filtro con opciones
  // vacías no ayuda a nadie
  const tipos = useMemo(() => {
    const vistos = new Map<string, string>();
    for (const c of comentarios) {
      const etiqueta = KIND_PLURALS[c.kind] ?? c.kind;
      if (!vistos.has(etiqueta)) vistos.set(etiqueta, etiqueta);
    }
    return [...vistos.keys()].sort((a, b) => a.localeCompare(b, "es"));
  }, [comentarios]);

  // los autores que hay de verdad, igual que con los tipos. «Sin autor» es
  // uno más: sin él, filtrar por los comentarios que no traen `/T` era
  // imposible aunque la lista los agrupase con ese nombre
  const autores = useMemo(() => {
    const vistos = new Set<string>();
    for (const c of comentarios) vistos.add(c.author || "Sin autor");
    return [...vistos].sort((a, b) => a.localeCompare(b, "es"));
  }, [comentarios]);

  // Ningún filtro puede quedarse sin salida: al borrar el último comentario
  // de un autor su opción desaparecía del desplegable y el panel se quedaba
  // en «Ningún comentario con ese filtro» sin nada que pulsar
  useEffect(() => {
    if (filtro !== "todos" && !tipos.includes(filtro)) setFiltro("todos");
  }, [filtro, tipos, setFiltro]);

  useEffect(() => {
    if (filtroAutor !== "todos" && !autores.includes(filtroAutor)) {
      setFiltroAutor("todos");
    }
  }, [filtroAutor, autores, setFiltroAutor]);

  // las respuestas no son filas sueltas: cuelgan de su comentario, y por eso
  // ni se filtran ni se cuentan aparte
  const respuestas = useMemo(() => {
    const m = new Map<string, AnotacionDoc[]>();
    for (const c of comentarios) {
      if (c.in_reply_to === null || c.in_reply_to === undefined) continue;
      const clave = `${c.page_index}-${c.in_reply_to}`;
      m.set(clave, [...(m.get(clave) ?? []), c]);
    }
    return m;
  }, [comentarios]);

  const raiz = comentarios.filter(
    (c) => c.in_reply_to === null || c.in_reply_to === undefined,
  );

  const lista = raiz.filter(
    (c) =>
      (filtro === "todos" || (KIND_PLURALS[c.kind] ?? c.kind) === filtro) &&
      (filtroAutor === "todos" ||
        (c.author || "Sin autor") === filtroAutor) &&
      (filtroEstado === "todos" ||
        (filtroEstado === "sin" ? !c.state : c.state === filtroEstado)),
  );
  const filtrando =
    filtro !== "todos" || filtroAutor !== "todos" || filtroEstado !== "todos";

  const panelRef = useRef<HTMLDivElement | null>(null);
  // posición a la que hay que devolver el foco cuando la lista se rehaga
  // tras borrar con el teclado (los índices de anotación se corren, así que
  // las filas son otras y el navegador pierde el foco)
  const volverARef = useRef<number | null>(null);

  /** Las filas de la lista, en orden de pantalla. */
  const filas = () =>
    Array.from(
      panelRef.current?.querySelectorAll<HTMLElement>(".com-row") ?? [],
    );

  // el atajo del panel (⌥⌘3) trae el foco a la fila elegida, o a la primera.
  // Solo cuando se pide: la lista se rehace con cada anotación nueva y no
  // debe robar el foco de donde esté el usuario
  useEffect(() => {
    const el = panelRef.current;
    if (focoPedido === 0 || !el) return;
    const f = Array.from(el.querySelectorAll<HTMLElement>(".com-row"));
    (f.find((n) => n.dataset.elegida === "1") ?? f[0] ?? el).focus();
  }, [focoPedido]);

  // tras borrar con Supr el foco se queda en la lista, en el sitio del que
  // se fue (como en Acrobat), no en el principio de la página
  useEffect(() => {
    const el = panelRef.current;
    const pos = volverARef.current;
    if (pos === null || !el) return;
    volverARef.current = null;
    const f = Array.from(el.querySelectorAll<HTMLElement>(".com-row"));
    (f.length === 0 ? el : f[Math.min(pos, f.length - 1)]).focus();
  }, [comentarios, filtro, filtroAutor, filtroEstado]);

  const hayElegida = lista.some(
    (c) => seleccionada?.page === c.page_index && seleccionada.index === c.index,
  );

  /** ↑/↓ mueven el foco por la lista; Enter y Espacio activan la fila;
   *  Supr y Retroceso la borran. */
  function onKeyDown(e: React.KeyboardEvent<HTMLDivElement>, c: AnotacionDoc) {
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      const f = filas();
      const i = f.indexOf(e.currentTarget);
      const delta = e.key === "ArrowDown" ? 1 : -1;
      f[Math.max(0, Math.min(i + delta, f.length - 1))]?.focus();
    } else if (e.key === "Home" || e.key === "End") {
      e.preventDefault();
      const f = filas();
      (e.key === "Home" ? f[0] : f[f.length - 1])?.focus();
    } else if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      onSelect(c);
    } else if (e.key === "Delete" || e.key === "Backspace") {
      // la tecla es de la lista: si sigue, la app borraría además la
      // anotación seleccionada en la página
      e.preventDefault();
      e.stopPropagation();
      volverARef.current = filas().indexOf(e.currentTarget);
      onDelete(c);
    }
  }

  /** Una fila de la lista, la misma para un comentario y para una respuesta
   *  (que solo va indentada): el mismo teclado y el mismo roving tabindex. */
  function fila(c: AnotacionDoc, primera: boolean, esRespuesta: boolean) {
    const elegida =
      seleccionada?.page === c.page_index && seleccionada.index === c.index;
    const fecha = fechaAnotacion(c.modified);
    const firma = firmaAnotacion(c.author, c.modified);
    const estado = nombreEstado(c.state ?? "");
    return (
      <div
        key={`${c.page_index}-${c.index}`}
        className={`com-row${elegida ? " on" : ""}${esRespuesta ? " com-respuesta" : ""}`}
        role="option"
        aria-selected={elegida}
        data-elegida={elegida ? "1" : "0"}
        // sin nada elegido, la primera fila es la que recibe el Tab: con
        // `-1` en todas la lista no se alcanzaba con el teclado
        tabIndex={elegida || (!hayElegida && primera) ? 0 : -1}
        title={[firma, c.contents].filter(Boolean).join("\n") || undefined}
        onClick={() => onSelect(c)}
        onKeyDown={(e) => onKeyDown(e, c)}
      >
        <Icon
          name={esRespuesta ? "forward" : (KIND_ICONS[c.kind] ?? "note")}
          size={13}
        />
        <span className="com-cuerpo">
          {/* autor, fecha y página en una fila que envuelve: en el sidebar
              de 200 px la hora era lo primero que se perdía */}
          <span className="com-firma dato">
            <span className="com-autor">{c.author || "Sin autor"}</span>
            {fecha && <span className="com-fecha">{fecha}</span>}
            <span className="com-pagina">pág. {c.page_index + 1}</span>
          </span>
          <span className="com-texto">
            {/* la fila es UN comentario: el plural es del filtro, que
                cuenta cuántos hay */}
            {c.contents || KIND_LABELS[c.kind] || c.kind}
          </span>
          {estado && <span className="com-estado-etiqueta">{estado}</span>}
        </span>
        <button
          className="com-borrar"
          tabIndex={-1}
          title={`Eliminar el comentario (${MOD}Z lo devuelve)`}
          aria-label="Eliminar el comentario"
          onClick={(e) => {
            e.stopPropagation();
            onDelete(c);
          }}
        >
          <Icon name="close" size={12} />
        </button>
      </div>
    );
  }

  return (
    <div
      className="com-panel"
      ref={panelRef}
      tabIndex={-1}
      onKeyDown={(e) => {
        // con un filtro puesto, Esc es la salida rápida; sin él la tecla
        // sigue hasta la app, donde sale de la herramienta
        if (e.key !== "Escape" || !filtrando) return;
        e.stopPropagation();
        setFiltro("todos");
        setFiltroAutor("todos");
        setFiltroEstado("todos");
      }}
    >
      {comentarios.length > 0 && (
        <span className="com-total dato">
          {filtrando
            ? `${lista.length} de ${plural(raiz.length, "comentario", "comentarios")}`
            : plural(raiz.length, "comentario", "comentarios")}
        </span>
      )}
      {comentarios.length > 0 && (
        <select
          className="size-select com-filtro"
          aria-label="Filtrar los comentarios por tipo"
          value={filtro}
          onChange={(e) => setFiltro(e.target.value)}
        >
          <option value="todos">Todos los tipos</option>
          {tipos.map((t) => (
            <option key={t} value={t}>
              {t}
            </option>
          ))}
        </select>
      )}
      {comentarios.length > 0 && (
        <select
          className="size-select com-filtro"
          aria-label="Filtrar los comentarios por autor"
          value={filtroAutor}
          onChange={(e) => setFiltroAutor(e.target.value)}
        >
          <option value="todos">Todos los autores</option>
          {autores.map((a) => (
            <option key={a} value={a}>
              {a}
            </option>
          ))}
        </select>
      )}
      {comentarios.length > 0 && (
        <select
          className="size-select com-filtro"
          aria-label="Filtrar los comentarios por estado"
          value={filtroEstado}
          onChange={(e) => setFiltroEstado(e.target.value)}
        >
          <option value="todos">Todos los estados</option>
          <option value="sin">Sin estado</option>
          {ESTADOS_COMENTARIO.map(([v, etiqueta]) => (
            <option key={v} value={v}>
              {etiqueta}
            </option>
          ))}
        </select>
      )}
      {comentarios.length === 0 && (
        <p className="sign-empty">Todavía no hay comentarios.</p>
      )}
      {comentarios.length > 0 && lista.length === 0 && (
        <p className="sign-empty">
          Ningún comentario con ese filtro. Esc lo quita.
        </p>
      )}
      {lista.length > 0 && (
        <p className="opt-hint com-pista">
          ↑ ↓ recorren · Enter va · Supr borra
        </p>
      )}
      <div role="listbox" aria-label="Comentarios del documento">
        {lista.map((c, i) => {
          const clave = `${c.page_index}-${c.index}`;
          const hijas = respuestas.get(clave) ?? [];
          return (
            <div key={clave} className="com-hilo">
              {fila(c, i === 0, false)}
              {/* el estado se ve y se cambia EN el propio comentario, no en
                  un panel de propiedades aparte */}
              <div className="com-acciones">
                <select
                  className="size-select com-estado"
                  aria-label={`Estado del comentario de la página ${c.page_index + 1}`}
                  value={c.state || ""}
                  onChange={(e) =>
                    onState(c, e.target.value as EstadoComentario)
                  }
                >
                  <option value="">Sin estado</option>
                  {ESTADOS_COMENTARIO.map(([v, etiqueta]) => (
                    <option key={v} value={v}>
                      {etiqueta}
                    </option>
                  ))}
                </select>
                <button
                  className="btn com-responder"
                  onClick={() => {
                    setRespondiendo(respondiendo === clave ? null : clave);
                    setBorrador("");
                  }}
                >
                  Responder
                </button>
              </div>
              {hijas.map((h) => fila(h, false, true))}
              {respondiendo === clave && (
                <div className="com-borrador">
                  <textarea
                    autoFocus
                    placeholder={`Tu respuesta… (${MOD}Enter la añade, Esc cancela)`}
                    value={borrador}
                    onChange={(e) => setBorrador(e.target.value)}
                    onKeyDown={(e) => {
                      e.stopPropagation();
                      if (e.key === "Escape") {
                        setRespondiendo(null);
                        return;
                      }
                      if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
                        e.preventDefault();
                        if (!borrador.trim()) return;
                        onReply(c, borrador.trim());
                        setRespondiendo(null);
                        setBorrador("");
                      }
                    }}
                  />
                  <div className="card-actions">
                    <button
                      className="btn"
                      onClick={() => setRespondiendo(null)}
                    >
                      Cancelar
                    </button>
                    <button
                      className="btn btn-primary"
                      disabled={!borrador.trim()}
                      onClick={() => {
                        onReply(c, borrador.trim());
                        setRespondiendo(null);
                        setBorrador("");
                      }}
                    >
                      Responder
                    </button>
                  </div>
                </div>
              )}
            </div>
          );
        })}
      </div>
    </div>
  );
}
