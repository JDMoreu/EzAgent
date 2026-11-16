# P-FIX-LOCKAUDIT — Auditoría de interbloqueos en el HUD y la app: ningún lock retenido durante operaciones de ventana entre hilos ni llamadas que vuelvan a pedirlo
Excepción a AGENTS.md: rama de migración a Rust; aplica `rust-migration/AGENTS.rust.md`.
**Antes de nada lee `rust-migration/briefs/_COMMON.md` y `rust-migration/AGENTS.rust.md`.**

## Fuente Python (la especificación, léela completa)
- La app Python corre la interfaz en un solo hilo (Tk) y no tiene estos interbloqueos; el comentario de `apply` en `rust/crates/ez-hud/src/win/peek/controller.rs` resume la regla: «nunca con el lock tomado. ShowWindow/SetWindowPos sobre una ventana de otro hilo espera a que ese hilo atienda el mensaje, y ese hilo puede estar en `render` esperando el lock»
- Rust: `rust/crates/ez-hud/src/win/**` (barra `drop`, `chat`, `peek`, `settings_window`, `layered*`, `ui_loop.rs`, `wndproc.rs`), `rust/crates/ez-hud/src/win/drop/ui/**` y `rust/crates/ez-app/src/app/**` (todo lo que llama a la barra, al chat o a la vista rápida desde el hilo de la interfaz o desde workers)

## Archivos que se tocan (solo estos)
- rust/crates/ez-hud/src/win/** y rust/crates/ez-app/src/app/** (solo para corregir interbloqueos, con cambios mínimos)
- pruebas nuevas en esos crates
- rust-migration/reports/P-FIX-LOCKAUDIT-r1.md

Los módulos de tu crate ya existen como esqueleto vacío y están declarados en
su `lib.rs`; reemplaza el contenido. Si un módulo se pasa de 600 líneas,
divídelo en una carpeta con submódulos (`foo.rs` + `foo/bar.rs`).

## Fixtures de referencia (solo para entender formatos; no hay pruebas de paridad en esta fase)
- (ninguno)

## Notas específicas
- **Contexto (tercer interbloqueo real en dos días):** (1) P-FIX-MICLOCK: el hilo de la llamada retenía el `Mutex` del micrófono durante toda la llamada y el menú se quedaba esperando. (2) P-FIX-MENU: `hide_from_menu` volvía a pedir un lock que el mismo hilo ya tenía. (3) Hoy, con la app real del usuario: la barra se congeló (fondo negro, «No responde») justo cuando apareció la actividad de una cadena de agentes. Causa (ya corregida por el orquestador en el commit `100d5da`): `PeekController::place` retenía `self.win.lock()` mientras llamaba a `resize`/`set_pos`/`show` de la ventana de la vista rápida (otro hilo, mensajes síncronos de Win32), y ese hilo estaba en `render` pidiendo `self.win.lock()`. Evidencia: los resúmenes `frame_stats` de la barra se cortaron justo en ese minuto.
- **Qué hacer:** recorre **todo** `ez-hud/src/win/**` y `ez-app/src/app/**` buscando: (a) cualquier lock (`Mutex`, `RwLock`, `parking_lot`) retenido mientras se llama a una operación de ventana que pueda ser síncrona entre hilos (`SetWindowPos`, `ShowWindow`, `SendMessage`, `DestroyWindow`, `UpdateLayeredWindow` sobre ventana ajena, `start()`/`stop()`/`join` de hilos de ventana, `resize`, `set_pos`, `show`, `hide`, `wake`…); (b) cualquier lock retenido mientras se llama a un callback o a otro componente que pueda pedir ese mismo lock u otro en orden inverso (ABBA) — sobre todo los callbacks de `DropHost`, `ChatUi`, `ChatEvents`, `PeekUi`, `MenuCallbacks`; (c) `render` de una ventana que pida un lock que el hilo de otra ventana retiene mientras le manda mensajes. Para cada hallazgo: archivo, función, los dos hilos y el orden de locks, y la corrección (clonar el `Arc` y soltar el lock antes de la llamada, o reordenar). Corrige todos los que puedan colgar la interfaz.
- **Pruebas de regresión:** una prueba de estrés de la vista rápida que reproduzca la carrera del incidente (hilo A: `set_activity`/`tick` del peek a ritmo alto con filas que cambian de tamaño —para forzar `place`—; hilo B: `render` del peek en bucle) y que falle con el código anterior al commit `100d5da` (compruébalo con `git stash`/checkout del archivo viejo) y pase con el actual, con un plazo (si a los N segundos no terminó, falla en vez de colgar la suite). Añade pruebas similares para cualquier otro interbloqueo que corrijas.
- `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test -p ez-hud -p ez-app` en verde; `cargo test -p ez-app --features replay --test replay_parity` en verde (≈7 min). Informe: tabla de hallazgos (corregido / sin riesgo y por qué).

## Criterios de aceptación
- Todo lo público del Python está portado, con los mismos textos y valores por defecto.
- `cargo fmt --all`, `cargo clippy -p <crate> --all-targets -- -D warnings` y
  `cargo test -p <crate>` limpios, ejecutados desde `rust/`.
- Informe en `rust-migration/reports/P-FIX-LOCKAUDIT-r1.md` y un commit en tu rama.
