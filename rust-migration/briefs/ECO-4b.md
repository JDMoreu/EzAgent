# ECO-4b — `ez-team`: correcciones de la revisión 1

**Lee `rust-migration/briefs/_COMMON-ECO.md`, `rust-migration/briefs/ECO-4.md` (la tarea
original, ya integrada), `docs/designs/espacio-elefante.md` §6 y §14 (manda §14), y el
informe del verificador `rust-migration/reports/ECO-4V-r1.md` si existe.**

## Archivos que se tocan (solo estos)
- `rust/crates/ez-team/**`, salvo los contratos `src/roles.rs`, `src/contract.rs` y
  `src/roles/*.md` (la plantilla `qa_validator.md` cambió en `eco/main`: adapta tus pruebas
  a su texto actual, no la edites).

## Cambios
1. **Perfil (§14.2).** `AgentProfile` gana `env: BTreeMap<String, String>` (tabla TOML
   `env`) y `prelaunch: Option<String>` (clave TOML `previo`; único valor admitido
   `"opencode_state"`, otro → error al cargar).
2. **Valores por omisión de `agentes.toml` (§14.2):** OpenCode con
   `argv = ["opencode", "--standalone"]`, `[agente.env]` con
   `OPENCODE_CONFIG_CONTENT = '{"model":"{modelo}{variante}"}'`, `previo = "opencode_state"`
   y `modelos_cmd = ["opencode", "models"]`; Claude con `esfuerzo_args = ["--effort", "{esfuerzo}"]`;
   Codex sin cambios. Conserva los comentarios y añade uno que explique por qué OpenCode
   no usa `--model`.
3. **Lanzamiento.** `pub struct LaunchCommand { pub argv: Vec<String>, pub env: BTreeMap<String, String>, pub prelaunch: Option<String> }`
   y `pub fn command(profile: &AgentProfile, seat: &Seat) -> LaunchCommand`, que sustituye
   `{modelo}`, `{esfuerzo}` y `{variante}` (`#<esfuerzo>` o vacío) en `argv`,
   `esfuerzo_args` (solo si hay esfuerzo) y los valores de `env`. Sustituye a `argv()`.
   Para el elefante: `pub fn elephant_first_line() -> String` con el texto exacto de §14.2.
4. **Validación (§14.2, m04).** Error si `agente` o `modelo` están vacíos (tras recortar);
   si `esfuerzo` está presente y vacío; o si hay `esfuerzo` y el perfil no puede aplicarlo
   (`esfuerzo_args` vacío y ningún valor de `env` contiene `{esfuerzo}` ni `{variante}`):
   «{Agente} no puede aplicar el esfuerzo «{esfuerzo}»: quita `esfuerzo` o configura
   `esfuerzo_args` en agentes.toml.».
5. **Familias (m03).** Pruebas con los tres ejemplos de §6 y `hy4-flash-free` → `hy`.
6. **Hallazgos del verificador** (`rust-migration/reports/ECO-4V-r1.md`, veredicto del
   elefante): corrige todos los Mayores y Menores, con estas precisiones:
   - Prueba del archivo por omisión: compara el contenido exacto con el bloque de §6 ya
     modificado por el punto 2 de este encargo.
   - Suscripción con `agentes = []`: cuenta como suscripción declarada (no habilita nada).
   - `RealSystem::run`: salida por canal con vencimiento; nunca `join()` sin límite.
   - Caché por `(profile.id, comando)`.
   - `card` **no** incluye errores: con errores la propuesta se rechaza al elefante y no
     hay tarjeta. `card` solo lleva las líneas de papel y los avisos (formato de §6).
   - Marca con `// DECISION:` cada decisión que tu informe o el de ECO-4 declaran.
   - Cada mutación que el verificador dejó sin detectar debe quedar detectada por una prueba.

## Pruebas
Las existentes adaptadas, más: carga de `env` y `previo` (y `previo` desconocido → error);
`command` para OpenCode con y sin esfuerzo (`OPENCODE_CONFIG_CONTENT` exacto), Claude con
`--effort`, Codex con `-c model_reasoning_effort=…`; cada error nuevo de validación;
`elephant_first_line`.

## Criterios de aceptación
fmt (`-p ez-team`), clippy y test de `ez-team` en verde; contratos sin cambios. Informe en
`rust-migration/reports/ECO-4b-r1.md`.
