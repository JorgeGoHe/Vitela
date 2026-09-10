//! **La composición de impresión** (Acrobat: el desplegable «Composición»
//! del diálogo de imprimir): varias páginas por hoja, folleto y póster.
//!
//! Las tres son lo mismo por dentro: **páginas nuevas con el contenido de
//! las viejas dentro**, cada una como un Form XObject colocado con su
//! matriz. No se toca la copia de trabajo: se escribe un PDF aparte que la
//! interfaz rasteriza e imprime por el camino de siempre, así que no hay
//! mutación ni paso de deshacer.
//!
//! Todo con lopdf: convertir una página en un XObject es coger su
//! `/Contents`, sus `/Resources` y su caja, y eso pdfium-render 0.8 no lo
//! expone. El `/Rotate` de la página **se hornea en la matriz**: se compone
//! lo que se ve, no lo que está escrito.

use lopdf::{Dictionary, Document as LoDoc, Object, ObjectId, Stream};
use serde::{Deserialize, Serialize};

/// Lo que la interfaz elige en el desplegable «Composición».
#[derive(Deserialize, Debug, Default, Clone)]
pub struct OpcionesComposicion {
    /// N-up: 2, 4, 6, 9 o 16, las cinco de Acrobat.
    #[serde(default)]
    pub por_hoja: Option<u8>,
    /// `"horizontal"` (por filas, el defecto) o `"vertical"` (por
    /// columnas).
    #[serde(default)]
    pub orden: Option<String>,
    /// Imprimir el borde de cada página, la casilla de Acrobat.
    #[serde(default)]
    pub borde: Option<bool>,
    /// Folleto: `"izquierda"` (por defecto) o `"derecha"`.
    #[serde(default)]
    pub encuadernacion: Option<String>,
    /// Folleto: `"ambas"` (por defecto), `"anverso"` o `"reverso"`, para
    /// quien imprime a doble cara a mano.
    #[serde(default)]
    pub caras: Option<String>,
    /// Póster: cuánto se amplía la página (2,0 = 200 %).
    #[serde(default)]
    pub escala: Option<f32>,
    /// Póster: solape entre hojas, en milímetros (Acrobat pone 0).
    #[serde(default)]
    pub solape_mm: Option<f32>,
    /// Póster: marcas de corte en las esquinas.
    #[serde(default)]
    pub marcas: Option<bool>,
    /// Qué páginas entran (todas si no se dice).
    #[serde(default)]
    pub page_indices: Option<Vec<u16>>,
    /// Dónde se escribe. Sin él, un temporal que barre el arranque.
    #[serde(default)]
    pub dest_path: Option<String>,
}

/// Lo que sale de componer.
#[derive(Serialize, Debug)]
pub struct Composicion {
    /// El PDF compuesto, para rasterizarlo e imprimirlo.
    pub path: String,
    /// **Hojas de papel**: es lo que dice el pie del diálogo («8 páginas →
    /// 2 hojas»). En un folleto a doble cara son la mitad de las caras.
    pub hojas: u16,
    /// Caras impresas, o sea páginas del PDF compuesto.
    pub caras: u16,
    /// Páginas del documento que han entrado.
    pub paginas: u16,
}

/// Un milímetro en puntos PDF.
const MM: f32 = 72.0 / 25.4;

/// Compone el documento para imprimir y devuelve el PDF resultante.
///
/// `modo` es `"nup"` (varias páginas por hoja), `"folleto"` o `"poster"`.
#[tauri::command(async)]
pub fn compose_print(
    work_path: String,
    modo: String,
    opciones: OpcionesComposicion,
) -> Result<Composicion, String> {
    let destino = match opciones.dest_path.clone() {
        Some(d) => d,
        None => {
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0);
            std::env::temp_dir()
                .join(format!("vitela-composicion-{nanos}.pdf"))
                .to_string_lossy()
                .into_owned()
        }
    };
    crate::on_pdfium_thread(move || {
        // se lee la copia de trabajo y se escribe fuera: no es una
        // mutación, así que no deja paso de deshacer
        crate::invalidate_doc_cache(&work_path);
        let mut doc = LoDoc::load(&work_path)
            .map_err(|e| crate::mensaje_llano(format!("No se ha podido leer el PDF: {e}")))?;
        let paginas: Vec<ObjectId> = doc.get_pages().into_values().collect();
        let elegidas: Vec<ObjectId> = match &opciones.page_indices {
            Some(v) => {
                let mut ids = Vec::new();
                for i in v {
                    if let Some(id) = paginas.get(*i as usize) {
                        ids.push(*id);
                    }
                }
                ids
            }
            None => paginas.clone(),
        };
        if elegidas.is_empty() {
            return Err("No hay ninguna página que componer".into());
        }
        let entradas: Vec<Entrada> = elegidas
            .iter()
            .map(|id| Entrada::de(&mut doc, *id))
            .collect::<Result<_, _>>()?;

        let hojas = match modo.as_str() {
            "nup" => compone_nup(&mut doc, &entradas, &opciones)?,
            "folleto" => compone_folleto(&mut doc, &entradas, &opciones)?,
            "poster" => compone_poster(&mut doc, &entradas, &opciones)?,
            otro => return Err(format!("Composición desconocida: {otro}")),
        };
        let caras = hojas.len() as u16;
        // los folletos a doble cara gastan media hoja por cara
        let papel = if modo == "folleto" && caras > 1 {
            caras.div_ceil(2)
        } else {
            caras
        };
        pon_las_paginas(&mut doc, hojas)?;
        doc.prune_objects();
        doc.save(&destino)
            .map_err(|e| crate::mensaje_llano(format!("No se ha podido escribir {destino}: {e}")))?;
        Ok(Composicion {
            path: destino,
            hojas: papel,
            caras,
            paginas: entradas.len() as u16,
        })
    })
}

/// Una página del documento ya convertida en Form XObject, con el tamaño
/// que **se ve** (el `/Rotate` horneado en la matriz).
struct Entrada {
    forma: ObjectId,
    /// Matriz que lleva el contenido a `[0, 0, ancho, alto]`.
    base: [f32; 6],
    ancho: f32,
    alto: f32,
}

impl Entrada {
    fn de(doc: &mut LoDoc, page_id: ObjectId) -> Result<Entrada, String> {
        let caja = crate::formularios2::caja_de_pagina(doc, page_id)?;
        let rot = rotacion(doc, page_id);
        let (w, h) = (caja[2] - caja[0], caja[3] - caja[1]);
        if w <= 1.0 || h <= 1.0 {
            return Err("Una de las páginas no tiene tamaño".into());
        }
        let (ancho, alto) = if rot % 180 == 90 { (h, w) } else { (w, h) };
        // rotar el contenido al revés que la página es lo que hace que se
        // componga lo que se ve
        let (a, b, c, d, tx, ty) = match rot {
            90 => (0.0, -1.0, 1.0, 0.0, 0.0, w),
            180 => (-1.0, 0.0, 0.0, -1.0, w, h),
            270 => (0.0, 1.0, -1.0, 0.0, h, 0.0),
            _ => (1.0, 0.0, 0.0, 1.0, 0.0, 0.0),
        };
        let base = [
            a,
            b,
            c,
            d,
            tx - caja[0] * a - caja[1] * c,
            ty - caja[0] * b - caja[1] * d,
        ];
        Ok(Entrada {
            forma: forma_de_pagina(doc, page_id, caja)?,
            base,
            ancho,
            alto,
        })
    }

    /// El `cm` con el que colocar esta página escalada por `s` en
    /// `(ox, oy)`.
    fn colocada(&self, s: f32, ox: f32, oy: f32) -> [f32; 6] {
        let m = self.base;
        [
            m[0] * s,
            m[1] * s,
            m[2] * s,
            m[3] * s,
            m[4] * s + ox,
            m[5] * s + oy,
        ]
    }
}

/// `/Rotate` de la página, heredado del árbol si hace falta.
fn rotacion(doc: &LoDoc, page_id: ObjectId) -> u16 {
    let mut actual = page_id;
    for _ in 0..32 {
        let Ok(dict) = doc.get_object(actual).and_then(|o| o.as_dict()) else {
            return 0;
        };
        if let Ok(r) = dict.get(b"Rotate").and_then(|o| o.as_i64()) {
            return r.rem_euclid(360) as u16;
        }
        match dict.get(b"Parent").and_then(|o| o.as_reference()) {
            Ok(p) => actual = p,
            Err(_) => return 0,
        }
    }
    0
}

/// Convierte una página en un Form XObject: su contenido, sus recursos y su
/// caja. Es lo que permite pintar una página dentro de otra.
fn forma_de_pagina(doc: &mut LoDoc, page_id: ObjectId, caja: [f32; 4]) -> Result<ObjectId, String> {
    let contenido = doc
        .get_page_content(page_id)
        .map_err(|e| crate::mensaje_llano(format!("No se ha podido leer la página: {e}")))?;
    let recursos = match doc
        .get_object(page_id)
        .and_then(|o| o.as_dict())
        .map_err(|e| e.to_string())?
        .get(b"Resources")
    {
        Ok(Object::Reference(id)) => Object::Reference(*id),
        Ok(Object::Dictionary(d)) => Object::Dictionary(d.clone()),
        _ => Object::Dictionary(Dictionary::new()),
    };
    let mut d = Dictionary::new();
    d.set("Type", Object::Name(b"XObject".to_vec()));
    d.set("Subtype", Object::Name(b"Form".to_vec()));
    d.set("FormType", 1i64);
    d.set(
        "BBox",
        Object::Array(vec![
            caja[0].into(),
            caja[1].into(),
            caja[2].into(),
            caja[3].into(),
        ]),
    );
    d.set("Resources", recursos);
    let mut s = Stream::new(d, contenido);
    let _ = s.compress();
    Ok(doc.add_object(Object::Stream(s)))
}

/// Una hoja compuesta: su tamaño y lo que va dentro.
struct Hoja {
    ancho: f32,
    alto: f32,
    ops: String,
    formas: Vec<(String, ObjectId)>,
}

impl Hoja {
    fn nueva(ancho: f32, alto: f32) -> Hoja {
        Hoja {
            ancho,
            alto,
            ops: String::new(),
            formas: Vec::new(),
        }
    }

    /// Pinta una página dentro con la matriz dada.
    fn pon(&mut self, e: &Entrada, m: [f32; 6]) {
        let nombre = format!("VitelaP{}", self.formas.len());
        self.ops.push_str(&format!(
            "q {:.4} {:.4} {:.4} {:.4} {:.2} {:.2} cm /{nombre} Do Q\n",
            m[0], m[1], m[2], m[3], m[4], m[5]
        ));
        self.formas.push((nombre, e.forma));
    }

    fn borde(&mut self, x: f32, y: f32, w: f32, h: f32) {
        self.ops
            .push_str(&format!("q 0.6 G 0.5 w {x:.2} {y:.2} {w:.2} {h:.2} re S Q\n"));
    }
}

/// Cambia el árbol de páginas del documento por las hojas compuestas.
fn pon_las_paginas(doc: &mut LoDoc, hojas: Vec<Hoja>) -> Result<(), String> {
    let pages_id = doc
        .catalog()
        .and_then(|c| c.get(b"Pages"))
        .and_then(|o| o.as_reference())
        .map_err(|e| e.to_string())?;
    let mut ids = Vec::new();
    for hoja in hojas {
        let mut xobj = Dictionary::new();
        for (nombre, id) in &hoja.formas {
            xobj.set(nombre.as_str(), Object::Reference(*id));
        }
        let mut recursos = Dictionary::new();
        recursos.set("XObject", Object::Dictionary(xobj));
        let mut stream = Stream::new(Dictionary::new(), hoja.ops.into_bytes());
        let _ = stream.compress();
        let contenido = doc.add_object(Object::Stream(stream));
        let mut pagina = Dictionary::new();
        pagina.set("Type", Object::Name(b"Page".to_vec()));
        pagina.set("Parent", Object::Reference(pages_id));
        pagina.set(
            "MediaBox",
            Object::Array(vec![
                0.into(),
                0.into(),
                hoja.ancho.into(),
                hoja.alto.into(),
            ]),
        );
        pagina.set("Resources", Object::Dictionary(recursos));
        pagina.set("Contents", Object::Reference(contenido));
        ids.push(Object::Reference(doc.add_object(pagina)));
    }
    let total = ids.len() as i64;
    let pages = doc
        .get_object_mut(pages_id)
        .and_then(|o| o.as_dict_mut())
        .map_err(|e| e.to_string())?;
    pages.set("Kids", Object::Array(ids));
    pages.set("Count", total);
    Ok(())
}

/// La rejilla de un N-up: las cinco de Acrobat.
fn rejilla(n: u8) -> Result<(u8, u8), String> {
    Ok(match n {
        2 => (1, 2),
        4 => (2, 2),
        6 => (2, 3),
        9 => (3, 3),
        16 => (4, 4),
        _ => return Err("Las páginas por hoja son 2, 4, 6, 9 o 16".into()),
    })
}

/// Varias páginas por hoja. El tamaño de la hoja es el de la primera
/// página, que es lo que hace Acrobat cuando no se le dice otra cosa.
fn compone_nup(
    _doc: &mut LoDoc,
    entradas: &[Entrada],
    op: &OpcionesComposicion,
) -> Result<Vec<Hoja>, String> {
    let n = op.por_hoja.unwrap_or(2);
    let (cols, filas) = rejilla(n)?;
    let vertical = op.orden.as_deref() == Some("vertical");
    let borde = op.borde.unwrap_or(false);
    let (hw, hh) = (entradas[0].ancho, entradas[0].alto);
    let margen = 8.0;
    let (celda_w, celda_h) = (
        (hw - margen * 2.0) / cols as f32,
        (hh - margen * 2.0) / filas as f32,
    );
    let mut hojas = Vec::new();
    for grupo in entradas.chunks(n as usize) {
        let mut hoja = Hoja::nueva(hw, hh);
        for (i, e) in grupo.iter().enumerate() {
            let (col, fila) = if vertical {
                ((i / filas as usize) as u8, (i % filas as usize) as u8)
            } else {
                ((i % cols as usize) as u8, (i / cols as usize) as u8)
            };
            // las filas se llenan de arriba abajo, que es como se lee
            let x0 = margen + col as f32 * celda_w;
            let y0 = margen + (filas - 1 - fila) as f32 * celda_h;
            let s = ((celda_w - 4.0) / e.ancho).min((celda_h - 4.0) / e.alto);
            let ox = x0 + (celda_w - e.ancho * s) / 2.0;
            let oy = y0 + (celda_h - e.alto * s) / 2.0;
            hoja.pon(e, e.colocada(s, ox, oy));
            if borde {
                hoja.borde(ox, oy, e.ancho * s, e.alto * s);
            }
        }
        hojas.push(hoja);
    }
    Ok(hojas)
}

/// El orden de las páginas de un folleto grapado por el centro: cada cara
/// lleva dos, y la primera es la última con la primera.
///
/// Con ocho páginas: 8-1, 2-7, 6-3, 4-5. Las que faltan para llenar el
/// múltiplo de cuatro van en blanco (`None`), que es el papel que sobra.
pub(crate) fn orden_de_folleto(paginas: usize) -> Vec<(Option<usize>, Option<usize>)> {
    let total = paginas.div_ceil(4) * 4;
    let hay = |i: usize| (i < paginas).then_some(i);
    let mut out = Vec::new();
    let (mut izq, mut der) = (total, 1usize);
    while der < izq {
        // cara de fuera: la última con la primera
        out.push((hay(izq - 1), hay(der - 1)));
        izq -= 1;
        der += 1;
        if der >= izq {
            break;
        }
        // cara de dentro, al revés
        out.push((hay(der - 1), hay(izq - 1)));
        der += 1;
        izq -= 1;
    }
    out
}

/// Folleto: dos páginas por cara, en el orden de grapar por el centro.
fn compone_folleto(
    _doc: &mut LoDoc,
    entradas: &[Entrada],
    op: &OpcionesComposicion,
) -> Result<Vec<Hoja>, String> {
    let derecha = op.encuadernacion.as_deref() == Some("derecha");
    let caras = op.caras.as_deref().unwrap_or("ambas");
    if !matches!(caras, "ambas" | "anverso" | "reverso") {
        return Err("Las caras son «ambas», «anverso» o «reverso»".into());
    }
    let (pw, ph) = (entradas[0].ancho, entradas[0].alto);
    let (hw, hh) = (pw * 2.0, ph);
    let mut hojas = Vec::new();
    for (i, (a, b)) in orden_de_folleto(entradas.len()).into_iter().enumerate() {
        // el anverso son las caras pares (la 1.ª, la 3.ª…) y el reverso las
        // impares: es lo que hay que meter dos veces en la impresora
        let salta = match caras {
            "anverso" => i % 2 == 1,
            "reverso" => i % 2 == 0,
            _ => false,
        };
        if salta {
            continue;
        }
        let mut hoja = Hoja::nueva(hw, hh);
        let (izq, der) = if derecha { (b, a) } else { (a, b) };
        for (hueco, pagina) in [(0.0, izq), (pw, der)] {
            let Some(p) = pagina else { continue };
            let e = &entradas[p];
            let s = (pw / e.ancho).min(ph / e.alto);
            hoja.pon(
                e,
                e.colocada(
                    s,
                    hueco + (pw - e.ancho * s) / 2.0,
                    (ph - e.alto * s) / 2.0,
                ),
            );
        }
        hojas.push(hoja);
    }
    Ok(hojas)
}

/// Póster: una página ampliada, repartida en varias hojas con su solape y
/// sus marcas de corte.
fn compone_poster(
    _doc: &mut LoDoc,
    entradas: &[Entrada],
    op: &OpcionesComposicion,
) -> Result<Vec<Hoja>, String> {
    let escala = op.escala.unwrap_or(2.0);
    if !(escala.is_finite() && escala > 1.0) {
        return Err("El póster amplía la página: la escala tiene que ser mayor que 1".into());
    }
    let solape = (op.solape_mm.unwrap_or(0.0).max(0.0)) * MM;
    let marcas = op.marcas.unwrap_or(false);
    let mut hojas = Vec::new();
    for e in entradas {
        let (hw, hh) = (e.ancho, e.alto);
        let (gw, gh) = (e.ancho * escala, e.alto * escala);
        let (paso_x, paso_y) = ((hw - solape).max(1.0), (hh - solape).max(1.0));
        let cols = (gw / paso_x).ceil().max(1.0) as usize;
        let filas = (gh / paso_y).ceil().max(1.0) as usize;
        for fila in 0..filas {
            for col in 0..cols {
                let mut hoja = Hoja::nueva(hw, hh);
                // la ventana de la página ampliada que le toca a esta hoja:
                // se coloca la página entera y se desplaza
                let ox = -(col as f32) * paso_x;
                // las filas van de arriba abajo, que es como se cuelga
                let oy = -(gh - (fila as f32 + 1.0) * paso_y).max(0.0)
                    + (hh - paso_y).min(0.0);
                hoja.pon(e, e.colocada(escala, ox, oy));
                if marcas {
                    marcas_de_corte(&mut hoja, solape.max(6.0));
                }
                hojas.push(hoja);
            }
        }
    }
    Ok(hojas)
}

/// Las cuatro esquinas marcadas, para cortar y pegar el póster.
fn marcas_de_corte(hoja: &mut Hoja, largo: f32) {
    let (w, h) = (hoja.ancho, hoja.alto);
    let l = largo.min(w / 4.0).min(h / 4.0);
    let mut ops = String::from("q 0 G 0.5 w ");
    for (x, y, dx, dy) in [
        (0.0, 0.0, 1.0, 1.0),
        (w, 0.0, -1.0, 1.0),
        (0.0, h, 1.0, -1.0),
        (w, h, -1.0, -1.0),
    ] {
        ops.push_str(&format!(
            "{x:.2} {y:.2} m {:.2} {y:.2} l S {x:.2} {y:.2} m {x:.2} {:.2} l S ",
            x + dx * l,
            y + dy * l
        ));
    }
    ops.push_str("Q\n");
    hoja.ops.push_str(&ops);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::crea_pdf;

    fn paginas_de(path: &str) -> usize {
        LoDoc::load(path).expect("cargar").get_pages().len()
    }

    fn tamano(path: &str, i: u32) -> (f32, f32) {
        let doc = LoDoc::load(path).expect("cargar");
        let id = *doc.get_pages().get(&i).expect("página");
        let c = crate::formularios2::caja_de_pagina(&doc, id).expect("caja");
        (c[2] - c[0], c[3] - c[1])
    }

    /// **El orden del folleto**, que es lo que lo hace magia negra: con
    /// ocho páginas, las caras salen 8-1, 2-7, 6-3, 4-5, y al grapar por el
    /// centro el libro se lee del derecho.
    #[test]
    fn el_folleto_ordena_las_paginas_para_grapar_por_el_centro() {
        let caras: Vec<(Option<usize>, Option<usize>)> = orden_de_folleto(8);
        assert_eq!(
            caras,
            vec![
                (Some(7), Some(0)),
                (Some(1), Some(6)),
                (Some(5), Some(2)),
                (Some(3), Some(4)),
            ]
        );
        // seis páginas se rellenan hasta ocho con dos huecos en blanco
        let caras = orden_de_folleto(6);
        assert_eq!(caras.len(), 4);
        assert_eq!(caras[0], (None, Some(0)), "la contraportada va en blanco");
        assert!(caras.iter().flat_map(|(a, b)| [*a, *b]).flatten().count() == 6);
    }

    /// **Las tres composiciones**, con el recuento de hojas que enseña el
    /// pie del diálogo. Componer **no toca la copia de trabajo**: escribe
    /// un PDF aparte, así que no hay paso de deshacer que gastar.
    #[test]
    fn nup_folleto_y_poster_dan_las_hojas_que_dicen() {
        let pdf = std::env::temp_dir().join("imprimir-composicion.pdf");
        let paginas: Vec<String> = (1..=10).map(|i| format!("Página {i}")).collect();
        let refs: Vec<&str> = paginas.iter().map(|s| s.as_str()).collect();
        crea_pdf(&refs, &pdf);
        let work = pdf.to_string_lossy().into_owned();
        let pasos = crate::historial::history_state(work.clone())
            .expect("historial")
            .undo;

        // 4-up de 10 páginas: tres hojas, la última con dos huecos
        let r = compose_print(
            work.clone(),
            "nup".into(),
            OpcionesComposicion {
                por_hoja: Some(4),
                borde: Some(true),
                ..Default::default()
            },
        )
        .expect("4-up");
        assert_eq!(r.hojas, 3, "10 páginas de cuatro en cuatro son tres hojas");
        assert_eq!(paginas_de(&r.path), 3);
        assert_eq!(r.paginas, 10);
        // la hoja tiene el tamaño de la página original
        let (w, h) = tamano(&r.path, 1);
        assert!((w - 595.28).abs() < 1.0 && (h - 841.89).abs() < 1.0, "{w}×{h}");
        std::fs::remove_file(&r.path).ok();

        // folleto de 8: cuatro caras, dos hojas de papel
        let ocho: Vec<u16> = (0..8).collect();
        let r = compose_print(
            work.clone(),
            "folleto".into(),
            OpcionesComposicion {
                page_indices: Some(ocho.clone()),
                ..Default::default()
            },
        )
        .expect("folleto");
        assert_eq!(r.caras, 4, "ocho páginas son cuatro caras");
        assert_eq!(r.hojas, 2, "y dos hojas de papel a doble cara");
        assert_eq!(paginas_de(&r.path), 4);
        // la cara del folleto es el doble de ancha que la página
        let (w, h) = tamano(&r.path, 1);
        assert!((w - 595.28 * 2.0).abs() < 1.0 && (h - 841.89).abs() < 1.0, "{w}×{h}");
        std::fs::remove_file(&r.path).ok();

        // solo el anverso: la mitad de las caras, para meter el papel dos
        // veces en la impresora
        let r = compose_print(
            work.clone(),
            "folleto".into(),
            OpcionesComposicion {
                page_indices: Some(ocho),
                caras: Some("anverso".into()),
                ..Default::default()
            },
        )
        .expect("folleto anverso");
        assert_eq!(r.caras, 2);
        std::fs::remove_file(&r.path).ok();

        // póster de una A4 al 200 %: cuatro hojas
        let r = compose_print(
            work.clone(),
            "poster".into(),
            OpcionesComposicion {
                page_indices: Some(vec![0]),
                escala: Some(2.0),
                marcas: Some(true),
                ..Default::default()
            },
        )
        .expect("póster");
        assert_eq!(r.hojas, 4, "una A4 al doble ocupa cuatro A4");
        assert_eq!(paginas_de(&r.path), 4);
        std::fs::remove_file(&r.path).ok();

        // una escala que no amplía no es un póster
        assert!(compose_print(
            work.clone(),
            "poster".into(),
            OpcionesComposicion {
                escala: Some(0.5),
                ..Default::default()
            },
        )
        .unwrap_err()
        .contains("mayor que 1"));
        // y una composición que no existe se dice
        assert!(compose_print(work.clone(), "espiral".into(), OpcionesComposicion::default())
            .unwrap_err()
            .contains("desconocida"));

        // el documento no se ha tocado: ni una página menos ni un paso de
        // deshacer gastado
        assert_eq!(paginas_de(&work), 10);
        assert_eq!(
            crate::historial::history_state(work.clone())
                .expect("historial")
                .undo,
            pasos
        );
        std::fs::remove_file(&pdf).ok();
    }

    /// El contenido de las páginas viejas tiene que **verse** en la hoja
    /// compuesta: es un Form XObject dentro, no una página en blanco con
    /// buen tamaño.
    #[test]
    fn la_hoja_compuesta_lleva_dentro_lo_que_habia_en_las_paginas() {
        let pdf = std::env::temp_dir().join("imprimir-contenido.pdf");
        crea_pdf(&["Uno", "Dos"], &pdf);
        let work = pdf.to_string_lossy().into_owned();
        let r = compose_print(
            work.clone(),
            "nup".into(),
            OpcionesComposicion {
                por_hoja: Some(2),
                ..Default::default()
            },
        )
        .expect("2-up");
        assert_eq!(paginas_de(&r.path), 1);
        let texto = crate::on_pdfium_thread({
            let p = r.path.clone();
            move || {
                crate::with_doc(&p, |doc| {
                    Ok(doc
                        .pages()
                        .get(0)
                        .map_err(|e| e.to_string())?
                        .text()
                        .map(|t| t.all())
                        .unwrap_or_default())
                })
            }
        })
        .expect("texto");
        assert!(texto.contains("Uno") && texto.contains("Dos"), "texto: {texto}");
        std::fs::remove_file(&r.path).ok();
        std::fs::remove_file(&pdf).ok();
    }
}
