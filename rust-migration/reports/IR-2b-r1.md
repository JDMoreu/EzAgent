# IR-2b — Corrección de OpenCode: tabla actual, WAL y avisos por plan

## Resultado
- Brief/tarea: IR-2b
- Estado: completado

## Cambios
- `rust/crates/ez-scan/src/usage/opencode.rs`: lee la tabla más reciente (`session_message` en v2.3.0; `message` como respaldo), agrega tokens/costo **por modelo**, y evalúa los avisos de límite por plan en vez de agotar el agente entero.
- `rust/crates/ez-scan/src/usage/sqlite.rs`: apertura común en solo lectura por URI `file:///…?mode=ro` con `SQLITE_OPEN_READ_ONLY | SQLITE_OPEN_URI`; nunca `immutable=1`, para ver el WAL. `has_table`/`max_time` para elegir el esquema.
- `rust/crates/ez-scan/src/usage/mcode.rs`: `GROUP BY model`; tokens y costo por modelo.
- `rust/crates/ez-scan/src/usage/claude.rs` y `usage/pi.rs`: desglose por `message.model`.
- `rust/crates/ez-scan/src/usage.rs`: `ModelUsage` y `AgentUsage.models`; el informe conserva el resto igual.
- `rust/crates/ez-ctl/src/scan_cmds.rs`: la salida de texto muestra la línea `modelos:` cuando hay desglose.
- `rust/crates/ez-scan/src/usage/opencode/tests.rs`: pruebas con el esquema actual, el viejo, ambos, Free + Go posterior, aviso Go vencido y plan reciente.
- `rust/crates/ez-scan/src/usage/*/tests.rs` y `tests/usage_readers.rs`: aserciones por modelo; el fixture de integración crea la base de OpenCode con el esquema actual.
- `rust/crates/ez-scan/tests/fixtures/usage/home/.local/share/opencode/log/opencode.log`: se retiró; el registro se escribe en la carpeta temporal de la prueba.

## Decisiones
- **Esquema:** en la base real `session_message` tiene 37 227 filas y su `MAX(time_created)` es de hoy; `message` es la tabla vieja (8 934 filas, 24-oct). Se elige la tabla con el `MAX(time_created)` mayor y se queda `message` como respaldo para versiones anteriores. No se leyó contenido de mensajes: solo `sqlite_master`, `PRAGMA table_info`, agregados numéricos y campos `model`/`providerID`.
- **Solo lectura con WAL:** `mode=ro` por URI y sin `immutable`, como pide el brief. Verificado contra la base real: `ez usage` ve el uso de hoy que está en el `-wal`.
- **Avisos por plan:** `Free` agota los modelos `*-free`; `OpenCode Go` agota `opencode-go/*`. Un aviso con reinicio deja de contar al pasar ese reinicio; sin reinicio, `Free` dura 24 h y el resto la ventana de `planes.toml` (5 h por omisión). Si hay uso posterior del mismo plan, el aviso tampoco cuenta.
- **Agente agotado:** solo si todos los modelos con uso en la ventana y los usados en los últimos 7 días están cubiertos por avisos vigentes. Si solo lo está el gratuito, el modelo queda `agotado` y el agente no. Los modelos usados en 7 días solo alimentan esa decisión; el desglose visible es el de la ventana.
- **Plan:** se toma del modelo con más tokens de la ventana (`OpenCode Go` en esta máquina), no del último aviso.
- **Antigüedad:** el aviso de «no se actualiza» se calcula con el `MAX(time_created)` de la tabla elegida, no con `session.time_updated` (que era el error de las 623 h).
- **Costo:** OpenCode guarda `cost` por mensaje; en esta máquina el plan Go lo deja en 0.0. No se inventa.

## Verificación
- `cargo fmt -p ez-scan -p ez-ctl --check`: sin cambios (`fmt_exit=0`).
- `cargo clippy -p ez-scan -p ez-ctl --all-targets -- -D warnings`: `Finished` sin avisos.
- `cargo test -p ez-scan`: 59 unitarias + 3 de `real_system` + 1 de `usage_readers`; 0 fallos.
  ```
  test result: ok. 59 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.16s
  test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.87s
  test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.04s
  ```
- `cargo test -p ez-ctl`: 56 pruebas entre unidad e integración; 0 fallos.
  ```
  test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
  test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
  test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.06s
  test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.12s
  test result: ok. 16 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.06s
  test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.05s
  test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.95s
  test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.06s
  test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.13s
  test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
  ```
- `ez usage --json` real en esta máquina (solo lectura, código 0, sin stderr):

```json
{"ok":true,"planes":"C:\\Users\\JDMoreu\\AppData\\Roaming\\EzAgent\\planes.toml","usage":{"generated_ms":1795127470819,"agents":[{"key":"claude","label":"Claude Code","installed":true,"window_hours":5.0,"used_percent":null,"observed_window_hours":null,"tokens":{"input":1240,"output":578488,"cache_read":249166415,"cache_write":903779,"total":250649922},"messages":560,"cost_usd":null,"models":[{"model":"claude-opus-5-5","tokens":{"input":1240,"output":578488,"cache_read":249166415,"cache_write":903779,"total":250649922},"messages":560,"cost_usd":null,"state":"desconocido"}],"limit":null,"resets_ms":null,"reset_text":null,"plan":"claude_pro","priority":null,"state":"desconocido","observed_ms":1795127468779,"sources":{"state":"sin porcentaje ni límite declarado que comparar","tokens":"message.usage de 1 sesión(es) reciente(s) de Claude Code","messages":"registros con usage de Claude Code","cost":null,"limit":null,"reset":null,"plan":"oauthAccount.organizationType de ~/.claude.json"},"warnings":[]},{"key":"opencode","label":"OpenCode","installed":true,"window_hours":5.0,"used_percent":null,"observed_window_hours":null,"tokens":{"input":2486111,"output":364568,"cache_read":194864512,"cache_write":0,"total":197715191},"messages":1081,"cost_usd":0.0,"models":[{"model":"opencode-go/deepseek-v4.1-flash","tokens":{"input":2486111,"output":364568,"cache_read":194864512,"cache_write":0,"total":197715191},"messages":1081,"cost_usd":0.0,"state":"desconocido"}],"limit":null,"resets_ms":null,"reset_text":null,"plan":"OpenCode Go","priority":null,"state":"desconocido","observed_ms":1795127468483,"sources":{"state":"sin porcentaje ni límite declarado que comparar","tokens":"tokens de session_message en opencode.db (SQLite en solo lectura)","messages":"filas de session_message en la ventana","cost":"cost de session_message en opencode.db","limit":null,"reset":null,"plan":"modelo en uso en la ventana o aviso de límite de OpenCode"},"warnings":["Aviso vigente de Free: agota los modelos gratuitos (*-free).","El aviso de OpenCode Go ya no cuenta: reinicio vencido o hubo uso posterior."]},{"key":"mcode","label":"MiniMax Code","installed":true,"window_hours":5.0,"used_percent":null,"observed_window_hours":null,"tokens":{"input":1523369,"output":542669,"cache_read":156169706,"cache_write":0,"total":158235744},"messages":955,"cost_usd":0.0,"models":[{"model":"minimax/MiniMax-M3.1-Flash","tokens":{"input":1523369,"output":542669,"cache_read":156169706,"cache_write":0,"total":158235744},"messages":955,"cost_usd":0.0,"state":"desconocido"}],"limit":null,"resets_ms":null,"reset_text":null,"plan":null,"priority":null,"state":"desconocido","observed_ms":1795116712791,"sources":{"state":"sin porcentaje ni límite declarado que comparar","tokens":"local_runtime_token_usage de ~/.minimax (SQLite en solo lectura)","messages":"filas de local_runtime_token_usage","cost":"cost_usd de local_runtime_token_usage","limit":null,"reset":null,"plan":null},"warnings":[]},{"key":"codex","label":"Codex","installed":true,"window_hours":5.0,"used_percent":7.0,"observed_window_hours":5.0,"tokens":null,"messages":null,"cost_usd":null,"models":[],"limit":{"unit":"percent","value":100.0},"resets_ms":1794952777000,"reset_text":"2026-11-17 16:59","plan":"plus","priority":null,"state":"desconocido","observed_ms":1794936359305,"sources":{"state":"El dato de Codex es de hace 2 días y su ventana ya reinició; no lo presento como actual.","tokens":"rate_limits no da tokens; da porcentaje","messages":null,"cost":null,"limit":"observado: rate_limits.used_percent (Codex)","reset":"resets_at de rate_limits (epoch en segundos)","plan":"plan_type de rate_limits (Codex)"},"warnings":["El dato de Codex es de hace 2 días y su ventana ya reinició; no lo presento como actual.","Último dato de cuota: hace 2 días, en 1 archivo(s) revisado(s)."]},{"key":"pi","label":"Pi","installed":true,"window_hours":5.0,"used_percent":null,"observed_window_hours":null,"tokens":{"input":0,"output":0,"cache_read":0,"cache_write":0,"total":0},"messages":0,"cost_usd":null,"models":[],"limit":null,"resets_ms":null,"reset_text":null,"plan":null,"priority":null,"state":"desconocido","observed_ms":null,"sources":{"state":"sin porcentaje ni límite declarado que comparar","tokens":"message.usage de 0 sesión(es) reciente(s) de Pi","messages":"registros con usage de Pi","cost":null,"limit":null,"reset":null,"plan":null},"warnings":[]},{"key":"qwen","label":"Qwen Code","installed":false,"window_hours":5.0,"used_percent":null,"observed_window_hours":null,"tokens":null,"messages":null,"cost_usd":null,"models":[],"limit":null,"resets_ms":null,"reset_text":null,"plan":null,"priority":null,"state":"desconocido","observed_ms":null,"sources":{"state":"sin porcentaje ni límite declarado que comparar","tokens":null,"messages":null,"cost":null,"limit":null,"reset":null,"plan":null},"warnings":["Qwen Code no está instalado; no hay uso que leer."]}],"warnings":[]}}
```

Resumen de la lectura real (OpenCode):

| Dato | Valor |
|---|---|
| Estado | `desconocido` (no `agotado`) |
| Plan | `OpenCode Go` |
| Tokens en la ventana (5 h) | 197 715 191 |
| Mensajes | 1 081 |
| Modelo | `opencode-go/deepseek-v4.1-flash` (197 715 191 tokens, `desconocido`) |
| Avisos | Free vigente para `*-free`; Go ya no cuenta (reinicio vencido o uso posterior) |
| Base | `session_message`, leída en `mode=ro`; sin aviso de «no se actualiza» |

## Pendiente o riesgos
- El desglose visible solo incluye modelos con uso en la ventana; los usados en los últimos 7 días se consultan únicamente para decidir si el agente entero está agotado.
- Si una versión futura de OpenCode cambia otra vez de tabla, el lector elegirá la más reciente entre las dos que conoce; una tabla nueva con otro nombre quedaría como «base sin tablas conocidas» y el uso sería `desconocido` (nunca un valor inventado).
- El costo de OpenCode es el que guarda la propia base (0.0 para el plan Go en esta máquina); no se deduce de tokens.
- `planes.toml` no existe en esta máquina: se usó la ventana de 5 h por omisión.
