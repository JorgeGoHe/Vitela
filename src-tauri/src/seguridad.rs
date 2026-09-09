//! Seguridad: proteger con contraseña (cifrado AES-256, revisión 6 de
//! ISO 32000-2, implementado con RustCrypto porque ni PDFium ni lopdf 0.34
//! saben escribir cifrado), aplanar anotaciones/formularios y redacción
//! real (elimina objetos del content stream, no solo los tapa).

use crate::{invalidate_doc_cache, on_pdfium_thread, pdfium, save_and_close, Rect};
use crate::historial::mutacion;
use aes::cipher::{
    block_padding::{NoPadding, Pkcs7},
    BlockEncrypt, BlockEncryptMut, KeyInit, KeyIvInit,
};
use lopdf::{Dictionary, Document as LoDoc, Object, ObjectId, Stream, StringFormat};
use pdfium_render::prelude::*;
use serde::Serialize;
use sha2::{Digest, Sha256, Sha384, Sha512};

type Aes128CbcEnc = cbc::Encryptor<aes::Aes128>;
type Aes256CbcEnc = cbc::Encryptor<aes::Aes256>;

fn aleatorio<const N: usize>() -> Result<[u8; N], String> {
    let mut buf = [0u8; N];
    getrandom::getrandom(&mut buf).map_err(|e| format!("Sin entropía: {e}"))?;
    Ok(buf)
}

/// Algoritmo 2.B de ISO 32000-2 (hash iterativo SHA-256/384/512 para R6).
fn hash_2b(password: &[u8], salt: &[u8], udata: &[u8]) -> Vec<u8> {
    let mut k: Vec<u8> = {
        let mut h = Sha256::new();
        h.update(password);
        h.update(salt);
        h.update(udata);
        h.finalize().to_vec()
    };
    let mut i: i32 = 0;
    let mut ultimo: i32 = 256;
    while i < 64 || ultimo > i - 32 {
        // K1 = (password ‖ K ‖ udata) × 64 — siempre múltiplo de 16 bytes
        let mut k1 = Vec::with_capacity(64 * (password.len() + k.len() + udata.len()));
        for _ in 0..64 {
            k1.extend_from_slice(password);
            k1.extend_from_slice(&k);
            k1.extend_from_slice(udata);
        }
        let e = Aes128CbcEnc::new_from_slices(&k[0..16], &k[16..32])
            .expect("clave AES-128 válida")
            .encrypt_padded_vec_mut::<NoPadding>(&k1);
        let suma: u32 = e[0..16].iter().map(|b| u32::from(*b)).sum();
        k = match suma % 3 {
            0 => Sha256::digest(&e).to_vec(),
            1 => Sha384::digest(&e).to_vec(),
            _ => Sha512::digest(&e).to_vec(),
        };
        ultimo = i32::from(*e.last().expect("E no vacío"));
        i += 1;
    }
    k.truncate(32);
    k
}

/// AES-256-CBC sin padding con IV cero (para /UE y /OE).
fn aes256_cbc_iv0_nopad(key: &[u8], data: &[u8]) -> Vec<u8> {
    Aes256CbcEnc::new_from_slices(key, &[0u8; 16])
        .expect("clave AES-256 válida")
        .encrypt_padded_vec_mut::<NoPadding>(data)
}

/// Cifra una cadena o stream: AES-256-CBC con IV aleatorio antepuesto y
/// padding PKCS#7 (formato AESV3).
fn cifra_contenido(fek: &[u8], data: &[u8]) -> Result<Vec<u8>, String> {
    let iv = aleatorio::<16>()?;
    let mut out = iv.to_vec();
    out.extend(
        Aes256CbcEnc::new_from_slices(fek, &iv)
            .expect("clave AES-256 válida")
            .encrypt_padded_vec_mut::<Pkcs7>(data),
    );
    Ok(out)
}

/// Recorre un objeto cifrando todas las cadenas y streams.
fn cifra_objeto(obj: &mut Object, fek: &[u8]) -> Result<(), String> {
    match obj {
        Object::String(bytes, fmt) => {
            *bytes = cifra_contenido(fek, bytes)?;
            *fmt = StringFormat::Hexadecimal;
        }
        Object::Array(items) => {
            for item in items {
                cifra_objeto(item, fek)?;
            }
        }
        Object::Dictionary(d) => {
            for (_, v) in d.iter_mut() {
                cifra_objeto(v, fek)?;
            }
        }
        Object::Stream(s) => {
            for (_, v) in s.dict.iter_mut() {
                cifra_objeto(v, fek)?;
            }
            let cifrado = cifra_contenido(fek, &s.content)?;
            s.dict.set("Length", cifrado.len() as i64);
            s.set_content(cifrado);
        }
        _ => {}
    }
    Ok(())
}

/// Lo que el diálogo de protección deja marcado; los tres van marcados por
/// defecto, como en Acrobat.
#[derive(serde::Deserialize, Clone, Copy, Debug)]
pub struct Permisos {
    #[serde(default = "si")]
    pub imprimir: bool,
    #[serde(default = "si")]
    pub copiar: bool,
    #[serde(default = "si")]
    pub editar: bool,
}

fn si() -> bool {
    true
}

impl Default for Permisos {
    fn default() -> Self {
        Permisos {
            imprimir: true,
            copiar: true,
            editar: true,
        }
    }
}

/// Máscara `/P` del spec (Tabla 22), con los bits numerados desde 1:
/// 3 (4) imprimir · 4 (8) modificar el contenido · 5 (16) copiar ·
/// 6 (32) comentar y rellenar · 9 (256) rellenar formularios ·
/// 10 (512) extraer para accesibilidad · 11 (1024) montar el documento ·
/// 12 (2048) imprimir en alta calidad. Los bits 1 y 2 van a 0 y todos los
/// reservados (7, 8 y 13–32) a 1: eso es el `-4` de «todo permitido».
///
/// La accesibilidad (bit 10) se deja siempre puesta, como hace Acrobat: un
/// lector de pantalla no es «copiar el texto».
pub(crate) fn mascara_p(permisos: &Permisos) -> i64 {
    let mut p: u32 = 0xFFFF_FFFC;
    if !permisos.imprimir {
        p &= !(4 | 2048);
    }
    if !permisos.copiar {
        p &= !16;
    }
    if !permisos.editar {
        p &= !(8 | 32 | 256 | 1024);
    }
    p as i32 as i64
}

/// Protección pendiente de un documento abierto: la que el usuario ha
/// puesto con «Proteger» y que `save_pdf` aplicará al guardar.
#[derive(Clone)]
pub(crate) struct Proteccion {
    pub user: String,
    pub owner: Option<String>,
    pub permisos: Permisos,
}

static PROTECCIONES: std::sync::LazyLock<
    std::sync::Mutex<std::collections::HashMap<String, Proteccion>>,
> = std::sync::LazyLock::new(Default::default);

fn protecciones() -> std::sync::MutexGuard<'static, std::collections::HashMap<String, Proteccion>> {
    PROTECCIONES.lock().unwrap_or_else(|e| e.into_inner())
}

fn anota_proteccion(
    work_path: &str,
    user: String,
    owner: Option<String>,
    permisos: Permisos,
) {
    protecciones().insert(
        work_path.to_string(),
        Proteccion {
            user,
            owner,
            permisos,
        },
    );
}

/// La protección puesta al documento abierto, si la hay. La consulta
/// `save_pdf`: guardar un documento protegido lo guarda protegido.
pub(crate) fn proteccion_de(work_path: &str) -> Option<Proteccion> {
    protecciones().get(work_path).cloned()
}

/// Olvida la protección de una copia de trabajo que se cierra.
pub(crate) fn olvida_proteccion(work_path: &str) {
    protecciones().remove(work_path);
}

/// Quita la protección del documento abierto: ni se guardará cifrado ni
/// pedirá contraseña al abrirlo. Es la acción explícita «Quitar la
/// contraseña…», que la UI solo ofrece si el documento tenía una.
///
/// La copia de trabajo de un PDF protegido ya está en claro (la descifra
/// `open_pdf`), así que aquí solo hay que olvidar la protección puesta y
/// barrer cualquier `/Encrypt` que quedara en el fichero.
#[tauri::command(async)]
pub fn remove_encryption(work_path: String) -> Result<(), String> {
    olvida_proteccion(&work_path);
    on_pdfium_thread(move || {
        invalidate_doc_cache();
        let mut doc = LoDoc::load(&work_path).map_err(|e| {
            crate::mensaje_llano(format!("No se ha podido leer el documento: {e}"))
        })?;
        if doc.trailer.get(b"Encrypt").is_err() {
            return Ok(());
        }
        doc.trailer.remove(b"Encrypt");
        let tmp = format!("{work_path}.tmp");
        doc.save(&tmp)
            .map_err(|e| crate::mensaje_llano(format!("No se ha podido guardar: {e}")))?;
        std::fs::rename(&tmp, &work_path).map_err(crate::mensaje_llano)
    })
}

/// Protege el PDF con contraseña (AES-256, R6). Con `dest_path` escribe ahí
/// una copia protegida y no toca el documento abierto; sin él, la
/// protección queda puesta al documento abierto y viaja con Guardar, que es
/// lo que hace Acrobat.
///
/// Por qué no se cifra la copia de trabajo en el sitio: quedaría ilegible
/// para el resto de comandos (PDFium pediría la contraseña en cada render)
/// y el documento en pantalla dejaría de funcionar. La protección se
/// registra y `save_pdf` la aplica al guardar.
#[tauri::command(async)]
pub fn encrypt_pdf(
    work_path: String,
    dest_path: Option<String>,
    user_password: String,
    owner_password: Option<String>,
    permisos: Option<Permisos>,
) -> Result<(), String> {
    if user_password.is_empty() {
        return Err("La contraseña no puede estar vacía".into());
    }
    let permisos = permisos.unwrap_or_default();
    let Some(dest_path) = dest_path else {
        anota_proteccion(&work_path, user_password, owner_password, permisos);
        return Ok(());
    };
    cifra_a(&work_path, &dest_path, &user_password, owner_password.as_deref(), permisos)
}

/// Escribe en `dest_path` una copia cifrada de `origen`.
pub(crate) fn cifra_a(
    origen: &str,
    dest_path: &str,
    user_password: &str,
    owner_password: Option<&str>,
    permisos: Permisos,
) -> Result<(), String> {
    let mut user = user_password.as_bytes().to_vec();
    user.truncate(127);
    let mut owner = owner_password
        .filter(|p| !p.is_empty())
        .map(|p| p.as_bytes().to_vec())
        .unwrap_or_else(|| user.clone());
    owner.truncate(127);
    let work_path = origen.to_string();
    let dest_path = dest_path.to_string();

    // lee la copia de trabajo: en el hilo de PDFium y con el caché
    // invalidado, para no competir con una mutación concurrente
    on_pdfium_thread(move || {
        invalidate_doc_cache();
        let mut doc = LoDoc::load(&work_path).map_err(|e| {
            crate::mensaje_llano(format!("No se ha podido leer el documento: {e}"))
        })?;

        // clave de cifrado del fichero y entradas del diccionario Encrypt
        let fek = aleatorio::<32>()?;
        let uvs = aleatorio::<8>()?;
        let uks = aleatorio::<8>()?;
        let ovs = aleatorio::<8>()?;
        let oks = aleatorio::<8>()?;

        let mut u = hash_2b(&user, &uvs, &[]);
        u.extend_from_slice(&uvs);
        u.extend_from_slice(&uks);
        let ue = aes256_cbc_iv0_nopad(&hash_2b(&user, &uks, &[]), &fek);
        let mut o = hash_2b(&owner, &ovs, &u);
        o.extend_from_slice(&ovs);
        o.extend_from_slice(&oks);
        let oe = aes256_cbc_iv0_nopad(&hash_2b(&owner, &oks, &u), &fek);

        crate::documento::marca_creador(&mut doc);

        // permisos del diálogo, metadatos cifrados
        let p: i64 = mascara_p(&permisos);
        let mut perms_block = [0u8; 16];
        perms_block[0..4].copy_from_slice(&(p as i32).to_le_bytes());
        perms_block[4..8].copy_from_slice(&[0xFF; 4]);
        perms_block[8] = b'T';
        perms_block[9] = b'a';
        perms_block[10] = b'd';
        perms_block[11] = b'b';
        perms_block[12..16].copy_from_slice(&aleatorio::<4>()?);
        let cipher = aes::Aes256::new_from_slice(&fek).expect("clave AES-256 válida");
        let mut perms = aes::Block::clone_from_slice(&perms_block);
        cipher.encrypt_block(&mut perms);

        // cifrar todas las cadenas y streams del documento
        let ids: Vec<lopdf::ObjectId> = doc.objects.keys().copied().collect();
        for id in ids {
            if let Some(obj) = doc.objects.get_mut(&id) {
                cifra_objeto(obj, &fek)?;
            }
        }

        let mut cf_std = Dictionary::new();
        cf_std.set("CFM", Object::Name(b"AESV3".to_vec()));
        cf_std.set("Length", 32i64);
        let mut cf = Dictionary::new();
        cf.set("StdCF", Object::Dictionary(cf_std));
        let mut enc = Dictionary::new();
        enc.set("Filter", Object::Name(b"Standard".to_vec()));
        enc.set("V", 5i64);
        enc.set("R", 6i64);
        enc.set("Length", 256i64);
        enc.set("CF", Object::Dictionary(cf));
        enc.set("StmF", Object::Name(b"StdCF".to_vec()));
        enc.set("StrF", Object::Name(b"StdCF".to_vec()));
        enc.set("U", Object::String(u, StringFormat::Hexadecimal));
        enc.set("UE", Object::String(ue, StringFormat::Hexadecimal));
        enc.set("O", Object::String(o, StringFormat::Hexadecimal));
        enc.set("OE", Object::String(oe, StringFormat::Hexadecimal));
        enc.set(
            "Perms",
            Object::String(perms.to_vec(), StringFormat::Hexadecimal),
        );
        enc.set("P", p);
        enc.set("EncryptMetadata", Object::Boolean(true));
        let enc_id = doc.add_object(enc);
        doc.trailer.set("Encrypt", Object::Reference(enc_id));
        if doc.trailer.get(b"ID").is_err() {
            let id1 = aleatorio::<16>()?.to_vec();
            let id2 = aleatorio::<16>()?.to_vec();
            doc.trailer.set(
                "ID",
                Object::Array(vec![
                    Object::String(id1, StringFormat::Hexadecimal),
                    Object::String(id2, StringFormat::Hexadecimal),
                ]),
            );
        }

        doc.save(&dest_path)
            .map_err(|e| format!("No se ha podido guardar: {e}"))?;
        Ok(())
    })
}

/// Puente FPDF_FILEWRITE → Vec<u8> para usar FPDF_SaveAsCopy con flags.
#[repr(C)]
struct EscritorEnMemoria {
    inner: FPDF_FILEWRITE,
    buf: Vec<u8>,
}

unsafe extern "C" fn escribe_bloque(
    p_this: *mut FPDF_FILEWRITE,
    data: *const std::os::raw::c_void,
    size: std::os::raw::c_ulong,
) -> std::os::raw::c_int {
    let escritor = p_this as *mut EscritorEnMemoria;
    let slice = std::slice::from_raw_parts(data as *const u8, size as usize);
    (*escritor).buf.extend_from_slice(slice);
    1
}

/// Guarda una copia DESCIFRADA de un PDF protegido. pdfium-render guarda
/// siempre con flags=0 (conserva el cifrado), así que aquí se llama a
/// FPDF_SaveAsCopy con FPDF_REMOVE_SECURITY vía bindings crudos. Debe
/// llamarse desde el hilo de PDFium.
pub(crate) fn guarda_descifrado(
    src_path: &str,
    password: &str,
    dest_path: &str,
) -> Result<(), String> {
    const FPDF_REMOVE_SECURITY: std::os::raw::c_int = 3;
    let bindings = pdfium()?.bindings();
    let doc = bindings.FPDF_LoadDocument(src_path, Some(password));
    if doc.is_null() {
        return Err("No se ha podido abrir el PDF con esa contraseña".into());
    }
    let mut escritor = Box::new(EscritorEnMemoria {
        inner: FPDF_FILEWRITE {
            version: 1,
            WriteBlock: Some(escribe_bloque),
        },
        buf: Vec::new(),
    });
    let ok = bindings.FPDF_SaveAsCopy(
        doc,
        &mut escritor.inner as *mut FPDF_FILEWRITE,
        FPDF_REMOVE_SECURITY as std::os::raw::c_ulong,
    );
    bindings.FPDF_CloseDocument(doc);
    if !bindings.is_true(ok) {
        return Err("No se ha podido preparar el documento sin contraseña".into());
    }
    std::fs::write(dest_path, &escritor.buf).map_err(|e| {
        crate::mensaje_llano(format!("No se ha podido preparar el documento: {e}"))
    })
}


/// Objeto (posiblemente referencia) resuelto a su diccionario.
fn dict_de<'a>(doc: &'a LoDoc, obj: &'a Object) -> Option<&'a Dictionary> {
    match obj {
        Object::Reference(rid) => doc.get_object(*rid).ok()?.as_dict().ok(),
        Object::Dictionary(d) => Some(d),
        _ => None,
    }
}

/// Entrada de un widget buscando hacia arriba por /Parent (los campos
/// heredan FT, V y DA del campo padre).
fn hereda(doc: &LoDoc, dict: &Dictionary, clave: &[u8]) -> Option<Object> {
    let mut actual = dict.clone();
    for _ in 0..32 {
        if let Ok(v) = actual.get(clave) {
            return Some(match v {
                Object::Reference(rid) => doc.get_object(*rid).ok()?.clone(),
                otro => otro.clone(),
            });
        }
        let Ok(Object::Reference(padre)) = actual.get(b"Parent") else {
            return None;
        };
        actual = doc.get_object(*padre).ok()?.as_dict().ok()?.clone();
    }
    None
}

/// Diccionario AcroForm del catálogo (directo o referencia).
fn acroform(doc: &LoDoc) -> Option<Dictionary> {
    let catalog = doc.catalog().ok()?;
    dict_de(doc, catalog.get(b"AcroForm").ok()?).cloned()
}

/// Texto de una cadena PDF: UTF-16BE con BOM o PDFDocEncoding (≈ latin-1).
fn texto_pdf(bytes: &[u8]) -> String {
    if bytes.len() >= 2 && bytes[0] == 0xFE && bytes[1] == 0xFF {
        let unidades: Vec<u16> = bytes[2..]
            .chunks_exact(2)
            .map(|c| u16::from_be_bytes([c[0], c[1]]))
            .collect();
        String::from_utf16_lossy(&unidades)
    } else {
        bytes.iter().map(|&b| b as char).collect()
    }
}

/// Estado (nombre) de una casilla a partir de /AS o /V: nombre, o cadena
/// (PDFium escribe "/Yes" como cadena al marcar desde la API).
fn estado_de(obj: &Object) -> Option<String> {
    let s = match obj {
        Object::Name(n) => String::from_utf8_lossy(n).into_owned(),
        Object::String(b, _) => texto_pdf(b),
        _ => return None,
    };
    Some(s.trim_start_matches('/').to_string())
}

/// Tamaño de fuente de una cadena /DA ("/Helv 12 Tf 0 g"); 0 = automático.
fn tamano_da(da: &str) -> f32 {
    let tokens: Vec<&str> = da.split_whitespace().collect();
    tokens
        .iter()
        .position(|t| *t == "Tf")
        .and_then(|i| i.checked_sub(1))
        .and_then(|i| tokens[i].parse::<f32>().ok())
        .unwrap_or(0.0)
}

/// Referencia a la Helvetica de /DR del AcroForm (o una nueva si no hay).
pub(crate) fn fuente_helvetica(doc: &mut LoDoc) -> ObjectId {
    let existente = acroform(doc).and_then(|form| {
        let dr = dict_de(doc, form.get(b"DR").ok()?)?;
        let fuentes = dict_de(doc, dr.get(b"Font").ok()?)?;
        match fuentes.get(b"Helv").ok()? {
            Object::Reference(rid) => Some(Ok(*rid)),
            Object::Dictionary(d) => Some(Err(d.clone())),
            _ => None,
        }
    });
    match existente {
        Some(Ok(rid)) => rid,
        Some(Err(d)) => doc.add_object(d),
        None => {
            let mut helv = Dictionary::new();
            helv.set("Type", Object::Name(b"Font".to_vec()));
            helv.set("Subtype", Object::Name(b"Type1".to_vec()));
            helv.set("BaseFont", Object::Name(b"Helvetica".to_vec()));
            helv.set("Encoding", Object::Name(b"WinAnsiEncoding".to_vec()));
            doc.add_object(helv)
        }
    }
}

/// Ids de las anotaciones de una página; las que estén como diccionario
/// directo dentro de /Annots pasan a objetos indirectos para poder
/// modificarlas por id.
fn annots_indirectos(doc: &mut LoDoc, page_id: ObjectId) -> Vec<ObjectId> {
    let (arr_ref, mut arr) = {
        let Ok(page) = doc.get_object(page_id).and_then(|o| o.as_dict()) else {
            return vec![];
        };
        match page.get(b"Annots") {
            Ok(Object::Reference(rid)) => match doc.get_object(*rid).and_then(|o| o.as_array()) {
                Ok(a) => (Some(*rid), a.clone()),
                Err(_) => return vec![],
            },
            Ok(Object::Array(a)) => (None, a.clone()),
            _ => return vec![],
        }
    };
    let mut ids = Vec::new();
    let mut cambiado = false;
    for item in arr.iter_mut() {
        match item {
            Object::Reference(rid) => ids.push(*rid),
            Object::Dictionary(d) => {
                let id = doc.add_object(std::mem::take(d));
                *item = Object::Reference(id);
                ids.push(id);
                cambiado = true;
            }
            _ => {}
        }
    }
    if cambiado {
        if let Some(rid) = arr_ref {
            if let Ok(o) = doc.get_object_mut(rid) {
                *o = Object::Array(arr);
            }
        } else if let Ok(page) = doc.get_object_mut(page_id).and_then(|o| o.as_dict_mut()) {
            page.set("Annots", Object::Array(arr));
        }
    }
    ids
}

/// Apariencia normal para un campo de texto sin /AP: XObject con el valor
/// en Helvetica. Devuelve el diccionario /AP a poner en el widget.
fn apariencia_texto(
    doc: &mut LoDoc,
    widget: &Dictionary,
    helv: ObjectId,
    da_form: &str,
) -> Option<Object> {
    let rect: Vec<f32> = widget
        .get(b"Rect")
        .ok()
        .and_then(|r| r.as_array().ok())
        .map(|a| a.iter().filter_map(|o| o.as_float().ok()).collect())?;
    if rect.len() != 4 {
        return None;
    }
    let w = (rect[2] - rect[0]).abs();
    let h = (rect[3] - rect[1]).abs();
    let valor = match hereda(doc, widget, b"V") {
        Some(Object::String(b, _)) => texto_pdf(&b),
        _ => String::new(),
    };
    let da = match hereda(doc, widget, b"DA") {
        Some(Object::String(b, _)) => texto_pdf(&b),
        _ => da_form.to_string(),
    };
    let mut size = tamano_da(&da);
    if size <= 0.0 {
        size = (h * 0.7).min(12.0);
    }
    // WinAnsi ≈ latin-1; lo que no quepa, '?'; escapar \ ( )
    let mut texto = Vec::with_capacity(valor.len());
    for c in valor.chars() {
        match c {
            '\n' | '\r' | '\t' => texto.push(b' '),
            '\\' | '(' | ')' => {
                texto.push(b'\\');
                texto.push(c as u8);
            }
            c if (c as u32) < 0x20 => {}
            c if (c as u32) <= 0xFF => texto.push(c as u8),
            _ => texto.push(b'?'),
        }
    }
    let y = ((h - size) / 2.0 + size * 0.22).max(1.0);
    let mut contenido =
        format!("/Tx BMC q BT /Helv {size:.2} Tf 0 g 2 {y:.2} Td (").into_bytes();
    contenido.extend_from_slice(&texto);
    contenido.extend_from_slice(b") Tj ET Q EMC");
    let mut fuentes = Dictionary::new();
    fuentes.set("Helv", Object::Reference(helv));
    let mut recursos = Dictionary::new();
    recursos.set("Font", Object::Dictionary(fuentes));
    let mut d = Dictionary::new();
    d.set("Type", Object::Name(b"XObject".to_vec()));
    d.set("Subtype", Object::Name(b"Form".to_vec()));
    d.set(
        "BBox",
        Object::Array(vec![0.into(), 0.into(), w.into(), h.into()]),
    );
    d.set("Resources", Object::Dictionary(recursos));
    let n_id = doc.add_object(Stream::new(d, contenido));
    let mut ap = Dictionary::new();
    ap.set("N", Object::Reference(n_id));
    Some(Object::Dictionary(ap))
}

/// Operadores de color para un `/BC` o `/BG` de `/MK` (gris, RGB o CMYK).
/// Array vacío = sin color, que en el spec es «no pintes nada».
fn ops_color(arr: &[Object], relleno: bool) -> Option<String> {
    let v: Vec<f32> = arr.iter().filter_map(|o| o.as_float().ok()).collect();
    let (op_g, op_rgb, op_cmyk) = if relleno {
        ("g", "rg", "k")
    } else {
        ("G", "RG", "K")
    };
    match v.len() {
        1 => Some(format!("{:.4} {op_g}", v[0])),
        3 => Some(format!("{:.4} {:.4} {:.4} {op_rgb}", v[0], v[1], v[2])),
        4 => Some(format!(
            "{:.4} {:.4} {:.4} {:.4} {op_cmyk}",
            v[0], v[1], v[2], v[3]
        )),
        _ => None,
    }
}

/// Dibujo del marco de un widget (fondo `/MK /BG` y borde `/MK /BC` con el
/// grosor de `/BS /W`) en una caja de `w`×`h` con origen en (0,0).
///
/// Es lo que pinta el entorno de formularios de PDFium al vuelo y lo que
/// `FPDFPage_Flatten` no encuentra por ninguna parte si el `/AP` no lo trae.
fn marco_widget(doc: &LoDoc, widget: &Dictionary, w: f32, h: f32) -> String {
    let mk = widget.get(b"MK").ok().and_then(|o| dict_de(doc, o));
    let bs = widget.get(b"BS").ok().and_then(|o| dict_de(doc, o));
    let grosor = bs
        .and_then(|d| d.get(b"W").ok())
        .and_then(|o| o.as_float().ok())
        .unwrap_or(1.0)
        .max(0.0);
    let mut ops = String::new();
    if let Some(fondo) = mk
        .and_then(|d| d.get(b"BG").ok())
        .and_then(|o| o.as_array().ok())
        .and_then(|a| ops_color(a, true))
    {
        ops.push_str(&format!("q {fondo} 0 0 {w:.2} {h:.2} re f Q\n"));
    }
    if grosor > 0.0 {
        if let Some(borde) = mk
            .and_then(|d| d.get(b"BC").ok())
            .and_then(|o| o.as_array().ok())
            .and_then(|a| ops_color(a, false))
        {
            ops.push_str(&format!(
                "q {borde} {grosor:.2} w {:.2} {:.2} {:.2} {:.2} re S Q\n",
                grosor / 2.0,
                grosor / 2.0,
                (w - grosor).max(0.0),
                (h - grosor).max(0.0)
            ));
        }
    }
    ops
}

/// Apariencia normal de una casilla sin `/AP`: el marco en el estado Off y
/// el marco con el aspa en el estado marcado. Devuelve `(/AP, estado)`.
fn apariencia_casilla(doc: &mut LoDoc, widget: &Dictionary) -> Option<(Object, String)> {
    let rect: Vec<f32> = widget
        .get(b"Rect")
        .ok()
        .and_then(|r| r.as_array().ok())
        .map(|a| a.iter().filter_map(|o| o.as_float().ok()).collect())?;
    if rect.len() != 4 {
        return None;
    }
    let w = (rect[2] - rect[0]).abs();
    let h = (rect[3] - rect[1]).abs();
    if w < 1.0 || h < 1.0 {
        return None;
    }
    let marco = marco_widget(doc, widget, w, h);
    // el nombre del estado marcado: el que ya use el campo, o «Yes»
    let encendido = hereda(doc, widget, b"V")
        .as_ref()
        .and_then(estado_de)
        .or_else(|| widget.get(b"AS").ok().and_then(estado_de))
        .filter(|e| e != "Off")
        .unwrap_or_else(|| "Yes".to_string());
    let aspa = format!(
        "{marco}q 0 g {:.2} w {:.2} {:.2} m {:.2} {:.2} l S {:.2} {:.2} m {:.2} {:.2} l S Q",
        (w.min(h) * 0.12).max(1.0),
        w * 0.2,
        h * 0.2,
        w * 0.8,
        h * 0.8,
        w * 0.2,
        h * 0.8,
        w * 0.8,
        h * 0.2
    );
    let forma = |contenido: &str| {
        let mut d = Dictionary::new();
        d.set("Type", Object::Name(b"XObject".to_vec()));
        d.set("Subtype", Object::Name(b"Form".to_vec()));
        d.set(
            "BBox",
            Object::Array(vec![0.into(), 0.into(), w.into(), h.into()]),
        );
        d.set("Resources", Object::Dictionary(Dictionary::new()));
        Stream::new(d, contenido.as_bytes().to_vec())
    };
    let off_id = doc.add_object(forma(&marco));
    let on_id = doc.add_object(forma(&aspa));
    let mut estados = Dictionary::new();
    estados.set("Off", Object::Reference(off_id));
    estados.set(encendido.clone(), Object::Reference(on_id));
    let mut ap = Dictionary::new();
    ap.set("N", Object::Dictionary(estados));
    let actual = hereda(doc, widget, b"V")
        .as_ref()
        .and_then(estado_de)
        .filter(|e| *e == encendido)
        .unwrap_or_else(|| "Off".to_string());
    Some((Object::Dictionary(ap), actual))
}

/// Estado /AS que debe mostrar una casilla según su /V y los estados de su
/// /AP /N. `None` si no hay nada que corregir.
fn estado_casilla(doc: &LoDoc, widget: &Dictionary) -> Option<String> {
    let ap = dict_de(doc, widget.get(b"AP").ok()?)?;
    let estados = dict_de(doc, ap.get(b"N").ok()?)?;
    let claves: Vec<String> = estados
        .iter()
        .map(|(k, _)| String::from_utf8_lossy(k).into_owned())
        .collect();
    let v = hereda(doc, widget, b"V").as_ref().and_then(estado_de);
    let actual = widget.get(b"AS").ok().and_then(estado_de);
    let deseado = match v {
        Some(v) if claves.contains(&v) => v,
        Some(_) if claves.iter().any(|k| k == "Off") => "Off".to_string(),
        _ => actual.clone()?,
    };
    let ya_es_nombre = matches!(widget.get(b"AS"), Ok(Object::Name(_)));
    if ya_es_nombre && actual.as_deref() == Some(deseado.as_str()) {
        None
    } else {
        Some(deseado)
    }
}

/// Pasada previa al aplanado (lopdf): FPDFPage_Flatten en modo impresión
/// descarta toda anotación sin el flag Print y todo widget sin apariencia,
/// así que aquí se pone el flag a todas (cubre PDFs de fuera y anotaciones
/// antiguas), se genera /AP a los campos de texto que no lo tengan y se
/// normaliza /AS de las casillas (PDFium escribe "/Yes" como cadena al
/// marcarlas y el aplanado no encuentra ese estado).
fn prepara_para_aplanar(work_path: &str) -> Result<(), String> {
    let mut doc =
        LoDoc::load(work_path).map_err(|e| format!("No se ha podido leer el PDF: {e}"))?;
    let da_form = acroform(&doc)
        .and_then(|f| match f.get(b"DA") {
            Ok(Object::String(b, _)) => Some(texto_pdf(b)),
            _ => None,
        })
        .unwrap_or_default();
    let mut helv: Option<ObjectId> = None;
    let paginas: Vec<ObjectId> = doc.get_pages().values().copied().collect();
    for page_id in paginas {
        for annot_id in annots_indirectos(&mut doc, page_id) {
            let Ok(dict) = doc.get_object(annot_id).and_then(|o| o.as_dict()).cloned() else {
                continue;
            };
            let mut cambios: Vec<(&str, Object)> = Vec::new();
            let flags = dict.get(b"F").ok().and_then(|f| f.as_i64().ok()).unwrap_or(0);
            if flags & 4 == 0 {
                cambios.push(("F", Object::Integer(flags | 4)));
            }
            let es_widget = matches!(
                dict.get(b"Subtype").and_then(|s| s.as_name()),
                Ok(b"Widget")
            );
            if es_widget {
                let ft = hereda(&doc, &dict, b"FT");
                match ft.as_ref().and_then(|f| f.as_name().ok()) {
                    Some(b"Tx") if dict.get(b"AP").is_err() => {
                        let h = match helv {
                            Some(h) => h,
                            None => *helv.insert(fuente_helvetica(&mut doc)),
                        };
                        if let Some(ap) = apariencia_texto(&mut doc, &dict, h, &da_form) {
                            cambios.push(("AP", ap));
                        }
                    }
                    // sin /AP el marco lo pintaba el visor al vuelo y el
                    // aplanado se quedaba sin nada que copiar: se genera
                    Some(b"Btn") if dict.get(b"AP").is_err() => {
                        if let Some((ap, estado)) = apariencia_casilla(&mut doc, &dict) {
                            cambios.push(("AP", ap));
                            cambios.push(("AS", Object::Name(estado.into_bytes())));
                        }
                    }
                    Some(b"Btn") => {
                        if let Some(estado) = estado_casilla(&doc, &dict) {
                            cambios.push(("AS", Object::Name(estado.clone().into_bytes())));
                            if matches!(dict.get(b"V"), Ok(Object::String(..))) {
                                cambios.push(("V", Object::Name(estado.into_bytes())));
                            }
                        }
                    }
                    _ => {}
                }
            }
            if cambios.is_empty() {
                continue;
            }
            if let Ok(d) = doc.get_object_mut(annot_id).and_then(|o| o.as_dict_mut()) {
                for (clave, valor) in cambios {
                    d.set(clave, valor);
                }
            }
        }
    }
    let tmp = format!("{work_path}.tmp");
    doc.save(&tmp)
        .map_err(|e| format!("No se ha podido guardar: {e}"))?;
    std::fs::rename(&tmp, work_path).map_err(|e| e.to_string())
}

/// Aplana anotaciones y campos de formulario: pasan a ser contenido fijo de
/// la página. Ojo: los resaltados/notas propios (sin /AP) desaparecen — la
/// UI avisa antes.
#[tauri::command(async)]
pub fn flatten_pdf(work_path: String) -> Result<(), String> {
    mutacion(work_path, |work_path| on_pdfium_thread(move || {
        // el caché puede tener el fichero abierto: cerrarlo antes del rename
        invalidate_doc_cache();
        prepara_para_aplanar(&work_path)?;
        let pdfium = pdfium()?;
        let doc = pdfium
            .load_pdf_from_file(&work_path, None)
            .map_err(|e| e.to_string())?;
        for i in 0..doc.pages().len() {
            let mut page = doc.pages().get(i).map_err(|e| e.to_string())?;
            page.flatten().map_err(|e| e.to_string())?;
        }
        save_and_close(doc, &work_path)?;
        Ok(())
    }))
}

#[derive(Serialize, Debug)]
pub struct RedactReport {
    pub textos: u32,
    pub imagenes: u32,
}

/// Redacta un área: elimina de verdad los objetos de texto e imagen que la
/// tocan y pinta un rectángulo negro encima. Con `dry_run` solo cuenta qué
/// caería (para el aviso de la UI). Granularidad de objeto: un bloque de
/// texto que asome por el área cae entero.
#[tauri::command(async)]
pub fn redact_area(
    work_path: String,
    page_index: u16,
    rect: Rect,
    dry_run: bool,
) -> Result<RedactReport, String> {
    // sin instantánea en el ensayo (dry_run): no se escribe nada
    let cuerpo = move |work_path: String| on_pdfium_thread(move || {
        let pdfium = pdfium()?;
        let doc = pdfium
            .load_pdf_from_file(&work_path, None)
            .map_err(crate::mensaje_llano)?;
        let mut page = doc.pages().get(page_index).map_err(crate::mensaje_llano)?;
        let page_h = page.height().value;
        // rect en coords PDF
        let rx0 = rect.x;
        let rx1 = rect.x + rect.w;
        let ry1 = page_h - rect.y; // borde superior
        let ry0 = page_h - rect.y - rect.h; // borde inferior
        let mut caen: Vec<usize> = Vec::new();
        let mut textos = 0u32;
        let mut imagenes = 0u32;
        {
            let objects = page.objects();
            for i in 0..objects.len() {
                let Ok(obj) = objects.get(i) else { continue };
                let es_texto = obj.as_text_object().is_some();
                let es_imagen = obj.as_image_object().is_some();
                if !es_texto && !es_imagen {
                    continue;
                }
                let Ok(b) = obj.bounds() else { continue };
                let solapa = b.left().value < rx1
                    && b.right().value > rx0
                    && b.bottom().value < ry1
                    && b.top().value > ry0;
                if solapa {
                    caen.push(i);
                    if es_texto {
                        textos += 1;
                    } else {
                        imagenes += 1;
                    }
                }
            }
        }
        if dry_run {
            drop(page);
            drop(doc);
            invalidate_doc_cache();
            return Ok(RedactReport { textos, imagenes });
        }
        for &i in caen.iter().rev() {
            let removed = page
                .objects_mut()
                .remove_object_at_index(i)
                .map_err(crate::mensaje_llano)?;
            // regla del proyecto: su Drop llama a FPDFPageObj_Destroy y
            // PDFium casca — fuga puntual asumida
            std::mem::forget(removed);
        }
        let negro = PdfPagePathObject::new_rect(
            &doc,
            PdfRect::new(
                PdfPoints::new(ry0),
                PdfPoints::new(rx0),
                PdfPoints::new(ry1),
                PdfPoints::new(rx1),
            ),
            None,
            None,
            Some(PdfColor::new(0, 0, 0, 255)),
        )
        .map_err(crate::mensaje_llano)?;
        page.objects_mut()
            .add_path_object(negro)
            .map_err(crate::mensaje_llano)?;
        page.regenerate_content().map_err(crate::mensaje_llano)?;
        drop(page);
        save_and_close(doc, &work_path)?;
        Ok(RedactReport { textos, imagenes })
    });
    if dry_run {
        cuerpo(work_path).map_err(crate::mensaje_llano)
    } else {
        mutacion(work_path, cuerpo)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::crea_pdf;

    #[test]
    fn cifrado_valida_con_pdfium() {
        let dir = std::env::temp_dir();
        let pdf = dir.join("seguridad-cifrar-src.pdf");
        let out = dir.join("seguridad-cifrar-out.pdf");
        crea_pdf(&["Contenido secreto"], &pdf);
        encrypt_pdf(
            pdf.to_string_lossy().to_string(),
            Some(out.to_string_lossy().to_string()),
            "clave123".into(),
            None,
            None,
        )
        .expect("cifrar");

        let out_s = out.to_string_lossy().to_string();
        on_pdfium_thread(move || {
            let pdfium = pdfium().expect("pdfium");
            // sin contraseña: debe fallar con error de contraseña
            match pdfium.load_pdf_from_file(&out_s, None) {
                Err(PdfiumError::PdfiumLibraryInternalError(
                    PdfiumInternalError::PasswordError,
                )) => {}
                otro => panic!("esperaba PasswordError, hay {otro:?}"),
            }
            // con la contraseña de usuario: abre y el texto sobrevive
            let doc = pdfium
                .load_pdf_from_file(&out_s, Some("clave123"))
                .expect("abrir con contraseña");
            let texto = doc
                .pages()
                .get(0)
                .unwrap()
                .text()
                .map(|t| t.all())
                .unwrap_or_default();
            assert!(texto.contains("Contenido secreto"), "{texto:?}");
        });
    }

    #[test]
    fn abrir_con_password_devuelve_copia_descifrada() {
        let dir = std::env::temp_dir();
        let pdf = dir.join("seguridad-abrir-src.pdf");
        let out = dir.join("seguridad-abrir-out.pdf");
        crea_pdf(&["Privado"], &pdf);
        encrypt_pdf(
            pdf.to_string_lossy().to_string(),
            Some(out.to_string_lossy().to_string()),
            "abc".into(),
            None,
            None,
        )
        .expect("cifrar");
        assert_eq!(
            crate::open_pdf(out.to_string_lossy().to_string(), None).unwrap_err(),
            "PASSWORD_REQUIRED"
        );
        let info = crate::open_pdf(
            out.to_string_lossy().to_string(),
            Some("abc".into()),
        )
        .expect("abrir con contraseña");
        assert!(info.had_password);
        // la copia de trabajo quedó sin cifrar
        let texto = crate::busqueda::get_page_text(info.work_path.clone(), 0).expect("texto");
        assert!(!texto.chars.is_empty());
        crate::close_document(info.work_path).expect("cerrar");
    }

    /// `/P` del fichero cifrado, tal como lo lee cualquier visor.
    fn permisos_de(path: &str) -> i64 {
        let doc = LoDoc::load(path).expect("cargar cifrado");
        let enc = match doc.trailer.get(b"Encrypt").expect("/Encrypt") {
            Object::Reference(rid) => doc.get_object(*rid).unwrap().as_dict().unwrap().clone(),
            Object::Dictionary(d) => d.clone(),
            otro => panic!("/Encrypt inesperado: {otro:?}"),
        };
        enc.get(b"P").and_then(|o| o.as_i64()).expect("/P")
    }

    /// La contraseña de permisos tiene que fijar los bits de `/P` de verdad:
    /// el diálogo no puede prometer lo que el fichero no dice.
    #[test]
    fn los_permisos_llegan_a_la_mascara_p() {
        let dir = std::env::temp_dir();
        let pdf = dir.join("seguridad-permisos-src.pdf");
        let out = dir.join("seguridad-permisos-out.pdf");
        crea_pdf(&["Con permisos"], &pdf);

        // los tres marcados: todo permitido, que es el -4 de siempre
        assert_eq!(mascara_p(&Permisos::default()), -4);

        encrypt_pdf(
            pdf.to_string_lossy().to_string(),
            Some(out.to_string_lossy().to_string()),
            "clave123".into(),
            Some("permisos456".into()),
            Some(Permisos {
                imprimir: false,
                copiar: true,
                editar: true,
            }),
        )
        .expect("cifrar sin imprimir");

        let p = permisos_de(&out.to_string_lossy());
        let bit = |n: u32| p & (1 << (n - 1)) != 0;
        assert!(!bit(3), "imprimir debe quedar prohibido: {p}");
        assert!(!bit(12), "y la impresión en alta calidad con él");
        assert!(bit(5), "copiar sigue permitido");
        assert!(bit(6), "comentar sigue permitido");
        assert!(bit(4), "editar sigue permitido");
        assert!(bit(7) && bit(8), "los bits reservados van a 1");
        assert!(bit(13) && bit(32), "y los de arriba también");

        // sin copiar: solo cae el bit 5; la accesibilidad (10) se queda
        let sin_copiar = mascara_p(&Permisos {
            imprimir: true,
            copiar: false,
            editar: true,
        });
        assert!(sin_copiar & 16 == 0 && sin_copiar & 4 != 0 && sin_copiar & 512 != 0);
        // sin editar: contenido, comentarios, formularios y montaje
        let sin_editar = mascara_p(&Permisos {
            imprimir: true,
            copiar: true,
            editar: false,
        });
        assert_eq!(sin_editar & (8 | 32 | 256 | 1024), 0);

        for f in [&pdf, &out] {
            std::fs::remove_file(f).ok();
        }
    }

    /// «Proteger» sin destino se aplica al documento abierto: ⌘S lo guarda
    /// cifrado, el documento en pantalla sigue funcionando (si se cifrara la
    /// copia de trabajo, PDFium pediría la contraseña en cada render) y
    /// «Quitar la contraseña…» lo devuelve a claro.
    #[test]
    fn proteger_el_documento_abierto_viaja_con_guardar() {
        let dir = std::env::temp_dir();
        let pdf = dir.join("seguridad-proteger-abierto.pdf");
        let guardado = dir.join("seguridad-proteger-abierto-guardado.pdf");
        crea_pdf(&["Documento abierto"], &pdf);
        let info = crate::open_pdf(pdf.to_string_lossy().to_string(), None).expect("abrir");
        let work = info.work_path.clone();

        encrypt_pdf(work.clone(), None, "clave123".into(), None, None).expect("proteger");

        // el documento en pantalla sigue siendo usable
        assert!(!crate::busqueda::get_page_text(work.clone(), 0)
            .expect("texto tras proteger")
            .chars
            .is_empty());

        crate::save_pdf(work.clone(), guardado.to_string_lossy().to_string()).expect("guardar");
        assert_eq!(
            crate::open_pdf(guardado.to_string_lossy().to_string(), None).unwrap_err(),
            "PASSWORD_REQUIRED",
            "lo guardado tiene que pedir la contraseña"
        );
        let protegido =
            crate::open_pdf(guardado.to_string_lossy().to_string(), Some("clave123".into()))
                .expect("abrir con contraseña");
        crate::close_document(protegido.work_path).expect("cerrar");

        // quitar la contraseña: lo guardado ya abre sin ella
        remove_encryption(work.clone()).expect("quitar la contraseña");
        crate::save_pdf(work.clone(), guardado.to_string_lossy().to_string()).expect("reguardar");
        let claro = crate::open_pdf(guardado.to_string_lossy().to_string(), None)
            .expect("abrir sin contraseña");
        crate::close_document(claro.work_path).expect("cerrar");

        crate::close_document(work).expect("cerrar la copia");
        for f in [&pdf, &guardado] {
            std::fs::remove_file(f).ok();
        }
    }

    #[test]
    fn aplanar_convierte_el_trazo_en_contenido() {
        let pdf = std::env::temp_dir().join("seguridad-flatten-test.pdf");
        crea_pdf(&["Página"], &pdf);
        let work = pdf.to_string_lossy().to_string();
        crate::anotaciones::add_stroke(
            work.clone(),
            0,
            vec![[100.0, 100.0], [200.0, 200.0], [250.0, 150.0]],
            None,
            None,
            None,
        )
        .expect("trazo");
        assert_eq!(crate::anotaciones::get_annotations(work.clone(), 0).unwrap().len(), 1);
        flatten_pdf(work.clone()).expect("aplanar");
        // la anotación desapareció pero su dibujo quedó en la página
        assert_eq!(crate::anotaciones::get_annotations(work, 0).unwrap().len(), 0);
    }

    /// Cuenta píxeles del render (600 px de ancho) que cumplan `pred`
    /// dentro del rect dado en coords de UI (puntos).
    fn pixeles_en(
        work: &str,
        x0: f32,
        y0: f32,
        x1: f32,
        y1: f32,
        pred: impl Fn(&image::Rgba<u8>) -> bool,
    ) -> u32 {
        let png = crate::render_page_png(work.to_string(), 0, 600).expect("render");
        let img = image::load_from_memory(&png).expect("PNG").to_rgba8();
        let escala = 600.0 / 595.28;
        let mut n = 0;
        for yy in (y0 * escala) as u32..(y1 * escala) as u32 {
            for xx in (x0 * escala) as u32..(x1 * escala) as u32 {
                if pred(img.get_pixel(xx, yy)) {
                    n += 1;
                }
            }
        }
        n
    }

    #[test]
    fn aplanar_conserva_apariencias_y_valores() {
        let pdf = std::env::temp_dir().join("seguridad-flatten-todo-test.pdf");
        crea_pdf(&["Formulario"], &pdf);
        let work = pdf.to_string_lossy().to_string();
        crate::formularios2::create_form_field(
            work.clone(),
            0,
            "text".into(),
            Rect { x: 60.0, y: 200.0, w: 250.0, h: 28.0 },
            "nombre".into(),
        )
        .expect("campo de texto");
        crate::formularios2::create_form_field(
            work.clone(),
            0,
            "checkbox".into(),
            Rect { x: 60.0, y: 250.0, w: 20.0, h: 20.0 },
            "acepto".into(),
        )
        .expect("casilla");
        let campos = crate::formularios::get_form_fields(work.clone(), 0).expect("campos");
        let texto = campos.iter().find(|c| c.name == "nombre").unwrap().annot_index;
        let casilla = campos.iter().find(|c| c.name == "acepto").unwrap().annot_index;
        crate::formularios::set_form_text(
            work.clone(),
            0,
            texto,
            "Relleno antes de aplanar".into(),
        )
        .expect("rellenar");
        crate::formularios::set_form_checked(work.clone(), 0, casilla, true).expect("marcar");
        crate::anotaciones2::add_stamp(
            work.clone(),
            0,
            "APROBADO".into(),
            [192, 57, 43, 255],
            300.0,
            500.0,
            22.0,
            None,
        )
        .expect("sello");
        crate::anotaciones::add_stroke(
            work.clone(),
            0,
            vec![[100.0, 600.0], [200.0, 620.0], [300.0, 600.0]],
            Some([46, 160, 67, 255]),
            Some(4.0),
            None,
        )
        .expect("trazo");
        crate::anotaciones2::add_shape(
            work.clone(),
            0,
            "rect".into(),
            100.0,
            650.0,
            250.0,
            720.0,
            [39, 67, 192, 255],
            None,
            3.0,
            None,
        )
        .expect("forma");
        assert_eq!(
            crate::anotaciones::get_annotations(work.clone(), 0).unwrap().len(),
            5
        );

        flatten_pdf(work.clone()).expect("aplanar");

        assert!(crate::anotaciones::get_annotations(work.clone(), 0)
            .unwrap()
            .is_empty());
        assert!(crate::formularios::get_form_fields(work.clone(), 0)
            .unwrap()
            .is_empty());
        let texto: String = crate::busqueda::get_page_text(work.clone(), 0)
            .unwrap()
            .chars
            .iter()
            .map(|c| c.ch.as_str())
            .collect();
        assert!(
            texto.contains("Relleno antes de aplanar"),
            "el valor del campo no está en la página: {texto:?}"
        );
        let rojos = pixeles_en(&work, 200.0, 470.0, 400.0, 530.0, |p| {
            p[0] > 150 && p[1] < 110 && p[2] < 110
        });
        let verdes = pixeles_en(&work, 90.0, 590.0, 310.0, 630.0, |p| {
            p[1] > 120 && p[0] < 110 && p[2] < 110
        });
        let azules = pixeles_en(&work, 90.0, 640.0, 260.0, 730.0, |p| {
            p[2] > 150 && p[0] < 110 && p[1] < 110
        });
        assert!(rojos > 0, "el sello desapareció al aplanar");
        assert!(verdes > 0, "el trazo desapareció al aplanar");
        assert!(azules > 0, "la forma desapareció al aplanar");
        let marca = pixeles_en(&work, 62.0, 252.0, 78.0, 268.0, |p| {
            p[0] < 100 && p[1] < 100 && p[2] < 100
        });
        assert!(marca > 0, "la marca de la casilla desapareció al aplanar");
    }

    /// Al aplanar, un campo deja de ser campo pero se sigue viendo igual
    /// (Acrobat): la casilla sin marcar conservaba la marca pero perdía el
    /// recuadro, porque el borde lo pintaba el entorno de formularios de
    /// PDFium desde /MK y no estaba en el /AP que copia el aplanado.
    #[test]
    fn aplanar_conserva_el_marco_de_las_casillas() {
        for sin_ap in [false, true] {
            let pdf = std::env::temp_dir()
                .join(format!("seguridad-flatten-casilla-{sin_ap}-test.pdf"));
            crea_pdf(&["Consentimiento"], &pdf);
            let work = pdf.to_string_lossy().to_string();
            crate::formularios2::create_form_field(
                work.clone(),
                0,
                "checkbox".into(),
                Rect { x: 60.0, y: 250.0, w: 20.0, h: 20.0 },
                "acepto".into(),
            )
            .expect("casilla");
            if sin_ap {
                // como los PDFs de fuera que dejan el marco en manos del
                // visor: el aplanado no tendría nada que copiar
                crate::cirugia(&work, |doc| {
                    let ids: Vec<_> = doc
                        .objects
                        .iter()
                        .filter(|(_, o)| {
                            o.as_dict()
                                .map(|d| matches!(d.get(b"FT").and_then(|f| f.as_name()), Ok(b"Btn")))
                                .unwrap_or(false)
                        })
                        .map(|(id, _)| *id)
                        .collect();
                    for id in ids {
                        if let Ok(d) = doc.get_object_mut(id).and_then(|o| o.as_dict_mut()) {
                            d.remove(b"AP");
                        }
                    }
                    Ok(())
                })
                .expect("quitar /AP");
            }
            let oscuros = |w: &str| {
                pixeles_en(w, 60.0, 250.0, 80.0, 270.0, |p| {
                    p[0] < 200 && p[1] < 200 && p[2] < 200
                })
            };
            assert!(
                oscuros(&work) > 0 || sin_ap,
                "la casilla no se ve ni antes de aplanar"
            );
            flatten_pdf(work.clone()).expect("aplanar");
            assert!(
                oscuros(&work) > 0,
                "sin /AP previo: {sin_ap} — el recuadro de la casilla desapareció al aplanar"
            );
            std::fs::remove_file(&pdf).ok();
        }
    }

    #[test]
    fn redaccion_elimina_texto_de_verdad() {
        let pdf = std::env::temp_dir().join("seguridad-redact-test.pdf");
        crea_pdf(&["Dato confidencial"], &pdf);
        let work = pdf.to_string_lossy().to_string();
        // el texto de crea_pdf está en (50,700) PDF → UI y≈128-142 en A4
        let area = Rect {
            x: 30.0,
            y: 110.0,
            w: 300.0,
            h: 60.0,
        };
        let preview =
            redact_area(work.clone(), 0, area.clone(), true).expect("dry run");
        assert_eq!(preview.textos, 1);
        let texto_antes = crate::busqueda::get_page_text(work.clone(), 0).unwrap();
        assert!(!texto_antes.chars.is_empty());

        let informe = redact_area(work.clone(), 0, area, false).expect("redactar");
        assert_eq!(informe.textos, 1);
        let texto = crate::busqueda::get_page_text(work, 0).expect("texto tras redactar");
        assert!(
            texto.chars.is_empty(),
            "el texto sigue siendo extraíble: {} chars",
            texto.chars.len()
        );
    }
}
