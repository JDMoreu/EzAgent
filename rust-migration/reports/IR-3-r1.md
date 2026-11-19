## Resultado
- Brief/tarea: IR-3
- Estado: completado

## Cambios
- `rust/crates/ez-scan/src/recommend.rs` (nuevo): elección de tipo, modelo y esfuerzo según §6 (nivel mínimo por papel y `--size`, descarte de no instalados y agotados por modelo, penalización de `cerca`/`desconocido`, costo, prioridad de `planes.toml`, familia distinta para `qa_validator` y preferencia por menor memoria con recursos justos), con razones y avisos en español neutro.
- `rust/crates/ez-scan/src/recommend/modelos.rs` (nuevo): `modelos.toml` editable con nivel, costo, familia y agente; valores de fábrica del catálogo y validación tolerante.
- `rust/crates/ez-scan/src/recommend/tests.rs` (nuevo): papel → nivel/esfuerzo, agotado por modelo, `cerca` contra `libre`, `reservar` al final, familia distinta de `qa_validator`, memoria y filtros.
- `rust/crates/ez-scan/src/lib.rs`: exporta `recommend`, `Catalog`, `Level`, `Pressure`, `Recommendation` y `Size`.
- `rust/crates/ez-scan/src/capacity.rs`: `refusal` pasa a `reason` público (sin cambio de comportamiento) para que `ez recommend` avise de los recursos justos.
- `rust/crates/ez-ctl/src/local/choice.rs` (nuevo): recolecta uso, catálogo, presión y último `developer`; rellena lo que falta en `ez delegate`; guarda de recursos con código 4 y `--force`; línea «Elegí … / Completé …».
- `rust/crates/ez-ctl/src/local.rs`: `--force`, `LocalError::Resources` (código 4), tipo opcional, delegación con la elección y la línea de la recomendación en el resumen.
- `rust/crates/ez-ctl/src/local/watch.rs`: implementa `MemoryProbe` con `ez-scan` (RAM, commit y el subagente que más usa) y lo instala en `ez watch`.
- `rust/crates/ez-ctl/src/scan_cmds.rs`: `ez recommend <papel> [--size] [--json]`.
- `rust/crates/ez-ctl/src/cli.rs`: ruta de `recommend`, `force` como opción sin valor y ayuda actualizada (archivo no listado en el brief; ver Decisiones).
- `rust/crates/ez-herdr/src/subagents/watch/memory.rs` (nuevo): rasgo `MemoryProbe`, aviso de memoria baja y su episodio.
- `rust/crates/ez-herdr/src/subagents/watch.rs`: instala el medidor y consulta la memoria cada vuelta; `Tick::memory_warned`.
- `rust/crates/ez-herdr/src/subagents/watch/children.rs` (nuevo): se movió aquí la lectura de `hc ls` para que `watch.rs` quede bajo 600 líneas (sin cambios de comportamiento).
- `rust/crates/ez-herdr/src/subagents/watch/novelties.rs`: frase de fábrica `Check /usage or switch provider` (crédito agotado de MiniMax Code, error 50110).
- `rust/crates/ez-herdr/src/subagents.rs`: reexporta `MemoryProbe` y `MemoryReading`.
- `rust/crates/ez-ctl/tests/delegate_local.rs` y `tests/scan_init.rs`: pruebas de `--agent` opcional, `--force` y `ez recommend` con agentes y `modelos.toml` falsos.

## Decisiones
- `# DECISION` en `recommend/modelos.rs` (tabla de fábrica): OpenCode y Claude Code cuestan `plan`; Qwen Code es el único `gratis`; Kimi K3 es `pago`; los niveles y familias son una propuesta editable. Los modelos `*-free` de OpenCode, al no estar en el catálogo del perfil, se declaran en `modelos.toml`.
- Un `modelos.toml` con entradas válidas reemplaza toda la fábrica; vacío, ilegible o sin entradas válidas deja la fábrica y avisa. Un archivo válido nunca se mezcla con la fábrica.
- Un modelo sin `familia` queda como `desconocida` (aviso) y no cuenta para la regla de familia distinta; sin `agente` se infiere de la fábrica y, si no está, se salta con aviso. `agente` no aparecía en el brief pero es necesario para saber a quién lanzar.
- No se filtra por la lista `models` del perfil: `modelos.toml` manda y el lanzamiento valida; así sirven modelos descubiertos (por ejemplo `*-free`).
- Se prefiere el nivel más bajo que cumpla el mínimo del papel (no se gasta un modelo alto cuando uno medio alcanza); a igual nivel, el más barato.
- Con recursos justos, un tipo sin subagentes vivos cuenta 0 MB (el que menos usa) y la elección se avisa.
- Esfuerzo: si el modelo no tiene el nivel pedido, se usa el válido más cercano; a igual distancia, el más alto (`medium` → `high` en `deepseek-v4.1-flash`, que no tiene `medium`).
- `qa_validator`: si no hay ningún modelo de otra familia instalado, se relaja la regla con aviso en vez de dejar al padre sin validador.
- Guarda: si la medición de recursos falla (por ejemplo, `hc` no responde), no se bloquea la delegación; se avisa y se sigue.
- El «último developer» es el registro con `since_ms` mayor (el último cambio de estado); el registro no guarda una fecha de creación aparte.
- `--force` deja un aviso en `warnings` (stderr) y no imprime una línea aparte; la elección se antepone al resumen y viaja en `chosen` con `--json`.
- El aviso de memoria se emite una vez por episodio y el episodio termina cuando la medición vuelve a estar por encima de los umbrales; nunca mata nada.
- La frase nueva de `novelties.rs` es global (igual que las demás de fábrica) y respeta las reglas de §3: agente parado, pantalla quieta y últimas 15 líneas con texto.

## Verificación
Desde `rust/`:

```
$ cargo fmt --check -p ez-scan -p ez-ctl -p ez-herdr
fmt=0

$ cargo clippy -p ez-scan -p ez-ctl -p ez-herdr --all-targets -- -D warnings
    Checking ez-scan v0.1.0 (...)
    Checking ez-ctl v0.1.0 (...)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 8.54s

$ cargo test -p ez-scan -p ez-ctl -p ez-herdr
ez-scan:  test result: ok. 73 passed; 0 failed  (lib)
          test result: ok. 3 passed; 0 failed   (real_system)
          test result: ok. 1 passed; 0 failed   (usage_readers)
ez-ctl:   test result: ok. 12 passed; 0 failed  (lib)
          test result: ok. 7 passed; 0 failed   (cli_errors)
          test result: ok. 10 passed; 0 failed  (delegate_local)
          test result: ok. 17 passed; 0 failed  (local_state)
          test result: ok. 6 passed; 0 failed   (roundtrip)
          test result: ok. 3 passed; 0 failed   (scan_init)
          test result: ok. 7 passed; 0 failed   (server_errors)
          test result: ok. 2 passed; 0 failed   (waits)
ez-herdr: test result: ok. 312 passed; 0 failed (lib)
          test result: ok. 1 passed; 0 failed   (agent_driver_parity)
          test result: ok. 1 passed; 0 failed   (herdr_parity)
          test result: ok. 2 passed; 0 failed   (metrics_parity)
          test result: ok. 27 passed; 0 failed  (permission)
          test result: ok. 1 passed; 0 failed   (tasks_activity_parity)
```

Comandos en vivo (solo lectura, esta máquina):

```
$ ez recommend developer
Recomiendo OpenCode · opencode-go/deepseek-v4.1-flash · high para developer (tamaño m).
Razones:
  - El papel «developer» pide como mínimo nivel medio y esfuerzo medium.
  - OpenCode · opencode-go/deepseek-v4.1-flash es de nivel medio, costo plan y familia deepseek.
  - No puedo saber el uso de OpenCode; lo penalicé frente a uno libre.
  - El modelo no tiene el esfuerzo medium; uso high.

$ ez recommend qa_validator
Recomiendo OpenCode · opencode-go/deepseek-v4-pro · high para qa_validator (tamaño m).
Razones:
  - El papel «qa_validator» pide como mínimo nivel alto y esfuerzo high.
  - OpenCode · opencode-go/deepseek-v4-pro es de nivel alto, costo plan y familia deepseek.
  - No puedo saber el uso de OpenCode; lo penalicé frente a uno libre.

$ ez recommend planner --json
{"ok":true,"recommendation":{"role":"planner","size":"m","agent":"opencode","label":"OpenCode","model":"opencode-go/deepseek-v4-pro","effort":"high","level":"alto","cost":"plan","family":"deepseek","reasons":["El papel «planner» pide como mínimo nivel alto y esfuerzo high.","OpenCode · opencode-go/deepseek-v4-pro es de nivel alto, costo plan y familia deepseek.","No puedo saber el uso de OpenCode; lo penalicé frente a uno libre."],"warnings":[]}}
```

No se lanzó ningún agente real ni se tocó la ventana de Herdr. `ez delegate` se probó con dobles: la elección y la línea «Elegí …» en `local::choice::tests`, la guarda (código 4 y `--force`) en la misma suite, y el comando completo en `scan_init` con agentes falsos en el `PATH` y `modelos.toml` temporal.

## Pendiente o riesgos
- La ruta de lanzamiento real de `ez delegate` sin `--agent` no se ejecutó de punta a punta: habría que abrir terminales en Herdr (prohibido en esta tarea). La elección, el guardado de `chosen` en el resultado y la guarda están cubiertos con dobles; el lanzamiento en sí es el mismo camino ya probado por HD-9.
- `ez-scan` no lee `opencode models` al recomendar (sería lento y no gasta cuota, pero abre el programa): los modelos descubiertos que no estén en el catálogo del perfil deben declararse en `modelos.toml` con su `agente`.
- `uso.json` de IR-2b sigue siendo por agente (ignora `modelo`); si el vigilante anota un agotamiento de un solo modelo, `ez usage` puede marcar el agente entero. La recomendación sí respeta el estado por modelo que entrega el informe de uso. Corregirlo toca `usage/depleted.rs`, fuera del alcance de IR-3.
- El aviso de memoria del vigilante consulta `hc ls` solo cuando la RAM o el commit están por debajo del umbral; si `hc` falla, no se emite el aviso (no se inventa el «que más usa»).
- Archivos tocados fuera de la lista del brief, necesarios: `ez-ctl/src/cli.rs` (ruta del comando, `--force` y ayuda), `ez-scan/src/lib.rs` y `capacity.rs` (API pública), `ez-herdr/src/subagents.rs` (reexportación) y `watch/children.rs` (partición por el límite de 600 líneas). No cambian comportamiento fuera de IR-3.
