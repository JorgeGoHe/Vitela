//! Imágenes: listar, insertar, mover/redimensionar, reemplazar, borrar y
//! extraer el contenido de un objeto de imagen.

use crate::historial::mutacion;
use crate::{on_pdfium_thread, pdfium, save_and_close, with_doc};
use base64::Engine;
use lopdf::{Dictionary, Document as LoDoc, Object, ObjectId, Stream};
use pdfium_render::prelude::*;
use serde::Serialize;

/// Contenido de un objeto de imagen como PNG en base64 (con máscaras y
/// transparencia aplicadas). La UI lo usa como vista previa al arrastrar.
#[tauri::command(async)]
pub fn get_image_data(path: String, page_index: u16, object_index: u32) -> Result<String, String> {
    on_pdfium_thread(move || {
        // Ojo: aquí NO se puede usar `with_doc`. Para devolver el bitmap al
        // tamaño de la metadata, `get_processed_image` transforma el objeto
        // de imagen y no lo deja como estaba, así que sobre el documento
        // cacheado falsearía los bounds del render, las miniaturas, la
        // exportación y `get_images` hasta la siguiente mutación. Se abre una
        // copia aparte, de solo lectura, que se descarta al salir.
        let doc = pdfium()?
            .load_pdf_from_file(&path, None)
            .map_err(crate::mensaje_llano)?;
        let page = doc.pages().get(page_index).map_err(crate::mensaje_llano)?;
        let obj = page
            .objects()
            .get(object_index as usize)
            .map_err(crate::mensaje_llano)?;
        let img_obj = obj.as_image_object().ok_or("No es una imagen")?;
        let img = img_obj
            .get_processed_image(&doc)
            .map_err(crate::mensaje_llano)?;
        let mut buf = std::io::Cursor::new(Vec::new());
        img.write_to(&mut buf, image::ImageFormat::Png)
            .map_err(|e| format!("No se ha podido codificar la imagen: {e}"))?;
        Ok(base64::engine::general_purpose::STANDARD.encode(buf.into_inner()))
    })
}

/// «Guardar imagen como…»: escribe el objeto de imagen tal como se ve —con
/// sus máscaras y su transparencia aplicadas— en un fichero PNG.
///
/// Es el mismo bitmap que `get_image_data`, pero **sin pasar por base64 ni
/// por el webview**: una foto de 12 MP en base64 son 30 MB de cadena
/// cruzando el canal para acabar en el disco, y el navegador de la sesión
/// de QA no puede escribir ficheros de todas formas.
///
/// Escribe fuera del documento: no muta nada ni deja paso de deshacer.
#[tauri::command(async)]
pub fn save_image_data(
    work_path: String,
    page_index: u16,
    object_index: u32,
    dest_path: String,
) -> Result<(), String> {
    on_pdfium_thread(move || {
        // copia aparte de solo lectura, por lo mismo que `get_image_data`:
        // `get_processed_image` transforma el objeto y no lo deja como
        // estaba
        let doc = pdfium()?
            .load_pdf_from_file(&work_path, None)
            .map_err(crate::mensaje_llano)?;
        let page = doc.pages().get(page_index).map_err(crate::mensaje_llano)?;
        let obj = page
            .objects()
            .get(object_index as usize)
            .map_err(crate::mensaje_llano)?;
        let img = obj
            .as_image_object()
            .ok_or("No es una imagen")?
            .get_processed_image(&doc)
            .map_err(crate::mensaje_llano)?;
        img.save_with_format(&dest_path, image::ImageFormat::Png)
            .map_err(|e| crate::mensaje_llano(format!("No se ha podido escribir {dest_path}: {e}")))
    })
}

#[derive(Serialize)]
pub struct ImageInfo {
    pub object_index: u32,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// Lista los objetos de imagen de una página (bounds en coords de UI).
#[tauri::command(async)]
pub fn get_images(path: String, page_index: u16) -> Result<Vec<ImageInfo>, String> {
    on_pdfium_thread(move || {
        with_doc(&path, |doc| {
            let page = doc.pages().get(page_index).map_err(|e| e.to_string())?;
            // espacio propio de la página: las cajas de los objetos no
            // llevan la rotación, y `page.height()` sí (ver `Geo`)
            let geo = crate::Geo::de_pagina(&page).propia();
            let objects = page.objects();
            let mut out = Vec::new();
            for i in 0..objects.len() {
                let Ok(obj) = objects.get(i) else { continue };
                if obj.as_image_object().is_none() {
                    continue;
                }
                let Ok(b) = obj.bounds() else { continue };
                // `bounds()` de un objeto de página son quadpoints; los
                // giros del PDF son múltiplos de 90°, así que su caja
                // envolvente es el rect
                let caja =
                    geo.pdf_rect_a_ui(&PdfRect::new(b.bottom(), b.left(), b.top(), b.right()));
                out.push(ImageInfo {
                    object_index: i as u32,
                    x: caja.x,
                    y: caja.y,
                    w: caja.w,
                    h: caja.h,
                });
            }
            Ok(out)
        })
    })
}

/// Inserta una imagen (png/jpg/webp…) con su tamaño natural a 72 dpi,
/// limitado a caber en la página. El punto dado (coords de UI) es la esquina
/// superior izquierda.
#[tauri::command(async)]
pub fn add_image(
    work_path: String,
    page_index: u16,
    image_path: String,
    x: f32,
    y: f32,
) -> Result<(), String> {
    mutacion(work_path, |work_path| {
        on_pdfium_thread(move || {
            let img = image::open(&image_path)
                .map_err(|e| format!("No se ha podido leer la imagen: {e}"))?;
            let pdfium = pdfium()?;
            let doc = pdfium
                .load_pdf_from_file(&work_path, None)
                .map_err(|e| e.to_string())?;
            let mut page = doc.pages().get(page_index).map_err(|e| e.to_string())?;
            let vista = crate::Geo::de_pagina(&page);
            let rot = vista.rot;
            // el tamaño se limita al de la página TAL COMO SE VE
            let (page_w, page_h) = (page.width().value, page.height().value);
            let mut w = img.width() as f32;
            let mut h = img.height() as f32;
            let max_w = page_w * 0.6;
            if w > max_w {
                let f = max_w / w;
                w *= f;
                h *= f;
            }
            if h > page_h * 0.8 {
                let f = page_h * 0.8 / h;
                w *= f;
                h *= f;
            }
            let mut obj =
                PdfPageImageObject::new_with_size(&doc, &img, PdfPoints::new(w), PdfPoints::new(h))
                    .map_err(|e| e.to_string())?;
            // girar al revés que la página: si no, en una página con /Rotate la
            // imagen sale tumbada
            if rot != 0 {
                obj.rotate_counter_clockwise_degrees(rot as f32)
                    .map_err(|e| e.to_string())?;
            }
            // el punto llega en el espacio propio y es la esquina superior
            // izquierda de lo que se ve: la caja en coordenadas PDF sale de
            // recorrer los ejes de la vista
            let ancla = vista.propia().ui_a_pdf(x, y);
            let (derecha, abajo) = vista.ejes();
            let esquinas = [
                ancla,
                (ancla.0 + derecha.0 * w, ancla.1 + derecha.1 * w),
                (ancla.0 + abajo.0 * h, ancla.1 + abajo.1 * h),
                (
                    ancla.0 + derecha.0 * w + abajo.0 * h,
                    ancla.1 + derecha.1 * w + abajo.1 * h,
                ),
            ];
            let (izq, abajo_pdf) = (
                esquinas.iter().map(|c| c.0).fold(f32::MAX, f32::min),
                esquinas.iter().map(|c| c.1).fold(f32::MAX, f32::min),
            );
            let b = obj.bounds().map_err(|e| e.to_string())?;
            obj.translate(
                PdfPoints::new(izq - b.left().value),
                PdfPoints::new(abajo_pdf - b.bottom().value),
            )
            .map_err(|e| e.to_string())?;
            page.objects_mut()
                .add_image_object(obj)
                .map_err(|e| e.to_string())?;
            page.regenerate_content().map_err(|e| e.to_string())?;
            drop(page);
            save_and_close(doc, &work_path)?;
            Ok(())
        })
    })
}

/// Mueve, redimensiona, **gira y voltea** una imagen. Los bounds llegan en
/// coords de UI (espacio propio de la página) y son los que tendría la
/// imagen sin girar; con `rotate` (múltiplos de 90, horarios) el resultado
/// queda centrado en esa caja, así que a 90° el ancho y el alto salen
/// intercambiados, como en Acrobat. `flip_h` y `flip_v` la voltean en
/// horizontal y en vertical.
#[tauri::command(async)]
#[allow(clippy::too_many_arguments)]
pub fn transform_image(
    work_path: String,
    page_index: u16,
    object_index: u32,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    rotate: Option<i16>,
    flip_h: Option<bool>,
    flip_v: Option<bool>,
) -> Result<(), String> {
    if w <= 1.0 || h <= 1.0 {
        return Err("Tamaño de imagen inválido".into());
    }
    let giro = rotate.unwrap_or(0).rem_euclid(360);
    if giro % 90 != 0 {
        return Err("La imagen solo se gira en múltiplos de 90°".into());
    }
    let flip_h = flip_h.unwrap_or(false);
    let flip_v = flip_v.unwrap_or(false);
    mutacion(work_path, move |work_path| {
        on_pdfium_thread(move || {
            let pdfium = pdfium()?;
            let doc = pdfium
                .load_pdf_from_file(&work_path, None)
                .map_err(|e| e.to_string())?;
            let mut page = doc.pages().get(page_index).map_err(|e| e.to_string())?;
            // los bounds que manda la UI vienen en el espacio propio de la
            // página, así que la altura para voltear la `y` es la propia, no la
            // que devuelve `page.height()` (esa ya lleva el /Rotate aplicado)
            let destino = crate::Geo::de_pagina(&page)
                .propia()
                .ui_rect_a_pdf(&crate::Rect { x, y, w, h });
            let mut obj = page
                .objects_mut()
                .get(object_index as usize)
                .map_err(|e| e.to_string())?;
            if obj.as_image_object().is_none() {
                return Err("No es una imagen".into());
            }
            let b = obj.bounds().map_err(|e| e.to_string())?;
            let old_w = b.right().value - b.left().value;
            let old_h = b.top().value - b.bottom().value;
            let (nueva_w, nueva_h) = (
                destino.right().value - destino.left().value,
                destino.top().value - destino.bottom().value,
            );
            if old_w > 0.0 && old_h > 0.0 {
                obj.scale(nueva_w / old_w, nueva_h / old_h)
                    .map_err(|e| e.to_string())?;
            }
            // voltear es escalar por −1 en ese eje; girar, un cuarto de vuelta.
            // Las dos cosas mueven el objeto de sitio (son respecto del origen
            // del papel), así que después se recoloca por el centro
            if flip_h || flip_v {
                obj.scale(
                    if flip_h { -1.0 } else { 1.0 },
                    if flip_v { -1.0 } else { 1.0 },
                )
                .map_err(|e| e.to_string())?;
            }
            if giro != 0 {
                // `rotate` viene en grados horarios, como `rotate_page`
                obj.rotate_counter_clockwise_degrees(-(giro as f32))
                    .map_err(|e| e.to_string())?;
            }
            let b2 = obj.bounds().map_err(|e| e.to_string())?;
            let (dx, dy) = if giro != 0 || flip_h || flip_v {
                // centrado en la caja pedida: a 90° el ancho y el alto salen
                // cambiados y encajar por la esquina la desplazaría
                (
                    (destino.left().value + destino.right().value) / 2.0
                        - (b2.left().value + b2.right().value) / 2.0,
                    (destino.bottom().value + destino.top().value) / 2.0
                        - (b2.bottom().value + b2.top().value) / 2.0,
                )
            } else {
                (
                    destino.left().value - b2.left().value,
                    destino.bottom().value - b2.bottom().value,
                )
            };
            obj.translate(PdfPoints::new(dx), PdfPoints::new(dy))
                .map_err(|e| e.to_string())?;
            drop(obj);
            page.regenerate_content().map_err(|e| e.to_string())?;
            drop(page);
            save_and_close(doc, &work_path)?;
            Ok(())
        })
    })
}

/// Trae la imagen al frente o la manda al fondo, que es lo que hace falta
/// cuando una imagen tapa el texto (o al revés). PDFium ordena los objetos
/// por su posición en el content stream: se saca el objeto y se vuelve a
/// poner, que es lo único que expone pdfium-render 0.8, cuidando de no
/// soltar nunca un objeto sacado (su `Drop` destruye el objeto y PDFium
/// casca).
/// Multiplica dos matrices de PDF (`a` aplicada **antes** que `b`).
fn por(a: [f64; 6], b: [f64; 6]) -> [f64; 6] {
    [
        a[0] * b[0] + a[1] * b[2],
        a[0] * b[1] + a[1] * b[3],
        a[2] * b[0] + a[3] * b[2],
        a[2] * b[1] + a[3] * b[3],
        a[4] * b[0] + a[5] * b[2] + b[4],
        a[4] * b[1] + a[5] * b[3] + b[5],
    ]
}

const IDENTIDAD: [f64; 6] = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

fn es_identidad(m: &[f64; 6]) -> bool {
    m.iter()
        .zip(IDENTIDAD.iter())
        .all(|(a, b)| (a - b).abs() < 1e-6)
}

fn escribe_cm(m: &[f64; 6]) -> String {
    format!("{} {} {} {} {} {} cm ", m[0], m[1], m[2], m[3], m[4], m[5])
}

/// Operadores que **pintan**: son los que hacen que un `q … Q` sea el
/// dibujo de un objeto y no solo estado gráfico o un recorte.
fn pinta(op: &[u8]) -> bool {
    matches!(
        op,
        b"S" | b"s"
            | b"f"
            | b"F"
            | b"f*"
            | b"B"
            | b"B*"
            | b"b"
            | b"b*"
            | b"sh"
            | b"Do"
            | b"EI"
            | b"TJ"
            | b"Tj"
            | b"'"
            | b"\""
    )
}

/// Los nombres del `/Resources /XObject` de la página que son imágenes.
fn nombres_de_imagen(doc: &LoDoc, page_id: ObjectId) -> Vec<Vec<u8>> {
    let Ok((propio, heredados)) = doc.get_page_resources(page_id) else {
        return Vec::new();
    };
    let mut dicts: Vec<&Dictionary> = Vec::new();
    if let Some(d) = propio {
        dicts.push(d);
    }
    for id in heredados {
        if let Ok(d) = doc.get_object(id).and_then(|o| o.as_dict()) {
            dicts.push(d);
        }
    }
    let mut out = Vec::new();
    for d in dicts {
        let Ok(xo) = d.get(b"XObject").and_then(|o| match o {
            Object::Reference(rid) => doc.get_object(*rid).and_then(|o| o.as_dict()),
            otro => otro.as_dict(),
        }) else {
            continue;
        };
        for (nombre, valor) in xo.iter() {
            let es_imagen = valor
                .as_reference()
                .ok()
                .and_then(|rid| doc.get_object(rid).ok())
                .and_then(|o| o.as_stream().ok())
                .map(|s| s.dict.get(b"Subtype").and_then(|o| o.as_name()).ok() == Some(b"Image"))
                .unwrap_or(false);
            if es_imagen {
                out.push(nombre.to_vec());
            }
        }
    }
    out
}

/// Dónde está el dibujo de la imagen número `ordinal` dentro del flujo de
/// contenido de la página, y con qué transformación se pinta.
struct Hallazgo {
    /// El trozo que hay que quitar del flujo.
    corte: std::ops::Range<usize>,
    /// El trozo que hay que volver a poner, ya listo para escribir.
    fragmento: Vec<u8>,
}

/// Busca en el flujo el `Do` de la imagen `ordinal` (contando solo las
/// imágenes, que es el orden en el que las da `get_images`) y devuelve qué
/// hay que cortar y qué hay que volver a escribir.
///
/// Si el `Do` está dentro de un `q … Q` que **no dibuja nada más**, se
/// mueve el grupo entero: así viajan con él su recorte, su opacidad y su
/// estado gráfico. Si no —un flujo escrito a mano donde la imagen comparte
/// grupo con otra cosa—, se quita solo el `Do` y se vuelve a escribir con
/// la matriz acumulada, que es lo único que se puede asegurar.
fn halla_imagen(datos: &[u8], imagenes: &[Vec<u8>], ordinal: usize) -> Option<Hallazgo> {
    struct Marco {
        inicio: usize,
        ctm: [f64; 6],
        pintadas: usize,
    }
    let mut pila: Vec<Marco> = Vec::new();
    let mut ctm = IDENTIDAD;
    let mut pintadas = 0usize;
    let mut vistas = 0usize;
    let mut numeros: Vec<f64> = Vec::new();
    let mut ultimo_nombre: Option<Vec<u8>> = None;
    let mut ultimo_ini = 0usize;
    let mut nombre_pendiente: Option<usize> = None;
    // lo que se sabe de la imagen buscada, en cuanto se la encuentra
    let mut objetivo: Option<(std::ops::Range<usize>, [f64; 6], Option<usize>)> = None;
    let mut hallazgo: Option<Hallazgo> = None;
    crate::texto::recorre_stream_con_pos(datos, |r, es_token| {
        if hallazgo.is_some() {
            return;
        }
        let trozo = &datos[r.clone()];
        if !es_token {
            // la barra de un nombre es un delimitador: el nombre viene en
            // el token siguiente
            if trozo == b"/" {
                nombre_pendiente = Some(r.start);
            }
            return;
        }
        if let Some(ini) = nombre_pendiente.take() {
            ultimo_nombre = Some(trozo.to_vec());
            ultimo_ini = ini;
            return;
        }
        // un token regular: número u operador
        if let Ok(n) = std::str::from_utf8(trozo).unwrap_or("x").parse::<f64>() {
            numeros.push(n);
            return;
        }
        match trozo {
            b"q" => {
                pila.push(Marco {
                    inicio: r.start,
                    ctm,
                    pintadas,
                });
            }
            b"Q" => {
                if let Some(m) = pila.pop() {
                    ctm = m.ctm;
                    // ¿se cierra el grupo donde estaba la imagen buscada?
                    if let Some((corte, propia, Some(nivel))) = &objetivo {
                        if *nivel == pila.len() && pintadas - m.pintadas == 1 {
                            let mut fragmento = b"q ".to_vec();
                            if !es_identidad(&m.ctm) {
                                fragmento.extend_from_slice(escribe_cm(&m.ctm).as_bytes());
                            }
                            fragmento.extend_from_slice(&datos[m.inicio + 1..r.end]);
                            hallazgo = Some(Hallazgo {
                                corte: m.inicio..r.end,
                                fragmento,
                            });
                        } else {
                            hallazgo = Some(Hallazgo {
                                corte: corte.clone(),
                                fragmento: fragmento_suelto(propia, datos, corte),
                            });
                        }
                    }
                }
            }
            b"cm" => {
                if numeros.len() >= 6 {
                    let n = &numeros[numeros.len() - 6..];
                    ctm = por([n[0], n[1], n[2], n[3], n[4], n[5]], ctm);
                }
            }
            b"Do" => {
                let nombre = ultimo_nombre.clone().unwrap_or_default();
                if imagenes.contains(&nombre) {
                    if vistas == ordinal {
                        objetivo = Some((ultimo_ini..r.end, ctm, pila.len().checked_sub(1)));
                    }
                    vistas += 1;
                }
                pintadas += 1;
            }
            otro if pinta(otro) => pintadas += 1,
            _ => {}
        }
        numeros.clear();
    });
    // el `Do` estaba fuera de todo `q … Q` (o el grupo no se cerró)
    if hallazgo.is_none() {
        if let Some((corte, propia, _)) = objetivo {
            let fragmento = fragmento_suelto(&propia, datos, &corte);
            hallazgo = Some(Hallazgo { corte, fragmento });
        }
    }
    hallazgo
}

/// El dibujo de la imagen escrito de cero, con la matriz que tenía donde
/// estaba: `q <matriz> cm /Nombre Do Q`.
fn fragmento_suelto(ctm: &[f64; 6], datos: &[u8], corte: &std::ops::Range<usize>) -> Vec<u8> {
    let mut out = b"q ".to_vec();
    out.extend_from_slice(escribe_cm(ctm).as_bytes());
    out.extend_from_slice(&datos[corte.clone()]);
    out.extend_from_slice(b" Q");
    out
}

/// **AC-098.** Lleva la imagen al fondo o al frente **en el flujo de
/// contenido**, con lopdf. Sacar el objeto y volver a añadirlo con PDFium
/// deja la lista bien en memoria pero `FPDF_GenerateContent` no reescribe
/// el flujo en ese orden: al guardar, la imagen volvía donde estaba y el
/// usuario pulsaba el botón sin que pasara nada.
fn reordena_en_contenido(
    doc: &mut LoDoc,
    page_index: u16,
    ordinal: usize,
    al_frente: bool,
) -> Result<(), String> {
    let page_id = *doc
        .get_pages()
        .get(&(page_index as u32 + 1))
        .ok_or("Página fuera de rango")?;
    let imagenes = nombres_de_imagen(doc, page_id);
    if imagenes.is_empty() {
        return Err("La página no tiene ninguna imagen".into());
    }
    let datos = doc
        .get_page_content(page_id)
        .map_err(|e| crate::mensaje_llano(format!("No se ha podido leer la página: {e}")))?;
    let Some(hallazgo) = halla_imagen(&datos, &imagenes, ordinal) else {
        return Err("Esa imagen ya no está en la página".into());
    };
    let mut resto = Vec::with_capacity(datos.len());
    resto.extend_from_slice(&datos[..hallazgo.corte.start]);
    resto.extend_from_slice(&datos[hallazgo.corte.end..]);
    let mut nuevo = Vec::with_capacity(datos.len() + 64);
    if al_frente {
        nuevo.extend_from_slice(&resto);
        nuevo.push(b'\n');
        nuevo.extend_from_slice(&hallazgo.fragmento);
    } else {
        nuevo.extend_from_slice(&hallazgo.fragmento);
        nuevo.push(b'\n');
        nuevo.extend_from_slice(&resto);
    }
    let mut stream = Stream::new(Dictionary::new(), nuevo);
    stream.compress().ok();
    let stream_id = doc.add_object(Object::Stream(stream));
    doc.get_object_mut(page_id)
        .and_then(|o| o.as_dict_mut())
        .map_err(|e| e.to_string())?
        .set("Contents", Object::Reference(stream_id));
    Ok(())
}

#[tauri::command(async)]
pub fn reorder_image(
    work_path: String,
    page_index: u16,
    object_index: u32,
    al_frente: bool,
) -> Result<(), String> {
    mutacion(work_path, move |work_path| {
        on_pdfium_thread(move || {
            // cuál es entre las imágenes de la página: es el orden en el
            // que van sus `Do` en el flujo, y el mismo que devuelve
            // `get_images`
            let ordinal = with_doc(&work_path, |doc| {
                let page = doc.pages().get(page_index).map_err(crate::mensaje_llano)?;
                let objetos = page.objects();
                if object_index as usize >= objetos.len() {
                    return Err("Esa imagen ya no está en la página".into());
                }
                let mut ordinal = 0usize;
                for i in 0..object_index as usize {
                    if objetos
                        .get(i)
                        .ok()
                        .and_then(|o| o.as_image_object().map(|_| ()))
                        .is_some()
                    {
                        ordinal += 1;
                    }
                }
                let es_imagen = objetos
                    .get(object_index as usize)
                    .ok()
                    .and_then(|o| o.as_image_object().map(|_| ()))
                    .is_some();
                if !es_imagen {
                    return Err("No es una imagen".into());
                }
                Ok(ordinal)
            })?;
            crate::cirugia_en_hilo(&work_path, move |doc| {
                reordena_en_contenido(doc, page_index, ordinal, al_frente)
            })
        })
    })
}

/// Reemplaza el contenido de una imagen manteniendo posición y tamaño.
#[tauri::command(async)]
pub fn replace_image(
    work_path: String,
    page_index: u16,
    object_index: u32,
    image_path: String,
) -> Result<(), String> {
    mutacion(work_path, |work_path| {
        on_pdfium_thread(move || {
            let img = image::open(&image_path)
                .map_err(|e| format!("No se ha podido leer la imagen: {e}"))?;
            let pdfium = pdfium()?;
            let doc = pdfium
                .load_pdf_from_file(&work_path, None)
                .map_err(|e| e.to_string())?;
            let mut page = doc.pages().get(page_index).map_err(|e| e.to_string())?;
            let (left, bottom, w, h) = {
                let obj = page
                    .objects()
                    .get(object_index as usize)
                    .map_err(|e| e.to_string())?;
                if obj.as_image_object().is_none() {
                    return Err("No es una imagen".into());
                }
                let b = obj.bounds().map_err(|e| e.to_string())?;
                (
                    b.left().value,
                    b.bottom().value,
                    b.right().value - b.left().value,
                    b.top().value - b.bottom().value,
                )
            };
            let removed = page
                .objects_mut()
                .remove_object_at_index(object_index as usize)
                .map_err(|e| e.to_string())?;
            // ver nota en delete_text_block: soltar el objeto extraído casca
            std::mem::forget(removed);
            let mut obj =
                PdfPageImageObject::new_with_size(&doc, &img, PdfPoints::new(w), PdfPoints::new(h))
                    .map_err(|e| e.to_string())?;
            obj.translate(PdfPoints::new(left), PdfPoints::new(bottom))
                .map_err(|e| e.to_string())?;
            page.objects_mut()
                .add_image_object(obj)
                .map_err(|e| e.to_string())?;
            page.regenerate_content().map_err(|e| e.to_string())?;
            drop(page);
            save_and_close(doc, &work_path)?;
            Ok(())
        })
    })
}

/// Recorta una imagen: se queda con el trozo que marca `rect` —en el
/// espacio propio de la página, como el resto de comandos que escriben— y
/// la deja ocupando exactamente ese rectángulo, que es lo que hace la
/// herramienta de recorte de Acrobat.
///
/// Recorta el **bitmap**, no la caja: PDFium no tiene «recortar», así que
/// se saca la imagen procesada, se corta con el crate `image` y se vuelve a
/// crear el objeto en su sitio (el camino de `replace_image`). Lo que se
/// quita fuera del recorte **desaparece del fichero**, así que no queda
/// escondido detrás como pasaría con un `/BBox`.
#[tauri::command(async)]
pub fn crop_image(
    work_path: String,
    page_index: u16,
    object_index: u32,
    rect: crate::Rect,
) -> Result<(), String> {
    if rect.w < 4.0 || rect.h < 4.0 {
        return Err("El área de recorte es demasiado pequeña".into());
    }
    mutacion(work_path, move |work_path| {
        on_pdfium_thread(move || {
            let pdfium = pdfium()?;
            let doc = pdfium
                .load_pdf_from_file(&work_path, None)
                .map_err(crate::mensaje_llano)?;
            let mut page = doc.pages().get(page_index).map_err(crate::mensaje_llano)?;
            let destino = crate::Geo::de_pagina(&page).propia().ui_rect_a_pdf(&rect);
            // el trozo que se queda, y el rectángulo del papel que va a ocupar
            let (recortada, corte_x, corte_y, corte_w, corte_h) = {
                let obj = page
                    .objects()
                    .get(object_index as usize)
                    .map_err(crate::mensaje_llano)?;
                let img = obj.as_image_object().ok_or("No es una imagen")?;
                let b = obj.bounds().map_err(|e| e.to_string())?;
                let (izq, abajo) = (b.left().value, b.bottom().value);
                let (ancho, alto) = (b.right().value - izq, b.top().value - abajo);
                if ancho <= 0.0 || alto <= 0.0 {
                    return Err("Esa imagen no tiene tamaño".into());
                }
                // la parte del rect que cae dentro de la imagen
                let x0 = destino.left().value.max(izq);
                let x1 = destino.right().value.min(izq + ancho);
                let y0 = destino.bottom().value.max(abajo);
                let y1 = destino.top().value.min(abajo + alto);
                if x1 - x0 < 1.0 || y1 - y0 < 1.0 {
                    return Err("El área de recorte se sale de la imagen".into());
                }
                let bitmap = img
                    .get_processed_image(&doc)
                    .map_err(crate::mensaje_llano)?;
                let (pw, ph) = (bitmap.width(), bitmap.height());
                // del papel a los píxeles del bitmap: la `y` del papel sube y la
                // de la imagen baja, así que el borde de arriba del recorte es
                // la fila de más arriba
                let a_px = |v: f32, largo: f32, total: u32| -> u32 {
                    ((v / largo) * total as f32)
                        .round()
                        .clamp(0.0, total as f32) as u32
                };
                let px = a_px(x0 - izq, ancho, pw);
                let py = a_px(abajo + alto - y1, alto, ph);
                let ancho_px = a_px(x1 - x0, ancho, pw).clamp(1, pw - px);
                let alto_px = a_px(y1 - y0, alto, ph).clamp(1, ph - py);
                let recortada =
                    image::imageops::crop_imm(&bitmap, px, py, ancho_px, alto_px).to_image();
                (
                    image::DynamicImage::ImageRgba8(recortada),
                    x0,
                    y0,
                    x1 - x0,
                    y1 - y0,
                )
            };
            let removed = page
                .objects_mut()
                .remove_object_at_index(object_index as usize)
                .map_err(|e| e.to_string())?;
            // ver nota en delete_text_block: soltar el objeto extraído casca
            std::mem::forget(removed);
            let mut obj = PdfPageImageObject::new_with_size(
                &doc,
                &recortada,
                PdfPoints::new(corte_w),
                PdfPoints::new(corte_h),
            )
            .map_err(crate::mensaje_llano)?;
            obj.translate(PdfPoints::new(corte_x), PdfPoints::new(corte_y))
                .map_err(|e| e.to_string())?;
            page.objects_mut()
                .add_image_object(obj)
                .map_err(|e| e.to_string())?;
            page.regenerate_content().map_err(|e| e.to_string())?;
            drop(page);
            save_and_close(doc, &work_path)
        })
    })
}

/// Elimina una imagen de la página.
#[tauri::command(async)]
pub fn delete_image(work_path: String, page_index: u16, object_index: u32) -> Result<(), String> {
    mutacion(work_path, |work_path| {
        on_pdfium_thread(move || {
            let pdfium = pdfium()?;
            let doc = pdfium
                .load_pdf_from_file(&work_path, None)
                .map_err(|e| e.to_string())?;
            let mut page = doc.pages().get(page_index).map_err(|e| e.to_string())?;
            {
                let obj = page
                    .objects()
                    .get(object_index as usize)
                    .map_err(|e| e.to_string())?;
                if obj.as_image_object().is_none() {
                    return Err("No es una imagen".into());
                }
            }
            let removed = page
                .objects_mut()
                .remove_object_at_index(object_index as usize)
                .map_err(|e| e.to_string())?;
            // ver nota en delete_text_block: soltar el objeto extraído casca
            std::mem::forget(removed);
            page.regenerate_content().map_err(|e| e.to_string())?;
            drop(page);
            save_and_close(doc, &work_path)?;
            Ok(())
        })
    })
}

#[cfg(test)]
mod tests {

    /// **AC-098.** «Traer al frente» y «Enviar al fondo» movían la imagen
    /// un puesto (o ninguno) en vez de llevarla al extremo: se pulsaba y
    /// no pasaba nada, que es peor que no tener el botón. El barrido va
    /// por páginas de 2, 3, 5 y 10 objetos, con la imagen en el medio.
    #[test]
    fn al_frente_y_al_fondo_llevan_la_imagen_al_extremo() {
        let dir = std::env::temp_dir();
        let png = dir.join("imagenes-orden.png");
        image::RgbaImage::from_pixel(30, 30, image::Rgba([20, 80, 220, 255]))
            .save(&png)
            .expect("crear el png");
        for total in [2usize, 3, 5, 10] {
            let pdf = dir.join(format!("imagenes-orden-{total}.pdf"));
            crate::tests::crea_pdf(&["Orden"], &pdf);
            let work = pdf.to_string_lossy().into_owned();
            // el documento nace con un objeto de texto; se completa con
            // los que falten y la imagen se pone en el medio
            let en_medio = total / 2;
            for n in 1..total {
                if n == en_medio {
                    add_image(
                        work.clone(),
                        0,
                        png.to_string_lossy().into_owned(),
                        40.0,
                        180.0,
                    )
                    .expect("insertar la imagen");
                } else {
                    crate::texto::add_text_block(
                        work.clone(),
                        0,
                        60.0,
                        100.0 + n as f32 * 20.0,
                        format!("Línea {n}"),
                        12.0,
                        None,
                        None,
                        None,
                        None,
                        None,
                    )
                    .expect("texto");
                }
            }
            let indice = |work: &str| {
                get_images(work.to_string(), 0).expect("imágenes")[0].object_index as usize
            };
            assert_eq!(indice(&work), en_medio, "la imagen se coloca en el medio");

            reorder_image(work.clone(), 0, en_medio as u32, false).expect("al fondo");
            assert_eq!(indice(&work), 0, "al fondo con {total} objetos");

            reorder_image(work.clone(), 0, 0, true).expect("al frente");
            assert_eq!(indice(&work), total - 1, "al frente con {total} objetos");
            std::fs::remove_file(&pdf).ok();
        }
        std::fs::remove_file(&png).ok();
    }

    /// «Guardar imagen como…»: el mismo bitmap que la vista previa, pero
    /// escrito directamente en el disco. Una foto de 12 MP en base64 son
    /// 30 MB de cadena cruzando el canal para acabar en un fichero.
    #[test]
    fn guardar_una_imagen_del_pdf_deja_un_png_que_se_abre() {
        let dir = std::env::temp_dir();
        let origen = dir.join("imagenes-guardar-origen.png");
        image::RgbaImage::from_pixel(40, 20, image::Rgba([10, 200, 30, 255]))
            .save(&origen)
            .expect("crear el png");
        let pdf = dir.join("imagenes-guardar.pdf");
        crate::tests::crea_pdf(&["Con foto"], &pdf);
        let work = pdf.to_string_lossy().into_owned();
        add_image(
            work.clone(),
            0,
            origen.to_string_lossy().into_owned(),
            80.0,
            80.0,
        )
        .expect("insertar la imagen");
        let imagenes = get_images(work.clone(), 0).expect("imágenes");
        assert_eq!(imagenes.len(), 1);

        let dest = dir.join("imagenes-guardar-salida.png");
        save_image_data(
            work.clone(),
            0,
            imagenes[0].object_index,
            dest.to_string_lossy().into_owned(),
        )
        .expect("guardar la imagen");
        let salida = image::open(&dest).expect("el PNG se abre").to_rgba8();
        assert_eq!(
            (salida.width(), salida.height()),
            (40, 20),
            "el tamaño del bitmap"
        );
        assert_eq!(salida.get_pixel(20, 10).0, [10, 200, 30, 255], "y su color");

        // un objeto que no es una imagen se dice en llano
        let err =
            save_image_data(work.clone(), 0, 999, dest.to_string_lossy().into_owned()).unwrap_err();
        assert!(
            !err.contains("os error") && !err.contains("Pdfium"),
            "jerga: {err}"
        );

        for p in [&origen, &pdf, &dest] {
            std::fs::remove_file(p).ok();
        }
    }
    use super::*;
    use crate::tests::crea_pdf;

    #[test]
    fn extrae_contenido_de_imagen_estampada() {
        let dir = std::env::temp_dir();
        let pdf = dir.join("imagenes-get-data-test.pdf");
        crea_pdf(&["Página con imagen"], &pdf);
        let work = pdf.to_string_lossy().to_string();

        // PNG 4x4 rojo opaco
        let mut img = image::RgbaImage::new(4, 4);
        for (_, _, p) in img.enumerate_pixels_mut() {
            *p = image::Rgba([210, 10, 10, 255]);
        }
        let mut buf = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(img)
            .write_to(&mut buf, image::ImageFormat::Png)
            .expect("codificar png");
        let png_b64 = base64::engine::general_purpose::STANDARD.encode(buf.into_inner());
        crate::firmas_visuales::stamp_signature(work.clone(), 0, png_b64, 50.0, 50.0, 80.0, 80.0)
            .expect("estampar");

        let imgs = get_images(work.clone(), 0).expect("listar imágenes");
        assert_eq!(imgs.len(), 1);
        let b64 = get_image_data(work, 0, imgs[0].object_index).expect("extraer contenido");
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(b64)
            .expect("base64");
        let out = image::load_from_memory(&bytes)
            .expect("PNG válido")
            .to_rgba8();
        let p = out.get_pixel(out.width() / 2, out.height() / 2);
        assert!(p[0] > 150 && p[1] < 100, "esperaba rojo, hay {p:?}");
    }

    #[allow(unused_imports)]
    use crate::{render_page_b64, tests::textos_de};

    /// **G3.** Recortar una imagen: se queda el trozo que se marca, en el
    /// sitio que se marca, y lo de fuera **desaparece del fichero** (no se
    /// esconde detrás). Se comprueba con los bounds y con los píxeles del
    /// bitmap que queda.
    #[test]
    fn recortar_una_imagen_se_queda_con_el_trozo_marcado() {
        let dir = std::env::temp_dir();
        let tmp = dir.join("imagenes-recortar-test.pdf");
        let png = dir.join("imagenes-recortar-test.png");
        crea_pdf(&["Con imagen"], &tmp);
        let work = tmp.to_string_lossy().into_owned();

        // mitad izquierda roja, mitad derecha azul: así se sabe qué trozo
        // ha quedado sin mirar coordenadas
        let mut img = image::RgbaImage::from_pixel(80, 40, image::Rgba([200, 30, 30, 255]));
        for x in 40..80 {
            for y in 0..40 {
                img.put_pixel(x, y, image::Rgba([30, 30, 200, 255]));
            }
        }
        img.save(&png).expect("crear png");
        add_image(
            work.clone(),
            0,
            png.to_string_lossy().into_owned(),
            100.0,
            200.0,
        )
        .expect("insertar");
        let antes = &get_images(work.clone(), 0).expect("imágenes")[0];
        assert!((antes.w - 80.0).abs() < 1.0 && (antes.h - 40.0).abs() < 1.0);

        // recortar la mitad derecha (la azul)
        let corte = crate::Rect {
            x: antes.x + 40.0,
            y: antes.y,
            w: 40.0,
            h: 40.0,
        };
        let indice = antes.object_index;
        crop_image(work.clone(), 0, indice, corte.clone()).expect("recortar");

        let despues = &get_images(work.clone(), 0).expect("imágenes")[0];
        assert!(
            (despues.w - 40.0).abs() < 1.5 && (despues.h - 40.0).abs() < 1.5,
            "la imagen recortada mide {:.1}x{:.1}",
            despues.w,
            despues.h
        );
        assert!(
            (despues.x - corte.x).abs() < 1.5 && (despues.y - corte.y).abs() < 1.5,
            "y se queda donde se marcó el recorte"
        );

        // lo que queda es el trozo azul: el rojo ya no está en el fichero
        let b64 = get_image_data(work.clone(), 0, despues.object_index).expect("bytes");
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(b64)
            .expect("base64");
        let recortada = image::load_from_memory(&bytes).expect("png").to_rgba8();
        let centro = recortada
            .get_pixel(recortada.width() / 2, recortada.height() / 2)
            .0;
        assert!(
            centro[2] > 150 && centro[0] < 100,
            "el trozo que queda tenía que ser el azul: {centro:?}"
        );

        // un recorte fuera de la imagen se dice, no se hace a medias
        let fuera = crate::Rect {
            x: 0.0,
            y: 0.0,
            w: 20.0,
            h: 20.0,
        };
        assert!(crop_image(work.clone(), 0, despues.object_index, fuera).is_err());

        std::fs::remove_file(&tmp).ok();
        std::fs::remove_file(&png).ok();
    }

    #[test]
    fn imagenes() {
        let dir = std::env::temp_dir();
        let tmp = dir.join("editor_pdf_test_imagenes.pdf");
        let png = dir.join("editor_pdf_test_imagen.png");
        let png2 = dir.join("editor_pdf_test_imagen2.png");
        crea_pdf(&["Con imagen"], &tmp);
        let work = tmp.to_string_lossy().into_owned();

        // dos PNGs pequeños de colores distintos
        image::RgbaImage::from_pixel(80, 40, image::Rgba([200, 30, 30, 255]))
            .save(&png)
            .expect("crear png");
        image::RgbaImage::from_pixel(40, 40, image::Rgba([30, 30, 200, 255]))
            .save(&png2)
            .expect("crear png2");

        // insertar en (100, 200): tamaño natural 80x40 pt
        add_image(
            work.clone(),
            0,
            png.to_string_lossy().into_owned(),
            100.0,
            200.0,
        )
        .expect("añadir imagen");
        let imgs = get_images(work.clone(), 0).expect("listar imágenes");
        assert_eq!(imgs.len(), 1, "imágenes: {}", imgs.len());
        let im = &imgs[0];
        assert!(
            (im.x - 100.0).abs() < 2.0 && (im.y - 200.0).abs() < 2.0,
            "posición: ({}, {})",
            im.x,
            im.y
        );
        assert!(
            (im.w - 80.0).abs() < 2.0 && (im.h - 40.0).abs() < 2.0,
            "tamaño: {}x{}",
            im.w,
            im.h
        );

        // mover y redimensionar
        transform_image(
            work.clone(),
            0,
            im.object_index,
            50.0,
            300.0,
            160.0,
            80.0,
            None,
            None,
            None,
        )
        .expect("transformar");
        let imgs = get_images(work.clone(), 0).expect("relistar");
        let im = &imgs[0];
        assert!(
            (im.x - 50.0).abs() < 2.0 && (im.y - 300.0).abs() < 2.0,
            "posición tras mover: ({}, {})",
            im.x,
            im.y
        );
        assert!(
            (im.w - 160.0).abs() < 2.0 && (im.h - 80.0).abs() < 2.0,
            "tamaño tras redimensionar: {}x{}",
            im.w,
            im.h
        );

        // reemplazar manteniendo bounds
        replace_image(
            work.clone(),
            0,
            im.object_index,
            png2.to_string_lossy().into_owned(),
        )
        .expect("reemplazar");
        let imgs = get_images(work.clone(), 0).expect("listar tras reemplazo");
        assert_eq!(imgs.len(), 1);
        assert!(
            (imgs[0].w - 160.0).abs() < 2.0 && (imgs[0].h - 80.0).abs() < 2.0,
            "bounds tras reemplazo: {}x{}",
            imgs[0].w,
            imgs[0].h
        );

        // eliminar
        delete_image(work.clone(), 0, imgs[0].object_index).expect("eliminar");
        assert!(get_images(work.clone(), 0)
            .expect("listar final")
            .is_empty());

        render_page_b64(work.clone(), 0, 200, None).expect("render tras imágenes");
        for f in [&tmp, &png, &png2] {
            std::fs::remove_file(f).ok();
        }
    }

    #[test]
    fn la_vista_previa_no_mueve_la_imagen() {
        let pdf = std::env::temp_dir().join("imagenes-preview-test.pdf");
        crea_pdf(&["Página con imagen"], &pdf);
        let work = pdf.to_string_lossy().to_string();

        // PNG 200x120 azul: en puntos no mide lo mismo que en píxeles tras
        // redimensionarlo, que es cuando pdfium-render reescala el objeto
        let png = std::env::temp_dir().join("imagenes-preview-test.png");
        let mut img = image::RgbaImage::new(200, 120);
        for (_, _, p) in img.enumerate_pixels_mut() {
            *p = image::Rgba([30, 80, 200, 255]);
        }
        img.save(&png).expect("guardar png");
        add_image(
            work.clone(),
            0,
            png.to_string_lossy().to_string(),
            100.0,
            300.0,
        )
        .expect("insertar imagen");
        let idx = get_images(work.clone(), 0)
            .expect("listar imágenes")
            .last()
            .expect("una imagen")
            .object_index;
        transform_image(
            work.clone(),
            0,
            idx,
            150.0,
            350.0,
            100.0,
            60.0,
            None,
            None,
            None,
        )
        .expect("redimensionar");

        let bounds = |v: &[ImageInfo]| -> Vec<(u32, i32, i32, i32, i32)> {
            v.iter()
                .map(|i| {
                    (
                        i.object_index,
                        i.x.round() as i32,
                        i.y.round() as i32,
                        i.w.round() as i32,
                        i.h.round() as i32,
                    )
                })
                .collect()
        };
        let antes = bounds(&get_images(work.clone(), 0).expect("bounds antes"));
        let render_antes =
            crate::render_page_png(work.clone(), 0, 400, true).expect("render antes");

        get_image_data(work.clone(), 0, idx).expect("vista previa");

        let despues = bounds(&get_images(work.clone(), 0).expect("bounds después"));
        let render_despues =
            crate::render_page_png(work.clone(), 0, 400, true).expect("render después");
        assert_eq!(antes, despues, "la vista previa movió la imagen");
        assert!(
            render_antes == render_despues,
            "la vista previa cambió el render de la página"
        );
        std::fs::remove_file(&png).ok();
    }

    /// La caja de la primera imagen de la página 0: (índice, x, y, w, h).
    fn caja(work: &str) -> (u32, f32, f32, f32, f32) {
        let i = &get_images(work.to_string(), 0).expect("listar imágenes")[0];
        (i.object_index, i.x, i.y, i.w, i.h)
    }

    /// Girar y voltear una imagen es la fila contextual de Acrobat: 90° a
    /// un lado, 90° al otro, espejo horizontal y vertical. Girada 90°, sus
    /// bounds salen con el ancho y el alto intercambiados y sigue centrada
    /// donde estaba.
    #[test]
    fn girar_y_voltear_una_imagen() {
        let dir = std::env::temp_dir();
        let pdf = dir.join("imagenes-girar-test.pdf");
        let png = dir.join("imagenes-girar-test.png");
        crea_pdf(&["Con imagen"], &pdf);
        let work = pdf.to_string_lossy().into_owned();
        // apaisada y con dos mitades de color distinto, para que el volteo
        // se note en el render
        let mut img = image::RgbaImage::new(120, 40);
        for (x, _, p) in img.enumerate_pixels_mut() {
            *p = if x < 60 {
                image::Rgba([210, 30, 30, 255])
            } else {
                image::Rgba([30, 30, 210, 255])
            };
        }
        img.save(&png).expect("crear png");
        add_image(
            work.clone(),
            0,
            png.to_string_lossy().into_owned(),
            100.0,
            300.0,
        )
        .expect("insertar");
        let (idx, x, y, w, h) = caja(&work);
        assert!(w > h, "de partida es más ancha que alta");

        transform_image(work.clone(), 0, idx, x, y, w, h, Some(90), None, None).expect("girar 90°");
        let (idx, gx, gy, gw, gh) = caja(&work);
        assert!(
            (gw - h).abs() < 2.0 && (gh - w).abs() < 2.0,
            "girada 90° tenía que medir {h}x{w} y mide {gw}x{gh}"
        );
        assert!(
            ((gx + gw / 2.0) - (x + w / 2.0)).abs() < 2.0
                && ((gy + gh / 2.0) - (y + h / 2.0)).abs() < 2.0,
            "la imagen girada se ha ido de sitio: ({gx},{gy})"
        );

        // voltear cambia el render sin cambiar la caja (girada 90°, las dos
        // mitades de color están una encima de otra: el espejo que se nota
        // es el vertical)
        let antes = crate::render_page_png(work.clone(), 0, 400, true).expect("render");
        transform_image(work.clone(), 0, idx, gx, gy, gw, gh, None, None, Some(true))
            .expect("voltear");
        let (idx, _, _, vw, vh) = caja(&work);
        assert!(
            (vw - gw).abs() < 2.0 && (vh - gh).abs() < 2.0,
            "voltear no cambia el tamaño: {vw}x{vh}"
        );
        let despues = crate::render_page_png(work.clone(), 0, 400, true).expect("render");
        assert!(antes != despues, "voltear tiene que verse en el render");

        // un giro que no sea múltiplo de 90 se rechaza en llano
        let err = transform_image(
            work.clone(),
            0,
            idx,
            100.0,
            300.0,
            60.0,
            40.0,
            Some(37),
            None,
            None,
        )
        .unwrap_err();
        assert!(err.contains("múltiplos de 90"), "{err}");
        std::fs::remove_file(&pdf).ok();
        std::fs::remove_file(&png).ok();
    }

    /// Traer al frente y enviar al fondo: cuando la imagen tapa el texto (o
    /// al revés) es lo único que arregla la página, y el resto del
    /// contenido no se puede perder por el camino.
    #[test]
    fn traer_al_frente_y_enviar_al_fondo() {
        let dir = std::env::temp_dir();
        let pdf = dir.join("imagenes-orden-test.pdf");
        let png = dir.join("imagenes-orden-test.png");
        crea_pdf(&["Texto de la página"], &pdf);
        let work = pdf.to_string_lossy().into_owned();
        image::RgbaImage::from_pixel(60, 30, image::Rgba([30, 160, 60, 255]))
            .save(&png)
            .expect("crear png");
        add_image(
            work.clone(),
            0,
            png.to_string_lossy().into_owned(),
            40.0,
            120.0,
        )
        .expect("insertar");
        assert_eq!(
            caja(&work).0,
            1,
            "la imagen entra la última, encima del texto"
        );

        reorder_image(work.clone(), 0, 1, false).expect("al fondo");
        assert_eq!(caja(&work).0, 0, "ahora se pinta la primera");
        let t = textos_de(&pdf).join(" ");
        assert!(
            t.contains("Texto de la página"),
            "el texto sigue ahí: {t:?}"
        );

        reorder_image(work.clone(), 0, 0, true).expect("al frente");
        assert_eq!(caja(&work).0, 1, "vuelve a estar encima");
        let t = textos_de(&pdf).join(" ");
        assert!(
            t.contains("Texto de la página"),
            "el texto sigue ahí: {t:?}"
        );
        std::fs::remove_file(&pdf).ok();
        std::fs::remove_file(&png).ok();
    }

    /// Deuda de R2: mover y redimensionar una imagen en una página GIRADA
    /// tiene que dejarla donde se pide, a 90° y a 270°. Es la ruta de
    /// escritura más delicada y no tenía juez.
    #[test]
    fn transformar_una_imagen_en_una_pagina_girada() {
        for veces in [1u8, 3] {
            let dir = std::env::temp_dir();
            let pdf = dir.join(format!("imagenes-girada-{veces}-test.pdf"));
            let png = dir.join(format!("imagenes-girada-{veces}-test.png"));
            crea_pdf(&["Girada"], &pdf);
            let work = pdf.to_string_lossy().into_owned();
            image::RgbaImage::from_pixel(60, 30, image::Rgba([200, 40, 40, 255]))
                .save(&png)
                .expect("crear png");
            for _ in 0..veces {
                crate::paginas::rotate_page(work.clone(), 0).expect("girar");
            }
            // la UI convierte el gesto al espacio propio de la página antes
            // de mandarlo; aquí se pide directamente en ese espacio
            add_image(
                work.clone(),
                0,
                png.to_string_lossy().into_owned(),
                80.0,
                200.0,
            )
            .expect("insertar en página girada");
            let (idx, ..) = caja(&work);
            let (dx, dy, dw, dh) = (120.0f32, 260.0f32, 90.0f32, 45.0f32);
            transform_image(work.clone(), 0, idx, dx, dy, dw, dh, None, None, None)
                .expect("transformar en página girada");
            let (_, x, y, w, h) = caja(&work);
            assert!(
                (x - dx).abs() < 1.5
                    && (y - dy).abs() < 1.5
                    && (w - dw).abs() < 1.5
                    && (h - dh).abs() < 1.5,
                "a {}° la imagen queda en ({x},{y}) {w}x{h} y se pidió ({dx},{dy}) {dw}x{dh}",
                veces as u32 * 90
            );
            std::fs::remove_file(&pdf).ok();
            std::fs::remove_file(&png).ok();
        }
    }
}
