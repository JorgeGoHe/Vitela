import { Component, type ErrorInfo, type ReactNode } from "react";
import Icon from "./Icon";

/**
 * Red de seguridad de la interfaz (el `ErrorBoundary` de React, que solo
 * existe como clase). Un fallo pintando una página o un panel tiraba el
 * árbol entero y dejaba **la ventana en blanco**, sin decir nada y sin
 * salida: el usuario no podía ni cerrar el documento.
 *
 * Ahora el fallo se queda dentro de su trozo —una página rota no se lleva
 * el panel, ni el panel al visor—, se cuenta en llano y hay dos salidas:
 * volver a intentarlo y, cuando la parte de fuera ofrece una, recargar el
 * documento. El detalle técnico va detrás de «Ver detalles»: es para
 * pegarlo en un informe, no para leerlo.
 */
export default class LimiteError extends Component<
  {
    /** Qué es lo que ha fallado, en la unidad del usuario: «este
     *  documento», «el panel de comentarios». */
    que: string;
    /** Vuelve a cargar el documento desde su copia de trabajo, si la parte
     *  de fuera sabe hacerlo. */
    onRecargar?: () => void;
    children: ReactNode;
  },
  { error: Error | null; detalles: boolean }
> {
  state: { error: Error | null; detalles: boolean } = {
    error: null,
    detalles: false,
  };

  static getDerivedStateFromError(error: Error) {
    return { error, detalles: false };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    // en la consola queda la pila entera, que es lo que hace falta para
    // arreglarlo; en pantalla, la frase
    console.error(`[Vitela] fallo al pintar ${this.props.que}`, error, info);
  }

  reintentar = () => {
    this.setState({ error: null, detalles: false });
  };

  render() {
    const { error } = this.state;
    if (!error) return this.props.children;
    return (
      <div className="limite-error">
        <p>Algo ha ido mal al mostrar {this.props.que}.</p>
        <p className="limite-error-pista">
          El documento no se ha tocado: los cambios siguen en su copia de
          trabajo.
        </p>
        <div className="card-actions">
          <button className="btn" onClick={this.reintentar}>
            <Icon name="undo" size={13} />
            Volver a intentarlo
          </button>
          {this.props.onRecargar && (
            <button
              className="btn btn-primary"
              onClick={() => {
                this.reintentar();
                this.props.onRecargar?.();
              }}
            >
              Recargar el documento
            </button>
          )}
        </div>
        <button
          className="btn limite-error-detalles"
          onClick={() => this.setState({ detalles: !this.state.detalles })}
        >
          {this.state.detalles ? "Ocultar detalles" : "Ver detalles"}
        </button>
        {this.state.detalles && (
          <pre className="dato limite-error-pila">
            {error.message}
            {error.stack ? `\n${error.stack}` : ""}
          </pre>
        )}
      </div>
    );
  }
}
