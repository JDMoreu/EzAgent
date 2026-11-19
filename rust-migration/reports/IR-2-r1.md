# IR-2 — Uso y límites de cada agente (`planes.toml`, lectores y `ez usage`)

## Resultado
- Brief/tarea: IR-2
- Estado: completado

## Cambios
- `rust/crates/ez-scan/src/usage.rs`: tipos públicos (`UsageReport`, `AgentUsage`, `UsageState`, `UsageLimit`, `Sources`), carpetas (`UsageDirs`), ventana y ensamblado de lo declarado con lo detectado.
- `rust/crates/ez-scan/src/usage/planes.rs`: lectura tolerante de `planes.toml`, prioridad y propuesta que no sobrescribe.
- `rust/crates/ez-scan/src/usage/codex.rs`: último `rate_limits` de las sesiones de Codex (`used_percent`, `window_minutes`, `resets_at`, `plan_type`) con aviso de dato viejo.
- `rust/crates/ez-scan/src/usage/claude.rs`: suma de `message.usage` en la ventana, `fallback_credit` y plan (`organizationType`); solo esos campos.
- `rust/crates/ez-scan/src/usage/opencode.rs`: tokens/costo de `opencode.db` en solo lectura y aviso de límite del registro con su cuenta atrás.
- `rust/crates/ez-scan/src/usage/mcode.rs`: `local_runtime_token_usage` de MiniMax Code en solo lectura.
- `rust/crates/ez-scan/src/usage/pi.rs`: `message.usage` (tokens y costo) de las sesiones de Pi.
- `rust/crates/ez-scan/src/usage/depleted.rs`: agotamientos que anotó el vigilante en `uso.json` (NV-1).
- `rust/crates/ez-scan/src/usage/jsonl.rs`: fechas, recorrido de `.jsonl` por antigüedad y cola de archivos grandes.
- `rust/crates/ez-scan/src/usage/tests.rs` y `usage/*/tests.rs`: pruebas de cada lector, del estado y de `planes.toml`.
- `rust/crates/ez-scan/tests/usage_readers.rs` y `tests/fixtures/usage/**`: prueba de punta a punta con una carpeta de usuario inventada (fechas de 2026-01-02; ningún dato real).
- `rust/crates/ez-scan/src/lib.rs`: exporta `usage` y `usage_report`.
- `rust/crates/ez-scan/Cargo.toml`: `rusqlite`, `toml` y `chrono` del workspace (los dos últimos, para leer `planes.toml` y las fechas).
- `rust/crates/ez-ctl/src/scan_cmds.rs`: `ez usage [--json]` y la propuesta de `planes.toml` al final de `ez init`.
- `rust/crates/ez-ctl/src/cli.rs`: **desviación mínima de alcance** (2 líneas): el brazo `"usage"` y su línea en `--help`; sin eso el comando no es alcanzable. Se explica abajo.
- `rust/Cargo.lock`: solo el efecto de compilar el crate.

## Decisiones
- `planes.toml` usa una tabla de primer nivel por tipo (`[codex]`, `[claude]`, …) más `[novedades]`, y `unidad = "tokens" | "mensajes"` para desambiguar `limite` (el brief no fija el formato). Lo declarado gana sobre lo detectado.
- OpenCode no tiene variable de entorno para su carpeta de datos: se admite `OPENCODE_DATA_DIR` solo para las pruebas (`UsageDirs::from_lookup`), sin tocar la carpeta real.
- Los lectores degradan a `desconocido` ante un campo que falta; nunca adivinan un límite. Si un TOML de planes tiene una prioridad o unidad inválida, se avisa y se ignora ese campo.
- Codex: si `resets_at` ya pasó, el porcentaje no se presenta como actual; el estado queda `desconocido` y se indica la antigüedad del dato. Es el caso de esta máquina (dato de hace 2 días).
- OpenCode: se lee la base en modo `ro` en vez de ejecutar `opencode stats`, porque el brief prohíbe lanzar agentes reales. En esta máquina la base no se actualiza desde hace 623 h; el lector lo avisa y el aviso `Free` del registro marca `agotado`.
- Claude Code no guarda costo por token en disco (`additionalModelCostsCache` son contadores, no precios): `cost_usd` queda en `null` en vez de inventarlo.
- El catálogo tiene `mcode` y `minimax` apuntando al mismo programa: `ez usage` informa una sola vez; `planes.toml` acepta `[minimax]` como alias de `mcode`.
- `ez init` crea `planes.toml` con `create_new` y no sobrescribe nada existente; si falla, avisa y `ez init` termina igual.
- `crates/ez-ctl/src/cli.rs` no estaba en la lista de archivos, pero el enlace de `"usage"` vive ahí. Se tocó lo mínimo (una línea en el `match` y otra en la ayuda) para que `ez usage --json` se pueda ejecutar, como pide el criterio del brief.

## Verificación
- `cargo fmt -p ez-scan -p ez-ctl --check`: sin cambios (`fmt_exit=0`).
- `cargo clippy -p ez-scan -p ez-ctl --all-targets -- -D warnings`: `Finished` sin avisos.
- `cargo test -p ez-scan`: 52 unitarias + 3 de `real_system` + 1 de `usage_readers`; 0 fallos.
  ```
  test result: ok. 52 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.06s
  test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.89s
  test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
  ```
- `cargo test -p ez-ctl`: 56 pruebas entre unidad e integración; 0 fallos. Líneas finales de cada binario, en orden:
  ```
  test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
  test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
  test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.05s
  test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.13s
  test result: ok. 16 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.06s
  test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.07s
  test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.08s
  test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.05s
  test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.13s
  test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
  ```
- `ez init` con `APPDATA`/`USERPROFILE`/`LOCALAPPDATA`/`MINIMAX_DATA_DIR`/`CODEX_HOME` falsos: creó `planes.toml` la primera vez y no lo tocó en las siguientes (se comprobó con una marca escrita a mano). No se ejecutó `ez init` sobre la configuración real.
- `ez usage --json` real en esta máquina (solo lectura, sin gastar cuota; código de salida 0, sin stderr):

```json
{"ok":true,"planes":"C:\\Users\\JDMoreu\\AppData\\Roaming\\EzAgent\\planes.toml","usage":{"generated_ms":1795125985332,"agents":[{"key":"claude","label":"Claude Code","installed":true,"window_hours":5.0,"used_percent":null,"observed_window_hours":null,"tokens":{"input":1084,"output":514968,"cache_read":233687764,"cache_write":824788,"total":235028604},"messages":493,"cost_usd":null,"limit":null,"resets_ms":null,"reset_text":null,"plan":"claude_pro","priority":null,"state":"desconocido","observed_ms":1795125933301,"sources":{"state":"sin porcentaje ni límite declarado que comparar","tokens":"message.usage de 1 sesión(es) reciente(s) de Claude Code","messages":"registros con usage de Claude Code","cost":null,"limit":null,"reset":null,"plan":"oauthAccount.organizationType de ~/.claude.json"},"warnings":[]},{"key":"opencode","label":"OpenCode","installed":true,"window_hours":5.0,"used_percent":null,"observed_window_hours":null,"tokens":{"input":0,"output":0,"cache_read":0,"cache_write":0,"total":0},"messages":0,"cost_usd":0.0,"limit":null,"resets_ms":null,"reset_text":null,"plan":"Free","priority":null,"state":"agotado","observed_ms":null,"sources":{"state":"aviso de límite de OpenCode en el registro (Free)","tokens":"message.tokens de opencode.db (SQLite en solo lectura)","messages":"filas de message en la ventana","cost":"message.cost de opencode.db","limit":null,"reset":null,"plan":"mensaje de límite del registro de OpenCode"},"warnings":["La base de OpenCode no se actualiza desde hace 623 h; puede no reflejar el uso de hoy."]},{"key":"mcode","label":"MiniMax Code","installed":true,"window_hours":5.0,"used_percent":null,"observed_window_hours":null,"tokens":{"input":1742281,"output":633701,"cache_read":211711458,"cache_write":0,"total":214087440},"messages":1171,"cost_usd":0.0,"limit":null,"resets_ms":null,"reset_text":null,"plan":null,"priority":null,"state":"desconocido","observed_ms":1795116712791,"sources":{"state":"sin porcentaje ni límite declarado que comparar","tokens":"local_runtime_token_usage de ~/.minimax (SQLite en solo lectura)","messages":"filas de local_runtime_token_usage","cost":"cost_usd de local_runtime_token_usage","limit":null,"reset":null,"plan":null},"warnings":[]},{"key":"codex","label":"Codex","installed":true,"window_hours":5.0,"used_percent":7.0,"observed_window_hours":5.0,"tokens":null,"messages":null,"cost_usd":null,"limit":{"unit":"percent","value":100.0},"resets_ms":1794952777000,"reset_text":"2026-11-17 16:59","plan":"plus","priority":null,"state":"desconocido","observed_ms":1794936359305,"sources":{"state":"El dato de Codex es de hace 2 días y su ventana ya reinició; no lo presento como actual.","tokens":"rate_limits no da tokens; da porcentaje","messages":null,"cost":null,"limit":"observado: rate_limits.used_percent (Codex)","reset":"resets_at de rate_limits (epoch en segundos)","plan":"plan_type de rate_limits (Codex)"},"warnings":["El dato de Codex es de hace 2 días y su ventana ya reinició; no lo presento como actual.","Último dato de cuota: hace 2 días, en 1 archivo(s) revisado(s)."]},{"key":"pi","label":"Pi","installed":true,"window_hours":5.0,"used_percent":null,"observed_window_hours":null,"tokens":{"input":0,"output":0,"cache_read":0,"cache_write":0,"total":0},"messages":0,"cost_usd":null,"limit":null,"resets_ms":null,"reset_text":null,"plan":null,"priority":null,"state":"desconocido","observed_ms":null,"sources":{"state":"sin porcentaje ni límite declarado que comparar","tokens":"message.usage de 0 sesión(es) reciente(s) de Pi","messages":"registros con usage de Pi","cost":null,"limit":null,"reset":null,"plan":null},"warnings":[]},{"key":"qwen","label":"Qwen Code","installed":false,"window_hours":5.0,"used_percent":null,"observed_window_hours":null,"tokens":null,"messages":null,"cost_usd":null,"limit":null,"resets_ms":null,"reset_text":null,"plan":null,"priority":null,"state":"desconocido","observed_ms":null,"sources":{"state":"sin porcentaje ni límite declarado que comparar","tokens":null,"messages":null,"cost":null,"limit":null,"reset":null,"plan":null},"warnings":["Qwen Code no está instalado; no hay uso que leer."]}],"warnings":[]}}
```

Resumen de la lectura real:

| Agente | Ventana | Uso observado | Estado | Reinicio | Fuente |
|---|---|---|---|---|---|
| Claude Code | 5 h | 235,0 M tokens · 493 mensajes | desconocido | — | `message.usage` |
| OpenCode | 5 h | 0 tokens (base sin actualizar) | agotado | — | aviso `Free` del registro |
| MiniMax Code | 5 h | 214,1 M tokens · 1171 mensajes · 0 USD | desconocido | — | SQLite de mcode |
| Codex | 5 h | 7,0 % observado (dato de hace 2 días) | desconocido | 2026-11-17 16:59 (ya pasó) | `rate_limits` |
| Pi | 5 h | 0 tokens | desconocido | — | `message.usage` |
| Qwen Code | 5 h | — | desconocido | — | no instalado |

## Pendiente o riesgos
- Claude Code, MiniMax Code y Pi **no guardan porcentaje de ventana ni límite** en disco: sin un `limite` declarado en `planes.toml` su estado será siempre `desconocido`, por diseño. Es lo que pide IR-0 §9.
- En esta máquina la base de OpenCode no se actualiza desde el 2026-10-24; el lector lo avisa. El estado `agotado` sale del último aviso `Free` del registro, no de tokens.
- El costo de Claude Code queda en `null`: no hay precios locales verificados. Los costos de OpenCode/mcode/Pi salen de sus propios campos.
- La propuesta de `planes.toml` solo incluye agentes instalados y no adivina límites; `[novedades].reintento_automatico` se lee pero todavía nadie lo usa (IR-3).
- `uso.json` se busca en `EZ_SUBAGENTS_DIR` o `%LOCALAPPDATA%\EzAgent\subagentes`, la misma raíz que usa NV-1; no se probó contra un archivo real del vigilante, solo con entradas inventadas en las pruebas. Una entrada sin `hasta` se respeta mientras siga en el archivo: el vigilante la escribe cuando ve el diálogo de límite y no la borra sola.
- Desviación de alcance: `crates/ez-ctl/src/cli.rs` (2 líneas) fuera de la lista del brief, necesaria para que `ez usage` exista; ver «Decisiones».
