import { useEffect, useRef } from "react";
import type { Reciente } from "../api";
import { MOD } from "../tipos";
import Icon from "./Icon";

/** Una entrada del menú: icono, nombre y, si la acción tiene atajo, el
 *  atajo a la derecha en Fragment Mono (como en los menús de Acrobat). */
function Entrada({
  icon,
  texto,
  atajo,
  motivo,
  activo,
  onSelect,
}: {
  icon: string;
  texto: string;
  atajo?: string;
  /** Por qué la entrada no se puede usar ahora mismo. Con motivo se enseña
   *  apagada y lo dice al pasar el ratón, como Acrobat: esconderla dejaría
   *  al usuario buscando una función que existe. */
  motivo?: string;
  /** La entrada es un conmutador y está encendido. Se marca con el «✓» de
   *  los menús del sistema: sin él, «Reglas» no dice si las va a poner o a
   *  quitar. `undefined` es «esto no se enciende ni se apaga». */
  activo?: boolean;
  onSelect: () => void;
}) {
  return (
    <button
      className="btn"
      role={activo === undefined ? "menuitem" : "menuitemcheckbox"}
      aria-checked={activo === undefined ? undefined : activo}
      disabled={!!motivo}
      title={
        motivo ?? (atajo ? `${texto.replace(/…$/, "")} (${atajo})` : undefined)
      }
      onClick={onSelect}
    >
      <Icon name={icon} size={14} />
      <span className="menu-texto">{texto}</span>
      {activo !== undefined && (
        <span className="menu-check" aria-hidden>
          {activo && <Icon name="check" size={13} />}
        </span>
      )}
      {atajo && <span className="menu-atajo dato">{atajo}</span>}
    </button>
  );
}

/** Menú «Acciones» de la barra superior (Archivo, Buscar, Ver, Documento,
 *  Seguridad, Insertar y Salida). Cada entrada cierra el menú antes de
 *  actuar; se recorre con ↑/↓, se activa con Enter y Esc lo cierra
 *  devolviendo el foco al botón.
 *
 *  Es el índice de la aplicación: **toda** entrada de aquí existe también
 *  en la barra del sistema con la misma etiqueta, y un test lo comprueba
 *  (`menu::el_menu_nativo_es_un_espejo_del_menu_de_la_app`). Lo que se
 *  llama distinto en cada sitio va en su lista de excepciones, con motivo. */
export default function MenuAcciones({
  recientes,
  abrirReciente,
  abierto,
  onToggle,
  onCerrar,
  openFile,
  saveFile,
  saveFileAs,
  closeDocument,
  addPdf,
  abrirExtraer,
  abrirReemplazar,
  abrirDividir,
  abrirCombinar,
  crearDesdeImagenes,
  insertPdfHere,
  recortarPagina,
  abrirMarcaAgua,
  abrirEncabezado,
  askRemoveMarginal,
  quitarFondo,
  openProperties,
  comparar,
  abrirPreferencias,
  signPdf,
  certificar,
  puedeCertificar,
  firmado,
  abrirProteger,
  cifrarConCertificado,
  puedeQuitarProteccion,
  quitarProteccion,
  abrirAplanar,
  sanear,
  redactar,
  nuevoCampo,
  reconocerCampos,
  adjuntarFichero,
  nuevoEnlace,
  printDocument,
  abrirExportar,
  exportPlainText,
  exportarWord,
  exportarHtml,
  exportarComentarios,
  importarComentarios,
  abrirComprimir,
  leerEnVozAlta,
  leyendo,
  buscar,
  buscarEnCarpeta,
  ver,
}: {
  /** Últimos ficheros abiertos, la misma lista que el estado vacío. */
  recientes: Reciente[];
  abrirReciente: (path: string) => void;
  abierto: boolean;
  onToggle: () => void;
  onCerrar: () => void;
  /** «Abrir…» y «Guardar»: estaban en la barra y en el menú nativo y no en
   *  el de la app, que es el índice de lo que se puede hacer. */
  openFile: () => void;
  saveFile: () => void;
  saveFileAs: () => void;
  /** Cierra el menú por su cuenta. */
  closeDocument: () => void;
  addPdf: () => void;
  abrirExtraer: () => void;
  abrirReemplazar: () => void;
  abrirDividir: () => void;
  /** Rejilla de «Combinar ficheros…»; «Añadir PDF…» se queda para uno solo. */
  abrirCombinar: () => void;
  /** «Crear PDF desde imágenes…»: escribe un fichero nuevo y lo abre. */
  crearDesdeImagenes: () => void;
  insertPdfHere: () => void;
  recortarPagina: () => void;
  abrirMarcaAgua: () => void;
  abrirEncabezado: () => void;
  askRemoveMarginal: (zona: "watermark" | "header") => void;
  /** «Quitar fondo…»: se lleva el color, la imagen y el texto puestos como
   *  fondo, que `remove_marginal_text` no sabía quitar. */
  quitarFondo: () => void;
  openProperties: () => void;
  /** «Comparar con otro PDF…»: dos visores lado a lado; no toca ninguno. */
  comparar: () => void;
  abrirPreferencias: () => void;
  signPdf: () => void;
  /** «Certificar documento…»: firmar diciendo además qué se puede cambiar
   *  después sin romper el sello. */
  certificar: () => void;
  /** Solo la primera firma puede certificar; con una puesta, la entrada se
   *  enseña apagada con el motivo al lado. */
  puedeCertificar: boolean;
  /** El documento lleva firma: proteger o cifrar lo reescriben y la rompen,
   *  así que se enseñan apagadas con el motivo en vez de fallar después. */
  firmado: boolean;
  abrirProteger: () => void;
  /** «Cifrar con certificado…»: en vez de una contraseña, la lista de
   *  destinatarios que podrán abrirlo con su clave privada. */
  cifrarConCertificado: () => void;
  /** Solo se ofrece quitar la contraseña si el documento la tiene. */
  puedeQuitarProteccion: boolean;
  quitarProteccion: () => void;
  abrirAplanar: () => void;
  /** «Quitar información oculta…»: ensayo previo y después confirmar. */
  sanear: () => void;
  redactar: () => void;
  nuevoCampo: () => void;
  /** «Reconocer campos…»: propone los campos de un formulario impreso y
   *  deja revisarlos antes de escribir nada. */
  reconocerCampos: () => void;
  /** «Adjuntar fichero…»: el primer adjunto no se podía poner porque la
   *  pestaña solo sale cuando ya hay alguno. */
  adjuntarFichero: () => void;
  nuevoEnlace: () => void;
  printDocument: () => void;
  abrirExportar: () => void;
  exportPlainText: () => void;
  exportarWord: () => void;
  /** «Página web (.html)…»: el documento como HTML plano, sin JavaScript. */
  exportarHtml: () => void;
  exportarComentarios: () => void;
  /** «Importar comentarios…»: la revisión que devuelve otro revisor en un
   *  `.xfdf` sobre su copia del documento. */
  importarComentarios: () => void;
  abrirComprimir: () => void;
  /** «Leer en voz alta»: empieza por la página que se está leyendo. */
  leerEnVozAlta: () => void;
  leyendo: boolean;
  /** Lleva el foco al campo de búsqueda de la barra, igual que ⌘F. */
  buscar: () => void;
  /** «Buscar en una carpeta…»: no necesita documento abierto. */
  buscarEnCarpeta: () => void;
  /** El grupo «Ver»: los mismos conmutadores que el menú del sistema, con
   *  su estado a la vista. Sin ellos aquí, reglas, guías, cuadrícula, modo
   *  nocturno y modo lectura solo se encontraban pasando el ratón por la
   *  píldora de navegación, y el menú dejaba de ser el índice de la app. */
  ver: {
    reglas: boolean;
    alternarReglas: () => void;
    guias: boolean;
    alternarGuias: () => void;
    cuadricula: boolean;
    alternarCuadricula: () => void;
    ajustarCuadricula: boolean;
    alternarAjuste: () => void;
    nocturno: boolean;
    alternarNocturno: () => void;
    lectura: boolean;
    alternarLectura: () => void;
    pantallaCompleta: boolean;
    alternarPantallaCompleta: () => void;
  };
}) {
  const botonRef = useRef<HTMLButtonElement | null>(null);
  const menuRef = useRef<HTMLDivElement | null>(null);

  // al abrir con teclado o ratón, el foco entra en la primera entrada
  useEffect(() => {
    if (!abierto) return;
    menuRef.current?.querySelector<HTMLButtonElement>(
      "button:not([disabled])",
    )?.focus();
  }, [abierto]);

  function cerrarYVolver() {
    onCerrar();
    botonRef.current?.focus();
  }

  /** Ejecuta la acción con el menú ya cerrado. */
  function ejecutar(accion: () => void) {
    return () => {
      onCerrar();
      accion();
    };
  }

  function onKeyDown(e: React.KeyboardEvent<HTMLDivElement>) {
    const items = Array.from(
      menuRef.current?.querySelectorAll<HTMLButtonElement>(
        "button:not([disabled])",
      ) ?? [],
    );
    if (items.length === 0) return;
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      const i = items.indexOf(document.activeElement as HTMLButtonElement);
      const delta = e.key === "ArrowDown" ? 1 : -1;
      items[(i + delta + items.length) % items.length].focus();
    } else if (e.key === "ArrowLeft" || e.key === "ArrowRight") {
      // un menú desplegado se traga las teclas: sin esto se pasaba de
      // página por debajo del menú
      e.preventDefault();
      e.stopPropagation();
    } else if (e.key === "Home") {
      e.preventDefault();
      items[0].focus();
    } else if (e.key === "End") {
      e.preventDefault();
      items[items.length - 1].focus();
    } else if (e.key === "Escape" || e.key === "Tab") {
      // Esc no debe seguir hasta la app (allí también sale de la herramienta)
      e.preventDefault();
      e.stopPropagation();
      cerrarYVolver();
    }
  }

  return (
    <div className="menu-wrap">
      <button
        ref={botonRef}
        className="btn"
        title="Acciones"
        aria-label="Acciones"
        aria-haspopup="menu"
        aria-expanded={abierto}
        onClick={onToggle}
      >
        <Icon name="more" size={14} />
        <span className="btn-etiqueta">Acciones</span>
      </button>
      {abierto && (
        <>
          <div className="menu-backdrop" onClick={onCerrar} />
          <div
            className="menu"
            role="menu"
            aria-label="Acciones"
            ref={menuRef}
            onKeyDown={onKeyDown}
          >
            {recientes.length > 0 && (
              <>
                <div className="menu-titulo">Abrir reciente</div>
                {recientes.map((r) => (
                  <button
                    key={r.path}
                    className="btn"
                    role="menuitem"
                    title={r.exists ? r.path : `Ya no está en ${r.path}`}
                    disabled={!r.exists}
                    onClick={ejecutar(() => abrirReciente(r.path))}
                  >
                    <Icon name="doc" size={14} />
                    <span className="menu-texto">{r.name}</span>
                  </button>
                ))}
              </>
            )}
            <div className="menu-titulo">Archivo</div>
            <Entrada
              icon="open"
              texto="Abrir…"
              atajo={`${MOD}O`}
              onSelect={ejecutar(openFile)}
            />
            <Entrada
              icon="save"
              texto="Guardar"
              atajo={`${MOD}S`}
              onSelect={ejecutar(saveFile)}
            />
            <Entrada
              icon="save"
              texto="Guardar como…"
              atajo={`⇧${MOD}S`}
              onSelect={ejecutar(saveFileAs)}
            />
            <Entrada icon="close" texto="Cerrar documento" onSelect={closeDocument} />
            <Entrada icon="merge" texto="Añadir PDF…" onSelect={ejecutar(addPdf)} />
            <Entrada
              icon="extract"
              texto="Extraer páginas…"
              onSelect={ejecutar(abrirExtraer)}
            />
            <Entrada
              icon="merge"
              texto="Insertar PDF aquí…"
              onSelect={ejecutar(insertPdfHere)}
            />
            <Entrada
              icon="merge"
              texto="Combinar ficheros…"
              onSelect={ejecutar(abrirCombinar)}
            />
            <Entrada
              icon="extract"
              texto="Reemplazar páginas…"
              onSelect={ejecutar(abrirReemplazar)}
            />
            <Entrada
              icon="extract"
              texto="Dividir documento…"
              onSelect={ejecutar(abrirDividir)}
            />
            <Entrada
              icon="image"
              texto="Crear PDF desde imágenes…"
              onSelect={ejecutar(crearDesdeImagenes)}
            />
            <Entrada
              icon="sliders"
              texto="Preferencias…"
              atajo={`${MOD},`}
              onSelect={ejecutar(abrirPreferencias)}
            />
            <div className="menu-titulo">Buscar</div>
            <Entrada
              icon="search"
              texto="Buscar…"
              atajo={`${MOD}F`}
              onSelect={ejecutar(buscar)}
            />
            <Entrada
              icon="search"
              texto="Buscar en una carpeta…"
              atajo={`⇧${MOD}F`}
              onSelect={ejecutar(buscarEnCarpeta)}
            />
            <div className="menu-titulo">Ver</div>
            <Entrada
              icon="ruler"
              texto="Reglas"
              atajo={`${MOD}R`}
              activo={ver.reglas}
              onSelect={ejecutar(ver.alternarReglas)}
            />
            <Entrada
              icon="ruler"
              texto="Guías"
              atajo={`${MOD};`}
              activo={ver.guias}
              onSelect={ejecutar(ver.alternarGuias)}
            />
            <Entrada
              icon="ruler"
              texto="Cuadrícula"
              atajo={`${MOD}U`}
              activo={ver.cuadricula}
              onSelect={ejecutar(ver.alternarCuadricula)}
            />
            <Entrada
              icon="ruler"
              texto="Ajustar a la cuadrícula"
              atajo={`⇧${MOD}U`}
              activo={ver.ajustarCuadricula}
              onSelect={ejecutar(ver.alternarAjuste)}
            />
            <Entrada
              icon="moon"
              texto="Modo nocturno del documento"
              atajo={`⇧${MOD}L`}
              activo={ver.nocturno}
              onSelect={ejecutar(ver.alternarNocturno)}
            />
            <Entrada
              icon="libro"
              texto="Modo lectura"
              atajo={`⇧${MOD}H`}
              activo={ver.lectura}
              onSelect={ejecutar(ver.alternarLectura)}
            />
            <Entrada
              icon="expand"
              texto="Pantalla completa"
              atajo={`${MOD}L`}
              activo={ver.pantallaCompleta}
              onSelect={ejecutar(ver.alternarPantallaCompleta)}
            />
            <Entrada
              icon="note"
              texto={leyendo ? "Dejar de leer en voz alta" : "Leer en voz alta"}
              atajo={`⇧${MOD}Y`}
              onSelect={ejecutar(leerEnVozAlta)}
            />
            <div className="menu-titulo">Documento</div>
            <Entrada
              icon="crop"
              texto="Recortar página…"
              onSelect={ejecutar(recortarPagina)}
            />
            <Entrada
              icon="water"
              texto="Marca de agua y fondo…"
              onSelect={ejecutar(abrirMarcaAgua)}
            />
            <Entrada
              icon="hf"
              texto="Encabezado, pie y numeración…"
              onSelect={ejecutar(abrirEncabezado)}
            />
            <Entrada
              icon="water"
              texto="Quitar marca de agua…"
              onSelect={ejecutar(() => askRemoveMarginal("watermark"))}
            />
            <Entrada
              icon="water"
              texto="Quitar fondo…"
              onSelect={ejecutar(quitarFondo)}
            />
            <Entrada
              icon="hf"
              texto="Quitar encabezados y pies…"
              onSelect={ejecutar(() => askRemoveMarginal("header"))}
            />
            <Entrada
              icon="copy"
              texto="Comparar con otro PDF…"
              onSelect={ejecutar(comparar)}
            />
            <Entrada
              icon="doc"
              texto="Propiedades del documento…"
              atajo={`${MOD}D`}
              onSelect={ejecutar(openProperties)}
            />
            <div className="menu-titulo">Seguridad</div>
            <Entrada
              icon="sign"
              texto="Firma digital (certificado)…"
              onSelect={ejecutar(signPdf)}
            />
            <Entrada
              icon="sign"
              texto="Certificar documento…"
              motivo={
                puedeCertificar
                  ? undefined
                  : "Ya hay una firma: solo la primera puede certificar"
              }
              onSelect={ejecutar(certificar)}
            />
            <Entrada
              icon="lock"
              texto="Proteger con contraseña…"
              motivo={
                firmado
                  ? "El documento va firmado: cifrarlo lo reescribe y la firma dejaría de valer"
                  : undefined
              }
              onSelect={ejecutar(abrirProteger)}
            />
            <Entrada
              icon="lock"
              texto="Cifrar con certificado…"
              motivo={
                firmado
                  ? "El documento va firmado: cifrarlo lo reescribe y la firma dejaría de valer"
                  : undefined
              }
              onSelect={ejecutar(cifrarConCertificado)}
            />
            {/* atenuada con su motivo, no escondida: dos criterios en el
                mismo grupo dejaban al usuario buscando una entrada que
                existe (al lado, «Certificar…» sí se atenúa) */}
            <Entrada
              icon="lock"
              texto="Quitar la contraseña…"
              motivo={
                puedeQuitarProteccion
                  ? undefined
                  : "Este documento no tiene contraseña puesta"
              }
              onSelect={ejecutar(quitarProteccion)}
            />
            <Entrada
              icon="flatten"
              texto="Fijar las anotaciones en la página…"
              onSelect={ejecutar(abrirAplanar)}
            />
            <Entrada
              icon="redact"
              texto="Redactar (censurar)…"
              onSelect={ejecutar(redactar)}
            />
            <Entrada
              icon="shrink"
              texto="Quitar información oculta…"
              onSelect={ejecutar(sanear)}
            />
            <div className="menu-titulo">Insertar</div>
            <Entrada
              icon="field"
              texto="Reconocer campos…"
              onSelect={ejecutar(reconocerCampos)}
            />
            <Entrada
              icon="field"
              texto="Añadir campo de formulario…"
              onSelect={ejecutar(nuevoCampo)}
            />
            <Entrada icon="link" texto="Añadir enlace…" onSelect={ejecutar(nuevoEnlace)} />
            <Entrada
              icon="clip"
              texto="Adjuntar fichero…"
              onSelect={ejecutar(adjuntarFichero)}
            />
            <div className="menu-titulo">Salida</div>
            <Entrada
              icon="printer"
              texto="Imprimir…"
              atajo={`${MOD}P`}
              onSelect={ejecutar(printDocument)}
            />
            <Entrada
              icon="image"
              texto="Exportar como imágenes…"
              onSelect={ejecutar(abrirExportar)}
            />
            <Entrada
              icon="extract"
              texto="Exportar texto…"
              onSelect={ejecutar(exportPlainText)}
            />
            <Entrada
              icon="doc"
              texto="Word (.docx)…"
              onSelect={ejecutar(exportarWord)}
            />
            <Entrada
              icon="link"
              texto="Página web (.html)…"
              onSelect={ejecutar(exportarHtml)}
            />
            <Entrada
              icon="note"
              texto="Exportar comentarios…"
              onSelect={ejecutar(exportarComentarios)}
            />
            <Entrada
              icon="sticky"
              texto="Importar comentarios…"
              onSelect={ejecutar(importarComentarios)}
            />
            <Entrada
              icon="shrink"
              texto="Reducir tamaño…"
              onSelect={ejecutar(abrirComprimir)}
            />
          </div>
        </>
      )}
    </div>
  );
}
