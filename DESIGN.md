# Sistema de diseño — Editor PDF («Mesa de trabajo»)

## Contexto de producto
- **Qué es:** visor y editor de PDF de escritorio (Tauri, Mac/Windows/Linux) en español, con edición real de texto, anotaciones, firmas, páginas, formularios y seguridad.
- **Para quién:** uso personal y profesional general; la alternativa ligera al Acrobat pesado con suscripción.
- **Espacio:** editores PDF de escritorio (Acrobat, PDF Expert, Apple Preview, Foxit).
- **Tipo de proyecto:** app de escritorio (APP UI — superficie calmada de trabajo, no web de marketing).
- **La frase guía:** «hace lo mismo que Acrobat pero abre al instante y no me estorba — la herramienta desaparece y queda el documento».

## Dirección estética
- **Dirección:** «Mesa de trabajo» — utilitaria cálida. El PDF es papel físico; la app es la mesa de un maquetista. Calma de taller a las 9 de la mañana.
- **Nivel de decoración:** mínimo-intencional. La única decoración es el lienzo de alfombrilla y las marcas de esquina de la página activa. El resto lo hace la tipografía.
- **Humor:** sobrio con un guiño (el verde). Si el usuario piensa «qué bonita», hemos fallado; tiene que pensar «qué a gusto».
- **Prueba de fuego** para cada pantalla nueva: ¿parece una mesa de trabajo con papel encima, o parece un programa? Si parece un programa, se quita algo.
- **Referencias:** pdfexpert.com (agrupación por tareas + chips de color), Acrobat (fila contextual), Apple Preview (contención).

## Tipografía
- **UI (todo):** Instrument Sans — grotesca compacta con esquinas vivas; se lee como herramienta, no como marca. 13px regular en chrome, medium (500) para acciones. Nada de bold gritón.
- **Datos "de imprenta":** Fragment Mono — número de página, zoom, tamaños de fichero, atajos en tooltips, progreso («renderizando pág. 4…»). Donde aparece, el usuario lee metadatos del papel, no interfaz. Un solo peso.
- **Voz del producto (racionada):** Fraunces (optical size bajo, ~350, itálica permitida) — SOLO estado vacío, onboarding y confirmación de firma. Aparece ~4 veces en toda la app. Si asoma en un botón, el sistema está roto.
- **Carga:** self-hosteadas en `src/assets/fonts/` (woff2, licencia OFL) — nada de CDN en Tauri. En la app QA (navegador) valen igual.
- **Escala:** 11px (metadatos mono) · 13px (UI base) · 14px (cuerpo diálogos) · 16px (títulos de diálogo, semibold) · 20px (títulos de sección) · 24-28px (Fraunces, voz).

## Color
- **Enfoque:** contenido — 1 acento + neutros cálidos; el color es escaso y significa algo.

| Token | Light «mesa de día» | Dark «lámpara de escritorio» | Uso |
|---|---|---|---|
| `--chrome` | `#F5F3EC` papel hueso | `#171511` nogal | toolbar, sidebar, fondos de app |
| `--superficie` | `#FCFBF7` | `#201E18` | paneles, diálogos, tarjetas |
| `--texto` | `#1D1C18` tinta | `#EDE9DD` | texto principal (nunca negro/blanco puros) |
| `--tenue` | `#6F6A5C` lápiz HB | `#97907C` | secundario, placeholders |
| `--acento` | `#2743C0` estilográfica | `#8E9EF5` tinta diluida | selección, acciones primarias, foco |
| `--peligro` | `#C0392B` corrector | `#E06A55` | borrar, redactar, errores |
| `--lienzo` | `#40604F` alfombrilla de corte | `#232B26` | el fondo donde flota el documento |
| éxito | `#2F6B3A` | `#8FC79A` | confirmaciones |

- **Colores de anotación** (chips de herramienta): amarillo `#F5C400`, verde `#2EA043`, azul `#2743C0`, rojo `#C0392B`, negro tinta `#1D1C18`.
- **Dark:** cálido siempre — PROHIBIDO el azul-pizarra (#0F172A y familia). Es la misma mesa, de noche.

## Espaciado
- **Base:** 4px. **Densidad:** compacta-cómoda (toolbar 8, paneles 16).
- **Escala:** 2xs(2) xs(4) sm(8) md(16) lg(24) xl(32) 2xl(48).

## Layout
- **Enfoque:** disciplinado. Una barra superior agrupada por tarea (Seleccionar · Comentar · Editar · Firmar) + fila contextual de propiedades por herramienta; sidebar plegable; píldora flotante de navegación con datos en Fragment Mono.
- **Radios:** controles 4px · diálogos/paneles 6px · **máximo 6px** — cantos de mesa de corte, nada de píldoras de 16px (excepción única: la píldora de navegación existente puede quedar a 8px).
- **Sombras:** duras y cortas — papel apoyado: `0 2px 3px rgba(0,0,0,.35)` para el documento; `0 4px 6px rgba(0,0,0,.3)` máximo para diálogos. PROHIBIDO blur ≥ 20px.
- **Marcas de esquina:** cuatro escuadras finas (~18px, 1.5px, blanco al 25-28%) enmarcando la página activa en el lienzo.

## Motion
- **Enfoque:** mínimo-funcional. Nada de celebraciones ni coreografías.
- **Easing:** entrar ease-out · salir ease-in · mover ease-in-out.
- **Duración:** micro 120ms · corta 200ms · nada por encima de 300ms.
- Si algo tarda, contador honesto en Fragment Mono, no skeleton shimmer.

## Anti-slop (prohibiciones)
- Gradientes (ni sutiles), glassmorphism/blur de fondo, blobs decorativos.
- Radios burbuja uniformes; sombras blandas de 40px.
- Iconos sparkle, emojis en la UI, badges «Pro»/«New», mascotas.
- Dark mode azul-pizarra. Fraunces fuera de su papel. Animaciones de celebración.
- system-ui como identidad: descartado a propósito — la app debe tener la misma cara en Mac, Windows y Linux.

## Log de decisiones
| Fecha | Decisión | Motivo |
|---|---|---|
| 2026-08-11 | Sistema inicial «Mesa de trabajo» | /design-consultation: investigación (PDF Expert, Acrobat) + voz independiente; Jorge eligió «ligero y rápido» como lo memorable y aprobó la preview completa |
| 2026-08-11 | Lienzo verde alfombrilla por defecto | El riesgo firma del sistema; si cansa, se añade conmutador a gris cálido |
| 2026-08-11 | Chrome siempre visible (se descarta el auto-ocultar de la voz independiente) | En una herramienta con modos, la barra es el volante |
| 2026-08-11 | Píldoras flotantes (nav y hint) a 8px; resto de radios 6/4 | /design-review F-004: la excepción cubre la familia de píldoras, no 20/999px |
| 2026-08-11 | Espaciado: la app usa una escala compacta (6/10/14/18) coherente, documentada aquí en vez de migrar a la de 8px | /design-review F-007: es internamente consistente; migrarla no aporta al usuario |
| 2026-08-11 | Overlays de anotación pintan el color real (get_annotations expone stroke color) | /design-review F-003: la preview no puede mentir |
