# Migración a Rust: estrategia «primero el código, luego las pruebas»

Decisión del usuario (2026-11-14): portar todo el código y dejar las pruebas de
paridad (y la puerta `gate.ps1`) para una segunda etapa. Mientras tanto cada
módulo se verifica solo con `cargo test -p <crate>` (pruebas unitarias propias),
`cargo fmt` y `cargo check`. **Las tareas de este registro NO están marcadas
`done` en `tasks.yaml`** salvo F2-03, la única con puerta verde.

## Estado por tarea (código escrito, sin paridad ni puerta)

| Tarea | Estado | Notas |
|---|---|---|
| F2-03 llm_providers | hecho con puerta | 228 casos de paridad |
| F2-05 downloads, models | código | manifiesto incrustado desde el Python |
| F3-01 tool_json, tool_markup, context, condenser | código | `Condenser` usa el trait `TextCompleter` (lo implementan F4-06) |
| F3-02 handoff, handoff_packet, orchestration | código | texto del hint en `ez-agent/src/text/` |
| F3-03 registry | código | `registry/schema.rs` y `registry/tests.rs` |
| F3-04 confirm, audit, journal | código | |
| F3-05 system_prompt (+ `AgentConfig` en ez-core) | código | `agent_config.rs` es nuevo |
| F3-06 skills, tools_skills | código | `python_interpreter()` busca Python en el PATH |
| F3-07 tools_core, tools_fs | código | `tools_fs/{util,handlers,describe}.rs`; `ToolInfo.bulk_forced` reemplaza a `_FolderBulk` |
| F3-08 tools_d | código | incluye `run_captured` (subproceso con timeout, sin shell) |

## Desviaciones del contrato (a revisar en la auditoría)

- **`Cargo.toml` de crates**: se añadieron dependencias `path` entre crates
  (`ez-llm`, `ez-agent` → `ez-core`; `ez-agent` → `ez-llm`) y `uuid` y
  `serde_yaml` a `ez-agent`. Sin eso los crates no podían compartir el núcleo.
- **`ez-core::casefold`** compila el archivo de `ez-stt` con `#[path]`.
- **Textos largos** (prompts, guía de orquestación) se extraen con un script
  desde el Python a `ez-agent/src/text/*.txt` y se incrustan con `include_str!`;
  `.gitattributes` con `-text` evita que el checkout cambie los finales de línea.
- Módulos nuevos de `ez-core`: `pyloads` (json.loads con los errores de CPython),
  `pyvalue` (helpers de Python: strip, split, repr, `%g`, casefold…), `agent_config`.

## Cómo seguir

Orden de dependencias pendiente: `memory` + `tools_memory` (F3-09a/b) → `search`/`tools_web` → `ez-swarm` → `ez-chat` →
`ez-herdr` → `ez-audio`/`ez-stt`/`ez-tts` → `ez-llm` messenger → `runtime` →
HUD (`ez-gfx`, `ez-hud`) → `ez-app`. Después: tests de paridad por dominio y
`gate.ps1 -Task <ID>`.

## Verificación de este tramo

- `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`
  y `cargo test --workspace`: limpios y en verde (incluida la paridad de F2-03).
- Pruebas unitarias propias por módulo; **sin** pruebas de paridad contra los
  fixtures de F3-01…F3-08 (se escriben en la etapa de pruebas).

## Notas para la etapa de pruebas

- `ToolResult::err` (outcome `completed`) y `ToolResult::failed` (outcome `failed`)
  distinguen `ToolResult(ok=False, ...)` del Python de los fallos del registro.
- `SkillLibrary` lee YAML con `serde_yaml` (YAML 1.2): puede discrepar de PyYAML en
  escalares ambiguos (`yes`, `no`, fechas) dentro del frontmatter.
- `get_datetime` sin zona usa `UTC±hh:mm` como etiqueta (Python da el nombre
  localizado de Windows).
- `open_path` abre con `explorer.exe` (Python usa `os.startfile`).
- F5-10b: `Menu::draw` debe usar `draw_with` (las filas avanzadas quedan en blanco si no); tenerlo en cuenta en F6-06b (menu_wiring).
