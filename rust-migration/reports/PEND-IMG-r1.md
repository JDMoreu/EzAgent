# PEND-IMG — el chat muestra JPEG, GIF, WebP y BMP igual que Pillow

## Resultado
- Brief/tarea: PEND-IMG
- Estado: completado

## Cambios
- `rust/Cargo.toml`: el crate `image` del workspace pasa a tener las features
  `png`, `jpeg`, `gif`, `webp` y `bmp` (las cinco familias de
  `media.IMAGE_EXTENSIONS`); sin dependencias nuevas de crate, solo las
  transitivas de `image`.
- `rust/Cargo.lock`: actualizado por cargo (`gif`, `image-webp`, `zune-jpeg`,
  `zune-core`, `color_quant`, `weezl`, `quick-error`).
- `rust/crates/ez-hud/src/paint/chat/imaging.rs` (nuevo): `decode(bytes)`,
  el equivalente de `PIL.Image.open(...).seek(0).convert("RGBA")` — el primer
  cuadro de GIF y WebP animados, y el alfa de BMP forzado a opaco porque Pillow
  lee todo BMP en modo RGB.
- `rust/crates/ez-hud/src/paint/chat/mod.rs`: declara `pub mod imaging`.
- `rust/crates/ez-hud/tests/chat_image_formats.rs` (nuevo): compara los píxeles
  decodificados con los que devuelve Pillow 13 del `.venv` para los diez
  fixtures, y comprueba que un archivo corrupto o de formato desconocido no
  decodifica.
- `rust/crates/ez-hud/tests/data/chat_images/` (nuevo): ocho imágenes de 8×6
  con su `.rgba` esperado y `generate.py` (generador reproducible con el Pillow
  del venv).
- `rust/crates/ez-app/src/app/chat_wiring/host.rs`: `ChatHost::load_image` usa
  `ez_hud::paint::chat::imaging::decode` en lugar de `decode_png` y del filtro
  `format != "png"`; el fallo de decodificación se registra con
  `tracing::warn!("No pude abrir la imagen {}: {error}")`, el mismo texto que el
  `logger.warning` del Python (`media.py:161`).
- `rust/crates/ez-app/src/app/chat_wiring/tests.rs`: la prueba del host añade un
  BMP fabricado a mano (dos píxeles), comprueba que se pinta y que un GIF con
  cabecera válida y cuerpo basura se queda sin imagen.

## Decisiones
- `// DECISION`: el decodificador vive en `ez-hud` (`paint/chat/imaging.rs`)
  porque `ez-app` no declara el crate `image` y el brief solo permite tocar
  `rust/Cargo.toml` para las features; el cableado de `ez-app` solo llama a la
  función.
- `// DECISION`: BMP con alfa 255. Pillow no lee el cuarto byte de un BMP de 32
  bits (`BmpImagePlugin` devuelve modo `RGB`), así que pintarlo como alfa daría
  imágenes transparentes donde el Python las muestra opacas. Fixture
  `alfa.bmp` con alfa 0 lo cubre.
- `// DECISION`: JPEG con tolerancia de 8 por canal (medido: máximo 7, p999 7
  en el fixture). Pillow usa libjpeg-turbo y `image` usa zune-jpeg: difieren el
  IDCT y el remuestreo de croma. Todo lo demás (PNG, GIF, BMP, WebP con y sin
  pérdida) coincide **byte a byte** con Pillow, incluido el primer cuadro de
  GIF y WebP animados.
- `// DECISION`: solo las cinco familias de `media.IMAGE_EXTENSIONS`. Pillow
  olfatea el contenido, así que abriría un TIFF/ICO/TGA renombrado a `.png`;
  aquí se pinta «No puedo mostrar esta imagen». No se añaden esas features
  porque el Python nunca las ofrece por extensión.

## Verificación
- `cargo fmt --all --check`: limpio.
- `cargo clippy -p ez-hud -p ez-app --all-targets -- -D warnings`: limpio.
- `cargo test -p ez-hud`: 520 passed, 0 failed (510 de unidad + 10 de
  integración; incluye `hud_chat_parity::every_chat_case_matches_python` en
  verde y con sus PNG intactos, y `chat_image_formats` con 3 pruebas).
- `cargo test -p ez-app`: 322 passed, 0 failed (incluye
  `chat_wiring::tests::host_reads_file_cards_previews_and_images`).
- Comparación con Pillow: 10 fixtures de 8×6 decodificados por ambos; delta
  máximo 0 en PNG/GIF/GIF transparente/GIF animado/BMP/BMP de 32 bits/WebP
  sin pérdida/WebP con pérdida/WebP animado, y 7 en JPEG (umbral de la prueba:
  8). Los `.rgba` los escribe `generate.py` desde
  `Image.open(...).seek(0).convert("RGBA")`.
- Reglas de `scan_diff.py` sobre el commit: `forbidden` vacío, `unwrap` vacío.
  `sizes` señala un archivo ajeno y anterior a esta tarea
  (`rust/crates/ez-app/src/app/activity/tasks.rs`, 604 líneas, commit
  `e4a5f75`); no se toca.
- Ningún lock nuevo: no hay estado compartido ni se retiene nada durante la
  lectura del archivo ni la decodificación.

## Fixtures consumidos
- `parity/fixtures/hud_chat.jsonl`: solo lectura, para confirmar que las
  tarjetas de imagen y sus textos ya estaban portados. 0 casos ejecutados (esta
  fase no escribe pruebas de paridad).
- Fixtures propios: 10 imágenes en `rust/crates/ez-hud/tests/data/chat_images/`
  (8 archivos + 10 `.rgba` esperados de Pillow), 3 casos de prueba.

## Pruebas portadas (`parity/test-map.csv`)
- `tests/test_chat_media.py::test_a_local_image_loads_as_rgba` → cubierta por
  `chat_image_formats::exact_formats_match_pillow` y por
  `chat_wiring::tests::host_reads_file_cards_previews_and_images`.
- `tests/test_chat_media.py::test_an_animated_gif_gives_only_its_first_frame` →
  `chat_image_formats::exact_formats_match_pillow` (fixtures `animado.gif` y
  `animado.webp`, comparados con el primer cuadro de Pillow).
- `tests/test_chat_media.py::test_a_corrupt_image_does_not_crash` →
  `chat_image_formats::a_corrupt_or_unknown_file_is_refused` y la tarjeta rota
  de la prueba del host.
- `tests/test_chat_media.py` (resto de rutas, fichas, vista previa y apertura)
  ya estaban portadas en `ez-chat/src/media.rs`; no se tocaron.

## Pendiente o riesgos
- La nota «imágenes del chat solo PNG» de
  `rust-migration/reports/code-first-progress.md` queda obsoleta; no es un
  archivo de mi alcance: la actualiza quien lleve la documentación.
- Un JPEG CMYK se convierte a RGB con las tablas de `image`, no con las de
  Pillow (que aplica su propia fórmula): puede haber diferencia de tono en
  archivos CMYK. No hay fixture porque el Python tampoco lo trata de forma
  especial.
- No se ejecutó la app con ventana; la decodificación se validó con pruebas
  contra los píxeles de Pillow, y el pintado (escalado, máscara redondeada y
  ficha) ya estaba cubierto por `hud_chat_parity`.
