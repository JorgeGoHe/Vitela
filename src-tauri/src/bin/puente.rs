//! Puente de QA sin ventana: sirve los comandos de la app por HTTP para que
//! la UI de Vite funcione en un navegador normal. Ver puente_dev.rs.
//! Uso: `bun run qa:puente` (o `cargo run --bin puente` desde src-tauri/).

#[cfg(debug_assertions)]
fn main() {
    tauri_app_lib::puente_dev::arrancar_bin();
}

#[cfg(not(debug_assertions))]
fn main() {
    eprintln!("El puente de QA solo existe en compilación de desarrollo.");
}
