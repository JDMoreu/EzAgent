# HD-9 — Corregir los cuatro fallos del producto que encontró HD-6

**Lee `rust-migration/briefs/_COMMON-HD.md`, `docs/designs/subagentes-herdr.md` y la
sección «Hallazgos del producto» de `rust-migration/reports/HD-6-r1.md`.** Rama
`work/HD-9` desde `eco/main` (ya trae HD-0…HD-8 y el ejemplo `e2e_delegate`).

## Archivos que se tocan
`rust/crates/ez-herdr/src/herdr/child.rs` (+ pruebas), `rust/crates/ez-herdr/src/herdr/text.rs`
(+ pruebas), `rust/crates/ez-herdr/src/subagents/watch.rs` (+ pruebas),
`rust/crates/ez-herdr/src/herdr/delegate*.rs`, `rust/crates/ez-ctl/src/local*.rs` y el
ejemplo `rust/crates/ez-ctl/examples/e2e_delegate*`.

## B1. Los errores de `hc run --json` llegan por `stderr`
Verifícalo tú (permitido: `hc run --bg --json --name <x> -- ping -n 30 127.0.0.1` dos
veces y luego `hc kill`/`hc rm`; nada más). Haz que todo lector de `hc run --json`
(`child.rs::hc_run_json`, `subagents/watch.rs::ensure_watcher` y cualquier otro) entienda
el JSON de error venga por `stdout` o por `stderr`. Consecuencias que deben quedar
probadas: `ensure_watcher` trata `name_taken` como «ya corre» (sin aviso espurio) y
`delegate` reintenta `n+1` con el nombre automático.

## B2. La envoltura de PowerShell no ejecuta un programa citado
`text.rs::command_line` cita la ruta del programa con comillas simples sin el operador
`&`; PowerShell la toma como cadena y el hijo muere. Cuando el programa va citado, usa
`& '<ruta>' <argumentos>`. Prueba unitaria del texto generado (ruta con espacios y con
`\`), y comprueba una vez a mano (sin dejar código de shell en el repo) que PowerShell
lo ejecuta.

## B3. El vigilante no debe leer la configuración real si recibe `--hc`
`ez watch --parent … --dir … --hc …` no debe cargar `VoiceConfig` cuando ya tiene la ruta
de `hc`. Prueba con `APPDATA` apuntando a una carpeta sin `voice.json`.

## B4. `--show tab` abre la pestaña en el espacio que se está mirando
`terminal.keep --place tab` usa el espacio a la vista, no el del padre. Usa la adopción
por etiquetas de Herdr: lee la etiqueta `herdr.space` del **padre** (`hc get`) y pon al
hijo `hc label <hijo> herdr.space=<esa>` (sin `herdr.tab`: Herdr crea una pestaña en ese
espacio) **sin mover el foco** de la persona. Si el padre no tiene `herdr.space` (no se
ve en Herdr), cae al `terminal.keep` actual con su aviso. Verifícalo en vivo **solo** en
un espacio de prueba propio (`space.new --focus false`, como HD-6) con terminales `ping`,
y comprueba con `herdr-app ls` que la pestaña quedó en el espacio del padre, que el hijo
sigue bajo su padre en `sub_agents` y que el foco no cambió. Limpia todo al final.

## E2E sin atajos
En `e2e_delegate`: quita la decisión 3 (usa la ruta del agente falso con `\`), la
decisión 6 (`space.select` antes de la pestaña) y comprueba que ya no aparece el aviso
«No pude arrancar el vigilante». **Ejecuta** el ejemplo y pega la salida: código 0.

## Reglas
Ninguna prueba ni medición pasa de 1 GB. No ejecutes `ez delegate` real con agentes
reales ni el paso 9 (humo real). Nada de shells en el código del producto (el `&` es
parte del texto que ya genera `command_line`, no una shell nueva).

## Criterios de aceptación
fmt, clippy y test de `ez-herdr` y `ez-ctl` en verde y el ejemplo con código 0 sin los
atajos. Informe en `rust-migration/reports/HD-9-r1.md`.
