# Errores de Python reproducidos a propósito

Regla de fidelidad (plan §0): si al portar se descubre un error en Python, **no
se corrige**: se reproduce tal cual y se anota aquí. La decisión de arreglarlo
se toma después de la auditoría final.

Formato de cada entrada:

```
## <id>
- Archivo y líneas: <ruta>:<desde>-<hasta>
- Qué hace mal: <efecto observable>
- Cómo se reproduce en Rust: <dónde queda el mismo comportamiento>
- Recomendación posterior: <qué habría que cambiar y qué fixtures tocaría>
```

Sin entradas todavía: las tareas de F2–F6 agregan las que encuentren.

## Hallazgos de F1-09 (TTS, `kokoro_onnx`)

Reproducidos tal cual en el oráculo (`parity/fixtures/tts.jsonl`); Rust devuelve `Err` donde Python lanza.

- `kokoro_onnx/sliding.py:130`: `edges[end - first + 1]` queda fuera del arreglo en la última ventana; `synthesize` siempre lanza `IndexError`.
- `kokoro_onnx/sliding.py:41`: `token_edges` con duraciones que suman 0 divide por cero y convierte NaN a `int64` (`-9223372036854775808`).
- `kokoro_onnx/pauses.py:38-39`: `pauses.insert` con menos de una trama (240 muestras a 24 kHz) lanza `ValueError` en `loudness.max()`.
- `kokoro_onnx/pauses.py:97`: con `timing.end` negativo usa el piso de Python (índice negativo); Rust satura a `usize`.
- `kokoro_onnx/__init__.py:105-106`: `_load_vocab({})` lanza `KeyError: 'vocab'` (un dict vacío no equivale a `None`).
- `kokoro_onnx/chunker.py:41`: `split_phonemes("a", max_length=0)` lanza `ValueError`; Rust devuelve `[]`.
- `np.mean` de 3 o más estilos suma por pares; la media secuencial de Rust puede diferir en el último bit (tolerancia `f32_rel = 1e-5`).
## Q-F6-04-01
- Archivo y líneas: `src/ezagent/voice/app.py:1692-1696`
- Qué hace mal: `_set_state` llama a `cfg.save_key("hud_hidden", False)` sin
  `try/except`; un fallo de disco (por ejemplo, `voice.json` de solo lectura)
  propaga `OSError` desde cualquier cambio de estado (`_hang_up`,
  `_start_call`, `_finish_turn`…) y puede dejar el hilo de la interfaz a medias.
- Cómo se reproduce en Rust: `states.rs::set_state` guarda la clave y, si falla,
  registra `No pude guardar hud_hidden: …` con `log`; el cambio de estado sigue
  porque `set_state` no devuelve `Result`.
- Recomendación posterior: envolver `save_key` en `try/except OSError` en Python
  (el estado ya cambió en memoria) y decidir si el puerto debe exponer el fallo
  a quien llama; no afecta a fixtures de paridad.


## Q-P-AGENT-A-01
- Archivo y líneas: `src/ezagent/agent/system_prompt.py:186-189`
- Qué hace mal: `_os_description()` usa `platform.system()` y `platform.release()`
  del equipo donde corre; el texto `{{os}}` y, por tanto, el prompt completo
  cambian de una máquina a otra (`Windows 11` en el oráculo). Los fixtures de
  `system_prompt` solo son deterministas en ese host.
- Cómo se reproduce en Rust: `system_prompt.rs` fija `DEFAULT_OS = "Windows 11"`
  y expone `set_os_description` para que la aplicación ponga la versión real; la
  prueba de paridad no cambia ese valor y compara el resto byte a byte.
- Recomendación posterior: inyectar la descripción del SO desde la configuración
  (o fijarla en el generador de fixtures) y regenerar `system_prompt.jsonl` en un
  host declarado; afecta a `system_prompt/build/*` y `system_prompt/parts/*`.
## Hallazgos de P-AGENT-B (traspasos, tools_d, archivos y núcleo)

### Q-PAGENTB-01
- Archivo y líneas: `src/ezagent/agent/tools_fs.py:426-431` (`move_path`), mismo patrón en `copy_path:361-366`.
- Qué hace mal: el `kind` del resultado se calcula **después** de mover (`shutil.move` u `os.replace`), cuando `src` ya no existe; mover una carpeta informa `"kind": "file"` en vez de `"folder"`. Los casos `tools_fs/call/move-folder`, `move-folder-empty` y `move-folder-overwrite` del oráculo lo fijan así.
- Cómo se reproduce en Rust: `tools_fs/handlers.rs::transfer` evalúa `kind_of(&src)` después de `move_into_place`/`record_move`, con el comentario del quirk; `copy_path` conserva `"folder"` porque el origen sigue existiendo.
- Recomendación posterior: calcular `kind` antes de mover en Python (`kind = "folder" if src.is_dir() else "file"` al entrar) y regenerar `parity/fixtures/tools_fs.jsonl` (3 casos cambian de `file` a `folder`).

## Hallazgos de P-HUD-A (geometría, modelo y pintura de la barra)

### Q-PHUDA-01
- Archivo y líneas: `src/ezagent/voice/hud/drop.py:111-120` (`emerge_order`), `188-195` (`form_center`, `row_center_y`).
- Qué hace mal: `clean_dock` y `layout_size` toleran una disposición desconocida (caen a la píldora), pero `emerge_order`, `form_center`, `row_center_y` y `slots` indexan `_SIDES[(layout, dock)]` y lanzan `KeyError` con el mismo valor que acaban de normalizar; el oráculo fija `('wide', 'sides')` en 24 casos.
- Cómo se reproduce en Rust: la API pública de `model::drop::geometry` cae a la píldora (decisión documentada); la prueba de paridad devuelve el texto del `KeyError` para `layout == "wide"` en esas tres funciones.
- Recomendación posterior: decidir una sola política (o validar la disposición al entrar, o caer siempre al valor por defecto) y regenerar los 24 casos de `hud_drop_model/geometry/{emerge-order,form-center,row-center}/wide-*`.

### Q-PHUDA-02
- Archivo y líneas: `src/ezagent/voice/hud/anim.py:134-144` (`lerp_rgb`).
- Qué hace mal: con `k` fuera de `0..1` devuelve enteros fuera de `0..255` (p. ej. `-4` o `306`); el contrato de color de la UI asume `u8` y los llama solo con `0..1`, pero el oráculo comprueba los extremos.
- Cómo se reproduce en Rust: `anim::lerp_rgb` recorta a `0..255` para el dibujo; `anim::lerp_rgb_raw` reproduce los enteros sin recortar y es lo que usa la prueba de paridad.
- Recomendación posterior: documentar el rango de `k` en Python o recortar allí también; no afecta a ningún llamador actual.

### Q-PHUDA-03
- Archivo y líneas: `src/ezagent/voice/hud/chat_geometry.py:116` (`_coverage`) y `157` (`_shadow`) junto a `drop.py:332-359` (`_MASKS`).
- Qué hace mal: las máscaras se cachean por `round(escala·100)` / `round(radio, 3)`, así que el primer uso de una clave fija la máscara para escalas distintas que redondean igual; el mismo fotograma cambia hasta 1 px según el orden de llamadas previo. Los PNG dorados dependen de ese orden (los cuadros quietos se generaron antes que las transiciones).
- Cómo se reproduce en Rust: `paint::drop::fluid::form_mask` usa la misma clave; la prueba de `hud_bar` ordena los casos como `parity/gen/hud_bar.py` (quietos primero; después emerger, silenciar y llamar por tema) para poblar la caché en el mismo orden.
- Recomendación posterior: redondear la escala antes de construir la máscara (`scale = round(scale, 2)`) para que caché y geometría coincidan siempre; regenerar los PNG.

### Q-PHUDA-04
- Archivo y líneas: `src/ezagent/voice/hud/theme_accent.py:32-39` (`parse_hex`).
- Qué hace mal: `str(value or "")` convierte `None` en `""` y `123` en `"123"`, de modo que acepta entradas que no son cadenas sin avisar; el oráculo incluye ambos casos.
- Cómo se reproduce en Rust: `theme::parse_hex` recibe `&str`; la prueba aplica la misma coerción (`Value::Null` → `""`, número → su texto) antes de llamar.
- Recomendación posterior: tipar la entrada como `str` en la firma o rechazar no-cadenas explícitamente; no cambia ninguna salida actual.
