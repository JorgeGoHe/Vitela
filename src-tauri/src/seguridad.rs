//! Seguridad: proteger con contraseña (cifrado AES-256, revisión 6 de
//! ISO 32000-2, implementado con RustCrypto porque ni PDFium ni lopdf 0.34
//! saben escribir cifrado), aplanar anotaciones/formularios y redacción
//! real (elimina objetos del content stream, no solo los tapa).

use crate::{invalidate_doc_cache, on_pdfium_thread, pdfium, save_and_close, Rect};
use aes::cipher::{
    block_padding::{NoPadding, Pkcs7},
    BlockEncrypt, BlockEncryptMut, KeyInit, KeyIvInit,
};
use lopdf::{Dictionary, Document as LoDoc, Object, StringFormat};
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

/// Protege el PDF con contraseña (AES-256, R6) y lo escribe en `dest_path`.
/// Si no se da contraseña de propietario se reutiliza la de usuario.
#[tauri::command]
pub fn encrypt_pdf(
    work_path: String,
    dest_path: String,
    user_password: String,
    owner_password: Option<String>,
) -> Result<(), String> {
    if user_password.is_empty() {
        return Err("La contraseña no puede estar vacía".into());
    }
    let mut user = user_password.into_bytes();
    user.truncate(127);
    let mut owner = owner_password
        .filter(|p| !p.is_empty())
        .map(String::into_bytes)
        .unwrap_or_else(|| user.clone());
    owner.truncate(127);

    let mut doc =
        LoDoc::load(&work_path).map_err(|e| format!("No se pudo leer el PDF: {e}"))?;

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

    // permisos: todo permitido (P = -4), metadatos cifrados
    let p: i64 = -4;
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
        .map_err(|e| format!("No se pudo guardar: {e}"))?;
    Ok(())
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
        return Err("No se pudo abrir el PDF cifrado".into());
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
        return Err("No se pudo guardar la copia descifrada".into());
    }
    std::fs::write(dest_path, &escritor.buf)
        .map_err(|e| format!("No se pudo escribir la copia: {e}"))
}

/// Aplana anotaciones y campos de formulario: pasan a ser contenido fijo de
/// la página. Ojo: los resaltados/notas propios (sin /AP) desaparecen — la
/// UI avisa antes.
#[tauri::command]
pub fn flatten_pdf(work_path: String) -> Result<(), String> {
    on_pdfium_thread(move || {
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
    })
}

#[derive(Serialize)]
pub struct RedactReport {
    pub textos: u32,
    pub imagenes: u32,
}

/// Redacta un área: elimina de verdad los objetos de texto e imagen que la
/// tocan y pinta un rectángulo negro encima. Con `dry_run` solo cuenta qué
/// caería (para el aviso de la UI). Granularidad de objeto: un bloque de
/// texto que asome por el área cae entero.
#[tauri::command]
pub fn redact_area(
    work_path: String,
    page_index: u16,
    rect: Rect,
    dry_run: bool,
) -> Result<RedactReport, String> {
    on_pdfium_thread(move || {
        let pdfium = pdfium()?;
        let doc = pdfium
            .load_pdf_from_file(&work_path, None)
            .map_err(|e| e.to_string())?;
        let mut page = doc.pages().get(page_index).map_err(|e| e.to_string())?;
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
                .map_err(|e| e.to_string())?;
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
        .map_err(|e| e.to_string())?;
        page.objects_mut()
            .add_path_object(negro)
            .map_err(|e| e.to_string())?;
        page.regenerate_content().map_err(|e| e.to_string())?;
        drop(page);
        save_and_close(doc, &work_path)?;
        Ok(RedactReport { textos, imagenes })
    })
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
            out.to_string_lossy().to_string(),
            "clave123".into(),
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
            out.to_string_lossy().to_string(),
            "abc".into(),
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
        let texto = crate::get_page_text(info.work_path, 0).expect("texto");
        assert!(!texto.chars.is_empty());
    }

    #[test]
    fn aplanar_convierte_el_trazo_en_contenido() {
        let pdf = std::env::temp_dir().join("seguridad-flatten-test.pdf");
        crea_pdf(&["Página"], &pdf);
        let work = pdf.to_string_lossy().to_string();
        crate::add_stroke(
            work.clone(),
            0,
            vec![[100.0, 100.0], [200.0, 200.0], [250.0, 150.0]],
        )
        .expect("trazo");
        assert_eq!(crate::get_annotations(work.clone(), 0).unwrap().len(), 1);
        flatten_pdf(work.clone()).expect("aplanar");
        // la anotación desapareció pero su dibujo quedó en la página
        assert_eq!(crate::get_annotations(work, 0).unwrap().len(), 0);
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
        let texto_antes = crate::get_page_text(work.clone(), 0).unwrap();
        assert!(!texto_antes.chars.is_empty());

        let informe = redact_area(work.clone(), 0, area, false).expect("redactar");
        assert_eq!(informe.textos, 1);
        let texto = crate::get_page_text(work, 0).expect("texto tras redactar");
        assert!(
            texto.chars.is_empty(),
            "el texto sigue siendo extraíble: {} chars",
            texto.chars.len()
        );
    }
}
