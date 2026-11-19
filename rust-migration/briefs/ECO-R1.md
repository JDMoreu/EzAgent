# ECO-R1 — Migración a papeles abiertos, etapas y nombres neutros

**Lee `rust-migration/briefs/_COMMON-ECO.md` y `docs/designs/espacio-elefante.md` §15
completo (manda sobre todo lo anterior).** Es una migración: el comportamiento con los
cinco papeles incluidos debe quedar **igual** que hoy (las pruebas existentes siguen
pasando, adaptadas a los tipos nuevos). Las bibliotecas de papeles (ECO-4c), varios
agentes principales por espacio, worktrees y delegación (ECO-2d) **no** son de este encargo.

## Contratos ya escritos (solo lectura; están en tu rama desde el primer commit)
- `rust/crates/ez-team/src/roles.rs`: `RoleId`, `RoleKind`, `RoleOrigin`, `RoleDef`,
  `builtin::*`. **El enum `Role` ya no existe.**
- `rust/crates/ez-team/src/contract.rs`: `Seat` (con `stage` y `delegates_to`) e
  `InstructionContext` (con `role_name`, `parent`, `delegable_roles`, `drafts`).
- `rust/crates/ez-team/src/roles/*.md`: los cinco papeles con cabecera `+++`, `_comun.md`
  y `_delegar.md` nuevos. Marcadores: `{papel_nombre}`, `{id}`, `{ronda}`, `{carpeta}`,
  `{encargo}`, `{criterios}`, `{plan}`, `{mi_encargo}`, `{informe}`, `{informe_qa}`,
  `{mutaciones_minimas}`, `{papeles_delegables}` (ids separados por coma), `{borradores}`.
- `rust/crates/ez-ctl/src/skill.md`: «Manual del agente principal».

## Archivos que se tocan
`rust/crates/ez-team/**`, `rust/crates/ez-arbiter/**`, `rust/crates/ez-ctl/**` y
`rust/crates/ez-hub/**`, salvo los contratos de arriba.

## Cambios
1. **`ez-team`.**
   - Lector de archivos de papel: cabecera TOML entre una línea `+++` inicial y la
     siguiente `+++`; campos de §15.2 (`nombre`, `tipo`, `descripcion` obligatorios; los
     booleanos por omisión `false`; `etapa` opcional, por omisión 1 si es ejecutor y 2 si
     es verificador); cuerpo = el resto. Errores claros (con el archivo) si falta un campo,
     `tipo` no es válido o `puede_delegar = true` en un verificador.
   - `pub fn builtin_roles() -> Vec<RoleDef>` con los cinco incluidos (`include_str!`).
   - `render_instructions(def: &RoleDef, ctx: &InstructionContext)`: `_comun.md` + cuerpo
     del papel + (si `can_delegate`) `_delegar.md`, con los marcadores de arriba; error si
     queda alguno.
   - `first_line(role_name, instrucciones)` y `main_first_line()` (antes
     `elephant_first_line`) con los textos exactos de §15.9.
   - `team`: `[[papel]]` con `etapa` (por omisión la del papel) y `delega_en`; valida
     contra una lista de `RoleDef` que recibe (hoy, la de los incluidos) con las reglas de
     §15.3; la tarjeta añade ` · etapa {n}` a cada línea.
2. **`ez-arbiter`.**
   - `Role` → `RoleId` en todo. `Command::TeamPropose` gana `roles: Vec<RoleDef>` (las
     definiciones de los papeles del equipo); se congelan con el equipo.
   - Las reglas fijas por papel pasan a ser propiedades: verificador → `kind`; protegido
     por QA → `writes_tests`; mutaciones → `mutations`; solo ronda 1 → `first_round_only`.
   - **Orden por etapas** (§15.4, primer y segundo punto): un trabajo de etapa *k* empieza
     cuando informaron todos los de etapas menores en la ronda; `ez send` solo a ejecutores
     y solo si no empezó una etapa posterior a la suya. Con las etapas sugeridas de los
     incluidos (1–5) el orden es el mismo de hoy.
   - Limpia = §15.4 (al menos un verificador informó; mutaciones por papel con
     `mutations`; violaciones por `writes_tests`). `{informe_qa}` de un papel con
     `mutations` = el informe del verificador con `writes_tests` de esa ronda.
   - Prefijo del id de subagente: `RoleId::short()`.
   - Textos (§15.9): todo mensaje de rechazo, evento o tarjeta dice «agente principal» y
     «subagente», nunca «elefante» ni «pez». `EZ_ROLE` = `main` o `sub`.
3. **`ez-ctl`.** La prueba `thirty_two_waits_do_not_block_report_and_the_next_wait_is_503`
   (`tests/waits.rs`) falla una de cada tres veces también en `eco/main` («solo entraron N
   esperas»). Encuentra la causa (¿se pierde o se retrasa alguna espera en el servidor?),
   corrígela y demuestra con 10 corridas seguidas en verde. **No** subas el tope de tiempo
   de la prueba. Después, `ez fish` → `ez delegate <papel> --brief <md>` (protocolo
   `agent.delegate`, args `{role, brief}`); comando nuevo `ez roles` (protocolo `roles`,
   args `{}`); ayuda y mensajes con los nombres nuevos; la prueba de `--skill` busca
   «# Manual del agente principal».
4. **`ez-hub`.** Adapta a los tipos nuevos (también el runtime de ECO-6a si ya está en tu
   rama); `agent.delegate` y `roles` en el `Handler` (`roles` responde con los incluidos:
   id, nombre, tipo y descripción por línea; ECO-6c lo amplía); `EZ_ROLE=main`; textos de
   la vista: «Agente principal · {agente}», pestaña «Subagentes», etiqueta «Agente
   principal» en el formulario. `--demo --selfcheck` sigue saliendo con 0 (si la cabecera
   cambia, ajusta la comprobación a «Agente principal · »).

## Pruebas
Las existentes adaptadas, más: lector de cabeceras (válido, falta campo, `tipo` inválido,
verificador que delega, id inválido); `builtin_roles` con sus propiedades; render de un
papel que delega (incluye `_delegar.md` y `{papeles_delegables}`); en el árbitro, un papel
propio (`security_auditor`, verificador, etapa 4) en el equipo que bloquea la etapa 5 hasta
informar y cuyo `fail` deja la ronda sucia; `ez delegate` y `ez roles` de ida y vuelta; una
prueba que recorre los textos visibles de `ez-arbiter`, `ez-ctl`, `ez-team` y la vista y
falla si aparece «elefante», «pez» o «peces».

## Criterios de aceptación
Desde `rust/`: `cargo fmt -p ez-team -p ez-arbiter -p ez-ctl -p ez-hub`, clippy y test de
los cuatro en verde; `--demo --selfcheck` con 0. Informe en
`rust-migration/reports/ECO-R1-r1.md`.
