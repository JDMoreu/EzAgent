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
