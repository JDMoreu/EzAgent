# HD-5 — Enjambre y equipo autónomo: los miembros cuelgan del primero

**Lee `rust-migration/briefs/_COMMON-HD.md` y `docs/designs/subagentes-herdr.md` (§3, §8,
§9).** Parte de `eco/main` con HD-2 integrado (`RunAgent` con `parent`, `hc_name`,
`labels`, `show`, `notify`, `outlive_parent`; `Placement`; `hc_slug`). En paralelo, HD-4
toca `ez-herdr/src/herdr/delegate.rs`, `ez-ctl` y `ez-fakes`: no los toques.

## Archivos que se tocan (solo estos)
- `rust/crates/ez-swarm/src/supervisor/host.rs` (campos de `LaunchRequest` y su paso a
  `RunAgent` en `HerdrHost::run_agent`).
- `rust/crates/ez-swarm/src/supervisor/launch.rs` (regla de la raíz y datos del pedido).
- Lo que lance miembros de nuevo en `ez-swarm/src/recovery*` (reabrir/reemplazar), **solo**
  para pasar la misma regla.
- Pruebas: `rust/crates/ez-swarm/src/supervisor/tests/launch.rs`, `test_support.rs` y las
  de recuperación.
- Literales `LaunchRequest { … }` de otros crates (p. ej. `ez-app`) que dejen de compilar:
  **solo** añadir los campos nuevos con su valor por omisión.

## Cambios
1. `LaunchRequest` gana `parent: Option<String>`, `hc_name: Option<String>`, `labels:
   Map<String, Value>`, `show: Placement`, `notify: bool`, `outlive_parent: bool`.
   `HerdrHost::run_agent` los pasa a `RunAgent`.
2. Raíz: el primer miembro del enjambre en orden de trabajo (el orden con que se guardan
   en el tablero) que **ya tiene dirección** y no es el que se lanza.
   - Con raíz: `parent = Some(raíz)`, `outlive_parent = true`, `notify = false`, `show =
     Tab`, `hc_name = Some(hc_slug(<nombre del miembro>))`, etiquetas `ez.swarm=<id>`,
     `ez.role=<papel>`, `ez.agent=<tipo>`, `ez.parent=<raíz>` y, si hay, `ez.model`,
     `ez.effort`.
   - Sin raíz con dirección: como hoy (sin padre) y un `info!` que lo diga.
   - La raíz se lanza como hoy.
3. Reabrir o reemplazar un miembro sigue la misma regla.

## Pruebas (host falso que registra cada `LaunchRequest`)
- Primer miembro: sin padre. Segundo y siguientes: padre = dirección del primero y los
  valores de §9. Raíz sin dirección todavía → sin padre.
- Reapertura/reemplazo de un miembro que no es raíz: con padre.
- `HerdrHost::run_agent` copia los campos a `RunAgent` (prueba con el runtime falso de
  `ez-herdr` si es posible; si no, explica en el informe cómo lo comprobaste).
- `cargo test -p ez-app` sigue en verde (compila con los campos nuevos).

## Criterios de aceptación
fmt, clippy y test de `ez-swarm` y `ez-app` en verde. Informe en
`rust-migration/reports/HD-5-r1.md`.
