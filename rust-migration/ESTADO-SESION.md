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
