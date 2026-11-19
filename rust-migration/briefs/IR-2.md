# IR-2 — Uso y límites de cada agente (`planes.toml`, lectores y `ez usage`)

**Lee `rust-migration/briefs/_COMMON-HD.md`, `docs/designs/inventario-recursos.md` §5 y
`rust-migration/reports/IR-0-r1.md` completo (sobre todo §7, §8 y §9: son los lectores a
implementar).** Rama desde `eco/main` con IR-1 integrado (`ez-scan`).

## Archivos
`rust/crates/ez-scan/src/usage*.rs` (+ pruebas y datos de ejemplo **inventados** en
`tests/fixtures/`), y lo mínimo en `ez-scan/src/lib.rs` y en `ez-ctl/src/scan_cmds.rs`.

## Qué hace
1. **Lectores** (uno por agente, §8 de IR-0), en solo lectura (SQLite con `mode=ro` y
   `query_only`; dependencia permitida: `rusqlite` con la versión que ya tenga el
   workspace, si la hay; si no, dilo y omite mcode), tolerantes a formatos que cambian
   (un campo que falta = `desconocido`, nunca adivinar):
   - Codex: `used_percent`, `window_minutes`, `resets_at` y `plan_type` de la sesión más
     reciente → estado de ventana real.
   - Claude Code, OpenCode, mcode, Pi: tokens y costo en la ventana (5 h por omisión, o la
     de `planes.toml`) y avisos de agotado si existen (log de OpenCode con su cuenta atrás).
   - Además, los agotamientos que anote el vigilante en `uso.json` (NV-1).
2. **`planes.toml`** (`%APPDATA%\EzAgent\planes.toml`): por tipo, `plan`, `ventana_horas`,
   `limite` (tokens o mensajes, opcional), `prioridad` (`primero` | `normal` |
   `reservar`), y `[novedades] reintento_automatico`. Sin archivo, valores vacíos.
   `ez init` (IR-1) propone uno a partir de lo detectado y **no** lo sobrescribe si existe.
3. **`ez usage [--json]`**: por agente, uso en la ventana, límite (declarado u
   observado), reinicio y estado `libre` / `cerca` (≥ 80 %) / `agotado` / `desconocido`,
   con la fuente de cada dato.

## Reglas
Nunca leas `auth.json`, `.env`, credenciales ni contenidos de conversaciones: solo los
campos numéricos y de estado que nombra IR-0. Nada sale del equipo. Ninguna prueba pasa de
1 GB; las pruebas usan carpetas de usuario **falsas** (`HOME`/`USERPROFILE`/`APPDATA`
temporales con los archivos de ejemplo), nunca las reales.

## Criterios
fmt, clippy y test de `ez-scan` y `ez-ctl` en verde, y **ejecuta** `ez usage --json` una
vez en esta máquina (lectura real, sin gastar) y pega la salida en el informe
`rust-migration/reports/IR-2-r1.md`.
