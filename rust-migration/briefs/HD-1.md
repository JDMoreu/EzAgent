# HD-1 — ez-term: lanzar con padre

**Lee `rust-migration/briefs/_COMMON-HD.md` y `docs/designs/subagentes-herdr.md` (§2, §10).**
En paralelo, HD-2 y HD-3 trabajan en `ez-herdr`: no lo toques.

## Archivos que se tocan (solo estos)
- `rust/crates/ez-term/src/types.rs`, `rust/crates/ez-term/src/client.rs` y las pruebas de
  `rust/crates/ez-term/tests/`.
- `rust/crates/ez-hub/src/runtime/launcher.rs`: **solo** añadir los campos nuevos a los
  dos literales `RunSpec { … }` (`parent: None, outlive_parent: false`), sin cambiar
  comportamiento.

## Cambios
1. `RunSpec` gana `parent: Option<String>` (documentado: «Terminal padre; con `Some`, el
   hijo cuelga de ella aunque `no_parent` sea `true`») y `outlive_parent: bool` (por
   omisión `false`). `Default` los deja en `None` y `false`.
2. `RunSpec::params()`: con `parent = Some(p)` envía `"parent": p` y `"no_parent": false`;
   envía siempre `"outlive_parent"`. Sin `parent`, los parámetros quedan como hoy (más
   `outlive_parent`).
3. `HcClient::children(&self, parent: &str) -> Result<Vec<Terminal>, TermError>`: llama a
   `terminal.list` (sin etiquetas) y devuelve las que tienen `parent == Some(parent)`.
   Comprueba en `docs/herdr/hc-schema-0.1.21.json` que `terminal.list` devuelve objetos
   `Terminal` con `parent`; si no, dilo en el informe.

## Pruebas
- `params()` con y sin `parent`, y con `outlive_parent`.
- `children()` con el transporte falso que ya usan las pruebas del cliente: filtra por
  padre y propaga errores.
- `cargo test -p ez-hub` debe seguir en verde.

## Criterios de aceptación
fmt, clippy y test de `ez-term` y `ez-hub` en verde. Informe en
`rust-migration/reports/HD-1-r1.md`.
