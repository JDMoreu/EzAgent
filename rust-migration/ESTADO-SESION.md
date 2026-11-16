# Estado de la sesión (pausada el 2026-11-16, ~02:10)

Rama de trabajo: `rust/main`, último commit `de8d108`. No hay workers en marcha ni
árboles de trabajo en `EzAgent-wt` (todo integrado o guardado en su rama).
`rust/main` **no se ha subido** a GitHub desde hace tiempo: todo está solo en local.

## Hecho (integrado en `rust/main`)
- **Migración a Rust:**
  - paridad por dominio completa;
  - reproducción de punta a punta, 25 escenarios / 31 YAML, en verde contra las trazas de Python;
  - última corrida completa verde después de integrar la mejora de nombres.
- **Arreglos de las pruebas del usuario:**
  - menú de Ajustes;
  - animación del chat al abrir y cerrar;
  - candado del micrófono;
  - micrófono correcto por índice de PortAudio;
  - secciones de Ajustes;
  - cuatro interbloqueos (vista rápida, barra, tarjeta y chat).
- **Mejoras pedidas:**
  - **Nombres dictados** (P-IMP-NAMES): «¿Te refieres a «TEST»?».
  - **Interrupción por voz con referencia de eco** (P-IMP-BARGEIN), con `barge_in_mode = "adaptive"` por omisión.
- **Modo autónomo (elefante y pez dorado):**
  - Diseño aprobado, v2 tras dos revisiones con peces dorados: `docs/designs/modo-autonomo-elefante-pez-dorado.md` (secciones 9 y 10; manda la 10).
  - Revisiones: `rust-migration/reports/EGM-REV-{A,B}-r1.md`.
  - **A1/A1b** (`ez-agent::autonomous`: estado, artefactos, veredicto, textos, herramientas con guardas): integrado.
  - **A2/A2b** (`ez-herdr::permission`: clasificador, lista de denegación, transacción y auditoría): integrado.

## Pendiente, en este orden
1. **Verificar la última fusión (P-IMP-BARGEIN):** la verificación se cortó al pausar. Hay que correr:
   - `cd rust && cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`
   - `cargo build -p ez-fakes --bins && cargo build -p ez-swarm --bin ezs && cargo test -p ez-app --features replay --test replay_parity` (≈7 min)
2. **AUTO-A3:** el bucle del modo autónomo en la app.
   - Brief: `rust-migration/briefs/AUTO-A3.md`; *prompt*: `rust-migration/tools/orquestacion/prompt-AUTO-A3.txt`.
   - Su rama `work/AUTO-A3` no tiene avance (se detuvo nada más empezar).
3. **Modo B** (multiagente sobre el enjambre, sección 10.18).
4. **UI** (sección 10.19): fila y marca en la barra y la vista rápida.
5. **Recompilar `release`** (`cargo build --release -p ez-app --bin ezagent-voice`) para que el usuario pruebe con `rust\run-dev.cmd`:
   - la interrupción por voz;
   - los nombres dictados;
   - el modo autónomo cuando esté A3.
6. **Pendientes menores de los informes:**
   - `voice_control close_and_watch` no resuelve nombres;
   - selector de micrófono: `--list-mics` solo lista MME;
   - tarea de rendimiento de `HudConfig::save` en el hilo de pintura.
7. **Después:** empaquetado del ejecutable Rust con sus DLL (F8) y auditoría final (F9).

## Cómo retomar con workers (OpenCode, `opencode-go/deepseek-v4.1-flash#max`)
Scripts en `rust-migration/tools/orquestacion/`:
- **`supervise.sh <ID> <worktree> <prompt>`:** relanza el worker si se corta la red o se cuelga al arrancar. Cambia `LOG_DIR` a una carpeta existente.
- **`make_briefs.py <wave.json>`:** genera los briefs.
- **`cleanup_worktrees.sh [--apply]`:** borra los árboles ya integrados.
- **`stackdump.py <pid>` y `symbolize.py`:** diagnostican un cuelgue leyendo las pilas del proceso colgado, sin depurador.

**Regla de disco:** un árbol por worker en `../EzAgent-wt/<ID>`, que se borra **en cuanto se integra** su rama. Cada `rust/target` pesa varios GB; llegaron a sumar 320 GB.
