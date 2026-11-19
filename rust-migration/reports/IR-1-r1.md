## Resultado
- Brief/tarea: IR-1
- Estado: completado

## Cambios
- `rust/Cargo.toml`: `crates/ez-scan` en `members` y `sysinfo = "=0.40.4"` en `[workspace.dependencies]` (excepción del brief, solo esas líneas).
- `rust/crates/ez-scan/Cargo.toml`: crate nuevo con `serde`, `serde_json`, `tracing`, `ez-core`, `ez-herdr` y `sysinfo`.
- `rust/crates/ez-scan/src/lib.rs`: API `system`, `resources`, `agents`, `check_capacity` sobre el trait inyectable `Probe`.
- `rust/crates/ez-scan/src/probe.rs`: el trait de medición (todo lo no real se prueba con un doble).
- `rust/crates/ez-scan/src/system.rs`: `SystemInfo`/`GpuInfo` y la fachada de `system()`.
- `rust/crates/ez-scan/src/resources.rs`: RAM y commit libres, CPU, disco y memoria por subagente.
- `rust/crates/ez-scan/src/real.rs`: `RealProbe` con `sysinfo` (CPU, RAM, commit, disco, procesos y GPU por DXGI) y `hc`/`herdr-app` (`--version`, `ls --json --label`, `get --json`).
- `rust/crates/ez-scan/src/agents.rs`: inventario del catálogo, ruta nativa (shim de npm → `.exe` real o `.cmd`), versión con tope de 10 s, modelos y esfuerzos; nunca abre un agente.
- `rust/crates/ez-scan/src/capacity.rs`: `Thresholds` (lectura cruda de `voice.json` → `resources.*`) y `check_capacity`.
- `rust/crates/ez-scan/src/entorno.rs`: `entorno.json` (§3), escritura atómica y lo que falta declarar.
- `rust/crates/ez-scan/tests/real_system.rs`: prueba real acotada (solo números positivos; < 1 GB).
- `rust/crates/ez-ctl/Cargo.toml`: dependencia de ruta `ez-scan` (necesaria para enrutar los comandos).
- `rust/crates/ez-ctl/src/scan_cmds.rs`: `ez init [--refresh] [--json]`, `ez resources [--json]` y `ez inventory --local [--json]`.
- `rust/crates/ez-ctl/src/cli.rs`, `lib.rs`: enrutado de los tres comandos y `--refresh`/`--local` como opciones sin valor.
- `rust/crates/ez-ctl/tests/scan_init.rs`: `ez init` contra `APPDATA` temporal y `PATH` con agentes falsos (`.cmd` y guion sin extensión → `.exe` falso), más `resources`/`inventory` sin Herdr ni árbitro.

## Decisiones
- `free_commit_mb` aproxima la memoria virtual libre con RAM libre + archivo de paginación libre: `sysinfo` no expone el límite de commit de Windows.
- La guarda cuenta **todos** los subagentes vivos del padre como pesados: no hay forma local de saber si compilan o miden.
- `Thresholds::load` lee el `voice.json` crudo porque `VoiceConfig` descarta las claves que no conoce (`resources` aún no es una de ellas).
- `ez init` siempre mide y escribe; `--refresh` se acepta con el mismo efecto (lo usará IR-3 para el escaneo por antigüedad).
- Un `voice.json` ilegible no bloquea el escaneo ni el inventario: se avisa y se usan los valores de fábrica.
- La ruta nativa solo acepta un `.exe` del guion cuyo nombre se parezca al del programa; si el guion solo lanza `node.exe` (Pi), se queda el `.cmd`. Sin shell.
- **Desviación del brief:** no se añadió el crate `windows` para DXGI. `sysinfo` 0.40 ya obtiene nombre y VRAM dedicada por DXGI en Windows, con lo que se evita `unsafe` (prohibido por `_COMMON-ECO`) y una dependencia redundante.
- **Archivo no listado:** se añadió `ez-scan` a las dependencias de `ez-ctl/Cargo.toml`; sin eso los comandos nuevos no compilan.
- `max_heavy_subagents` se compara con `>=`: la guarda corre antes de lanzar, así que con 3 vivos ya no se lanza el cuarto.

## Verificación
- `cargo fmt -p ez-scan -p ez-ctl -- --check`: `fmt_exit=0` (sin diferencias).
- `cargo clippy -p ez-scan -p ez-ctl --all-targets -- -D warnings`:
  `Finished \`dev\` profile [unoptimized + debuginfo] target(s) in 2.68s`
- `cargo test -p ez-scan`:
  `test result: ok. 19 passed; 0 failed; 0 ignored` (lib)
  `test result: ok. 3 passed; 0 failed; 0 ignored` (real_system)
- `cargo test -p ez-ctl`:
  `test result: ok. 4 passed; 0 failed` (lib) · `7 passed` (cli_errors) · `9 passed` (delegate_local) · `16 passed` (local_state) · `6 passed` (roundtrip) · `2 passed` (scan_init) · `7 passed` (server_errors) · `2 passed` (waits); 0 fallos.
- `ez init --json` real, con `APPDATA`, `MINIMAX_DATA_DIR`, `CODEX_HOME` y `OPENCODE_CONFIG_DIR` temporales (PATH real; exit 0):

```json
{"ok":true,"path":"C:\\Users\\JDMoreu\\AppData\\Local\\Temp\\ez-init-real-9zfwjj\\appdata\\EzAgent\\entorno.json","entorno":{"version":1,"generated_ms":1795122501790,"system":{"os":"Windows 11 Home","os_version":"11 (26300)","cpu_model":"AMD Ryzen 5 5500GT with Radeon Graphics","physical_cores":6,"logical_cores":12,"total_ram_mb":28562,"disk_free_mb":239368,"gpus":[{"name":"AMD Radeon RX 9070","dedicated_vram_mb":16191},{"name":"AMD Radeon(TM) Graphics","dedicated_vram_mb":4079}],"hc_version":"hc 0.1.21","herdr_app_version":"herdr-app 0.1.16"},"agents":[{"key":"claude","label":"Claude Code","installed":true,"path":"C:\\Users\\JDMoreu\\.local\\bin\\claude.EXE","version":"2.1.328 (Claude Code)","models":["fable","opus","sonnet","haiku"],"efforts":["low","medium","high","xhigh","max"]},{"key":"opencode","label":"OpenCode","installed":true,"path":"C:\\Users\\JDMoreu\\AppData\\Local\\pi-node\\current/node_modules/@opencode/cli/bin/opencode.exe","version":"opencode v2.3.0","models":["deepseek-v4.1-flash","deepseek-v4-pro","glm-5.3","kimi-k3","minimax-m3.1-flash","claude-haiku-5-5"],"efforts":["low","high","max","medium","xhigh","none"]},{"key":"mcode","label":"MiniMax Code","installed":true,"path":"C:\\Users\\JDMoreu\\.minimax-code\\mcode.cmd","version":null,"models":[],"efforts":["default","low","medium","high","xhigh","max"]},{"key":"minimax","label":"MiniMax (mcode)","installed":true,"path":"C:\\Users\\JDMoreu\\.minimax-code\\mcode.cmd","version":null,"models":[],"efforts":["default","low","medium","high","xhigh","max"]},{"key":"codex","label":"Codex","installed":true,"path":"C:\\Users\\JDMoreu\\AppData\\Local\\Programs\\OpenAI\\Codex\\bin\\codex.EXE","version":"codex-cli 0.169.0","models":[],"efforts":["low","medium","high","xhigh","max","ultra"]},{"key":"pi","label":"Pi","installed":true,"path":"C:\\Users\\JDMoreu\\AppData\\Local\\pi-node\\current\\pi.cmd","version":null,"models":[],"efforts":["off","minimal","low","medium","high","xhigh","max"]},{"key":"qwen","label":"Qwen Code","installed":false,"path":null,"version":null,"models":["qwen3-coder-plus"],"efforts":[]}],"missing":["Falta declarar el plan y el límite de uso de Claude Code (planes.toml).","Falta declarar el plan y el límite de uso de OpenCode (planes.toml).","Falta declarar el plan y el límite de uso de MiniMax Code (planes.toml).","Falta declarar el plan y el límite de uso de MiniMax (mcode) (planes.toml).","Falta declarar el plan y el límite de uso de Codex (planes.toml).","Falta declarar el plan y el límite de uso de Pi (planes.toml).","Qwen Code no está instalado; instálalo o quítalo del catálogo."]}}
```

- `ez resources --json` real (`HC_TERMINAL_ID=t18q`, sin abrir nada):
  `{"ok":true,"parent":"t18q","resources":{"free_ram_mb":14075,"free_commit_mb":26875,"cpu_percent":42.63911819458008,"disk_free_mb":239373,"subagents":[]}}`
- `ez inventory --local` real: lista Claude Code, OpenCode, mcode (×2 alias), Codex, Pi y Qwen Code como no instalado; no creó `entorno.json` (la carpeta `APPDATA` temporal quedó vacía).

## Pendiente o riesgos
- IR-3 debe conectar `ez_scan::check_capacity` en `ez delegate` (no se tocó `local.rs`, HD-9) y usar `entorno.json` con la regla de 7 días; `Thresholds::load` es la vía ya probada.
- En OpenCode, `opencode models` falla en esta máquina («Background service failed to start»); el inventario se queda con el catálogo, sin romper nada.
- mcode y Pi no dan versión por `--version` sin un shell (se ejecuta su `.cmd`); se muestran como «versión desconocida» a propósito.
- El consumo de uso y límites por agente (IR-2) no se implementó aquí; `missing` solo enumera lo que falta declarar.
