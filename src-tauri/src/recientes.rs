//! Lista de ficheros recientes (paridad #4). Se guarda en
//! `DIR_DATOS/recientes.json`, como la biblioteca de firmas, y la mantiene
//! la UI: `open_pdf` no la toca a propósito, porque un documento solo entra
//! en la lista cuando se ha abierto de verdad (si el PDF está protegido y
//! el usuario cancela la contraseña, no ha abierto nada).
//!
//! Acrobat muestra ocho, marca los que ya no están en su sitio y deja
//! quitarlos; nunca lanza un error de ruta.

use serde::{Deserialize, Serialize};

/// Tope de la lista, el mismo que Acrobat.
pub(crate) const MAXIMO: usize = 8;

/// Lo que se guarda en disco: la ruta y cuándo se abrió. El resto
/// (`name`, `dir`, `exists`) se recalcula al listar, para que renombrar o
/// mover un fichero por fuera no deje datos rancios.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub(crate) struct Entrada {
    pub path: String,
    pub opened_at: String,
}

#[derive(Serialize, Debug)]
pub struct Reciente {
    pub path: String,
    /// Nombre del fichero, para la línea principal.
    pub name: String,
    /// Carpeta que lo contiene, para la línea secundaria.
    pub dir: String,
    /// Si sigue estando donde se dejó (se calcula al listar).
    pub exists: bool,
    /// Cuándo se abrió por última vez, en ISO 8601.
    pub opened_at: String,
}

/// Fichero de la lista dentro del directorio de datos de la app. `None`
/// mientras el directorio no esté fijado (tests y arranques a medias): la
/// lista de recientes es una comodidad, nunca un motivo de error.
fn fichero() -> Option<std::path::PathBuf> {
    let dir = crate::firmas_visuales::DIR_DATOS.get()?;
    let _ = std::fs::create_dir_all(dir);
    Some(dir.join("recientes.json"))
}

pub(crate) fn lee(fichero: &std::path::Path) -> Vec<Entrada> {
    std::fs::read_to_string(fichero)
        .ok()
        .and_then(|t| serde_json::from_str::<Vec<Entrada>>(&t).ok())
        .unwrap_or_default()
}

fn escribe(fichero: &std::path::Path, lista: &[Entrada]) -> Result<(), String> {
    let json = serde_json::to_string_pretty(lista).map_err(|e| {
        crate::mensaje_llano(format!("No se ha podido guardar la lista de recientes: {e}"))
    })?;
    std::fs::write(fichero, json).map_err(|e| {
        crate::mensaje_llano(format!("No se ha podido guardar la lista de recientes: {e}"))
    })
}

/// Sube `path` al principio de la lista (o lo mete si no estaba) y recorta
/// al tope. Devuelve la lista resultante.
pub(crate) fn toca_en(fichero: &std::path::Path, path: &str) -> Result<Vec<Entrada>, String> {
    let path = path.trim();
    if path.is_empty() {
        return Err("Ruta vacía".into());
    }
    let mut lista = lee(fichero);
    lista.retain(|e| e.path != path);
    lista.insert(
        0,
        Entrada {
            path: path.to_string(),
            opened_at: chrono::Local::now().to_rfc3339(),
        },
    );
    lista.truncate(MAXIMO);
    escribe(fichero, &lista)?;
    Ok(lista)
}

pub(crate) fn quita_en(fichero: &std::path::Path, path: &str) -> Result<Vec<Entrada>, String> {
    let mut lista = lee(fichero);
    lista.retain(|e| e.path != path);
    escribe(fichero, &lista)?;
    Ok(lista)
}

/// Convierte las entradas guardadas en lo que espera la UI, mirando en el
/// disco cuáles siguen existiendo.
pub(crate) fn detalla(lista: Vec<Entrada>) -> Vec<Reciente> {
    lista
        .into_iter()
        .map(|e| {
            let p = std::path::Path::new(&e.path);
            Reciente {
                name: p
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| e.path.clone()),
                dir: p
                    .parent()
                    .map(|d| d.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                exists: p.is_file(),
                path: e.path,
                opened_at: e.opened_at,
            }
        })
        .collect()
}

/// Los ocho últimos documentos abiertos, del más reciente al más antiguo.
#[tauri::command(async)]
pub fn list_recent() -> Result<Vec<Reciente>, String> {
    let Some(f) = fichero() else {
        return Ok(Vec::new());
    };
    Ok(detalla(lee(&f)))
}

/// Registra un documento como recién abierto. Lo llama la UI DESPUÉS de
/// abrirlo con éxito, no `open_pdf`.
#[tauri::command(async)]
pub fn touch_recent(path: String) -> Result<Vec<Reciente>, String> {
    let Some(f) = fichero() else {
        return Ok(Vec::new());
    };
    Ok(detalla(toca_en(&f, &path)?))
}

/// Quita un documento de la lista (el «Quitar» de los que ya no existen).
#[tauri::command(async)]
pub fn remove_recent(path: String) -> Result<Vec<Reciente>, String> {
    let Some(f) = fichero() else {
        return Ok(Vec::new());
    };
    Ok(detalla(quita_en(&f, &path)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fichero_de_prueba(nombre: &str) -> std::path::PathBuf {
        let f = std::env::temp_dir().join(format!("editor_pdf_test_{nombre}.json"));
        let _ = std::fs::remove_file(&f);
        f
    }

    #[test]
    fn ordena_por_ultimo_abierto_y_corta_en_ocho() {
        let f = fichero_de_prueba("recientes_orden");
        for i in 0..10 {
            toca_en(&f, &format!("/tmp/doc{i}.pdf")).expect("tocar");
        }
        let lista = lee(&f);
        assert_eq!(lista.len(), MAXIMO, "el tope son ocho");
        assert_eq!(lista[0].path, "/tmp/doc9.pdf", "el último abierto va primero");
        assert!(
            !lista.iter().any(|e| e.path == "/tmp/doc0.pdf"),
            "los más viejos se caen"
        );

        // volver a abrir uno ya listado lo sube sin duplicarlo
        toca_en(&f, "/tmp/doc5.pdf").expect("tocar de nuevo");
        let lista = lee(&f);
        assert_eq!(lista[0].path, "/tmp/doc5.pdf");
        assert_eq!(
            lista.iter().filter(|e| e.path == "/tmp/doc5.pdf").count(),
            1,
            "sin duplicados"
        );
        assert_eq!(lista.len(), MAXIMO);
        std::fs::remove_file(&f).ok();
    }

    #[test]
    fn marca_los_que_ya_no_existen_y_deja_quitarlos() {
        let f = fichero_de_prueba("recientes_existe");
        let vivo = std::env::temp_dir().join("editor_pdf_test_reciente_vivo.pdf");
        std::fs::write(&vivo, b"%PDF-1.7\n").expect("crear fichero");
        let vivo = vivo.to_string_lossy().into_owned();
        toca_en(&f, "/tmp/no-existe-jamas.pdf").expect("tocar");
        toca_en(&f, &vivo).expect("tocar");

        let lista = detalla(lee(&f));
        assert_eq!(lista[0].path, vivo);
        assert!(lista[0].exists, "el que está debe salir como existente");
        assert_eq!(lista[0].name, "editor_pdf_test_reciente_vivo.pdf");
        assert!(!lista[0].dir.is_empty(), "la carpeta va aparte del nombre");
        assert!(!lista[0].opened_at.is_empty());
        assert!(!lista[1].exists, "el que falta se marca, no da error");

        quita_en(&f, "/tmp/no-existe-jamas.pdf").expect("quitar");
        let lista = detalla(lee(&f));
        assert_eq!(lista.len(), 1);
        assert_eq!(lista[0].path, vivo);

        std::fs::remove_file(&f).ok();
        std::fs::remove_file(&vivo).ok();
    }

    #[test]
    fn una_lista_corrupta_no_rompe_nada() {
        let f = fichero_de_prueba("recientes_corrupto");
        std::fs::write(&f, b"esto no es JSON").expect("escribir basura");
        assert!(lee(&f).is_empty(), "se empieza de cero, sin error");
        toca_en(&f, "/tmp/doc.pdf").expect("tocar sobre lista corrupta");
        assert_eq!(lee(&f).len(), 1);
        std::fs::remove_file(&f).ok();
    }
}
