# ECO-8 — Prueba de punta a punta con agentes falsos y herdr-core real

**Lee `rust-migration/briefs/_COMMON-ECO.md`, `docs/designs/espacio-elefante.md` §14 y §15,
y la API real de `ez_hub::runtime`, `ez-arbiter`, `ez-ctl`, `ez-team` y `ez-term`.** En
paralelo, otro pez hace ECO-6b en la vista de `ez-hub` (`ui/**`, comandos y modo
demostración): no toques esos archivos.

Objetivo: demostrar que el ciclo completo funciona **sin modelos de lenguaje**: un agente
principal falso propone la tarea y el equipo, delega por etapas en subagentes falsos que
corren en terminales reales de hc, el árbitro verifica, hay dos rondas limpias, la tarea se
cierra y la rama se integra.

## Archivos que se tocan (solo estos)
- `rust/crates/ez-fakes/src/fake_ws_agent.rs` (+ submódulos), `src/bin/fake_ws_agent.rs`,
  la línea `pub mod` en `src/lib.rs` y su `[[bin]] fake-ws-agent` en `Cargo.toml`.
  **No** toques `fake_agent.rs` (lo usan las pruebas de la app de voz).
- `rust/crates/ez-hub/examples/e2e_fake.rs` (+ módulos de apoyo bajo `examples/e2e_fake/`).
- `rust/crates/ez-hub/src/runtime/engine.rs` y `mod.rs`, **solo** para añadir
  `RuntimeOptions { ez_dir: Option<PathBuf> }` y `HubRuntime::start_with(data_dir, options)`
  (`start` llama a `start_with` con valores por omisión): el ejemplo necesita decirle
  dónde está `ez.exe`.

## `fake-ws-agent` (un TUI mínimo)
- Al arrancar escribe el estado con OSC 7501: `ESC ] 7501 ; :state=idle:app=ez-fake ESC \`
  (formato confirmado en vivo: hc lo muestra como `status {app, state}`), imprime
  `fake listo> ` y lee líneas de stdin. Antes de trabajar pasa a `state=working` y al
  terminar vuelve a `state=idle`.
- Modo por argumento: `--exec` (ejecutor), `--verify` (verificador) o `--main <guion>`.
- **Subagente** (línea que contiene `Lee <ruta> y síguelo`): lee las instrucciones, saca
  la ruta del informe del comando `ez report … --file "<ruta>"` que traen, y:
  - `--exec`: escribe `fake/<id>.txt` en su carpeta actual (el worktree) con una línea;
  - `--verify`: no cambia el proyecto;
  - escribe el informe (`## Veredicto` / `pass` / `## Hallazgos` vacío / `## Evidencia`) y
    ejecuta `ez report --verdict pass --file "<ruta>" --blocking 0 --major 0 --minor 0`, más
    `--mutations-tried 3 --mutations-caught 3` si el comando de las instrucciones las pide.
- **Agente principal** (`--main <guion.toml>`, al recibir la línea que empieza por
  «Eres el agente principal»): sigue el guion, que lista los comandos `ez` en orden; los
  pasos `wait` repiten `ez wait --timeout 30` hasta ver el evento esperado (o fallan tras
  10 min). Escribe antes los borradores que el guion trae en línea (encargo, criterios,
  equipo). Todo con `std::process::Command` (`ez` está en `PATH`), sin shell.

## `examples/e2e_fake.rs`
1. Compila primero (o exige compilados) `ez`, `fake-ws-agent` y localiza `hc`; si falta
   `hc`, termina con código 2 y un mensaje claro.
2. Prepara en `std::env::temp_dir()` un repositorio git (commit inicial), una carpeta de
   datos propia y su `agentes.toml` con tres perfiles: `fake_main`
   (`argv = [<fake-ws-agent>, "--main", <guion>]`), `fake_exec`
   (`[<fake-ws-agent>, "--exec"]`, modelos `["exec-1"]`, familia `exec`) y `fake_verify`
   (`[<fake-ws-agent>, "--verify"]`, modelos `["verify-1"]`, familia `verify`).
3. Guion del agente principal: criterio `C1` = `["git", "--version"]` con
   `contiene = "git version"`; equipo con los cinco papeles incluidos (planner y developer
   con `fake_exec`; plan_reviewer, qa y qa_validator con `fake_verify`); ronda 1 completa
   por etapas; ronda 2 solo de verificación (qa y qa_validator); `ez close`.
4. Arranca `HubRuntime::start_with` (sin ventana), crea el espacio y un agente principal
   con `fake_main`, y se suscribe a los eventos del árbitro: aprueba las tarjetas de tarea
   y equipo y, al cerrar, elige `merge` en la de integración.
5. Éxito (código 0) si en ≤ 10 min: hay `task_closed` e `integrated`; la rama base del
   repositorio tiene un merge con los archivos `fake/*.txt`; la bitácora tiene las dos
   rondas limpias; ningún token aparece en la bitácora ni en `estado.json`. Si no, código 1
   con lo que faltó y los últimos 30 eventos.
6. Al final mata y borra **solo** las terminales de hc que creó (por la etiqueta de su
   espacio) y borra sus carpetas temporales.

## Pruebas
Unitarias de `fake-ws-agent` (lectura de la ruta del informe, armado del comando `ez
report`, lectura del guion). **Ejecuta** el ejemplo
(`cargo run -p ez-hub --example e2e_fake`) y pega en el informe su salida y el código.
Si falla por un error del producto (no del ejemplo), descríbelo con evidencia y **no** lo
arregles fuera de tu alcance: el informe dice dónde está.

## Criterios de aceptación
fmt, clippy y test de `ez-fakes` y `ez-hub` en verde y el ejemplo con código 0 (o, si
falla por el producto, el diagnóstico con evidencia). Informe en
`rust-migration/reports/ECO-8-r1.md`.
