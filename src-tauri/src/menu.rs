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
}

/// Entrada que resuelve el sistema, no la UI: no tiene id ni emite evento.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Nativa {
    /// ⌘C: en macOS tiene que llegar al webview por la cadena de
    /// respondedores; un id propio se quedaría con el atajo y la copia
    /// dejaría de funcionar en la app empaquetada.
    Copiar,
    /// ⌘A, por lo mismo.
    SeleccionarTodo,
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
                Elemento::Nativa(Nativa::Copiar),
                Elemento::Nativa(Nativa::SeleccionarTodo),
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
                e("comprimir", "Reducir tamaño…", None, true),
            ],
        },
        Grupo {
            titulo: "Ayuda",
            entradas: vec![
                e("atajos", "Atajos de teclado", None, false),
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
                        Nativa::Copiar => PredefinedMenuItem::copy(app, Some("Copiar")),
                        Nativa::SeleccionarTodo => {
                            PredefinedMenuItem::select_all(app, Some("Seleccionar todo"))
                        }
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

    /// Copiar y Seleccionar todo son entradas del sistema: con un id
    /// propio, macOS se quedaría con ⌘C y ⌘A y no llegarían al webview.
    /// Salir y Acerca de existen fuera de macOS (dentro están en el menú de
    /// la aplicación).
    #[test]
    fn las_entradas_del_sistema_no_tienen_id() {
        let nativas: Vec<Nativa> = estructura()
            .into_iter()
            .flat_map(|g| g.entradas)
            .filter_map(|el| match el {
                Elemento::Nativa(n) => Some(n),
                _ => None,
            })
            .collect();
        for n in [
            Nativa::Copiar,
            Nativa::SeleccionarTodo,
            Nativa::Salir,
            Nativa::AcercaDe,
        ] {
            assert!(nativas.contains(&n), "falta la entrada nativa {n:?}");
        }
        for id in ids() {
            assert!(
                !["copiar", "seleccionar-todo", "salir", "acerca-de"].contains(&id),
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
    /// **`#[ignore]` mientras la UI del ciclo 4 no esté integrada**: el
    /// backend y la UI se desarrollan en ramas distintas y en `src/` sigue
    /// la lista vieja (`extraer`, `ampliar`, `nocturno`…). En cuanto las dos
    /// mitades estén en la misma rama, se quita el `#[ignore]` y el test
    /// pasa a correr en CI, que es el sitio donde tiene que fallar si
    /// alguien vuelve a separar las listas. Se comprueba a mano con
    /// `cargo test -- --ignored`.
    #[ignore = "hasta que la UI del ciclo 4 enrute la lista de menu::estructura()"]
    #[test]
    fn los_ids_del_menu_estan_todos_en_la_ui() {
        let fuentes = fuentes_de_la_ui();
        let de_la_ui = ids_que_enruta_la_ui(&fuentes);
        let del_menu = ids();
        let faltan: Vec<&str> = del_menu
            .iter()
            .copied()
            .filter(|id| !de_la_ui.iter().any(|k| k == id))
            .collect();
        assert!(
            faltan.is_empty(),
            "la UI no enruta estos ids del menú nativo: {faltan:?}"
        );
        let sobran: Vec<&String> = de_la_ui
            .iter()
            .filter(|k| !del_menu.contains(&k.as_str()))
            .collect();
        assert!(
            sobran.is_empty(),
            "la UI enruta ids que el menú nativo no emite: {sobran:?}"
        );
    }
}
