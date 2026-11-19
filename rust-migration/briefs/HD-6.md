# HD-6 — Prueba de punta a punta de `ez delegate` con Herdr real y agentes falsos

**Lee `rust-migration/briefs/_COMMON-HD.md` y `docs/designs/subagentes-herdr.md` completo
(sobre todo §11 y §13).** Parte de `eco/main` con HD-2, HD-3, HD-4, HD-7 y HD-8
integrados.

## Archivos que se tocan (solo estos)
- Nuevo `rust/crates/ez-ctl/examples/e2e_delegate.rs` (+ módulos en
  `rust/crates/ez-ctl/examples/e2e_delegate/`) y su `[[example]]` en
  `rust/crates/ez-ctl/Cargo.toml` (con las `dev-dependencies` que necesite).
- Si encuentras un error del producto: **no lo arregles**; descríbelo con evidencia
  (salida, archivo y línea) en el informe.

## Qué hace `cargo run -p ez-ctl --example e2e_delegate`
1. Compila (o exige) `ez` y `fake-ws-agent` (`cargo build -p ez-ctl --bin ez -p ez-fakes
   --bin fake-ws-agent`) y localiza `hc` y `herdr-app`; si falta alguno o la ventana de
   Herdr no responde a `herdr-app ls`: código 2 y mensaje claro.
2. Prepara bajo `std::env::temp_dir()` (nombre único `ez-e2e-<id>`):
   - `appdata/EzAgent/voice.json` con `agents` = para cada tipo (`claude`, `opencode`,
     `mcode`, `qwen`, `codex`, `pi`) `[<fake-ws-agent>, "--echo", <registros>, "--as",
     <tipo>]`, y `hc_path`/`herdr_app_path` con las rutas reales (los perfiles quedan los
     de fábrica: así se valida contra el catálogo real).
   - `minimax/config.yaml` con un proveedor y un modelo (formato de las pruebas de
     `ez-herdr/src/prelaunch`) y su bloque `defaultModelThinking: effort:`;
     `codex/models_cache.json` con un modelo y sus niveles; `opencode-config/` vacía.
   - Una carpeta de trabajo con `git init` y un commit.
3. Padre: `herdr-app do space.new --name ez-e2e-<id> --folder <trabajo> --argv
   '[<fake-ws-agent>, "--echo", <registros>, "--as", "padre"]' --focus false` (queda con
   panel propio). Guarda antes el foco actual (`herdr-app status`) para restaurarlo.
4. Delegaciones: ejecuta el binario `ez` (no la biblioteca) con `HC_TERMINAL_ID=<padre>`,
   `APPDATA`, `MINIMAX_DATA_DIR`, `CODEX_HOME` y `OPENCODE_CONFIG_DIR` temporales y sin
   `EZ_URL`/`EZ_TOKEN`, y `--json`:
   | papel | --agent | --model | --effort | --show |
   |---|---|---|---|---|
   | planner | claude | sonnet | low | tree |
   | developer | opencode | deepseek-v4.1-flash | high | tab |
   | developer | qwen | qwen3-coder-plus | — | right |
   | qa | codex | (el del caché) | (un nivel suyo) | tree |
   | qa_validator | mcode | (el del config) | high | tab |
   | reviewer | pi | proveedor/libre | medium | tree |
   Cada `--brief` es un archivo con una marca única (`E2E-<id>-<n>`).
5. Comprobaciones por hijo (cada una con PASS/FAIL en la salida):
   `hc get`: `parent` = padre (también **después** de ubicar), `name` = §6
   (`plan1-claude`, `dev1-opencode`, `dev2-qwen`, `qa1-codex`, `valqa1-mcode`,
   `reviewer1-pi`), etiquetas §3. Registro del agente falso: el primer mensaje contiene
   «Eres un subagente de EzAgent con el papel «<papel>»», la terminal del padre y la marca
   del brief; argv/entorno de su tipo: Claude Code `--model sonnet --effort low`; OpenCode
   `OPENCODE_CONFIG_CONTENT` con `opencode-go/deepseek-v4.1-flash#high` y `--standalone`;
   Qwen Code `--model qwen3-coder-plus`; Codex `--model <m>` y `-c
   model_reasoning_effort=<n>`; Pi `--model proveedor/libre --thinking medium`; mcode
   `--model <proveedor/modelo>` y `effort: high` en el `config.yaml` **temporal**.
   `herdr-app ls`: el padre tiene a los seis en `sub_agents`; `dev1-opencode` y
   `valqa1-mcode` con pestaña propia; `dev2-qwen` en la pestaña del padre; salida de `ez`
   con `placed` correcto.
6. Rechazos (código 1, sin terminal nueva: compara `hc ls --all` antes y después): tipo
   `nada`; Claude Code `--model gpt-5`; Claude Code `--effort ultra`; OpenCode
   `deepseek-v4-pro --effort low`; Codex modelo ausente del caché; mcode modelo ausente;
   Qwen Code con `--effort high`. Aceptado con aviso: Pi con modelo libre (ya en el paso 4).
   Uso (código 2): `--show arriba`; `--name T9`. Sin `HC_TERMINAL_ID` (código 3).
6b. **Estado y avisos (§13)**, con `EZ_SUBAGENTS_DIR` temporal en todo lo que ejecutes:
   el agente falso del tipo `claude` se lanza con `--report terminado --ez <ruta de ez>`
   (ponlo en su comando del `voice.json` de prueba) y el de `qwen` sin informe. Comprueba:
   el registro de `plan1-claude` pasa a `terminado` y el **padre falso recibe** (en sus
   `lines`) el aviso «[EzAgent · aviso] plan1-claude (Claude Code · sonnet) terminó…» sin
   que nadie lo pida; `ez validate plan1-claude --pass --summary ok` ejecutado con
   `HC_TERMINAL_ID` = otro hijo del padre → `validado` y aviso de validado en el padre;
   `hc kill` de `dev2-qwen` → `fallido` («la terminal terminó sin informe») y aviso en el
   padre en menos de 15 s; `ez tasks --json` (con `HC_TERMINAL_ID` = padre) muestra
   `validado`, `fallido` y `en_proceso`; el vigilante `ez-watch-…` existe una sola vez.
7. Limpieza siempre (también si falla), incluido el vigilante `ez-watch-…`: mata y borra con `hc` toda terminal con
   `ez.parent=<padre>` y el padre; cierra el espacio (`space.close --id <id> --confirmed
   true`); devuelve el foco guardado (`pane.focus`); borra la carpeta temporal. Comprueba
   que `herdr-app ls` ya no tiene `ez-e2e-<id>` ni sus terminales.
8. Salida: tabla PASS/FAIL y un JSON con todo en `<temp>/ez-e2e-<id>.json` (copiado a
   `rust-migration/reports/HD-6-e2e.json`). Código 0 solo si todo pasó.

## Criterios de aceptación
fmt y clippy de `ez-ctl` en verde, y **ejecuta** el ejemplo y pega su salida completa y su
código en `rust-migration/reports/HD-6-r1.md`. Si falla por el producto, el diagnóstico con
evidencia (no lo arregles).

## 9. Humo con los CLI reales, sin encargo (opt-in: `EZ_E2E_REAL=1`)
Los agentes falsos no detectan fallos del lanzamiento real (la revisión de HD-0b encontró
dos: el guion de bash de npm y los ids de sesión de OpenCode; HD-4 vio un OpenCode real
salir con código 1). Con `EZ_E2E_REAL=1`, para cada tipo del catálogo **instalado**
(`claude`, `opencode`, `codex`, `qwen`, `mcode`, `pi`), usa la **ruta del producto**
(`Herdr::run_agent` con `parent` = el padre falso, `show = tree` y **tarea vacía**, con el
`voice.json` real en solo lectura): abrir la interfaz sin encargo no llama a ningún
modelo. Espera hasta 30 s a que el programa arranque (`hc get`: no terminó; `status` si lo
reporta), lee la pantalla (`hc read --lines 0`) y ciérralo (`hc kill` + `hc rm`).
Resultado por tipo: arrancó / salió con código N (con las últimas líneas de pantalla) /
no instalado. **Nunca** escribas nada en esas interfaces ni les envíes mensajes. El agente
principal avisará a la persona antes de ejecutar este paso: déjalo implementado y
ejecuta solo la parte con agentes falsos; el humo real lo corre el agente principal.
