# IR-4 — Prueba de punta a punta del inventario, el uso, la recomendación y la guarda

**Lee `rust-migration/briefs/_COMMON-HD.md`, `_COMMON-ECO.md`,
`docs/designs/inventario-recursos.md` y `rust-migration/briefs/HD-6.md` (la prueba de punta
a punta de `ez delegate`: copia su forma).** Rama `work/IR-4` desde `eco/main` (IR-1, IR-2 e
IR-3 integrados).

## Archivos que se tocan
Solo nuevos: `rust/crates/ez-ctl/examples/e2e_inventory.rs` (+ módulos en
`examples/e2e_inventory/`), su `[[example]]` en `ez-ctl/Cargo.toml`, y los datos de ejemplo
que necesite bajo `examples/e2e_inventory/fixtures/` (**inventados**: nada copiado de la
máquina de la persona). Si encuentras un fallo del producto, **no lo arregles**: anótalo
en el informe con su reproducción (como hizo HD-6).

## Qué hace `cargo run -p ez-ctl --example e2e_inventory`
Todo bajo `std::env::temp_dir()` con nombre único, con `HOME`/`USERPROFILE`/`APPDATA`/
`LOCALAPPDATA`/`CODEX_HOME`/`MINIMAX_DATA_DIR`/`EZ_SUBAGENTS_DIR` falsos, agentes falsos de
`ez-fakes` en el `PATH` y `hc` real (terminales de prueba con nombre único, cerradas al
final aunque algo falle). Comprueba, con el `ez` compilado, y cuenta `OK`/`FALLO`:
1. `ez init --json`: `entorno.json` atómico con sistema, recursos y los agentes falsos;
   crea `planes.toml` y no lo pisa en la segunda corrida.
2. `ez usage --json` con datos de ejemplo de cada lector (Codex con `rate_limits` vigente y
   vencido, Claude con `message.usage`, OpenCode con el esquema `session_message` y un
   aviso Free + uso posterior de Go, mcode, Pi): estados y tokens por modelo esperados.
3. `ez recommend` para `developer`, `qa_validator` (con un `developer` previo en el registro
   del padre falso → familia distinta) y `planner`, con un `modelos.toml` de la prueba.
4. `ez delegate developer` **sin `--agent`** con agentes falsos: elige según la
   recomendación, lo dice, el hijo nace bajo el padre falso con su registro.
5. Guarda: umbrales absurdos en `voice.json` → código 4 con el texto de §4; `--force` lanza
   con aviso.
6. `ez watch` con umbrales de memoria absurdos → el padre falso recibe **una** vez
   `[EzAgent · aviso] Memoria baja: …` en dos vueltas.
7. Agotamiento por modelo: un `uso.json` del vigilante con el modelo gratuito → `ez usage`
   no agota a todo OpenCode y `ez recommend` no lo descarta.

## Reglas
Sin agentes reales ni cuota; no toques la ventana de Herdr de la persona (si necesitas un
espacio, uno de prueba que se cierra). Nada pasa de 1 GB. Nunca leas datos reales de uso de
la persona: todo con carpetas falsas.

## Criterios
fmt y clippy de `ez-ctl` en verde, la corrida completa con su salida literal y el recuento
`N/N OK`, y una lista de fallos del producto encontrados (o «ninguno»). Informe
`rust-migration/reports/IR-4-r1.md`. Compila en primer plano y espera el resultado.
