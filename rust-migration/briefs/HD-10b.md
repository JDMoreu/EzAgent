# HD-10b — Corrección de la revisión de HD-10: `ez tasks --json` con una sola forma

Revisión del agente principal sobre `work/HD-10` (be06a64 + e6b4dd4). Bien: `list_lenient`,
`validation` toda en `null` = `None`, aviso único por archivo ilegible, `refresh_activity`
tolerante (buena decisión), mutación comprobada. **Mal**: la decisión 1. `ez tasks --json`
devuelve un array sin ilegibles y un objeto con ilegibles: quien lo consuma (el ejemplo de
HD-6 y, pronto, la app de voz en NV-3) se rompe **justo** cuando hay un archivo dañado. La
salida JSON debe tener una sola forma. El fallo del brief fue mío (pedí `"invalid"` sin
ver que la salida era un array).

## Cambios (solo `ez-ctl/src/local/state.rs` y `ez-ctl/tests/local_state.rs`)
1. `--json`: stdout es **siempre** el array de registros válidos (como antes de HD-10). Cada
   ilegible va a **stderr** como `registro ilegible: <ruta>: <error>`. Código 0.
2. Sin `--json`: igual que ahora (tabla y, al final, las líneas de ilegibles), pero esas
   líneas también a **stderr**, para que la tabla en stdout sea solo la tabla.
3. Pruebas: con un válido + uno roto, `--json` → stdout se parsea como array de 1 elemento
   y stderr nombra el roto; sin `--json` → stdout tiene la tabla sin la línea del roto y
   stderr la tiene.

## Entrega
fmt, clippy y test de `ez-ctl` en verde; repite la corrida contra la copia de la carpeta
real con `roto.json` mostrando stdout y stderr por separado. Informe
`rust-migration/reports/HD-10b-r1.md`, commit en `work/HD-10`. Compila en primer plano.
