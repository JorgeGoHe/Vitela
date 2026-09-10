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
//! hay dos caminos que puedan separarse. **La lista de ids de
//! [`estructura`] es la única que hay**: la UI enruta exactamente esos
//! nombres y un test cruza las dos mitades (`los_ids_del_menu_estan_todos_en_la_ui`).
//!
//! Lo que el sistema hace mejor que nosotros va como entrada nativa
//! ([`Nativa`]): Copiar y Seleccionar todo (que en macOS tienen que llegar
//! al webview por la cadena de respondedores, no por un evento), y Salir y
//! Acerca de, que en macOS viven en el menú de la aplicación y en Windows y
//! Linux hay que reponer en Archivo y en Ayuda.

use serde::Serialize;
use std::sync::OnceLock;
use tauri::menu::{AboutMetadata, MenuBuilder, MenuItemBuilder, PredefinedMenuItem, SubmenuBuilder};
use tauri::{AppHandle, Emitter, Runtime};

/// Nombre del evento hacia la UI. Carga: `{ "id": "guardar" }`.
pub(crate) const EVENTO: &str = "menu-accion";

/// Una entrada del menú nativo que emite `menu-accion`.
pub(crate) struct Entrada {
    /// El `id` que viaja en el evento. Es el contrato con la UI: está en
    /// CLAUDE.md y no se cambia sin avisar.
    pub id: &'static str,
    pub etiqueta: &'static str,
    /// Atajo en la sintaxis de Tauri (`CmdOrCtrl+O`), o `None`.
    pub atajo: Option<&'static str>,
    /// Se apaga cuando no hay documento abierto, en vez de desaparecer.
    pub necesita_documento: bool,
    /// **Temporal, y se quita al integrar.** La mitad de la UI de este id
    /// llega en otra rama: el ciclo se desarrolla en paralelo y el backend
    /// va primero. El test cruzado lo avisa por stderr en vez de fallar; en
    /// cuanto la UI lo enruta se le quita la marca y el test vuelve a
    /// exigirlo, que es lo que evita que una entrada de menú se quede
    /// muerta sin que nadie se entere.
    #[allow(dead_code)] // lo lee el test cruzado; se va con la marca
    pub pendiente_ui: bool,
}

/// Entrada que resuelve el sistema, no la UI: no tiene id ni emite evento.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Nativa {
    /// En macOS ya está en el menú de la aplicación; en Windows y Linux se
    /// repone al final de Archivo.
    Salir,
    /// Ídem, al final de Ayuda.
    AcercaDe,
}

impl Nativa {
    /// macOS ya las pone en el menú de la aplicación (el primero, con el
    /// nombre de la app), así que ahí no se repiten.
    fn solo_fuera_de_macos(self) -> bool {
        matches!(self, Nativa::Salir | Nativa::AcercaDe)
    }
}

/// Un elemento de un grupo del menú.
pub(crate) enum Elemento {
    Separador,
    Accion(Entrada),
    Nativa(Nativa),
}

/// Un grupo del menú: su título y sus elementos.
pub(crate) struct Grupo {
    pub titulo: &'static str,
    pub entradas: Vec<Elemento>,
}

fn e(
    id: &'static str,
    etiqueta: &'static str,
    atajo: Option<&'static str>,
    necesita_documento: bool,
) -> Elemento {
    Elemento::Accion(Entrada {
        id,
        etiqueta,
        atajo,
        necesita_documento,
        pendiente_ui: false,
    })
}

/// Igual que [`e`] pero con la marca temporal `pendiente_ui`: la mitad de
/// la UI de este id llega en otra rama. Se le quita al integrar.
fn ep(
    id: &'static str,
    etiqueta: &'static str,
    atajo: Option<&'static str>,
    necesita_documento: bool,
) -> Elemento {
    Elemento::Accion(Entrada {
        id,
        etiqueta,
        atajo,
        necesita_documento,
        pendiente_ui: true,
    })
}

fn sep() -> Elemento {
    Elemento::Separador
}

/// La estructura entera del menú, como datos: así se puede comprobar en un
/// test (ids únicos, nada sin etiqueta, y que la UI los enruta todos) sin
/// levantar una ventana.
pub(crate) fn estructura() -> Vec<Grupo> {
    vec![
        Grupo {
            titulo: "Archivo",
            entradas: vec![
                e("abrir", "Abrir…", Some("CmdOrCtrl+O"), false),
                e("abrir-reciente", "Abrir reciente…", None, false),
                e("crear-desde-imagenes", "Crear PDF desde imágenes…", None, false),
                sep(),
                e("guardar", "Guardar", Some("CmdOrCtrl+S"), true),
                e("guardar-como", "Guardar como…", Some("Shift+CmdOrCtrl+S"), true),
                e("cerrar-documento", "Cerrar documento", None, true),
                sep(),
                e("anadir-pdf", "Añadir PDF…", None, true),
                e("insertar-pdf", "Insertar PDF aquí…", None, true),
                e("combinar-ficheros", "Combinar ficheros…", None, true),
                e("reemplazar-paginas", "Reemplazar páginas…", None, true),
                e("extraer-paginas", "Extraer páginas…", None, true),
                e("dividir-documento", "Dividir documento…", None, true),
                sep(),
                e("imprimir", "Imprimir…", Some("CmdOrCtrl+P"), true),
                sep(),
                Elemento::Nativa(Nativa::Salir),
            ],
        },
        Grupo {
            titulo: "Editar",
            entradas: vec![
                e("deshacer", "Deshacer", Some("CmdOrCtrl+Z"), true),
                e("rehacer", "Rehacer", Some("Shift+CmdOrCtrl+Z"), true),
                sep(),
                // ⌘C y ⌘A dejan de ser entradas del sistema: con
                // `PredefinedMenuItem` AppKit se queda la tecla antes que el
                // webview y, como la selección del visor no es del DOM, en
                // la app empaquetada no copiaban nada. Ahora emiten
                // `menu-accion` como el resto y la UI hace lo mismo que su
                // atajo.
                e("copiar", "Copiar", Some("CmdOrCtrl+C"), true),
                e("seleccionar-todo", "Seleccionar todo", Some("CmdOrCtrl+A"), true),
                sep(),
                e("buscar", "Buscar…", Some("CmdOrCtrl+F"), true),
                e("buscar-siguiente", "Coincidencia siguiente", Some("CmdOrCtrl+G"), true),
                e(
                    "buscar-anterior",
                    "Coincidencia anterior",
                    Some("Shift+CmdOrCtrl+G"),
                    true,
                ),
                sep(),
                e("preferencias", "Preferencias…", Some("CmdOrCtrl+,"), false),
            ],
        },
        Grupo {
            titulo: "Ver",
            entradas: vec![
                e("zoom-mas", "Acercar", Some("CmdOrCtrl+Plus"), true),
                e("zoom-menos", "Alejar", Some("CmdOrCtrl+-"), true),
                sep(),
                e("zoom-pagina", "Página entera", Some("CmdOrCtrl+0"), true),
                e("zoom-100", "Tamaño real", Some("CmdOrCtrl+1"), true),
                e("zoom-ancho", "Ajustar al ancho", Some("CmdOrCtrl+2"), true),
                sep(),
                e("pagina-una", "Una sola página", None, true),
                e("pagina-continua", "Desplazamiento continuo", None, true),
                e("pagina-dos", "Dos páginas", None, true),
                e("pagina-dos-continua", "Dos páginas continuas", None, true),
                sep(),
                e("girar-vista-derecha", "Girar la vista a la derecha", Some("Shift+CmdOrCtrl+Plus"), true),
                e("girar-vista-izquierda", "Girar la vista a la izquierda", Some("Shift+CmdOrCtrl+-"), true),
                sep(),
                e("vista-atras", "Vista anterior", Some("Alt+Left"), true),
                e("vista-adelante", "Vista siguiente", Some("Alt+Right"), true),
                sep(),
                e("panel-lateral", "Panel lateral", Some("Alt+CmdOrCtrl+1"), false),
                e("pantalla-completa", "Pantalla completa", Some("CmdOrCtrl+L"), false),
                e("modo-nocturno", "Modo nocturno del documento", None, false),
                sep(),
                // en Acrobat «Leer en voz alta» vive en Ver, que es donde lo
                // busca quien ya lo ha usado. La etiqueta conmuta en la app
                // mientras suena; aquí se queda la de encenderlo
                ep("leer-en-voz-alta", "Leer en voz alta", Some("Shift+CmdOrCtrl+Y"), true),
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
                sep(),
                e("anadir-campo", "Añadir campo de formulario…", None, true),
                e("anadir-enlace", "Añadir enlace…", None, true),
                sep(),
                e("firmar", "Firma digital (certificado)…", None, true),
                e("proteger", "Proteger con contraseña…", None, true),
                e("quitar-proteccion", "Quitar la contraseña…", None, true),
                e("aplanar", "Fijar las anotaciones en la página…", None, true),
                e("redactar", "Redactar (censurar)…", None, true),
                e("sanitizar", "Quitar información oculta…", None, true),
                sep(),
                e("propiedades", "Propiedades del documento…", Some("CmdOrCtrl+D"), true),
                sep(),
                e("exportar-imagenes", "Exportar como imágenes…", None, true),
                e("exportar-texto", "Exportar texto…", None, true),
                e("exportar-word", "Exportar a Word (.docx)…", None, true),
                ep("exportar-comentarios", "Exportar comentarios…", None, true),
                e("comprimir", "Reducir tamaño…", None, true),
            ],
        },
        Grupo {
            titulo: "Ayuda",
            entradas: vec![
                // ⌘/ es la tecla de la ayuda de teclado en Acrobat y en
                // media docena de apps más; va en el menú para que se vea
                e("atajos", "Atajos de teclado", Some("CmdOrCtrl+/"), false),
                sep(),
                Elemento::Nativa(Nativa::AcercaDe),
            ],
        },
    ]
}

/// Los ids del menú, en orden. Es la lista que enruta la UI; la usan los
/// tests que cruzan las dos mitades del contrato.
#[cfg(test)]
pub(crate) fn ids() -> Vec<&'static str> {
    estructura()
        .into_iter()
        .flat_map(|g| g.entradas)
        .filter_map(|el| match el {
            Elemento::Accion(entrada) => Some(entrada.id),
            _ => None,
        })
        .collect()
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
        for elemento in grupo.entradas {
            match elemento {
                Elemento::Separador => sub = sub.separator(),
                Elemento::Nativa(nativa) => {
                    if cfg!(target_os = "macos") && nativa.solo_fuera_de_macos() {
                        continue;
                    }
                    let item = match nativa {
                        Nativa::Salir => PredefinedMenuItem::quit(app, Some("Salir")),
                        Nativa::AcercaDe => PredefinedMenuItem::about(
                            app,
                            Some("Acerca de Vitela"),
                            Some(AboutMetadata::default()),
                        ),
                    }
                    .map_err(|e| e.to_string())?;
                    sub = sub.item(&item);
                }
                Elemento::Accion(entrada) => {
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
        let sub = sub.build().map_err(|e| e.to_string())?;
        menu = menu.item(&sub);
    }
    // Ventana lo pone el sistema, pero minimizar y cerrar se esperan
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

/// El handle de la app, para poder atenuar el menú desde `open_pdf` y
/// `close_document` sin arrastrar un `AppHandle` por toda la firma de los
/// comandos. Vacío en los tests y en el puente de QA, donde no hay ventana.
static APP: OnceLock<AppHandle> = OnceLock::new();

/// Lo llama el setup de Tauri. Sin esto, [`refleja_documento`] no hace nada.
pub(crate) fn registra_app(app: &AppHandle) {
    let _ = APP.set(app.clone());
}

/// Atenúa o enciende el menú según haya documento abierto. Lo llaman
/// `open_pdf` y `close_document` (y la UI, con `set_menu_state`, cuando
/// cambia de documento sin pasar por ellos). No bloquea: el menú se vuelve
/// a montar en el hilo principal en cuanto pueda.
pub(crate) fn refleja_documento(hay_documento: bool) {
    let Some(app) = APP.get() else { return };
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        let _ = instala(&handle, hay_documento);
    });
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

    fn entradas() -> Vec<Entrada> {
        estructura()
            .into_iter()
            .flat_map(|g| g.entradas)
            .filter_map(|el| match el {
                Elemento::Accion(entrada) => Some(entrada),
                _ => None,
            })
            .collect()
    }

    /// Los `id` son el contrato con la UI (están escritos en CLAUDE.md): no
    /// puede haber dos iguales ni ninguno vacío, ni una entrada sin
    /// etiqueta. Un despiste aquí es una entrada de menú que no hace nada.
    #[test]
    fn los_ids_del_menu_son_unicos_y_estan_completos() {
        let mut vistos: Vec<&str> = Vec::new();
        for grupo in estructura() {
            assert!(!grupo.titulo.is_empty(), "un grupo sin título");
            let entradas: Vec<Entrada> = grupo
                .entradas
                .into_iter()
                .filter_map(|el| match el {
                    Elemento::Accion(entrada) => Some(entrada),
                    _ => None,
                })
                .collect();
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
        assert_eq!(vistos, ids(), "ids() y estructura() no dicen lo mismo");
    }

    /// Dos entradas con el mismo atajo son una que no funciona: el sistema
    /// se queda con la primera y la segunda deja de responder sin decir
    /// nada. Con 25 atajos ya no se lleva de memoria.
    #[test]
    fn ningun_atajo_esta_dos_veces() {
        let mut vistos: Vec<(&str, &str)> = Vec::new();
        for entrada in entradas() {
            let Some(atajo) = entrada.atajo else { continue };
            if let Some((otro, _)) = vistos.iter().find(|(_, a)| *a == atajo) {
                panic!("{atajo} está en {otro} y en {}", entrada.id);
            }
            vistos.push((entrada.id, atajo));
        }
        assert!(
            vistos.iter().any(|(id, a)| *id == "atajos" && *a == "CmdOrCtrl+/"),
            "la pantalla que enseña los atajos tiene que tener el suyo"
        );
    }

    /// Sin documento abierto, lo que necesita uno se atenúa; lo que no,
    /// sigue disponible (abrir, preferencias, atajos).
    #[test]
    fn sin_documento_solo_quedan_las_entradas_que_valen_sin_el() {
        let sueltas: Vec<&str> = entradas()
            .into_iter()
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

    /// Salir y Acerca de sí son del sistema (fuera de macOS; dentro están
    /// en el menú de la aplicación). **Copiar y Seleccionar todo ya no**:
    /// con `PredefinedMenuItem` AppKit se queda ⌘C y ⌘A antes que el
    /// webview y, como la selección del visor no es del DOM, en la app
    /// empaquetada no copiaban nada. Ahora emiten `menu-accion` como el
    /// resto, que es el único camino que la UI controla.
    #[test]
    fn copiar_y_seleccionar_todo_emiten_evento_y_salir_es_del_sistema() {
        let nativas: Vec<Nativa> = estructura()
            .into_iter()
            .flat_map(|g| g.entradas)
            .filter_map(|el| match el {
                Elemento::Nativa(n) => Some(n),
                _ => None,
            })
            .collect();
        for n in [Nativa::Salir, Nativa::AcercaDe] {
            assert!(nativas.contains(&n), "falta la entrada nativa {n:?}");
        }
        for id in ["copiar", "seleccionar-todo"] {
            assert!(
                ids().contains(&id),
                "{id} tiene que emitir menu-accion, no resolverlo el sistema"
            );
        }
        for id in ids() {
            assert!(
                !["salir", "acerca-de"].contains(&id),
                "{id} tiene que ser una entrada nativa, no un id"
            );
        }
    }

    /// El código de la UI (`src/`), leído entero: los ficheros `.ts` y
    /// `.tsx`.
    fn fuentes_de_la_ui() -> Vec<(String, String)> {
        fn recorre(dir: &std::path::Path, out: &mut Vec<(String, String)>) {
            let Ok(entradas) = std::fs::read_dir(dir) else { return };
            for e in entradas.flatten() {
                let ruta = e.path();
                if ruta.is_dir() {
                    recorre(&ruta, out);
                } else if matches!(ruta.extension().and_then(|s| s.to_str()), Some("ts") | Some("tsx"))
                {
                    if let Ok(texto) = std::fs::read_to_string(&ruta) {
                        out.push((ruta.to_string_lossy().into_owned(), texto));
                    }
                }
            }
        }
        let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../src");
        let mut out = Vec::new();
        recorre(&src, &mut out);
        assert!(!out.is_empty(), "no se ha encontrado el código de la UI en {src:?}");
        out
    }

    /// Las claves del mapa `accionesMenu` de la UI, esté en el fichero que
    /// esté. Se toman las que van a la sangría del primer nivel del objeto
    /// (las de dentro de una función anidada van más adentro).
    fn ids_que_enruta_la_ui(fuentes: &[(String, String)]) -> Vec<String> {
        let (fichero, texto) = fuentes
            .iter()
            .find(|(_, t)| t.contains("accionesMenu") && t.contains("accionesMenu"))
            .expect("la UI no define accionesMenu en ningún fichero");
        let desde = texto.find("accionesMenu").unwrap();
        let abre = texto[desde..]
            .find('{')
            .map(|i| desde + i)
            .unwrap_or_else(|| panic!("accionesMenu sin cuerpo en {fichero}"));
        // el bloque, contando llaves (basta: el objeto no lleva llaves
        // dentro de cadenas)
        let mut nivel = 0i32;
        let mut fin = abre;
        for (i, c) in texto[abre..].char_indices() {
            match c {
                '{' => nivel += 1,
                '}' => {
                    nivel -= 1;
                    if nivel == 0 {
                        fin = abre + i;
                        break;
                    }
                }
                _ => {}
            }
        }
        let bloque = &texto[abre + 1..fin];
        let sangria = bloque
            .lines()
            .find(|l| !l.trim().is_empty())
            .map(|l| l.len() - l.trim_start().len())
            .unwrap_or(0);
        let mut claves = Vec::new();
        for linea in bloque.lines() {
            if linea.trim().is_empty() || linea.len() - linea.trim_start().len() != sangria {
                continue;
            }
            let resto = linea.trim_start();
            let (clave, tras) = if let Some(r) = resto.strip_prefix('"') {
                match r.split_once('"') {
                    Some((c, tras)) => (c.to_string(), tras),
                    None => continue,
                }
            } else {
                let n = resto
                    .find(|c: char| !c.is_ascii_alphanumeric() && c != '-' && c != '_')
                    .unwrap_or(resto.len());
                (resto[..n].to_string(), &resto[n..])
            };
            if !clave.is_empty() && tras.trim_start().starts_with(':') {
                claves.push(clave);
            }
        }
        assert!(
            !claves.is_empty(),
            "no se han podido leer las claves de accionesMenu en {fichero}"
        );
        claves
    }

    /// **El test que cruza las dos listas** (la regla del ciclo 4): cada id
    /// que emite el menú nativo tiene que estar enrutado en la UI, y la UI
    /// no puede enrutar ids que el menú no emita. Sin este cruce las dos
    /// mitades se separan en silencio: el `?.()` de JavaScript se traga el
    /// id que no existe y la entrada de menú no hace nada.
    ///
    /// Corre en CI: es el sitio donde tiene que fallar si alguien vuelve a
    /// separar las dos listas.
    #[test]
    fn los_ids_del_menu_estan_todos_en_la_ui() {
        let fuentes = fuentes_de_la_ui();
        let de_la_ui = ids_que_enruta_la_ui(&fuentes);
        let del_menu = ids();
        let pendientes: Vec<&str> = entradas()
            .iter()
            .filter(|e| e.pendiente_ui)
            .map(|e| e.id)
            .collect();
        let faltan: Vec<&str> = del_menu
            .iter()
            .copied()
            .filter(|id| !de_la_ui.iter().any(|k| k == id) && !pendientes.contains(id))
            .collect();
        assert!(
            faltan.is_empty(),
            "la UI no enruta estos ids del menú nativo: {faltan:?}"
        );
        // los marcados como pendientes avisan pero no bloquean: su mitad de
        // la UI llega en otra rama y al integrar se les quita la marca
        let sin_enrutar: Vec<&&str> = pendientes
            .iter()
            .filter(|id| !de_la_ui.iter().any(|k| k == **id))
            .collect();
        if !sin_enrutar.is_empty() {
            eprintln!(
                "[aviso] ids del menú que la UI todavía no enruta (marcados \
                 `pendiente_ui`, quitar la marca al integrar): {sin_enrutar:?}"
            );
        }
        let sobran: Vec<&String> = de_la_ui
            .iter()
            .filter(|k| !del_menu.contains(&k.as_str()))
            .collect();
        assert!(
            sobran.is_empty(),
            "la UI enruta ids que el menú nativo no emite: {sobran:?}"
        );
    }

    /// Entradas del menú «Acciones» de la app que **no deben** estar en la
    /// barra del sistema, con su motivo. La lista tiene que quedarse corta
    /// y argumentada: el menú nativo es un espejo, y cada hueco es una
    /// función que quien la busca por el menú no encuentra.
    const NO_VAN_EN_LA_BARRA: &[(&str, &str)] = &[];

    /// Entradas que dicen lo mismo con otras palabras en cada sitio, con su
    /// motivo. En el menú de la app la entrada va debajo de un título de
    /// grupo que la completa («Salida ▸ Word (.docx)…»); en la barra del
    /// sistema el grupo es otro y la etiqueta tiene que decir sola lo que
    /// hace.
    const EQUIVALENTES: &[(&str, &str, &str)] = &[
        (
            "word (.docx)",
            "exportar a word (.docx)",
            "en la app va bajo el título «Salida»; en la barra del sistema, \
             dentro de «Documento», la etiqueta tiene que decir sola qué hace",
        ),
        (
            "dejar de leer en voz alta",
            "leer en voz alta",
            "la etiqueta de la app conmuta mientras suena; el id es el mismo \
             y el menú del sistema se queda con la de encenderlo",
        ),
    ];

    /// Una etiqueta comparable: sin mayúsculas, sin los puntos suspensivos
    /// del final y sin espacios de sobra.
    fn llana(s: &str) -> String {
        s.trim().trim_end_matches('…').trim().to_lowercase()
    }

    /// Los textos de las `<Entrada …>` del menú «Acciones» de la app. Se
    /// leen del propio JSX: `texto="…"` y también `texto={cond ? "a" : "b"}`,
    /// que es como se escribe una etiqueta que conmuta.
    fn etiquetas_del_menu_de_la_app(fuentes: &[(String, String)]) -> Vec<String> {
        let (fichero, texto) = fuentes
            .iter()
            .find(|(f, t)| f.ends_with("MenuAcciones.tsx") && t.contains("<Entrada"))
            .expect("no se encuentra MenuAcciones.tsx");
        let mut out: Vec<String> = Vec::new();
        for trozo in texto.split("<Entrada").skip(1) {
            // el cuerpo de la etiqueta, hasta el cierre de la entrada
            let fin = trozo.find("/>").unwrap_or(trozo.len());
            let cuerpo = &trozo[..fin];
            let Some(i) = cuerpo.find("texto=") else { continue };
            let resto = &cuerpo[i + "texto=".len()..];
            let literales: Vec<String> = if let Some(dentro) = resto.strip_prefix('"') {
                dentro
                    .split_once('"')
                    .map(|(t, _)| vec![t.to_string()])
                    .unwrap_or_default()
            } else {
                // `{ … }`: se cogen todas las cadenas que haya dentro
                let mut nivel = 0i32;
                let mut hasta = resto.len();
                for (j, c) in resto.char_indices() {
                    match c {
                        '{' => nivel += 1,
                        '}' => {
                            nivel -= 1;
                            if nivel == 0 {
                                hasta = j;
                                break;
                            }
                        }
                        _ => {}
                    }
                }
                resto[..hasta]
                    .split('"')
                    .skip(1)
                    .step_by(2)
                    .map(|t| t.to_string())
                    .collect()
            };
            out.extend(literales.into_iter().filter(|t| !t.trim().is_empty()));
        }
        assert!(
            out.len() > 20,
            "solo se han leído {} entradas de {fichero}",
            out.len()
        );
        out
    }

    /// **R36b — la dirección que le faltaba al espejo.** El test de arriba
    /// prueba que todo id del menú nativo está enrutado; nada probaba lo
    /// contrario, y por eso «Leer en voz alta» y «Exportar comentarios…»
    /// llevaban un ciclo entero en el menú «Acciones» de la app y no en la
    /// barra del sistema, que es donde los busca quien ya los conoce.
    ///
    /// Se cruza por **etiqueta**, que es lo único que las entradas del menú
    /// de la app tienen (no llevan id): cada `<Entrada texto="…">` tiene que
    /// tener su etiqueta en `estructura()`, salvo lo que esté en
    /// `NO_VAN_EN_LA_BARRA` con su motivo.
    #[test]
    fn el_menu_nativo_es_un_espejo_del_menu_de_la_app() {
        let fuentes = fuentes_de_la_ui();
        let de_la_app = etiquetas_del_menu_de_la_app(&fuentes);
        let del_sistema: Vec<String> = entradas().iter().map(|e| llana(e.etiqueta)).collect();

        let faltan: Vec<&String> = de_la_app
            .iter()
            .filter(|t| {
                let l = llana(t);
                if NO_VAN_EN_LA_BARRA.iter().any(|(e, _)| llana(e) == l) {
                    return false;
                }
                let equivalente = EQUIVALENTES
                    .iter()
                    .find(|(app, _, _)| llana(app) == l)
                    .map(|(_, sistema, _)| llana(sistema));
                let buscada = equivalente.unwrap_or(l);
                !del_sistema.contains(&buscada)
            })
            .collect();
        assert!(
            faltan.is_empty(),
            "estas entradas del menú «Acciones» no están en el menú nativo, \
             que tiene que ser su espejo: {faltan:?}. Si de verdad no deben \
             estar en la barra del sistema, van en NO_VAN_EN_LA_BARRA con su \
             motivo; si es que se llaman distinto, en EQUIVALENTES"
        );

        // y las dos listas de excepciones no pueden envejecer en silencio:
        // una excepción que nombra algo que ya no existe tapa un hueco de
        // verdad el día que la etiqueta vuelve
        let sobra: Vec<&str> = NO_VAN_EN_LA_BARRA
            .iter()
            .map(|(e, _)| *e)
            .chain(EQUIVALENTES.iter().map(|(app, _, _)| *app))
            .filter(|e| !de_la_app.iter().any(|t| llana(t) == llana(e)))
            .collect();
        assert!(
            sobra.is_empty(),
            "estas excepciones nombran entradas que ya no están en el menú \
             de la app: {sobra:?}"
        );
    }
}