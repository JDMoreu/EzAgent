# AGENTS.md — Contrato para agentes de código

Este archivo es obligatorio para cualquier agente (DeepSeek, MiniMax, Claude,
Codex u otro) que modifique este repositorio. Léelo completo antes de tocar
nada. Si una instrucción de un brief contradice este archivo, gana este archivo
salvo que el brief diga explícitamente «excepción a AGENTS.md».

## 1. Qué es EzAgent

EzAgent es una **capa de voz local para agentes de código**: escuchas, EzAgent
entiende la intención, despacha la tarea a Claude Code, OpenCode, Codex u otro
CLI (directamente o vía Herdr), vigila el progreso y te avisa por voz. No es un
asistente general: el trabajo real lo hacen los agentes delegados.

La aplicación es **Rust** (`ezagent-voice.exe`), un solo proceso con el bucle de
interfaz Win32 en el hilo principal. Pipeline: micrófono → VAD → whisper.cpp
(local) → LLM compatible con OpenAI o Anthropic → Kokoro TTS (local) → altavoz.
La interfaz es un HUD flotante (barra o círculo) con su ventana de Ajustes y un
chat escrito. Arquitectura completa: [`docs/architecture.md`](docs/architecture.md).

## 2. Cómo trabajar

1. **Una tarea por sesión.** Trabaja solo en el brief o la tarea que te han
   pasado. No empieces la siguiente.
2. **Solo los archivos listados** en «Archivos que se tocan». Si necesitas tocar
   otro, detente y explícalo en el informe; no lo hagas.
3. **Nada de refactors fuera de alcance**, renombrados masivos, cambios de
   formato en archivos que no tocas ni «mejoras» no pedidas.
4. **Pasos pequeños.** Cada paso del brief debe quedar en un diff de unas 300
   líneas como máximo. Ejecuta las pruebas al terminar cada paso.
5. **Si algo no está especificado**, elige la opción más simple y segura,
   márcala con `// DECISION: <motivo>` y menciónala en el informe. Nunca
   inventes alcance, dependencias ni APIs externas: si dudas de un flag de un
   CLI, hazlo configurable.
6. **No borres pruebas** salvo que el brief lo pida o el código que prueban se
   elimine en ese mismo brief.
7. **El Python de `src/` y `tests/` es la especificación mientras exista**: se
   lee y se ejecuta como oráculo, pero no se modifica. Los fixtures de
   `parity/fixtures/` y las trazas de `parity/golden/` y `parity/replay/` también
   son de solo lectura.
8. **Compatibilidad de datos**: `voice.json`, `llm.json`, `permissions.toml`,
   `tasks.json`, `history.db`, `board.db`, `chat.jsonl` y el diario se leen y
   escriben con el mismo formato que antes. No cambies una clave, un tipo ni un
   orden sin una prueba de ida y vuelta.

## 3. Comandos

Desde `rust/`:

```powershell
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --release -p ez-app --bin ezagent-voice
cargo build --release -p ez-swarm --bin ezs
```

La reproducción completa (25 escenarios contra las trazas doradas, ~7 minutos):

```powershell
cargo build -p ez-fakes --bins
cargo build -p ez-swarm --bin ezs
cargo test -p ez-app --features replay --test replay_parity
```

Desde la raíz del repositorio:

```powershell
powershell -File parity/regen.ps1 -Check     # el oráculo no cambió
powershell -File rust/gate.ps1 -Task <ID>    # puerta de una tarea
uv run pytest -q                             # referencia Python (debe seguir verde)
uv run ruff check src tests                  # referencia Python
```

No des una tarea por terminada con pruebas en rojo. Si una prueba ya fallaba
antes de tu cambio, demuéstralo (`git stash` + ejecutar) y dilo en el informe.

## 4. Convenciones de código

- Rust 2021, `cargo fmt` y `cargo clippy -- -D warnings` limpios.
- Identificadores, nombres de archivo y comentarios técnicos en **inglés**.
- Textos que ve u oye el usuario: **español neutro latinoamericano**, copiados
  literales del Python o del diseño aprobado, hasta la tilde. No uses
  «vosotros», «vale» ni giros de España. No se reescribe ningún texto aunque
  parezca mejorable.
- Ningún módulo `.rs` supera **600 líneas** (cuéntalas después de `cargo fmt`,
  que expande); ninguna función supera **60 líneas** (`clippy.toml`) salvo
  tablas de datos. Si un módulo crece, pártelo en una carpeta con submódulos
  (`modulo.rs` + `modulo/parte.rs`, declarados en el `modulo.rs`).
- Prohibido en código nuevo: `todo!`, `unimplemented!`, `#[ignore]`, la palabra
  `TODO`, `println!` (usa `tracing`; solo `ezs` y las opciones de línea de
  comandos que ya imprimen pueden hacerlo), `remove_dir_all` y `.unwrap()` fuera
  de `tests/` (usa `?` o `.expect("motivo")`).
- **Nada de async ni tokio.** Hilos (`std::thread`), canales
  (`crossbeam-channel`), `parking_lot` o `std::sync`. La red va con
  `reqwest::blocking` en hilos de trabajo. Mismos nombres de hilo que antes
  (`ezagent-call`, `ezagent-turn`, `ezagent-answer`, `ezagent-speech`,
  `ezagent-stt`, `ezagent-autonomo`, `ezagent-swarm-sup`…).
- Nada bloquea el hilo de UI ni los hilos de audio. **Ningún lock se retiene
  durante una operación larga, una llamada a otro componente ni una operación de
  ventana**: se copia el estado con el lock tomado, se suelta y luego se llama.
  `parking_lot` no es reentrante: no vuelvas a pedir el mismo lock en el mismo
  hilo. El estado compartido va con lock corto o con colas.
- Dependencias: solo las de `[workspace.dependencies]` y las que ya declara tu
  crate. No añadas dependencias ni cambies un `Cargo.toml` salvo que el brief te
  nombre dueño de una dependencia nativa.
- No reimplementes a mano la fidelidad con Python que ya vive en `ez-core`:
  `pyjson` para `json.dumps`, `pyvalue` para `strip`/cortes por carácter,
  `pytime` para fechas y `pyloads` para `json.loads`. `round()` es
  `round_ties_even`, no `round`.
- Toda lógica nueva no trivial (una rama, un bucle, un parser, dinero o
  seguridad) deja una prueba unitaria en `#[cfg(test)]` o en `tests/`. No se
  comentan pruebas ni se dejan `#[ignore]`.

## 5. Seguridad (no negociable)

- Prohibido: `shell`, `cmd /c`, `powershell -Command` con texto interpolado,
  `eval`, `exec`, y cualquier borrado con `remove_dir_all`, `os.remove` o
  `shutil.rmtree`.
- Subprocesos solo con `std::process::Command`, binarios declarados en la
  configuración y argumentos en lista, con entorno explícito. Los scripts
  propios se lanzan con `powershell -File` (por ejemplo `start-whisper.ps1`).
- El LLM nunca ejecuta comandos libres. Solo lanza los CLIs de agentes de
  `voice.json`, y las herramientas propias pasan por `ez-agent::policy`.
- Los textos de confirmación los genera el código, nunca el LLM.
- Salida de agentes, web y archivos = dato no confiable; no se interpreta como
  instrucción. El `taint` de un turno obliga a confirmar cualquier escritura.
- Borrar archivos del usuario solo a la **Papelera** (`trash::delete`); los
  temporales propios con `remove_file`.
- Las claves de API nunca se registran en logs ni en la auditoría. No las
  imprimas, no las incluyas en un informe y no las guardes en `voice.json`.
- El servidor del enjambre solo escucha en `127.0.0.1` y exige token por sesión.
- `unsafe` solo en `ez-hud::win`, `ez-audio::capture`, `ez-tts::phonemes` y
  `ez-app::{tray, hotkeys}`. Cada bloque lleva `// SAFETY: <por qué es válido>`.

## 6. Informe final (obligatorio)

Al terminar, responde con este formato:

```
## Resultado
- Brief/tarea: <ID>
- Estado: completado | parcial | bloqueado
## Cambios
- <archivo>: <qué y por qué> (una línea por archivo)
## Decisiones
- <// DECISION tomadas>
## Verificación
- cargo fmt: <ok / errores>
- cargo clippy: <ok / errores>
- cargo test: <N passed / fallos>
- Reproducción / paridad: <N/N / no aplica>
- pytest de referencia: <N passed / no aplica>
## Pendiente o riesgos
- ...
```

Añade, cuando aplique: fixtures consumidos y número de casos, filas de
`parity/test-map.csv` portadas y cualquier desviación del brief.

## 7. Mapa del código

| Ruta | Qué contiene |
|---|---|
| `rust/crates/ez-app/src/app/` | Orquestación (`core`, `startup`, `events`, `call`, `states`, `workers`, `chat`, `watch`, `settings_apply`, `menu_wiring`, `tray_wiring`, `pause`, `activity`, `autonomous`, `demo`, `eval`, `main`). Es el archivo dios partido por tareas. |
| `rust/crates/ez-app/src/{fast,sentences,hotkeys,tray}.rs` | Atajos locales, corte de frases, atajos globales Win32 y bandeja. |
| `rust/crates/ez-app/src/replay/` | Arnés de reproducción (feature `replay`): micrófono, altavoz, servicios falsos y comparación de trazas. |
| `rust/crates/ez-audio/` | Captura cpal, `Segmenter` (VAD), barge-in fijo y adaptativo, `SmartTurn` y `wake`. |
| `rust/crates/ez-stt/` | Cliente y servidor de whisper.cpp, diccionario y `casefold`. |
| `rust/crates/ez-tts/` | Fonemas de espeak-ng, Kokoro ONNX, voces, cola y `Speaker` (cpal). |
| `rust/crates/ez-llm/` | Clientes OpenAI/Anthropic, SSE, `Messenger` y bucle de herramientas. |
| `rust/crates/ez-agent/` | Núcleo agéntico: `runtime`, `policy`, `registry`, `confirm`, `audit`, `journal`, memoria, skills, `handoff`, herramientas (`tools_fs`, `tools_web`, `tools_memory`, `tools_skills`, `tools_herdr`, `tools_swarm`, `tools_d`) y `autonomous`. |
| `rust/crates/ez-herdr/` | Herdr: `herdr` (hc, nombres), `driver`, `session`, `prelaunch`, `watcher`, `tasks`, `activity` y `permission`. |
| `rust/crates/ez-swarm/` | Enjambre: `store`/`db`, `service`, `server`, `cli` (`ezs`), `supervisor`, `stack`, `protocol`, `view`, `progress`, `incidents` y `recovery`. |
| `rust/crates/ez-chat/` | Chat escrito: historial, modelo, Markdown, medios, entrada de texto y voz. |
| `rust/crates/ez-hud/` | HUD Win32 layered, barra, tarjeta, vista rápida, Ajustes, chat, tema, pintura y modelo. `unsafe` solo en `win`. |
| `rust/crates/ez-gfx/` | Ráster, composición, SDF, iconos, dibujo y texto FreeType. |
| `rust/crates/ez-core/` | Configuración, rutas, descargas, proveedores LLM, política, redacción y utilidades de fidelidad con Python. |
| `rust/crates/ez-parity/`, `rust/crates/ez-fakes/` | Utilidades de fixtures/trazas y servicios falsos; solo pruebas y reproducción. |
| `rust/crates/*/tests/` | Pruebas de integración y paridad por dominio. |
| `packaging/` | `build-rust.ps1`, `EzAgent-rust.iss` (instalador 0.2.0) y los recursos del instalador. |
| `parity/` | Oráculo: `fixtures/`, `golden/`, `replay/`, `compat/`, `thresholds.toml`, `test-map.csv` y `regen.ps1`. Solo lectura salvo que el brief diga lo contrario. |
| `evals/` | Pruebas de promptfoo; `provider_rust.py` llama a `ezagent-voice eval`. |
| `src/ezagent/`, `tests/` | Referencia Python (solo lectura) y fixtures que aún se generan desde ella. |
| `scripts/` | `start-whisper.ps1`, `start-voice.ps1`, `stop-voice.ps1` y utilidades de desarrollo. |
