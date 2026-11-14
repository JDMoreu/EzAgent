# Registro de ejecución — migración de EzAgent a Rust

- Fecha: 2026-11-13 / 2026-11-14
- Rama: `rust/main` (nace del tag `py-baseline`)
- Orquestador: OpenCode + DeepSeek V4.1 Flash (yo), que además hizo de revisor
  en cada entrega. No se usó Codex en esta corrida: el bucle de validación
  (§6) quedó montado y probado, pero la revisión la hice yo con la puerta
  automática y lectura del diff contra el Python.
- Trabajadores: subagentes en paralelo, cada uno con un encargo acotado a sus
  archivos.

## 1. Punto de partida congelado

| Qué | Resultado |
|---|---|
| Commit del trabajo pendiente en `feat/swarm-board` | `75f7468`, 101 entradas (incluida `src/ezagent/voice/hud/` completa) |
| Tag del oráculo | `py-baseline` (anotado) |
| Pruebas Python | 1881 passed, 1 skipped (66 s) |
| ruff | `check` y `format --check` limpios |
| Medido | 45 056 líneas Python en 116 archivos; 1882 pruebas recolectadas; 252 turnos reales en `latency.jsonl` |

## 2. Fases ejecutadas

### F0 — Preparación: completo

| Tarea | Qué quedó | Evidencia |
|---|---|---|
| F0-01 | Tag `py-baseline` y árbol limpio | `git tag -l py-baseline` |
| F0-02 | `rust-migration/`: `AGENTS.rust.md`, `config.toml`, `tasks.yaml` (100 tareas), `briefs/_TEMPLATE.md`, `python-quirks.md`, `tools/` | flags verificados con `--help`; modelo `opencode-go/deepseek-v4.1-flash` (variantes low/high/max) |
| F0-03 | Workspace con 30+ dependencias de versión exacta; `rusqlite` bundled con FTS5 | `cargo build --workspace` verde; 2 pruebas de SQLite |
| F0-04 | 15 crates y 225 archivos `.rs` con todos los módulos de §3 declarados | ningún trabajador necesita tocar `lib.rs` |
| F0-05 | `rust/gate.ps1` (alcance, fmt, clippy, tests, patrones prohibidos, tamaños, paridad Python) + `scan_diff.py --selftest` | `GATE OK (autoprueba)`: cada regla rechaza su caso de ejemplo |
| F0-06 | `baseline-metrics.json`: P50/P95 reales por etapa, 14 micrófonos, arranque 1865 ms, tamaño y pruebas | lo que exige sesión gráfica (CPU/RAM en reposo, `frame_stats`) queda marcado con su motivo |

### F1 — Oráculo de paridad: marco + 26 dominios

| Tarea | Estado | Detalle |
|---|---|---|
| F1-01 | completo | `parity/gen/_common.py` (reloj congelado, semilla 20251113, perfil temporal, JSONL, `ensure_ascii`), `regen.ps1` (`-Check`), `ez-parity` (fixtures, PNG por canal + SSIM + umbrales `thresholds.toml`, normalizador de trazas) con 10 pruebas |
| F1-02 | completo | 1 932 casos de texto (`speech_text` 452, `redact` 53, `stt_dictionary` 102, `tool_json` 169, `tool_markup` 111, `condenser` 41, `chat_markdown` 185, `chat_text_input` 299, `chat_trace` 54, `chat_speech` 45, `handoff` 123, `handoff_packet` 129, `app_text` 169) |
| F1-03 | completo | 561 casos (`config` 63, `llm_providers` 228, `agent_paths` 44, `policy` 144, `tools_d` 82) |
| F1-04 | completo | 243 casos (`system_prompt` 31, `registry` 68, `confirm` 37, `audit` 20, `orchestration` 46, `skills` 41) |
| F1-06 | completo | 644 casos en `parity/gen/swarm*.py`: `protocol`, `ezs`, tablero y volcado SQLite, salud, avisos, incidentes, recuperación y `models`/`store`/`mail` |
| F1-10a | completo | 298 PNG dorados (barra 146, botones 50, órbita 50, Marea 40, texto 12) + 361 casos con las curvas de animación |
| F1-13 | completo | `parity/test-map.csv` con las 1 882 pruebas de pytest en estado `pendiente` |
| F1-05, F1-07, F1-08, F1-09, F1-10b, F1-10c, F1-11, F1-12 | pendientes | ver §5 |

**Determinismo verificado**: los 27 dominios (3 780 casos) se regeneraron y
compararon por sha256, y `run_all.py --check` queda estable. Los 298 PNG
también son estables entre corridas.

### F2 — Base compartida

| Tarea | Estado | Detalle |
|---|---|---|
| F2-01 | completo | `pyjson` (36 casos `dumps` + 3 round-trip byte a byte), `pytime` (isoformat UTC/local), `dirs`, `logging`; 11 pruebas |
| F2-06 | completo | `speech_text` (452), `redact` (53), `stt_dictionary` (102): 607 casos en verde, 18 pruebas |
| F2-02…F2-05, F2-07…F2-12 | pendientes | hay fixtures listos para F2-02/03/04; F2-07+ esperan `ez-gfx` |

### F3–F9

Sin empezar. `tasks.yaml` tiene las 100 tareas con dependencias, archivos y
fixtures asignados, listas para que el bucle las tome.

## 3. Verificación final de esta corrida

```
uv run pytest -q                    1881 passed, 1 skipped
cargo test --workspace              27 pruebas, 0 fallos
cargo clippy --workspace --all-targets -- -D warnings     limpio
cargo fmt --all --check              limpio
powershell -File rust/gate.ps1 -SelfTest                  GATE OK
powershell -File rust/gate.ps1 -Task F2-06                GATE OK
powershell -File rust/gate.ps1 -Task F2-01                GATE OK
uv run python parity/gen/run_all.py --check               estable
```

## 4. Decisiones y desviaciones del plan

1. **Codex no se usó como orquestador.** Hice el papel con la puerta automática
   y revisión propia. `config.toml` deja los comandos verificados
   (`opencode run --model … --auto`, `codex exec
   --dangerously-bypass-approvals-and-sandbox`) para retomarlo.
2. **`freetype-rs` no está cableado en `ez-gfx` todavía.** La compilación
   `bundled` de `freetype-sys` 0.23 falla en Windows (incluye `libpng` sin la
   ruta de `zlib.h`). F2-10 es la dueña de compilar FreeType 2.14.3 desde
   fuentes sin libpng; dejarlo declarado en `[workspace.dependencies]` evita
   bloquear todo el workspace.
3. **`ort` queda declarado con `load-dynamic`** y sin cablear hasta F4-04: así
   el workspace compila sin la DLL de ONNX Runtime y la versión se fija en la
   tarea que la usa (Python tiene 1.31.1).
4. **`parity/.gitattributes` con `* -text`**: el oráculo es byte a byte; una
   conversión CRLF en el checkout rompería el sha256.
5. **`chat_trace` depende de la zona horaria** de la máquina que genera los
   fixtures (documentado en su generador). Regenerar en otra zona cambia `t`.
6. **`system_prompt` fija `Windows 11`** (`platform.release()`): el oráculo se
   corre en esta máquina.
7. **`config` y `tools_d` registran detalles de Windows** (saltos CRLF y claves
   del entorno limpio); si el port corre en Linux hay que normalizar.
8. **`llm_providers.py` (generador) supera 600 líneas** (662): es una tabla de
   datos de fixtures, no lógica.

## 5. Pendiente, en orden de dependencias

1. **Cerrar F1**: F1-06 (enjambre), F1-07 (memoria/chat/diario), F1-08 (audio),
   F1-09 (fonemas y PCM de Kokoro), F1-10b/c (menú, ajustes, chat, vista
   rápida), F1-11 (`ez-fakes`), F1-12 (arnés de reproducción y 25 escenarios).
   Sin F1-11/F1-12 no hay F7.
2. **F2-02/03/04** ya tienen fixtures: son los tres siguientes ports naturales.
3. **F2-07…F2-11 (`ez-gfx`)** es el camino crítico del HUD: empieza por el
   raster y la composición, sigue con porta de `Resample.c`/`BoxBlur.c` y
   termina en FreeType.
4. **F3, F4, F5, F6, F7, F8** según `tasks.yaml`.
5. **F7-06** necesita al usuario: micrófono, LLM y Herdr reales.
6. **Métricas que faltan en `baseline-metrics.json`**: CPU y RAM en reposo,
   `frame_stats` del HUD y arranque completo. Se miden con
   `uv run ezagent-voice --demo-ui` en una sesión gráfica.

## 6. Cómo continuar

```powershell
# estado de una tarea
uv run python rust-migration/tools/task_state.py F2-02 in_progress

# puerta de la tarea (alcance, fmt, clippy, tests, patrones, tamaños, pytest)
powershell -NoProfile -ExecutionPolicy Bypass -File rust/gate.ps1 -Task F2-02

# regenerar o verificar el oráculo
powershell -File parity/regen.ps1
powershell -File parity/regen.ps1 -Check

# mapa de pruebas
uv run python rust-migration/tools/make_test_map.py
```

El encargo de un trabajador es el de `rust-migration/AGENTS.rust.md` §10 con el
brief de `rust-migration/briefs/<ID>.md` y el informe en
`rust-migration/reports/<ID>-r<ronda>.md`.
