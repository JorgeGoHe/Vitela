//! Menú nativo del sistema: Archivo, Editar, Ver, Documento y Ayuda.
//!
//! Es un **espejo** del menú «Acciones» de la app, no un segundo sitio con
//! cosas distintas: todo lo que está en la barra de herramientas está
//! también aquí, con su atajo escrito al lado, que es lo que hace Acrobat.
//! Con esto la búsqueda de menús de macOS («Ayuda ▸ buscar») encuentra por
//! fin «Marca de agua» o «Reducir tamaño».
//!
//! El menú no ejecuta nada: cada entrada emite el evento `menu-accion` con
//! `{ "id": "…" }` y la UI lo enruta a la misma función que su botón. Así no
//! hay dos caminos que puedan separarse.

use serde::Serialize;
use tauri::menu::{MenuBuilder, MenuItemBuilder, PredefinedMenuItem, SubmenuBuilder};
use tauri::{AppHandle, Emitter, Runtime};

/// Nombre del evento hacia la UI. Carga: `{ "id": "guardar" }`.
pub(crate) const EVENTO: &str = "menu-accion";

/// Una entrada del menú nativo.
pub(crate) struct Entrada {
    /// El `id` que viaja en el evento. Es el contrato con la UI: está en
    /// CLAUDE.md y no se cambia sin avisar.
    pub id: &'static str,
    pub etiqueta: &'static str,
    /// Atajo en la sintaxis de Tauri (`CmdOrCtrl+O`), o `None`.
    pub atajo: Option<&'static str>,
    /// Se apaga cuando no hay documento abierto, en vez de desaparecer.
    pub necesita_documento: bool,
}

/// Un grupo del menú: su título y sus entradas, con `None` por separador.
pub(crate) struct Grupo {
    pub titulo: &'static str,
    pub entradas: Vec<Option<Entrada>>,
}

fn e(
    id: &'static str,
    etiqueta: &'static str,
    atajo: Option<&'static str>,
    necesita_documento: bool,
) -> Option<Entrada> {
    Some(Entrada {
        id,
        etiqueta,
        atajo,
        necesita_documento,
    })
}

/// La estructura entera del menú, como datos: así se puede comprobar en un
/// test (ids únicos, nada sin etiqueta) sin levantar una ventana.
pub(crate) fn estructura() -> Vec<Grupo> {
    vec![
        Grupo {
            titulo: "Archivo",
            entradas: vec![
                e("abrir", "Abrir…", Some("CmdOrCtrl+O"), false),
                e("abrir-reciente", "Abrir reciente…", None, false),
                None,
                e("guardar", "Guardar", Some("CmdOrCtrl+S"), true),
                e("guardar-como", "Guardar como…", Some("Shift+CmdOrCtrl+S"), true),
                e("cerrar-documento", "Cerrar documento", None, true),
                None,
                e("anadir-pdf", "Añadir PDF…", None, true),
                e("insertar-pdf", "Insertar PDF aquí…", None, true),
                e("combinar-ficheros", "Combinar ficheros…", None, true),
                e("reemplazar-paginas", "Reemplazar páginas…", None, true),
                e("extraer-paginas", "Extraer páginas…", None, true),
                e("dividir-documento", "Dividir documento…", None, true),
                None,
                e("imprimir", "Imprimir…", Some("CmdOrCtrl+P"), true),
            ],
        },
        Grupo {
            titulo: "Editar",
            entradas: vec![
                e("deshacer", "Deshacer", Some("CmdOrCtrl+Z"), true),
                e("rehacer", "Rehacer", Some("Shift+CmdOrCtrl+Z"), true),
                None,
                e("copiar", "Copiar", Some("CmdOrCtrl+C"), true),
                e("seleccionar-todo", "Seleccionar todo", Some("CmdOrCtrl+A"), true),
                None,
                e("buscar", "Buscar…", Some("CmdOrCtrl+F"), true),
                e("buscar-siguiente", "Coincidencia siguiente", Some("CmdOrCtrl+G"), true),
                e(
                    "buscar-anterior",
                    "Coincidencia anterior",
                    Some("Shift+CmdOrCtrl+G"),
                    true,
                ),
                None,
                e("preferencias", "Preferencias…", Some("CmdOrCtrl+,"), false),
            ],
        },
        Grupo {
            titulo: "Ver",
            entradas: vec![
                e("zoom-mas", "Acercar", Some("CmdOrCtrl+Plus"), true),
                e("zoom-menos", "Alejar", Some("CmdOrCtrl+-"), true),
                None,
                e("zoom-pagina", "Página entera", Some("CmdOrCtrl+0"), true),
                e("zoom-100", "Tamaño real", Some("CmdOrCtrl+1"), true),
                e("zoom-ancho", "Ajustar al ancho", Some("CmdOrCtrl+2"), true),
                None,
                e("pagina-una", "Una sola página", None, true),
                e("pagina-continua", "Desplazamiento continuo", None, true),
                e("pagina-dos", "Dos páginas", None, true),
                e("pagina-dos-continua", "Dos páginas continuas", None, true),
                None,
                e("girar-vista-derecha", "Girar la vista a la derecha", Some("Shift+CmdOrCtrl+Plus"), true),
                e("girar-vista-izquierda", "Girar la vista a la izquierda", Some("Shift+CmdOrCtrl+-"), true),
                None,
                e("panel-lateral", "Panel lateral", Some("Alt+CmdOrCtrl+1"), false),
                e("pantalla-completa", "Pantalla completa", Some("CmdOrCtrl+L"), false),
                e("modo-nocturno", "Modo nocturno del documento", None, false),
            ],
        },
        Grupo {
            titulo: "Documento",
            entradas: vec![
                e("organizar-paginas", "Organizar páginas…", None, true),
                e("recortar-pagina", "Recortar página…", None, true),
                e("marca-de-agua", "Marca de agua…", None, true),
                e("encabezado-pie", "Encabezado, pie y numeración…", None, true),
                e("quitar-marca-de-agua", "Quitar marca de agua…", None, true),
                e("quitar-encabezados", "Quitar encabezados y pies…", None, true),
                None,
                e("anadir-campo", "Añadir campo de formulario…", None, true),
                e("anadir-enlace", "Añadir enlace…", None, true),
                None,
                e("firmar", "Firma digital (certificado)…", None, true),
                e("proteger", "Proteger con contraseña…", None, true),
                e("quitar-proteccion", "Quitar la contraseña…", None, true),
                e("aplanar", "Fijar las anotaciones en la página…", None, true),
                e("redactar", "Redactar (censurar)…", None, true),
                e("sanitizar", "Quitar información oculta…", None, true),
                None,
                e("propiedades", "Propiedades del documento…", Some("CmdOrCtrl+D"), true),
                None,
                e("exportar-imagenes", "Exportar como imágenes…", None, true),
                e("exportar-texto", "Exportar texto…", None, true),
                e("comprimir", "Reducir tamaño…", None, true),
            ],
        },
        Grupo {
            titulo: "Ayuda",
            entradas: vec![e("atajos", "Atajos de teclado", None, false)],
        },
    ]
}

#[derive(Serialize, Clone)]
struct Accion {
    id: String,
}

/// Monta el menú nativo y lo pone en la app. `hay_documento` apaga las
/// entradas que no aplican: en Acrobat se atenúan, no desaparecen.
pub(crate) fn instala<R: Runtime>(app: &AppHandle<R>, hay_documento: bool) -> Result<(), String> {
    let mut menu = MenuBuilder::new(app);
    // en macOS el primer submenú es el de la app (Acerca de, Ocultar, Salir)
    #[cfg(target_os = "macos")]
    {
        let app_menu = SubmenuBuilder::new(app, "Vitela")
            .about(None)
            .separator()
            .services()
            .separator()
            .hide()
            .hide_others()
            .show_all()
            .separator()
            .quit()
            .build()
            .map_err(|e| e.to_string())?;
        menu = menu.item(&app_menu);
    }
    for grupo in estructura() {
        let mut sub = SubmenuBuilder::new(app, grupo.titulo);
        for entrada in grupo.entradas {
            match entrada {
                None => sub = sub.separator(),
                Some(entrada) => {
                    let mut item = MenuItemBuilder::with_id(entrada.id, entrada.etiqueta)
                        .enabled(!entrada.necesita_documento || hay_documento);
                    if let Some(atajo) = entrada.atajo {
                        item = item.accelerator(atajo);
                    }
                    let item = item.build(app).map_err(|e| e.to_string())?;
                    sub = sub.item(&item);
                }
            }
        }
        // Ventana lo pone el sistema, pero minimizar y cerrar se esperan
        if grupo.titulo == "Ayuda" {
            sub = sub.separator();
        }
        let sub = sub.build().map_err(|e| e.to_string())?;
        menu = menu.item(&sub);
    }
    let ventana = SubmenuBuilder::new(app, "Ventana")
        .item(&PredefinedMenuItem::minimize(app, None).map_err(|e| e.to_string())?)
        .item(&PredefinedMenuItem::close_window(app, None).map_err(|e| e.to_string())?)
        .build()
        .map_err(|e| e.to_string())?;
    let menu = menu.item(&ventana).build().map_err(|e| e.to_string())?;
    app.set_menu(menu).map_err(|e| e.to_string())?;
    Ok(())
}

/// Reenvía a la UI el `id` de la entrada elegida. El backend no ejecuta
/// nada: la acción vive en un solo sitio, el de la barra de herramientas.
pub(crate) fn reenvia<R: Runtime>(app: &AppHandle<R>, id: &str) {
    let _ = app.emit(EVENTO, Accion { id: id.to_string() });
}

/// La UI avisa de si hay documento abierto para que las entradas que no
/// aplican se atenúen. Se vuelve a montar el menú entero, que es barato y
/// pasa pocas veces (al abrir y al cerrar).
#[tauri::command(async)]
pub fn set_menu_state(app: AppHandle, has_document: bool) -> Result<(), String> {
    let (tx, rx) = std::sync::mpsc::channel();
    let handle = app.clone();
    app.run_on_main_thread(move || {
        let _ = tx.send(instala(&handle, has_document));
    })
    .map_err(|e| e.to_string())?;
    rx.recv().map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Los `id` son el contrato con la UI (están escritos en CLAUDE.md): no
    /// puede haber dos iguales ni ninguno vacío, ni una entrada sin
    /// etiqueta. Un despiste aquí es una entrada de menú que no hace nada.
    #[test]
    fn los_ids_del_menu_son_unicos_y_estan_completos() {
        let mut vistos: Vec<&str> = Vec::new();
        for grupo in estructura() {
            assert!(!grupo.titulo.is_empty(), "un grupo sin título");
            let entradas: Vec<Entrada> = grupo.entradas.into_iter().flatten().collect();
            assert!(!entradas.is_empty(), "el grupo {} está vacío", grupo.titulo);
            for entrada in entradas {
                assert!(!entrada.id.is_empty(), "entrada sin id en {}", grupo.titulo);
                assert!(
                    !entrada.etiqueta.is_empty(),
                    "entrada {} sin etiqueta",
                    entrada.id
                );
                assert!(
                    !vistos.contains(&entrada.id),
                    "el id {} está dos veces",
                    entrada.id
                );
                if let Some(atajo) = entrada.atajo {
                    assert!(
                        !atajo.contains(' '),
                        "el atajo de {} no es sintaxis de Tauri: {atajo}",
                        entrada.id
                    );
                }
                vistos.push(entrada.id);
            }
        }
        assert!(vistos.len() > 30, "el menú nativo se ha quedado corto");
    }

    /// Sin documento abierto, lo que necesita uno se atenúa; lo que no,
    /// sigue disponible (abrir, preferencias, atajos).
    #[test]
    fn sin_documento_solo_quedan_las_entradas_que_valen_sin_el() {
        let sueltas: Vec<&str> = estructura()
            .into_iter()
            .flat_map(|g| g.entradas)
            .flatten()
            .filter(|e| !e.necesita_documento)
            .map(|e| e.id)
            .collect();
        for id in ["abrir", "preferencias", "atajos", "panel-lateral"] {
            assert!(sueltas.contains(&id), "{id} tiene que valer sin documento");
        }
        for id in ["guardar", "imprimir", "firmar", "redactar"] {
            assert!(!sueltas.contains(&id), "{id} necesita un documento abierto");
        }
    }
}
