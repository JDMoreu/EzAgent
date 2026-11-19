# IR-3 — Recomendación de agente, modelo y esfuerzo; guarda de recursos en `ez delegate`

**Lee `rust-migration/briefs/_COMMON-HD.md`, `_COMMON-ECO.md` y
`docs/designs/inventario-recursos.md` (§4 y §6 completos).** Rama `work/IR-3` desde
`eco/main` (ya tiene `ez-scan` con `system`, `resources`, `agents`, `check_capacity`,
`usage_report` y `planes.toml`; y el registro de subagentes tolerante de HD-10).

## Archivos que se tocan
- `rust/crates/ez-scan/src/recommend*.rs` (nuevo) + pruebas: `modelos.toml` y la elección.
- `rust/crates/ez-ctl/src/scan_cmds.rs` (`ez recommend`) y `ez-ctl/src/local.rs` /
  `local/*.rs` solo donde `ez delegate` elige el agente y donde va la guarda; sus pruebas.
- `rust/crates/ez-herdr/src/subagents/watch*.rs` solo para los avisos de memoria (abajo) y
  `watch/novelties.rs` solo para una frase de fábrica nueva; sus pruebas.

## Qué hace
1. **`modelos.toml`** (`%APPDATA%\EzAgent\modelos.toml`, editable; valores de fábrica en el
   código): por modelo, `nivel` (`alto`/`medio`/`bajo`), `costo` (`gratis`/`plan`/`pago`) y
   `familia` (p. ej. `anthropic`, `deepseek`, `openai`, `minimax`, `qwen`). Los de fábrica
   salen del catálogo; marca cada valor que elijas con `# DECISION`. Sin archivo, fábrica.
2. **`ez recommend <papel> [--size s|m|l] [--json]`**: tipo + modelo + esfuerzo con sus
   razones (texto que lee una persona, en español neutro), aplicando §6 al pie de la
   letra: descarta no instalados y `agotado` **por modelo** (IR-2b da estado por modelo);
   penaliza `cerca` y `desconocido` frente a `libre`; a igual nivel gana el más barato y
   luego la `prioridad` de `planes.toml` (`reservar` va al final); `qa_validator` usa una
   familia distinta a la del último `developer` del mismo padre (registro de subagentes);
   con recursos justos prefiere el agente de menor memoria y lo avisa.
3. **`ez delegate <papel>` sin `--agent`**: usa la recomendación y lo dice en la salida
   («Elegí OpenCode · opencode-go/deepseek-v4.1-flash · medium porque …»). Con `--agent`
   sin modelo/esfuerzo, completa solo lo que falte con la misma lógica.
4. **Guarda de recursos** en `ez delegate`: `ez_scan::check_capacity` antes de lanzar;
   código 4 con el texto de §4; `--force` la salta con aviso en la salida.
5. **Avisos de memoria del vigilante** (§4): cada vuelta mide RAM y commit libres; por
   debajo de los umbrales avisa al padre **una vez por episodio**: `[EzAgent · aviso]
   Memoria baja: RAM libre N MB, virtual libre M MB. El subagente que más usa es <nombre>
   (X MB).` No mata nada. Ojo: `ez-scan` depende de `ez-herdr`, así que el vigilante no
   puede depender de `ez-scan`: define en `ez-herdr` un rasgo pequeño (p. ej.
   `MemoryProbe`) y que `ez watch` (en `ez-ctl`) lo implemente con `ez-scan`.
6. **Frase de fábrica nueva** en `watch/novelties.rs`: `Check /usage or switch provider`
   (el aviso de créditos agotados de MiniMax Code, error 50110 del 19-nov), con las mismas
   reglas de §3 de `novedades-subagentes.md` (agente parado, pantalla quieta, últimas 15
   líneas con texto).

## Pruebas (dobles para todo lo que no sea medir de verdad)
Papel → nivel/esfuerzo; agotado por modelo (el gratuito agotado no descarta OpenCode Go);
`cerca` pierde contra `libre`; `reservar` al final; `qa_validator` con familia distinta;
recursos justos → el de menor memoria + aviso; `ez delegate` sin `--agent` elige y lo
dice; guarda con código 4 y `--force`; aviso de memoria una vez por episodio; la frase de
mcode bloquea solo con el agente parado.

## Criterios
fmt, clippy y test de `ez-scan`, `ez-ctl` y `ez-herdr` en verde. **Ejecuta** en esta
máquina `ez recommend developer`, `ez recommend qa_validator` y `ez recommend planner
--json` (solo leen) y pega la salida. No lances agentes reales: prueba `ez delegate` con
los agentes falsos de `ez-fakes`. Informe `rust-migration/reports/IR-3-r1.md`. Compila en
primer plano y espera el resultado.
