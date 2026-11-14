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
| F3-06 skills | código | falta `tools_skills` (depende de `tools_d`) |

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

Orden de dependencias pendiente: `tools_d` → `tools_skills` → `tools_core`/`tools_fs`
→ `memory` + `tools_memory` → `search`/`tools_web` → `ez-swarm` → `ez-chat` →
`ez-herdr` → `ez-audio`/`ez-stt`/`ez-tts` → `ez-llm` messenger → `runtime` →
HUD (`ez-gfx`, `ez-hud`) → `ez-app`. Después: tests de paridad por dominio y
`gate.ps1 -Task <ID>`.
