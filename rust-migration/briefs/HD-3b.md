# HD-3b — Correcciones de la revisión de HD-3

Revisión del agente principal sobre `work/HD-3` (commits b4835b5..a759ae3). Las pruebas
pasan y detectan mutaciones (verificado), pero hay tres correcciones. Mismo alcance que
HD-3, **más** `rust/crates/ez-herdr/src/driver/launch.rs` **solo** para cambiar la
visibilidad de `match_mcode_model` y `resolve_codex` (y de `Wanted` si hace falta) a
`pub(crate)`.

## R3-1. No duplicar la lógica de mcode y Codex
La `// DECISION` de `choice.rs` dice que `match_mcode_model` y `resolve_codex` de
`launch.rs` «escriben configuración»: **no es así**, ambas son funciones puras (solo
comparan con el catálogo ya leído). Duplicarlas permite que la validación y el
lanzamiento diverjan. Reúsalas (`pub(crate)`) y borra `mcode_matches`, `union_levels` y
la copia de la lógica de `check_codex`; actualiza la `// DECISION`.

## R3-2. Esfuerzo sin modelo en perfiles con `model_efforts` (OpenCode)
Hoy `ez delegate developer --agent opencode --effort ultra` se acepta: sin modelo no se
mira `model_efforts` y el perfil «tiene forma» de fijar esfuerzo. Pero el lanzamiento usa
el modelo por omisión de OpenCode con la variante `#ultra`, que no existe. Regla: con
esfuerzo y sin modelo en un perfil con `model_efforts`, valida contra los niveles del
modelo por omisión (`hooks.opencode_default_model`, solo lectura; compara por la parte
tras la última `/`); si ese modelo no está en `model_efforts` o no se conoce, contra la
unión de todos los niveles de `model_efforts`. Mensaje: el mismo «El modelo {m} no tiene
el esfuerzo …» (o, con la unión, «OpenCode no tiene el esfuerzo «X». Valores válidos:
…»). Pruebas: `ultra` sin modelo → rechazo; `high` sin modelo → aceptado; modelo por
omisión conocido con niveles propios.

## R3-3. El programa se busca igual que al lanzar
`check_program` rearma el comando por su cuenta (`profile.command` o `agents[key]`).
Usa la misma función que `build_launch` (`agent_command(cfg, &key, &profile)`) para que
validación y lanzamiento miren el mismo programa. Prueba con un `agents` del
`voice.json` que reemplaza el comando por una ruta absoluta.

## Entrega
fmt, clippy y test de `ez-herdr` en verde. Informe en
`rust-migration/reports/HD-3b-r1.md` con la verificación real.
