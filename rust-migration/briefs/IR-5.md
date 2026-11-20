# IR-5 — Los fallos que encontró IR-4

**Lee `rust-migration/briefs/_COMMON-HD.md`, `_COMMON-ECO.md`,
`docs/designs/inventario-recursos.md` y `rust-migration/reports/IR-4-r1.md` (sección «Fallos
del producto», con sus reproducciones).** Rama `work/IR-5` desde `eco/main`.

## Archivos que se tocan
`rust/crates/ez-scan/src/recommend/modelos.rs`, `ez-scan/src/real.rs`, `ez-scan/src/entorno.rs`
(+ lo mínimo en `ez-scan/src/lib.rs`), `rust/crates/ez-ctl/src/local/choice.rs`,
`ez-ctl/src/scan_cmds.rs` solo si `ez init` necesita pasar el uso, y sus pruebas.

## Cambios
1. **`modelos.toml` con nombres con punto.** `[deepseek-v4.1-flash]` sin comillas es en TOML
   una tabla anidada (`deepseek-v4` → `1-flash`). El lector debe reconstruir el nombre
   (tabla sin `nivel`/`costo` cuyas subtablas sí los tienen → `padre.hijo`) y aceptar
   también la forma citada; el ejemplo de la cabecera y lo que proponga `ez init` usan la
   forma citada. Si **ninguna** entrada es válida, el aviso debe decir claramente que se
   usan los valores de fábrica y por qué.
2. **La guarda mide con el `hc` configurado.** `real.rs` usa `VoiceConfig::new()` (fábrica)
   y no el `voice.json` cargado: con `hc_path` absoluto y sin `hc` en el `PATH`, la medición
   falla y la guarda deja lanzar. Usa la configuración cargada (la misma que `ez delegate`).
3. **Aviso falso con modelo explícito sin esfuerzo.** Con `--agent` y `--model` dados y sin
   `--effort`, completar solo el esfuerzo (el del catálogo del agente o el de fábrica del
   papel) sin «No pude completar el modelo…». Si de verdad no hay esfuerzo válido, el aviso
   dice eso y nada más.
4. **`entorno.json` con «Uso y límites»** (§3 del diseño): `ez init` añade el resumen de
   `usage_report` por agente (estado, plan, ventana, reinicio, fuente), sin tokens de
   conversación ni datos sensibles.

## Pruebas
Una por fallo, con la reproducción de IR-4 convertida en prueba (carpetas falsas):
punto sin comillas → la preferencia se respeta; `hc_path` sin `PATH` → código 4 con
umbrales absurdos; modelo explícito sin esfuerzo → sin aviso falso; `entorno.json` con su
sección de uso. Además, vuelve a correr `cargo run -p ez-ctl --example e2e_inventory`
(debe seguir 86/86 o más) y pega el recuento.

## Criterios
fmt, clippy y test de `ez-scan` y `ez-ctl` en verde. **Memoria del equipo justa**: compila
con `CARGO_BUILD_JOBS=2`, en primer plano, y nunca dos `cargo` a la vez. Informe
`rust-migration/reports/IR-5-r1.md`.
