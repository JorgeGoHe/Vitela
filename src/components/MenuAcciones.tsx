import Icon from "./Icon";

/** Menú «Más acciones» de la barra superior (Archivo, Documento, Seguridad,
 *  Insertar y Salida). Cada entrada cierra el menú antes de actuar. */
export default function MenuAcciones({
  abierto,
  onToggle,
  onCerrar,
  saveFileAs,
  closeDocument,
  addPdf,
  extractCurrentPage,
  insertPdfHere,
  recortarPagina,
  abrirMarcaAgua,
  abrirEncabezado,
  askRemoveMarginal,
  openProperties,
  signPdf,
  abrirProteger,
  abrirAplanar,
  redactar,
  nuevoCampo,
  nuevoEnlace,
  printDocument,
  abrirExportar,
  exportPlainText,
  abrirComprimir,
}: {
  abierto: boolean;
  onToggle: () => void;
  onCerrar: () => void;
  saveFileAs: () => void;
  /** Cierra el menú por su cuenta. */
  closeDocument: () => void;
  addPdf: () => void;
  extractCurrentPage: () => void;
  insertPdfHere: () => void;
  recortarPagina: () => void;
  abrirMarcaAgua: () => void;
  abrirEncabezado: () => void;
  askRemoveMarginal: (zona: "watermark" | "header") => void;
  openProperties: () => void;
  signPdf: () => void;
  abrirProteger: () => void;
  abrirAplanar: () => void;
  redactar: () => void;
  nuevoCampo: () => void;
  nuevoEnlace: () => void;
  printDocument: () => void;
  abrirExportar: () => void;
  exportPlainText: () => void;
  abrirComprimir: () => void;
}) {
  return (
    <div className="menu-wrap">
      <button
        className="btn btn-icon"
        title="Más acciones"
        aria-label="Más acciones"
        aria-haspopup="menu"
        aria-expanded={abierto}
        onClick={onToggle}
      >
        ⋯
      </button>
      {abierto && (
        <>
          <div className="menu-backdrop" onClick={onCerrar} />
          <div className="menu">
            <div className="menu-titulo">Archivo</div>
            <button
              className="btn"
              onClick={() => {
                onCerrar();
                saveFileAs();
              }}
            >
              <Icon name="save" size={14} />
              Guardar como…
            </button>
            <button className="btn" onClick={closeDocument}>
              <Icon name="close" size={14} />
              Cerrar documento
            </button>
            <button
              className="btn"
              onClick={() => {
                onCerrar();
                addPdf();
              }}
            >
              <Icon name="merge" size={14} />
              Añadir PDF…
            </button>
            <button
              className="btn"
              onClick={() => {
                onCerrar();
                extractCurrentPage();
              }}
            >
              <Icon name="extract" size={14} />
              Extraer página…
            </button>
            <button
              className="btn"
              onClick={() => {
                onCerrar();
                insertPdfHere();
              }}
            >
              <Icon name="merge" size={14} />
              Insertar PDF aquí…
            </button>
            <div className="menu-titulo">Documento</div>
            <button
              className="btn"
              onClick={() => {
                onCerrar();
                recortarPagina();
              }}
            >
              <Icon name="crop" size={14} />
              Recortar página…
            </button>
            <button
              className="btn"
              onClick={() => {
                onCerrar();
                abrirMarcaAgua();
              }}
            >
              <Icon name="water" size={14} />
              Marca de agua…
            </button>
            <button
              className="btn"
              onClick={() => {
                onCerrar();
                abrirEncabezado();
              }}
            >
              <Icon name="hf" size={14} />
              Encabezado, pie y numeración…
            </button>
            <button
              className="btn"
              onClick={() => {
                onCerrar();
                askRemoveMarginal("watermark");
              }}
            >
              <Icon name="water" size={14} />
              Quitar marca de agua…
            </button>
            <button
              className="btn"
              onClick={() => {
                onCerrar();
                askRemoveMarginal("header");
              }}
            >
              <Icon name="hf" size={14} />
              Quitar encabezados y pies…
            </button>
            <button
              className="btn"
              onClick={() => {
                onCerrar();
                openProperties();
              }}
            >
              <Icon name="doc" size={14} />
              Propiedades del documento…
            </button>
            <div className="menu-titulo">Seguridad</div>
            <button
              className="btn"
              onClick={() => {
                onCerrar();
                signPdf();
              }}
            >
              <Icon name="sign" size={14} />
              Firma digital (certificado)…
            </button>
            <button
              className="btn"
              onClick={() => {
                onCerrar();
                abrirProteger();
              }}
            >
              <Icon name="lock" size={14} />
              Proteger con contraseña…
            </button>
            <button
              className="btn"
              onClick={() => {
                onCerrar();
                abrirAplanar();
              }}
            >
              <Icon name="flatten" size={14} />
              Aplanar anotaciones…
            </button>
            <button
              className="btn"
              onClick={() => {
                onCerrar();
                redactar();
              }}
            >
              <Icon name="redact" size={14} />
              Redactar (censurar)…
            </button>
            <div className="menu-titulo">Insertar</div>
            <button
              className="btn"
              onClick={() => {
                onCerrar();
                nuevoCampo();
              }}
            >
              <Icon name="field" size={14} />
              Añadir campo de formulario…
            </button>
            <button
              className="btn"
              onClick={() => {
                onCerrar();
                nuevoEnlace();
              }}
            >
              <Icon name="link" size={14} />
              Añadir enlace…
            </button>
            <div className="menu-titulo">Salida</div>
            <button
              className="btn"
              onClick={() => {
                onCerrar();
                printDocument();
              }}
            >
              <Icon name="printer" size={14} />
              Imprimir…
            </button>
            <button
              className="btn"
              onClick={() => {
                onCerrar();
                abrirExportar();
              }}
            >
              <Icon name="image" size={14} />
              Exportar como imágenes…
            </button>
            <button
              className="btn"
              onClick={() => {
                onCerrar();
                exportPlainText();
              }}
            >
              <Icon name="extract" size={14} />
              Exportar texto…
            </button>
            <button
              className="btn"
              onClick={() => {
                onCerrar();
                abrirComprimir();
              }}
            >
              <Icon name="shrink" size={14} />
              Reducir tamaño…
            </button>
          </div>
        </>
      )}
    </div>
  );
}
