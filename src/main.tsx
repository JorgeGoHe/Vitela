import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { subtiposSinNombre } from "./tipos";

// en desarrollo, un aviso si algún subtipo de comentario del backend se ha
// quedado sin nombre en español: así se coló `FileAttachment` en el filtro
// del panel, en inglés y sin icono (AC-076)
if (import.meta.env.DEV) {
  const faltan = subtiposSinNombre();
  if (faltan.length > 0) {
    console.warn(
      `[Vitela] subtipos de comentario sin nombre en español: ${faltan.join(", ")}`,
    );
  }
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
