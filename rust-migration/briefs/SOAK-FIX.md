# SOAK-FIX — Prueba de fondo lista para grabar: corregir el cuelgue de r04 y la subida de memoria, y salida en vivo por minuto
Excepción a AGENTS.md: rama de migración a Rust; aplica `rust-migration/AGENTS.rust.md`.
**Antes de nada lee `rust-migration/briefs/_COMMON.md` y `rust-migration/AGENTS.rust.md`.**

## Fuente Python (la especificación, léela completa)
- Arnés de robustez ya integrado: `parity/robustness/run_robustness.py` (`compare` y `soak`), `parity/robustness/README.md`, `parity/robustness/scenarios/r01..r08`, `rust/crates/ez-app/src/replay/soak.rs`, `rust/crates/ez-app/src/bin/ezagent_replay.rs`, `rust/crates/ez-fakes/src/fake_whisper.rs`, `fake_llm.rs`; informe `rust-migration/reports/F7-05-r1.md`.
- Lo observado en la primera corrida (1 h, 197 vueltas, con el equipo muy cargado): (a) **dos vueltas de `r04-llm-cut-stream` no terminaron en 180 s** (vueltas 36 y 52; las demás de r04 pasaron en ~10 s); (b) **la RAM del proceso `ezagent-replay --soak` subió**: 37 MB al arrancar, ~62 MB a los 50 min, 104 MB a los 64 min (hilos 24 y handles 717, aparentemente estables). Registro: `C:/Users/JDMoreu/AppData/Local/Temp/claude/40010d84cecbf54d/ffb0b8cb-44a7-4d4d-8f38-01a6793a649e/scratchpad/soak-parcial-1.log`. El muestreador solo escribía al final, así que no hay serie temporal.

## Archivos que se tocan (solo estos)
- rust/crates/ez-app/src/replay/** y rust/crates/ez-app/src/bin/ezagent_replay.rs (arnés)
- rust/crates/**/src/** solo para corregir un defecto real de la app que encuentres (cambio mínimo con su prueba)
- rust/crates/ez-fakes/src/** si el falso es el culpable
- parity/robustness/** (muestreador, salida en vivo, README)
- rust-migration/reports/SOAK-FIX-r1.md

Los módulos de tu crate ya existen como esqueleto vacío y están declarados en
su `lib.rs`; reemplaza el contenido. Si un módulo se pasa de 600 líneas,
divídelo en una carpeta con submódulos (`foo.rs` + `foo/bar.rs`).

## Fixtures de referencia (solo para entender formatos; no hay pruebas de paridad en esta fase)
- (ninguno)

## Notas específicas
- (1) **Cuelgue de r04.** Reprodúcelo: corre `r04-llm-cut-stream` muchas veces seguidas (al menos 100, en paralelo con carga de CPU si hace falta, por ejemplo una compilación) y captura el estado cuando se cuelga (hilos y en qué espera cada uno: añade registros de depuración o usa el volcado de pilas si lo necesitas). Distingue si es la app (un turno que nunca termina cuando el SSE se corta en cierto punto, un lock, un canal que nadie cierra), el falso (`fake-llm` que no cierra la conexión) o el arnés. Corrige la causa con una prueba que fallaba antes. Criterio: 200 corridas seguidas de r04 sin cuelgue. (2) **Memoria.** Corre el soak 20-30 min muestreando RSS cada minuto **y escribiendo cada muestra al momento**; mira la tendencia tras el calentamiento. Si sube, encuentra qué crece (la traza del arnés acumulada en memoria, cachés de la app, el pintado del HUD fuera de pantalla, hilos de turnos que no se liberan…) con mediciones, no suposiciones. Si es el arnés, que no acumule (escribir a disco o vaciar por vuelta); si es la app, corrígelo con su prueba. Criterio: tras 10 min de calentamiento, RSS estable ±5 % durante al menos 20 min.
- (3) **Lista para grabar un video.** `python parity/robustness/run_robustness.py soak --minutes 120` (y `--detach` para que sobreviva a un cierre) debe: compilar o comprobar los binarios, mostrar en su consola **una línea por minuto** con hora, minuto transcurrido/total, vuelta, escenarios ok/error, RAM (MB), hilos y handles, y al final un veredicto claro en español («APTO: memoria estable ±X %, hilos y handles estables, N vueltas, 0 errores» o «NO APTO: …»); guardar la serie en `soak.csv` y el veredicto en `soak.json` a medida que avanza. Con `--detach`, que la consola visible muestre esa misma salida (no una ventana vacía). Documenta en `parity/robustness/README.md` cómo lanzarla y qué significa cada columna. Haz una corrida de prueba de 5 minutos con salida real y pégala en el informe. **No** dejes ninguna prueba larga corriendo al terminar y cierra los procesos que hayas lanzado. `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings` y `cargo test -p ez-app -p ez-fakes` en verde.

## Criterios de aceptación
- Todo lo público del Python está portado, con los mismos textos y valores por defecto.
- `cargo fmt --all`, `cargo clippy -p <crate> --all-targets -- -D warnings` y
  `cargo test -p <crate>` limpios, ejecutados desde `rust/`.
- Informe en `rust-migration/reports/SOAK-FIX-r1.md` y un commit en tu rama.
