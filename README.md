# Editor PDF

Editor de PDF de escritorio, rápido y multiplataforma (macOS y Windows).
A diferencia de la mayoría de editores libres, la edición de texto es **real**:
reescribe el contenido del PDF en lugar de poner parches encima.

## Funciones

- **Visor** — render nítido con PDFium, zoom, navegación, miniaturas y caché
  con prefetch para saltar de página al instante.
- **Selección y búsqueda** — selecciona texto con el ratón, cópialo (⌘C /
  Ctrl+C) y busca en todo el documento con resaltado de coincidencias
  (Enter/Shift+Enter para navegar).
- **Edición de texto real** — edita cualquier bloque de texto del documento o
  añade texto nuevo en el punto que quieras, con elección de fuente y tamaño
  o detección automática de la fuente del documento.
- **Imágenes** — inserta imágenes (PNG, JPG, WebP…), muévelas arrastrando,
  redimensiónalas con el tirador, reemplázalas o elimínalas — también las que
  ya venían en el PDF.
- **Páginas** — reordena, rota, elimina, une otro PDF o extrae páginas a un
  fichero nuevo, todo desde la barra de miniaturas.
- **Anotaciones** — resaltados, dibujo a mano alzada y notas adhesivas,
  como anotaciones estándar visibles en cualquier visor. Todas se pueden
  eliminar con un clic y deshacer.
- **Formularios AcroForm** — rellena campos de texto y marca casillas
  directamente sobre el documento.
- **Firma digital** — firma PAdES básica (PKCS#7 detached, RSA/SHA-256) con
  certificado PEM o contenedor `.p12`/`.pfx` protegido con contraseña.
- **Seguro por diseño** — trabajas siempre sobre una copia; el fichero
  original no se toca hasta que pulsas Guardar.
- **Interfaz cuidada** — tema claro y oscuro automático según el sistema.

## Stack

| Capa | Tecnología |
|------|------------|
| Shell de escritorio | [Tauri 2](https://tauri.app) |
| Interfaz | React + TypeScript + Vite |
| Núcleo | Rust |
| Motor PDF | [PDFium](https://pdfium.googlesource.com/pdfium/) vía `pdfium-render` |
| Firma digital | lopdf + RustCrypto (`cms`, `rsa`, `sha2`) — sin dependencias del sistema |

La UI nunca toca el PDF directamente: todo pasa por comandos Tauri hacia el
núcleo Rust, que serializa el acceso a PDFium en un hilo dedicado y cachea el
documento abierto.

## Compilar

Requisitos: [Rust](https://rustup.rs), [Bun](https://bun.sh) y el binario de
PDFium para tu plataforma (de
[pdfium-binaries](https://github.com/bblanchon/pdfium-binaries/releases)):

```bash
# macOS (arm64): libpdfium.dylib de pdfium-mac-arm64.tgz
# Windows (x64): pdfium.dll (en bin/) de pdfium-win-x64.tgz
# → copiar a src-tauri/lib/

bun install
bun run tauri dev     # desarrollo
bun run tauri build   # instalador (.dmg / .exe)
```

También hay un workflow de GitHub Actions
([`build.yml`](.github/workflows/build.yml)) que compila los instaladores de
Windows y macOS descargando PDFium automáticamente.

## Tests

```bash
cd src-tauri && cargo test
```

La suite cubre render, extracción y búsqueda de texto, gestión de páginas,
anotaciones, formularios, edición de texto, imágenes y la firma digital
(verificando ByteRange, hash y estructura CMS del PDF firmado).
