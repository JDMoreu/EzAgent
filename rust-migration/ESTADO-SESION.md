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
1. ~~Verificar la última fusión (P-IMP-BARGEIN)~~ **Hecho al retomar:** fmt, clippy, 101 binarios y reproducción 3/3 en verde; `release` recompilada. Órdenes por si hay que repetir:
   - `cd rust && cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`
   - `cargo build -p ez-fakes --bins && cargo build -p ez-swarm --bin ezs && cargo test -p ez-app --features replay --test replay_parity` (≈7 min)
2. ~~AUTO-A3~~ **Hecho e integrado** (bucle del modo autónomo, modo A completo); verificado: 101 binarios + reproducción 3/3; `release` recompilada.
   - Brief: `rust-migration/briefs/AUTO-A3.md`; *prompt*: `rust-migration/tools/orquestacion/prompt-AUTO-A3.txt`.
   - Su rama `work/AUTO-A3` no tiene avance (se detuvo nada más empezar).
3. ~~Modo B~~ **Hecho e integrado** (equipo sobre el enjambre, sin `--yolo`, verificador aparte, tareas `fix`). Limitación: el diálogo de confianza al abrir un miembro en carpeta nueva no se aprueba solo.
4. ~~UI~~ **Hecho e integrado** (fila «Autónomo · ronda N» en la vista rápida, marca en la órbita, parar desde la fila). Verificado todo: 101 binarios + reproducción 3/3; `release` recompilada.
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

## Autorizaciones del usuario
- 2026-11-16: «sí, súbelo a GitHub al terminar» → al cerrar la auditoría F9, `git push -u origin rust/main` (solo la rama; no fusionar a `main` ni publicar release sin preguntar).

## Tanda final «Termina todos los pendientes» (2026-11-16)
Integrado en `rust/main`: PEND-IMG, PEND-MICS, F8-01 (instalador 0.2.0), F7-03 (compatibilidad), PEND-MENORES (+ cerrojo de `voice.json` y vaciado de la cola al cerrar), F8-02 (eval; limpieza sin seguir enlaces), F8-01b (recursos junto al exe, `EzAgent.bat` con Rust). Arreglos propios: Herdr 20 s en `list_agents`, fake-hc tolerante a carga, animaciones del chat, carril `swarm.*` en la reproducción.

En curso (si se cierra Claude Code, comprobar con `git worktree list`, `opencode session list` y procesos `opencode`/`ezagent-replay`; retomar con `supervise.sh <ID> <wt> <prompt> resume`):
- F7-05: código listo en `work/F7-05` (8/8 casos iguales a Python), **sin fusionar**; el soak de 2 h corre desacoplado desde `../EzAgent-wt/F7-05` y deja el veredicto en `../EzAgent-wt/soak-F7-05/soak.json`. Fusionar y borrar el worktree cuando termine.
- F8-03: documentación en `rust-migration/docs-staging/` y lista `rust-migration/F7-06-prueba-en-vivo.md` (se activan tras F9).
- TMAP-1..6: mapa de pruebas Python → Rust en `parity/test-map/TMAP-N.csv`; luego fusionar en `parity/test-map.csv` (columna `nota`) y revisar hallazgos (`#[ignore = "TMAP: ..."]`).
- **Decisión del usuario (16-nov): la versión Python deja de existir; solo Rust.** PYOUT-1 (en curso): Rust sin `.venv` (DLL de onnxruntime 1.31.1 y espeak-ng por `ezagent-voice downloads`, pruebas contra salidas congeladas en `parity/frozen/`). PYOUT-2 (después de F7-04): borrar `src/ezagent`, `tests/`, empaquetado Python y la opción `py` (queda la etiqueta `py-baseline`); luego F9 sobre el árbol solo Rust.
- Hechos tras el corte: F8-03 integrado; app instalada sin consola, `ezagent-voice downloads`, `barge_in_mode` conservado (a95f3ff). Workers con prioridad baja y `jobs = 2` en `../EzAgent-wt/.cargo/config.toml`; `cleanup_worker.ps1` cierra lo que deja cada worker (protege el soak).
- **Prueba de 2 h: en espera por pedido del usuario (16-nov, 16:49).** El usuario la correrá y grabará en video cuando avise; **no lanzarla por cuenta propia**. Primera corrida detenida tras 1 h/197 vueltas: r04 se colgó 2 veces y la RAM subió 37 → 104 MB. SOAK-FIX (en curso) corrige ambas cosas y deja `run_robustness.py soak --minutes 120` con una línea por minuto y veredicto. F7-05 ya está fusionado.
- `rust/main` subido a GitHub (fe2f4e2, 16-nov 16:5x); volver a subir al terminar cada bloque.
- **Hecho (16-nov, tarde):** mapa de pruebas completo (`parity/test-map.csv`: 1882 filas, 0 referencias rotas, 0 `#[ignore]`; 3 diferencias con Python corregidas); PYOUT-1 integrado (DLL por `downloads`, 104 binarios en verde sin `.venv`). Ningún `.rs` pasa de 600 líneas.
- **En curso:** SOAK-FIX (sesión propia; trabaja también en `../EzAgent-wt/F7-05-work`, rama `work/F7-05`: al terminar, fusionar **las dos** ramas y los cambios sin commit de esa carpeta); PYOUT-2a (recursos a `assets/`, empaquetado solo Rust, CI con cargo). Usar `supervise2.sh` (informe «completado», limpieza y prioridad baja).
- **Siguiente:** F7-04 (rendimiento contra Python, con el equipo tranquilo) → PYOUT-2b (borrar `src/ezagent`, `tests/`, opción `py`, arneses Python de la app; `pyproject` solo para herramientas) → F9 → subir.
- **PAUSA (16-nov, 18:45) por pedido del usuario: va a grabar un video y necesita la CPU.** Se detuvieron todas las pruebas, compilaciones y workers, y los `cua-driver` que abre OpenCode por cada worker (ventanas superpuestas en pantalla). **No lanzar nada hasta que el usuario lo diga.**
  - Soak de verificación de SOAK-FIX cortado en el minuto 24/30: RAM estable ~45 MB (antes subía de 37 a 104 MB), 0 errores en 70 vueltas (`scratchpad/soak30-verificacion.csv`).
  - Pendiente al retomar: verificar e integrar `work/SOAK-FIX` (ya une `work/F7-05`) y `work/PYOUT-2a` (sus informes dicen «completado» pero la verificación quedó «pendiente»: correr fmt, clippy, `cargo test --workspace` con `EZAGENT_HOME` vacío, paridad visual, `packaging/build.ps1 -SkipTests`); luego F7-04 → PYOUT-2b → F9 → subir.
  - El usuario correrá y grabará él mismo la prueba de 2 h (16-nov, 18:50): **no lanzarla**; solo darle la orden. Se cerró además el servicio de OpenCode (seguía ejecutando la sesión de PYOUT-2a por dentro aunque el cliente estaba cerrado).
  - 19:35: OpenCode 2.3 reanudaba solo las sesiones suspendidas (F7-05-s1, SOAK-FIX-s1, PYOUT-2a-s1) al abrirlo el usuario. Se exportaron a `scratchpad/opencode-<id>.json` y se borraron con `opencode session delete`; no queda ninguna sesión reanudable. La sesión F7-05 dejó un commit más en `work/F7-05` (`daf3e92`, el soak guarda cada muestra) que hay que unir a SOAK-FIX al retomar.
  - Al lanzar workers otra vez: `supervise2.sh` (rechaza informes con «pendiente») y cerrar sus `cua-driver` al terminar.
Después: F7-04 (rendimiento, con el equipo tranquilo: después del soak y de TMAP), F9 (auditoría con veredicto), activar docs, y `git push -u origin rust/main` (autorizado).

## Espacio de trabajo con elefante (desde 2026-11-18)
Nueva dirección aprobada por el usuario: EzAgent como espacio de trabajo (ventana Tauri con
espacios, terminal del elefante y subterminales de peces), elefante = el CLI que elige el
usuario, EzAgent = árbitro (`ez`). Diseño: `docs/designs/espacio-elefante.md`.
- Rama `eco/main` (sale de `rust/main` en fdd25fe; commit b6ae9f8 con diseño, contratos,
  esqueleto y briefs). Los pendientes de la migración (SOAK-FIX, PYOUT-2a, F7-04, F9)
  siguen en sus ramas, sin tocar.
- **Ola 1 en curso** (OpenCode `deepseek-v4.1-flash#max`, `supervise2.sh` con
  `EZ_LOG_DIR`/`EZ_MODEL`): ECO-1 `ez-term`, ECO-2 `ez-arbiter`, ECO-3 `ez-ctl`,
  ECO-4 `ez-team`, ECO-5 `ez-hub`; revisor del diseño ECO-REV0 con `kimi-k3.1-pro`.
  Worktrees `../EzAgent-wt/ECO-*`, ramas `work/ECO-*`, prompts
  `rust-migration/tools/orquestacion/prompt-ECO-*.txt`.
- Al terminar cada uno: puerta (fmt, clippy -p, test -p), pez verificador nuevo,
  fusionar en `eco/main`, borrar el worktree. Luego ola 2 (integración, nota de voz, E2E).
- El elefante (Claude) no escribe código de producción: solo diseño, contratos,
  plantillas, encargos, verificación e integración de ramas.
- **2026-11-18 20:22: cuota de 5 h de OpenCode Go agotada.** Revisión ECO-REV0 hecha
  (veredictos en el diseño §14). ECO-3 fusionado en `eco/main`. ECO-4 pasó la puerta
  (sin fusionar; su verificador ECO-4V no terminó y su falla inyectada ya se deshizo).
  ECO-1, ECO-2, ECO-5 y ECO-3b (worktree `ECO-3`, rama `work/ECO-3b`) quedaron a medias
  **sin commit** en sus worktrees: retomar con
  `supervise2.sh <ID> <worktree> <prompt> resume` (continúa su sesión `<ID>-s1`).
  Encargos de corrección listos: ECO-1b, ECO-3b (en curso), ECO-4b; faltan ECO-2b (cuando
  termine ECO-2) y, si hace falta, ECO-5b.
- **Desde 20:30 los peces usan `opencode/hy4-flash-free`** (decisión del usuario).
- **Integrado en `eco/main` (2026-11-18, ~22:00):** ECO-1+1b (`ez-term`), ECO-2+2b
  (`ez-arbiter`), ECO-3+3b (`ez-ctl`), ECO-4+4b (`ez-team`), ECO-5+5b (`ez-hub`, con
  `--demo --selfcheck`), ECO-7 (nota de voz; falta prueba con micrófono real), ECO-REV0.
- **En curso:** ECO-2c (worktree `ECO-2`) y ECO-6a (worktree `ECO-6a`), en paralelo.
- **Siguiente:** ECO-6b (vista conectada; encargo listo), ECO-8 (punta a punta con
  agente falso y hc real; luego prueba con CLIs reales, que gasta cuota: preguntar).
- Pendientes menores: prueba del aviso de familia del validador (`ez-team`, team.rs);
  usar `ez_herdr::prelaunch::discover_codex_models` para los modelos de Codex.
- `cleanup_worktrees.sh`: usar `BASE=eco/main`, mirar primero sin `--apply`.

## Noche del 2026-11-18 → 19 (el usuario duerme)
- **Autorización del usuario (22:40):** seguir trabajando sin pausar y **apagar el PC al
  terminar** («tienes la capacidad de apagar el pc si me voy a dormir»). Vale solo para
  esta noche.
- **Apagado rodante (pedido del usuario, 22:43):** programado para las **00:00**; mientras
  Claude siga trabajando, lo corre **1 h más** antes de cada hora límite (`shutdown /a` y
  `shutdown /s /t <segundos hasta la nueva hora>`). Un temporizador en segundo plano avisa
  15 min antes de cada hora límite. Si Claude se queda sin límite de uso, no lo corre y el
  PC se apaga solo. Al terminar: guardar todo, `shutdown /a` y `shutdown /s /t 300`.
- Plan de la noche: ECO-R1 → ECO-4c + ECO-2d → ECO-6c → ECO-6b → ECO-8 (agentes falsos y
  hc real), cada uno con puerta, verificación y fusión. **No** lanzar la prueba con CLIs
  reales (gasta cuotas: preguntar). Si algo necesita una decisión del usuario, decidir lo
  más simple y seguro con `// DECISION` y dejarlo anotado aquí para la mañana.
- Al terminar, dejar un resumen para el usuario en `rust-migration/reports/NOCHE-2026-11-18.md`.
