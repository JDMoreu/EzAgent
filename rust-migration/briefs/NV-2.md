# NV-2 — `ez retry`: retomar un subagente bloqueado o fallido

**Lee `docs/designs/novedades-subagentes.md` §4 y `inventario-recursos.md` §6.** Rama
desde `eco/main` con NV-1 e IR-3 integrados.

## Qué hace
`ez retry <nombre> [--agent T] [--model M] [--effort E] [--auto] [--same]` en el modo
local de `ez-ctl` (lógica en `ez-herdr`): mismo encargo (`brief` guardado en el registro:
añade el campo `brief_path` o el texto completo al registro si hace falta), mismo padre,
nombre `<nombre>-r<n>`, **sesión nueva** si cambia tipo o modelo (para OpenCode: sin
`--session` previa), primer mensaje con la nota de §4, registro viejo → `fallido`
(«reemplazado por …») y nuevo con `retry_of`. `--auto` usa la recomendación excluyendo
lo agotado de `uso.json`. Reintento automático solo con `planes.toml`
`[novedades] reintento_automatico = true` (lo dispara el vigilante tras un `bloqueado`
por límite).

## Pruebas
Runtime falso: retry manual, `--same`, `--auto` (evita el agotado), nombre `-r2` si ya
existe `-r1`, registro enlazado, política automática encendida y apagada.

## Criterios
fmt, clippy y test de `ez-herdr` y `ez-ctl` en verde. Informe `NV-2-r1.md`.

## Adenda del agente principal (antes de lanzar, 19-nov tarde)
- **Lee también** `_COMMON-HD.md`, `_COMMON-ECO.md` y `docs/designs/subagentes-herdr.md` §13.
- **Archivos que se tocan**: `ez-herdr/src/subagents.rs` (campos `brief_path`/`retry_of`
  con `#[serde(default)]`: los registros viejos deben seguir leyéndose), un módulo nuevo
  `ez-herdr/src/subagents/retry*.rs`, `ez-herdr/src/subagents/watch*.rs` solo para el
  disparo automático, `ez-ctl/src/cli.rs` (ruta del comando), `ez-ctl/src/local/retry.rs`
  (nuevo) y sus pruebas.
- **Dependencias**: `ez-scan` depende de `ez-herdr`; la lógica de `ez-herdr` no puede llamar
  a `ez-scan`. `--auto` se resuelve en `ez-ctl` con `ez_scan::recommend` (IR-3) y se pasa
  la elección ya hecha a `ez-herdr`; para el disparo automático del vigilante, inyecta un
  rasgo (como `MemoryProbe` de IR-3) que `ez watch` implementa.
- **El agotamiento es por modelo** (IR-2b, IR-3b): `--auto` tras «Free limit reached» del
  modelo gratuito puede elegir otro modelo **del mismo agente** (p. ej. OpenCode Go).
- **La terminal vieja**: si sigue viva, `ez retry` la cierra **después** de que la nueva
  arranque bien (libera memoria) y lo dice en la salida; `--keep` la conserva. Nunca la
  cierra si la nueva no arrancó.
- **Prueba de punta a punta** con `ez-fakes` (`fake_ws_agent`) y `hc` real como en HD-6:
  un hijo falso que muestra «Free limit reached» → el vigilante lo deja `bloqueado` → `ez
  retry <n> --auto` → nace `<n>-r1` bajo el mismo padre con otro modelo, el viejo queda
  `fallido` («reemplazado por <n>-r1») y su terminal cerrada. Sin agentes reales.
- Compila en primer plano y espera el resultado.
