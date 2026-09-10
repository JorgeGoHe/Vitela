//! **Comparar dos PDF** (Acrobat: «Comparar archivos»), que es la pregunta
//! de quien recibe la versión 3 de un contrato y quiere saber qué han
//! cambiado.
//!
//! Dos decisiones que se notan:
//!
//! - **Las páginas se emparejan por parecido, no por número.** Insertar
//!   una página al principio no puede marcar el documento entero como
//!   cambiado, que es exactamente lo que hace comparar la 1 con la 1 y la
//!   2 con la 2. Se busca la subsecuencia común más larga de páginas que
//!   se parecen (Jaccard de sus palabras), y lo que queda fuera es lo que
//!   se ha añadido o quitado.
//! - **La granularidad es el bloque de texto**, que es la unidad que
//!   Vitela sabe señalar en la página: un bloque cuyo texto ya no está en
//!   la otra versión se marca entero. Diferenciar palabra a palabra
//!   dentro de una línea daría rectángulos que no se corresponden con
//!   ningún objeto del PDF y la interfaz no podría pintarlos donde están.
//!
//! **No toca ninguno de los dos ficheros**: los abre de solo lectura, sin
//! copia de trabajo y sin paso de deshacer.

use serde::Serialize;

/// Una diferencia entre los dos documentos, ya emparejada.
#[derive(Serialize, Debug)]
pub struct Diferencia {
    /// `"igual"`, `"cambiado"`, `"añadido"` (está en B y no en A) o
    /// `"quitado"` (estaba en A y no en B).
    pub tipo: String,
    /// La página del primer documento, si la hay.
    pub pagina_a: Option<u16>,
    /// La del segundo.
    pub pagina_b: Option<u16>,
    /// Los bloques que han dejado de estar, en el espacio propio de la
    /// página del primer documento.
    pub rects_a: Vec<crate::Rect>,
    /// Los que aparecen, en el del segundo.
    pub rects_b: Vec<crate::Rect>,
    /// Lo que decían esos bloques, para la lista de diferencias.
    pub texto_a: String,
    pub texto_b: String,
}

/// El texto de una página, por bloques y ya normalizado para comparar.
struct Pagina {
    bloques: Vec<(String, crate::Rect)>,
    palabras: std::collections::BTreeSet<String>,
}

/// Compara dos documentos y devuelve una entrada por página emparejada,
/// más las que solo están en uno de los dos.
///
/// Comparar **no toca ninguno de los dos ficheros**, y por eso salir de la
/// comparación no pregunta nada.
#[tauri::command(async)]
pub fn compare_pdf(a: String, b: String) -> Result<Vec<Diferencia>, String> {
    crate::on_pdfium_thread(move || {
        let paginas_a = lee(&a)?;
        let paginas_b = lee(&b)?;
        // los dos documentos se sueltan del caché: comparar no es trabajar
        // con ellos, y el caché tiene sitio para cuatro
        crate::invalidate_doc_cache(&a);
        crate::invalidate_doc_cache(&b);
        Ok(empareja(&paginas_a, &paginas_b))
    })
}

/// Lee las páginas de un documento, con sus bloques y sus palabras.
fn lee(path: &str) -> Result<Vec<Pagina>, String> {
    let total = crate::with_doc(path, |doc| Ok(doc.pages().len()))?;
    let mut out = Vec::new();
    for p in 0..total {
        let bloques: Vec<(String, crate::Rect)> = crate::texto::get_text_blocks(path.to_string(), p)?
            .into_iter()
            .filter(|b| !b.text.trim().is_empty())
            .map(|b| {
                (
                    b.text.trim().to_string(),
                    crate::Rect { x: b.x, y: b.y, w: b.w, h: b.h },
                )
            })
            .collect();
        let palabras = bloques
            .iter()
            .flat_map(|(t, _)| palabras_de(t))
            .collect::<std::collections::BTreeSet<String>>();
        out.push(Pagina { bloques, palabras });
    }
    Ok(out)
}

/// Las palabras de un texto, normalizadas: minúsculas y sin puntuación.
/// Comparar «Total:» con «total» no puede dar una diferencia.
fn palabras_de(texto: &str) -> Vec<String> {
    texto
        .split(|c: char| !c.is_alphanumeric())
        .filter(|p| !p.is_empty())
        .map(|p| p.to_lowercase())
        .collect()
}

/// Cuánto se parecen dos páginas: el Jaccard de sus palabras, de 0 a 1.
/// Dos páginas vacías se parecen del todo, que es lo que son.
fn parecido(a: &Pagina, b: &Pagina) -> f32 {
    if a.palabras.is_empty() && b.palabras.is_empty() {
        return 1.0;
    }
    let comunes = a.palabras.intersection(&b.palabras).count() as f32;
    let todas = a.palabras.union(&b.palabras).count() as f32;
    if todas == 0.0 {
        0.0
    } else {
        comunes / todas
    }
}

/// A partir de aquí dos páginas son «la misma, con cambios». Por debajo,
/// son páginas distintas: una que se fue y otra que llegó.
const MISMA_PAGINA: f32 = 0.4;

/// Empareja las páginas de los dos documentos por la subsecuencia común
/// más larga de páginas parecidas, conservando el orden.
fn empareja(a: &[Pagina], b: &[Pagina]) -> Vec<Diferencia> {
    let (n, m) = (a.len(), b.len());
    // tabla clásica de la subsecuencia común más larga, con «se parecen»
    // en el sitio de «son iguales»
    let mut tabla = vec![vec![0u32; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            tabla[i][j] = if parecido(&a[i], &b[j]) >= MISMA_PAGINA {
                tabla[i + 1][j + 1] + 1
            } else {
                tabla[i + 1][j].max(tabla[i][j + 1])
            };
        }
    }
    let mut out = Vec::new();
    let (mut i, mut j) = (0usize, 0usize);
    while i < n && j < m {
        if parecido(&a[i], &b[j]) >= MISMA_PAGINA {
            out.push(compara_pagina(&a[i], &b[j], i as u16, j as u16));
            i += 1;
            j += 1;
        } else if tabla[i + 1][j] >= tabla[i][j + 1] {
            out.push(solo_en_a(&a[i], i as u16));
            i += 1;
        } else {
            out.push(solo_en_b(&b[j], j as u16));
            j += 1;
        }
    }
    while i < n {
        out.push(solo_en_a(&a[i], i as u16));
        i += 1;
    }
    while j < m {
        out.push(solo_en_b(&b[j], j as u16));
        j += 1;
    }
    out
}

fn solo_en_a(p: &Pagina, i: u16) -> Diferencia {
    Diferencia {
        tipo: "quitado".into(),
        pagina_a: Some(i),
        pagina_b: None,
        rects_a: p.bloques.iter().map(|(_, r)| r.clone()).collect(),
        rects_b: Vec::new(),
        texto_a: junta(&p.bloques),
        texto_b: String::new(),
    }
}

fn solo_en_b(p: &Pagina, j: u16) -> Diferencia {
    Diferencia {
        tipo: "añadido".into(),
        pagina_a: None,
        pagina_b: Some(j),
        rects_a: Vec::new(),
        rects_b: p.bloques.iter().map(|(_, r)| r.clone()).collect(),
        texto_a: String::new(),
        texto_b: junta(&p.bloques),
    }
}

fn junta(bloques: &[(String, crate::Rect)]) -> String {
    bloques
        .iter()
        .map(|(t, _)| t.as_str())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Dos páginas que son la misma: qué bloques han cambiado. Un bloque que
/// dice exactamente lo mismo que otro de la otra versión no es una
/// diferencia aunque haya cambiado de sitio —mover un párrafo no es
/// reescribirlo—, y por eso se comparan los textos y no las posiciones.
fn compara_pagina(a: &Pagina, b: &Pagina, i: u16, j: u16) -> Diferencia {
    let normal = |t: &str| palabras_de(t).join(" ");
    let en_b: Vec<String> = b.bloques.iter().map(|(t, _)| normal(t)).collect();
    let en_a: Vec<String> = a.bloques.iter().map(|(t, _)| normal(t)).collect();
    let mut usados_b = vec![false; b.bloques.len()];
    let mut rects_a = Vec::new();
    let mut texto_a = Vec::new();
    for (k, (texto, rect)) in a.bloques.iter().enumerate() {
        match en_b
            .iter()
            .enumerate()
            .find(|(z, t)| !usados_b[*z] && **t == en_a[k])
        {
            Some((z, _)) => usados_b[z] = true,
            None => {
                rects_a.push(rect.clone());
                texto_a.push(texto.clone());
            }
        }
    }
    let mut rects_b = Vec::new();
    let mut texto_b = Vec::new();
    for (z, (texto, rect)) in b.bloques.iter().enumerate() {
        if !usados_b[z] {
            rects_b.push(rect.clone());
            texto_b.push(texto.clone());
        }
    }
    Diferencia {
        tipo: if rects_a.is_empty() && rects_b.is_empty() {
            "igual".into()
        } else {
            "cambiado".into()
        },
        pagina_a: Some(i),
        pagina_b: Some(j),
        rects_a,
        rects_b,
        texto_a: texto_a.join("\n"),
        texto_b: texto_b.join("\n"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::crea_pdf;

    fn tipos(d: &[Diferencia]) -> Vec<&str> {
        d.iter().map(|x| x.tipo.as_str()).collect()
    }

    /// **Comparar dos PDF** (orden 6 del ciclo 9). Lo que hay que probar
    /// es que no miente en los tres casos que se dan de verdad: el
    /// documento que no ha cambiado, la palabra corregida y la página
    /// insertada al principio —que comparando por número marcaría el
    /// documento entero como distinto—.
    #[test]
    fn comparar_dice_lo_que_ha_cambiado_y_no_se_desplaza_con_una_pagina_nueva() {
        let dir = std::env::temp_dir();
        let uno = dir.join("comparar-uno.pdf");
        let dos = dir.join("comparar-dos.pdf");
        crea_pdf(&["Contrato de arrendamiento", "Segunda pagina del contrato"], &uno);

        // dos documentos iguales: ninguna diferencia
        crea_pdf(&["Contrato de arrendamiento", "Segunda pagina del contrato"], &dos);
        let d = compare_pdf(
            uno.to_string_lossy().into_owned(),
            dos.to_string_lossy().into_owned(),
        )
        .expect("comparar");
        assert_eq!(tipos(&d), vec!["igual", "igual"], "{d:?}");
        assert!(d.iter().all(|x| x.rects_a.is_empty() && x.rects_b.is_empty()));

        // una palabra cambiada: una página cambiada, con su rectángulo
        crea_pdf(&["Contrato de compraventa", "Segunda pagina del contrato"], &dos);
        let d = compare_pdf(
            uno.to_string_lossy().into_owned(),
            dos.to_string_lossy().into_owned(),
        )
        .expect("comparar");
        assert_eq!(tipos(&d), vec!["cambiado", "igual"], "{d:?}");
        assert_eq!(d[0].rects_a.len(), 1, "el bloque de la primera página");
        assert_eq!(d[0].rects_b.len(), 1);
        assert!(d[0].texto_a.contains("arrendamiento"));
        assert!(d[0].texto_b.contains("compraventa"));
        assert!(d[0].rects_a[0].w > 10.0, "el rectángulo tiene que estar puesto");

        // **una página insertada al principio no desplaza el resto**: es
        // el caso que hace inútil comparar la 1 con la 1
        crea_pdf(
            &[
                "Portada nueva del expediente",
                "Contrato de arrendamiento",
                "Segunda pagina del contrato",
            ],
            &dos,
        );
        let d = compare_pdf(
            uno.to_string_lossy().into_owned(),
            dos.to_string_lossy().into_owned(),
        )
        .expect("comparar");
        assert_eq!(tipos(&d), vec!["añadido", "igual", "igual"], "{d:?}");
        assert_eq!(d[0].pagina_b, Some(0));
        assert_eq!(d[0].pagina_a, None);
        assert_eq!(d[1].pagina_a, Some(0), "la del contrato sigue siendo la misma");
        assert_eq!(d[1].pagina_b, Some(1));

        // y quitar una se cuenta como lo que es
        let d = compare_pdf(
            dos.to_string_lossy().into_owned(),
            uno.to_string_lossy().into_owned(),
        )
        .expect("comparar al revés");
        assert_eq!(tipos(&d), vec!["quitado", "igual", "igual"], "{d:?}");

        // comparar no toca ninguno de los dos ficheros
        assert_eq!(
            std::fs::read(&uno).expect("leer").len(),
            std::fs::metadata(&uno).expect("peso").len() as usize
        );
        std::fs::remove_file(&uno).ok();
        std::fs::remove_file(&dos).ok();
    }
}
