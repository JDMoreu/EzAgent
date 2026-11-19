# HD-6 — Prueba de punta a punta de `ez delegate` con Herdr real y agentes falsos

- Rama: `work/HD-6` (copia de `eco/main`)
- Ejemplo: `cargo run -p ez-ctl --example e2e_delegate`
- Corrida definitiva: `ez-e2e-20436` — **126 comprobaciones: 126 pasan, 0 fallan**, código de salida **0**

## Resultado
- Brief/tarea: HD-6
- Estado: completado

## Cambios
- `rust/crates/ez-ctl/examples/e2e_delegate.rs`: raíz del ejemplo; incluye los 10 módulos con `#[path = "e2e_delegate/…"]` y el `main` que arma el sandbox, el padre falso, las seis delegaciones, las comprobaciones, la limpieza y el JSON final.
- `rust/crates/ez-ctl/examples/e2e_delegate/proc.rs`: ejecución de procesos sin `shell`, con tiempo límite y captura de salida; ningún lock se retiene durante la espera.
- `rust/crates/ez-ctl/examples/e2e_delegate/setup.rs`: sandbox temporal (`appdata/EzAgent/voice.json`, `minimax/config.yaml`, `codex/models_cache.json`, `opencode-config/`, repo git con commit) y entorno temporal (`APPDATA`, `LOCALAPPDATA`, `MINIMAX_DATA_DIR`, `CODEX_HOME`, `OPENCODE_CONFIG_DIR`, `EZ_SUBAGENTS_DIR`, `EZ_HC_PATH`).
- `rust/crates/ez-ctl/examples/e2e_delegate/checks.rs`: tabla PASS/FAIL y accumulator del JSON de resultado.
- `rust/crates/ez-ctl/examples/e2e_delegate/proxy.rs`: modo lanzador: cuando el agente falso invoca `ez report`, el ejemplo corre el `ez.exe` real con el entorno temporal (el de `hc run` no lo propaga) y reintenta hasta que exista el registro.
- `rust/crates/ez-ctl/examples/e2e_delegate/children.rs`: comprobaciones por hijo (`hc get`: padre, nombre, etiquetas §3; registro del agente falso: primer mensaje, terminal del padre, marca del brief; argv y entorno por tipo).
- `rust/crates/ez-ctl/examples/e2e_delegate/placement.rs`: ubicación de los hijos (`tree`, `tab`, `right`) y comprobación de `herdr-app ls` (`sub_agents`, pestañas propias, pestaña del padre).
- `rust/crates/ez-ctl/examples/e2e_delegate/rejects.rs`: rechazos del catálogo (código 1), usos incorrectos (código 2) y falta de `HC_TERMINAL_ID` (código 3), comparando `hc ls --all` antes y después para comprobar que no se crea ninguna terminal.
- `rust/crates/ez-ctl/examples/e2e_delegate/state.rs`: estado y avisos de §13 (`terminado` → `validado`, fallo por `hc kill` → `fallido`, `ez tasks --json`, vigilante único).
- `rust/crates/ez-ctl/examples/e2e_delegate/cleanup.rs`: limpieza siempre (hijos, vigilante, padre, `space.close`, foco restaurado, carpeta temporal) y comprobación de que no queda nada de la prueba en Herdr.
- `rust/crates/ez-ctl/examples/e2e_delegate/smoke.rs`: humo real opt-in con `EZ_E2E_REAL=1` (§9); **implementado pero no ejecutado** en esta corrida.
- `rust/crates/ez-ctl/Cargo.toml`: `[[example]] name = "e2e_delegate"` (sin `dev-dependencies` nuevas: usa `ez-core`, `ez-herdr` y `serde_json`, ya presentes).
- `rust-migration/reports/HD-6-e2e.json`: JSON de la corrida definitiva (`"ok": true`, `"failed": 0`).

Ningún archivo fuera de ese alcance fue tocado. No se corrigió ningún error del producto: los hallazgos van en «Hallazgos del producto» con evidencia.

## Decisiones (`// DECISION` en el código)
1. `e2e_delegate.rs:23` — Los módulos viven en `examples/e2e_delegate/`; como la raíz del ejemplo es `examples/e2e_delegate.rs`, Rust los buscaría en `examples/`, así que cada uno declara su ruta con `#[path]`.
2. `setup.rs:150` — El entorno del subproceso no lleva claves de API: la prueba no habla con ningún modelo.
3. `setup.rs:185` — La ruta del agente falso va con barras normales (`C:/…`) en `voice.json`: `ez-herdr` envuelve en PowerShell el comando de los agentes con variables de entorno y cita el programa entre comillas simples cuando su ruta no es «simple»; en PowerShell un programa citado no se ejecuta, así que el envoltorio no parsea. Con el catálogo de fábrica (nombres sueltos) el defecto no aparece, pero con ruta absoluta sí.
4. `setup.rs:208` — El agente de tipo `claude` declara su estado (§13) con `--report terminado --ez <ruta>`; el lanzador es el propio ejemplo, que llama a `ez` con el entorno temporal, porque `hc run` no tiene `--env` y el hijo hereda el entorno del núcleo (sin él, el `ez report` del subagente leería la configuración real de la persona).
5. `proxy.rs:53` — Si el registro del subagente todavía no existe (`ez delegate` lo escribe después de entregar el encargo), el lanzador reintenta unos segundos (hasta ~15 s): es una carrera real, no un fallo.
6. `children.rs:188` — Tras `space.new --focus false` el ejemplo hace `space.select` de su propio espacio: `terminal.keep --place tab` abre la pestaña en el espacio que está a la vista y, sin eso, la primera pestaña del hijo se abría en el espacio de la persona.
7. `rejects.rs:104` — El caso «sin `HC_TERMINAL_ID`» lleva un tipo de agente válido: si no, el comando se rechaza antes por el tipo y no se llega a la comprobación. Su código esperado es el 3 de §4, no el 2.

Desviación del brief (documentada): el `--ez` del agente de tipo `claude` apunta al lanzador del propio ejemplo en vez de al binario `ez` directo, por la limitación de entorno de `hc run` descrita en la decisión 4.

## Verificación

### `cargo fmt -p ez-ctl -- --check`
```
FMT_EXIT=0
```
(sin salida: no hay diferencias de formato)

### `cargo clippy -p ez-ctl --all-targets -- -D warnings`
```
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.64s
CLIPPY_EXIT=0
```

### `cargo test -p ez-ctl`
```
running 4 tests
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
     Running tests\cli_errors.rs (target\debug\deps\cli_errors-8e5385f72d357325.exe)
running 7 tests
test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.03s
running 9 tests
test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.12s
running 16 tests
test result: ok. 16 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.06s
running 6 tests
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.08s
     Running tests\server_errors.rs (target\debug\deps\server_errors-958e25e68a4c4a66.exe)
running 7 tests
test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.04s
running 2 tests
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.14s
running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
TEST_EXIT=0
```
51 pruebas en 8 binarios, todas en verde.

### Ejemplo de punta a punta
```
cargo build -p ez-ctl --bin ez -p ez-fakes --bin fake-ws-agent --example e2e_delegate
.\target\debug\examples\e2e_delegate.exe
```
Salida completa (copia literal, corrida `ez-e2e-20436`, salida del proceso **0**):

```
[EzAgent] prueba ez-e2e-20436
[EzAgent] resultado: C:\Users\JDMoreu\AppData\Local\Temp\ez-e2e-20436.json
[EzAgent] copia: C:\Users\JDMoreu\Documents\EzAgent-wt\HD-6\rust-migration\reports\HD-6-e2e.json
PASS  el espacio de la prueba                               espacio sp925ed1fcf958471c6ae367b9fdda000b con el padre t169
PASS  el espacio de la prueba está a la vista               código 0: {
  "ok": true
}
PASS  delegar en planner-claude                             plan1-claude (t16b) en tree
PASS  delegar en developer-opencode                         dev1-opencode (t16d) en tab; avisos: No pude arrancar el vigilante: {"error":{"hint":"pick another name, or kill the one holding it","kind":"name_taken","message":"a live terminal is already named \"ez-watch-a-t169\""}}
PASS  delegar en developer-qwen                             dev2-qwen (t16f) en right; avisos: No pude arrancar el vigilante: {"error":{"hint":"pick another name, or kill the one holding it","kind":"name_taken","message":"a live terminal is already named \"ez-watch-a-t169\""}}
PASS  delegar en qa-codex                                   qa1-codex (t16g) en tree; avisos: No pude arrancar el vigilante: {"error":{"hint":"pick another name, or kill the one holding it","kind":"name_taken","message":"a live terminal is already named \"ez-watch-a-t169\""}}
PASS  delegar en qa_validator-mcode                         valqa1-mcode (t16h) en tab; avisos: No pude arrancar el vigilante: {"error":{"hint":"pick another name, or kill the one holding it","kind":"name_taken","message":"a live terminal is already named \"ez-watch-a-t169\""}}
PASS  delegar en reviewer-pi                                reviewer1-pi (t16j) en tree; avisos: El catálogo de Pi no lista modelos; uso «proveedor/libre» sin verificar. | No pude arrancar el vigilante: {"error":{"hint":"pick another name, or kill the one holding it","kind":"name_taken","message":"a live terminal is already named \"ez-watch-a-t169\""}}
PASS  el padre de plan1-claude                              t169 (se esperaba t169)
PASS  el nombre de plan1-claude                             «plan1-claude» (se esperaba «plan1-claude»)
PASS  la etiqueta agent de plan1-claude                     «claude» (se esperaba «claude»)
PASS  la etiqueta ez.agent de plan1-claude                  «claude» (se esperaba «claude»)
PASS  la etiqueta ez.role de plan1-claude                   «planner» (se esperaba «planner»)
PASS  la etiqueta ez.model de plan1-claude                  «sonnet» (se esperaba «sonnet»)
PASS  la etiqueta ez.effort de plan1-claude                 «low» (se esperaba «low»)
PASS  el primer mensaje de plan1-claude                     el mensaje trae el papel
PASS  la terminal del padre en el mensaje de plan1-claude   el mensaje nombra a t169
PASS  la marca del encargo en plan1-claude                  el mensaje trae E2E-20436-1
PASS  el argv de plan1-claude                               ["C:/Users/JDMoreu/Documents/EzAgent-wt/HD-6/rust/target/debug/fake-ws-agent.exe", "--echo", "C:\\Users\\JDMoreu\\AppData\\Local\\Temp\\ez-e2e-20436\\registros", "--report", "terminado", "--ez", "C:\\Users\\JDMoreu\\Documents\\EzAgent-wt\\HD-6\\rust\\target\\debug\\examples\\e2e_delegate.exe", "--as", "claude", "--model", "sonnet", "--effort", "low"]
PASS  el padre de dev1-opencode                             t169 (se esperaba t169)
PASS  el nombre de dev1-opencode                            «dev1-opencode» (se esperaba «dev1-opencode»)
PASS  la etiqueta agent de dev1-opencode                    «opencode» (se esperaba «opencode»)
PASS  la etiqueta ez.agent de dev1-opencode                 «opencode» (se esperaba «opencode»)
PASS  la etiqueta ez.role de dev1-opencode                  «developer» (se esperaba «developer»)
PASS  la etiqueta ez.model de dev1-opencode                 «deepseek-v4.1-flash» (se esperaba «deepseek-v4.1-flash»)
PASS  la etiqueta ez.effort de dev1-opencode                «high» (se esperaba «high»)
PASS  el primer mensaje de dev1-opencode                    el mensaje trae el papel
PASS  la terminal del padre en el mensaje de dev1-opencode  el mensaje nombra a t169
PASS  la marca del encargo en dev1-opencode                 el mensaje trae E2E-20436-2
PASS  el argv de dev1-opencode                              ["C:\\Users\\JDMoreu\\Documents\\EzAgent-wt\\HD-6\\rust\\target\\debug\\fake-ws-agent.exe", "--echo", "C:\\Users\\JDMoreu\\AppData\\Local\\Temp\\ez-e2e-20436\\registros", "--as", "opencode", "--standalone"]
PASS  el OPENCODE_CONFIG_CONTENT de dev1-opencode           {"model":"opencode-go/deepseek-v4.1-flash#high"} (se esperaba opencode-go/deepseek-v4.1-flash#high)
PASS  el padre de dev2-qwen                                 t169 (se esperaba t169)
PASS  el nombre de dev2-qwen                                «dev2-qwen» (se esperaba «dev2-qwen»)
PASS  la etiqueta agent de dev2-qwen                        «qwen» (se esperaba «qwen»)
PASS  la etiqueta ez.agent de dev2-qwen                     «qwen» (se esperaba «qwen»)
PASS  la etiqueta ez.role de dev2-qwen                      «developer» (se esperaba «developer»)
PASS  la etiqueta ez.model de dev2-qwen                     «qwen3-coder-plus» (se esperaba «qwen3-coder-plus»)
PASS  el primer mensaje de dev2-qwen                        el mensaje trae el papel
PASS  la terminal del padre en el mensaje de dev2-qwen      el mensaje nombra a t169
PASS  la marca del encargo en dev2-qwen                     el mensaje trae E2E-20436-3
PASS  el argv de dev2-qwen                                  ["C:/Users/JDMoreu/Documents/EzAgent-wt/HD-6/rust/target/debug/fake-ws-agent.exe", "--echo", "C:\\Users\\JDMoreu\\AppData\\Local\\Temp\\ez-e2e-20436\\registros", "--as", "qwen", "--model", "qwen3-coder-plus"]
PASS  el padre de qa1-codex                                 t169 (se esperaba t169)
PASS  el nombre de qa1-codex                                «qa1-codex» (se esperaba «qa1-codex»)
PASS  la etiqueta agent de qa1-codex                        «codex» (se esperaba «codex»)
PASS  la etiqueta ez.agent de qa1-codex                     «codex» (se esperaba «codex»)
PASS  la etiqueta ez.role de qa1-codex                      «qa» (se esperaba «qa»)
PASS  la etiqueta ez.model de qa1-codex                     «gpt-5-codex» (se esperaba «gpt-5-codex»)
PASS  la etiqueta ez.effort de qa1-codex                    «medium» (se esperaba «medium»)
PASS  el primer mensaje de qa1-codex                        el mensaje trae el papel
PASS  la terminal del padre en el mensaje de qa1-codex      el mensaje nombra a t169
PASS  la marca del encargo en qa1-codex                     el mensaje trae E2E-20436-4
PASS  el argv de qa1-codex                                  ["C:/Users/JDMoreu/Documents/EzAgent-wt/HD-6/rust/target/debug/fake-ws-agent.exe", "--echo", "C:\\Users\\JDMoreu\\AppData\\Local\\Temp\\ez-e2e-20436\\registros", "--as", "codex", "--model", "gpt-5-codex", "-c", "model_reasoning_effort=medium"]
PASS  el padre de valqa1-mcode                              t169 (se esperaba t169)
PASS  el nombre de valqa1-mcode                             «valqa1-mcode» (se esperaba «valqa1-mcode»)
PASS  la etiqueta agent de valqa1-mcode                     «mcode» (se esperaba «mcode»)
PASS  la etiqueta ez.agent de valqa1-mcode                  «mcode» (se esperaba «mcode»)
PASS  la etiqueta ez.role de valqa1-mcode                   «qa_validator» (se esperaba «qa_validator»)
PASS  la etiqueta ez.model de valqa1-mcode                  «minimax/MiniMax-M3.1-Flash» (se esperaba «minimax/MiniMax-M3.1-Flash»)
PASS  la etiqueta ez.effort de valqa1-mcode                 «high» (se esperaba «high»)
PASS  el primer mensaje de valqa1-mcode                     el mensaje trae el papel
PASS  la terminal del padre en el mensaje de valqa1-mcode   el mensaje nombra a t169
PASS  la marca del encargo en valqa1-mcode                  el mensaje trae E2E-20436-5
PASS  el argv de valqa1-mcode                               ["C:/Users/JDMoreu/Documents/EzAgent-wt/HD-6/rust/target/debug/fake-ws-agent.exe", "--echo", "C:\\Users\\JDMoreu\\AppData\\Local\\Temp\\ez-e2e-20436\\registros", "--as", "mcode", "--model", "minimax/MiniMax-M3.1-Flash"]
PASS  el padre de reviewer1-pi                              t169 (se esperaba t169)
PASS  el nombre de reviewer1-pi                             «reviewer1-pi» (se esperaba «reviewer1-pi»)
PASS  la etiqueta agent de reviewer1-pi                     «pi» (se esperaba «pi»)
PASS  la etiqueta ez.agent de reviewer1-pi                  «pi» (se esperaba «pi»)
PASS  la etiqueta ez.role de reviewer1-pi                   «reviewer» (se esperaba «reviewer»)
PASS  la etiqueta ez.model de reviewer1-pi                  «proveedor/libre» (se esperaba «proveedor/libre»)
PASS  la etiqueta ez.effort de reviewer1-pi                 «medium» (se esperaba «medium»)
PASS  el primer mensaje de reviewer1-pi                     el mensaje trae el papel
PASS  la terminal del padre en el mensaje de reviewer1-pi   el mensaje nombra a t169
PASS  la marca del encargo en reviewer1-pi                  el mensaje trae E2E-20436-6
PASS  el argv de reviewer1-pi                               ["C:/Users/JDMoreu/Documents/EzAgent-wt/HD-6/rust/target/debug/fake-ws-agent.exe", "--echo", "C:\\Users\\JDMoreu\\AppData\\Local\\Temp\\ez-e2e-20436\\registros", "--as", "pi", "--model", "proveedor/libre", "--thinking", "medium"]
PASS  plan1-claude está bajo su padre                       t16b no está en sub_agents (["t16b", "t16d", "t16f", "t16g", "t16h", "t16j"])
PASS  dev1-opencode está bajo su padre                      t16d no está en sub_agents (["t16b", "t16d", "t16f", "t16g", "t16h", "t16j"])
PASS  dev2-qwen está bajo su padre                          t16f no está en sub_agents (["t16b", "t16d", "t16f", "t16g", "t16h", "t16j"])
PASS  qa1-codex está bajo su padre                          t16g no está en sub_agents (["t16b", "t16d", "t16f", "t16g", "t16h", "t16j"])
PASS  valqa1-mcode está bajo su padre                       t16h no está en sub_agents (["t16b", "t16d", "t16f", "t16g", "t16h", "t16j"])
PASS  reviewer1-pi está bajo su padre                       t16j no está en sub_agents (["t16b", "t16d", "t16f", "t16g", "t16h", "t16j"])
PASS  plan1-claude queda en el árbol                        placed=tree
PASS  dev1-opencode tiene pestaña propia                    placed=tab; pestaña tb2a6f4dfa0e508f4ad100163c1a0bb54a
PASS  dev2-qwen está en la pestaña del padre                placed=right; pestaña tb01df7d40912544d0e1ea765355b36c5c
PASS  qa1-codex queda en el árbol                           placed=tree
PASS  valqa1-mcode tiene pestaña propia                     placed=tab; pestaña tb6cccc7089eb4f473eed9edd41a32d8a2
PASS  reviewer1-pi queda en el árbol                        placed=tree
PASS  un tipo desconocido                                   código 1 (se esperaba 1): No conozco el agente «nada». Agentes del catálogo: claude, codex, mcode, minimax, opencode, pi, qwen.
PASS  un tipo desconocido no crea terminal                  24 antes, 24 después
PASS  un modelo fuera del catálogo de Claude Code           código 1 (se esperaba 1): Claude Code no tiene el modelo «gpt-5». Modelos: fable, opus, sonnet, haiku.
PASS  un modelo fuera del catálogo de Claude Code no crea terminal  24 antes, 24 después
PASS  un esfuerzo que Claude Code no tiene                  código 1 (se esperaba 1): El esfuerzo 'ultra' no existe en claude. Valores válidos: low, medium, high, xhigh, max.
PASS  un esfuerzo que Claude Code no tiene no crea terminal  24 antes, 24 después
PASS  un esfuerzo que el modelo de OpenCode no tiene        código 1 (se esperaba 1): El modelo deepseek-v4-pro no tiene el esfuerzo 'low'. Valores válidos: high, max.
PASS  un esfuerzo que el modelo de OpenCode no tiene no crea terminal  24 antes, 24 después
PASS  un modelo que Codex no tiene en su caché              código 1 (se esperaba 1): Codex no tiene el modelo 'gpt-9-imaginario'. Modelos: gpt-5-codex.
PASS  un modelo que Codex no tiene en su caché no crea terminal  24 antes, 24 después
PASS  un modelo que mcode no tiene                          código 1 (se esperaba 1): MiniMax Code no tiene el modelo «minimax/modelo-que-no-existe». Modelos: minimax/MiniMax-M3.1-Flash.
PASS  un modelo que mcode no tiene no crea terminal         24 antes, 24 después
PASS  un esfuerzo en Qwen Code                              código 1 (se esperaba 1): Qwen Code no tiene niveles de esfuerzo.
PASS  un esfuerzo en Qwen Code no crea terminal             24 antes, 24 después
PASS  una ubicación que no existe                           código 2 (se esperaba 2): --show debe ser tree, tab, right o down (no «arriba»).

ez — comando del espacio de trabajo de EzAgent

Uso:
  ez --skill                     Imprime el manual del agente principal
  ez --help                      Muestra esta ayuda
  ez --version                   Muestra la versión
  ez status                      Estado de la tarea y de la ronda
  ez inventory                   Agentes y modelos disponibles
  ez task new --title T --brief MD --criteria TOML
  ez team propose --file TOML
  ez round start
  ez round close
  ez delegate <papel> --brief MD
  ez send <subagente> --file MD
  ez roles                       Papeles disponibles
  ez verify
  ez close
  ez pause --reason TEXTO
  ez wait [--timeout SEGUNDOS]
  ez report --verdict pass|fail --file MD --blocking N --major N --minor N
            [--mutations-tried N] [--mutations-caught N]

Modo local (Herdr, sin EZ_URL ni EZ_TOKEN):
  ez delegate <papel> --agent <tipo> [--model <modelo>] [--effort <nivel>]
               --brief <archivo> [--name <nombre>] [--show tree|tab|right|down]
               [--cwd <carpeta>] [--no-notify] [--outlive-parent] [--json]
  ez report --state terminado|fallido [--summary TEXTO] [--file INFORME]
  ez validate <nombre> --pass|--fail [--summary TEXTO] [--file INFORME]
  ez tasks [--parent <padre>] [--json]
  ez watch --parent <padre> --dir <raíz> [--hc <ruta>]

Opciones generales:
  --json    Imprime la respuesta completa como JSON (cualquier comando)

Códigos de salida: 0 bien, 1 rechazado, 2 uso incorrecto, 3 sin conexión.
PASS  una ubicación que no existe no crea terminal          24 antes, 24 después
PASS  un nombre con mayúsculas                              código 2 (se esperaba 2): El nombre «T9» no vale: usa minúsculas, números, «-», «_» o «.», hasta 32 caracteres.

ez — comando del espacio de trabajo de EzAgent

Uso:
  ez --skill                     Imprime el manual del agente principal
  ez --help                      Muestra esta ayuda
  ez --version                   Muestra la versión
  ez status                      Estado de la tarea y de la ronda
  ez inventory                   Agentes y modelos disponibles
  ez task new --title T --brief MD --criteria TOML
  ez team propose --file TOML
  ez round start
  ez round close
  ez delegate <papel> --brief MD
  ez send <subagente> --file MD
  ez roles                       Papeles disponibles
  ez verify
  ez close
  ez pause --reason TEXTO
  ez wait [--timeout SEGUNDOS]
  ez report --verdict pass|fail --file MD --blocking N --major N --minor N
            [--mutations-tried N] [--mutations-caught N]

Modo local (Herdr, sin EZ_URL ni EZ_TOKEN):
  ez delegate <papel> --agent <tipo> [--model <modelo>] [--effort <nivel>]
               --brief <archivo> [--name <nombre>] [--show tree|tab|right|down]
               [--cwd <carpeta>] [--no-notify] [--outlive-parent] [--json]
  ez report --state terminado|fallido [--summary TEXTO] [--file INFORME]
  ez validate <nombre> --pass|--fail [--summary TEXTO] [--file INFORME]
  ez tasks [--parent <padre>] [--json]
  ez watch --parent <padre> --dir <raíz> [--hc <ruta>]

Opciones generales:
  --json    Imprime la respuesta completa como JSON (cualquier comando)

Códigos de salida: 0 bien, 1 rechazado, 2 uso incorrecto, 3 sin conexión.
PASS  un nombre con mayúsculas no crea terminal             24 antes, 24 después
PASS  sin `HC_TERMINAL_ID`                                  código 3 (se esperaba 3): Ejecútalo desde la terminal de un agente en Herdr.
PASS  sin `HC_TERMINAL_ID` no crea terminal                 24 antes, 24 después
PASS  no quedan terminales nuevas                           24 antes, 24 después
PASS  el registro de plan1-claude pasa a «terminado»        Some("terminado")
PASS  el padre recibe el aviso de «terminado»               [EzAgent · aviso] plan1-claude (Claude Code · sonnet) terminó. Resumen del subagente: «eco E2E-20436-1:». Informe: C:\Users\JDMoreu\AppData\Local\Temp\ez-e2e-20436\registros\t16b-informe.md. Para validarlo: ez validate plan1-claude --pass|--fail --file <evidencia>
PASS  validar plan1-claude desde un hermano                 código 0: {"name":"plan1-claude","terminal":"t16b","parent":"t169","agent":"claude","label":"Claude Code","model":"sonnet","effort":"low","role":"planner","cwd":"C:\\Users\\JDMoreu\\AppData\\Local\\Temp\\ez-e2e-20436\\work","brief_head":"Trabaja en E2E-20436-1: revisa la carpeta del proyecto y escribe un informe corto.","state":"validado","since_ms":1795109249102,"history":[{"state":"en_proceso","at_ms":1795109240182,"by":"ez delegate","detail":"encargo entregado"},{"state":"terminado","at_ms":1795109240197,"by":"t16b","detail":"el subagente terminó: eco E2E-20436-1:"},{"state":"validado","at_ms":1795109249102,"by":"t16d","detail":"validado por t16d"}],"summary":"eco E2E-20436-1:","report":"C:\\Users\\JDMoreu\\AppData\\Local\\Temp\\ez-e2e-20436\\registros\\t16b-informe.md","validation":{"by":"t16d","verdict":true,"summary":"ok","file":null,"at_ms":1795109249101},"notified":"terminado"}
PASS  el registro de plan1-claude pasa a «validado»         Some("validado")
PASS  el padre recibe el aviso de «validado»                [EzAgent · aviso] plan1-claude quedó validado por t16d: «ok».
PASS  matar a dev2-qwen                                     código 0: {"address":"t16f","argv":["C:/Users/JDMoreu/Documents/EzAgent-wt/HD-6/rust/target/debug/fake-ws-agent.exe","--echo","C:\\Users\\JDMoreu\\AppData\\Local\\Temp\\ez-e2e-20436\\registros","--as","qwen","--model","qwen3-coder-plus"],"cols":49,"created_ms":1795109242107,"cwd":"C:\\Users\\JDMoreu\\AppData\\Local\\Temp\\ez-e2e-20436\\work","die_with_parent":true,"exit_code":1,"finished_ms":1795109252654,"identity":"ghostty","keep":true,"labels":{"agent":"qwen","ez.agent":"qwen","ez.model":"qwen3-coder-plus","ez.parent":"t169","ez.role":"developer","herdr.space":"sp925ed1fcf958471c6ae367b9fdda000b","herdr.tab":"tb01df7d40912544d0e1ea765355b36c5c","where":"ez-e2e-20436 / 1"},"name":"dev2-qwen","notify":false,"parent":"t169","pid":22512,"reason":"killed","reported":true,"rm":false,"rows":63,"screen":"exact","signal":null,"state":"finished","status":null,"status_ms":1795109252654,"template":null,"title":"","turn":1}
PASS  el registro de dev2-qwen pasa a «fallido»             Some("fallido")
PASS  el padre recibe el aviso de «fallido»                 [EzAgent · aviso] dev2-qwen (Qwen Code · qwen3-coder-plus) falló: la terminal terminó sin informe (código 1). Revisa su pantalla: hc read t16f
PASS  `ez tasks` muestra «validado»                         ["en_proceso", "fallido", "validado", "en_proceso", "en_proceso", "en_proceso"]
PASS  `ez tasks` muestra «fallido»                          ["en_proceso", "fallido", "validado", "en_proceso", "en_proceso", "en_proceso"]
PASS  `ez tasks` muestra «en_proceso»                       ["en_proceso", "fallido", "validado", "en_proceso", "en_proceso", "en_proceso"]
PASS  el vigilante del padre existe una sola vez            ["t16c"]
PASS  los hijos de la prueba se borran                      ["t16b", "t16d", "t16f", "t16g", "t16h", "t16j"]
PASS  el vigilante de la prueba se borra                    ["t16c"]
PASS  el espacio de la prueba se cierra                     código 0: {
  "ok": true
}
PASS  las terminales de la prueba se borran                 []
PASS  el foco vuelve a donde estaba                         pane p65206280d673d5a1c9118cd021e814b3 / agent t6h; ahora pane p65206280d673d5a1c9118cd021e814b3 / agent t6h
PASS  la carpeta de la prueba se borra                      C:\Users\JDMoreu\AppData\Local\Temp\ez-e2e-20436
PASS  no queda nada de la prueba en Herdr                   []
126 comprobaciones: 126 pasan, 0 fallan.
```

Datos de esa corrida:
- Padre: `t169`, espacio `sp925ed1fcf958471c6ae367b9fdda000b`.
- Hijos: `plan1-claude (t16b) tree`, `dev1-opencode (t16d) tab`, `dev2-qwen (t16f) right`, `qa1-codex (t16g) tree`, `valqa1-mcode (t16h) tab`, `reviewer1-pi (t16j) tree`.
- Vigilante único: `t169` → `ez-watch-a-t169` (`t16c`).
- Avisos recibidos por el padre (leídos de su pantalla):
  - «[EzAgent · aviso] plan1-claude (Claude Code · sonnet) terminó. Resumen del subagente: «eco E2E-20436-1:». Informe: …\t16b-informe.md. Para validarlo: ez validate plan1-claude --pass|--fail --file <evidencia>»
  - «[EzAgent · aviso] plan1-claude quedó validado por t16d: «ok».»
  - «[EzAgent · aviso] dev2-qwen (Qwen Code · qwen3-coder-plus) falló: la terminal terminó sin informe (código 1). Revisa su pantalla: hc read t16f»
- `ez tasks --json` = `["en_proceso","fallido","validado","en_proceso","en_proceso","en_proceso"]`.
- Al terminar: foco devuelto al pane `p65206280d673d5a1c9118cd021e814b3` / agente `t6h`, espacio cerrado, `herdr-app ls` sin terminales ni espacios de la prueba, carpeta temporal borrada.

### Cobertura del brief
| Punto del brief | Estado | Dónde se comprueba |
|---|---|---|
| 1. Exigir `ez`, `fake-ws-agent`, `hc`, `herdr-app` (código 2 si falta algo) | cubierto | `setup.rs` / `proc.rs` |
| 2. Sandbox temporal con `voice.json`, `minimax/config.yaml`, `codex/models_cache.json`, `opencode-config/`, repo git | cubierto | `setup.rs` |
| 3. Padre con `space.new --focus false` y foco guardado | cubierto | `setup.rs`, `placement.rs` |
| 4. Las seis delegaciones con `ez` binario y entorno temporal | cubierto | `children.rs` |
| 5. Comprobaciones por hijo (`hc get`, registro, argv/entorno, `herdr-app ls`, `placed`) | cubierto | `children.rs`, `placement.rs` |
| 6. Rechazos (1), usos (2), sin `HC_TERMINAL_ID` (3), sin terminal nueva | cubierto | `rejects.rs` |
| 6b. Estado y avisos de §13 | cubierto | `state.rs` |
| 7. Limpieza siempre y foco devuelto | cubierto | `cleanup.rs` |
| 8. Salida PASS/FAIL + JSON en `<temp>` y copia en `rust-migration/reports/HD-6-e2e.json`, código 0 solo si todo pasa | cubierto | `checks.rs`, raíz |
| 9. Humo real opt-in `EZ_E2E_REAL=1` | implementado, **no ejecutado** | `smoke.rs` |

## Hallazgos del producto (NO corregidos)

1. **`hc run` escribe el JSON de error en `stderr`, no en `stdout`.** Consecuencia: el vigilante no puede distinguir `name_taken` y cada delegación posterior a la primera emite el aviso espurio «No pude arrancar el vigilante: {"error":{…,"kind":"name_taken"…}}».
   - `rust/crates/ez-herdr/src/subagents/watch.rs:455` (`ensure_watcher`) lee el resultado de `hc run --json`.
   - `rust/crates/ez-herdr/src/herdr/child.rs:490` (`hc_run_json`) solo mira `stdout`.
   - Evidencia (sonda manual con `hc run --json --bg --no-parent --no-notify --name <dup> -- cmd /c "echo dos"`): `stdout` vacío, `stderr` = `{"error":{"hint":"pick another name, or kill the one holding it","kind":"name_taken","message":"a live terminal is already named \"ez-watch-a-t169\""}}`, código de salida 1.
   - Efecto secundario: el mismo límite impide el reintento de nombre `n+1` de §6 (`rust/crates/ez-herdr/src/herdr/delegate/names.rs:43`, `default_name`).

2. **La línea de comando de PowerShell cita el programa entre comillas simples sin el operador `&`.** Con una ruta absoluta, el envoltorio no parsea y el hijo muere al instante («Token 'echo' inesperado en la expresión o la instrucción»).
   - `rust/crates/ez-herdr/src/herdr/text.rs:109` (`command_line`), port de `src/ezagent/voice/agent_session.py:67`.
   - Evidencia: sonda con PowerShell, programa citado → error de parseo; con ruta `C:/…` sin citar → arranca y registra `OPENCODE_CONFIG_CONTENT={"model":"opencode-go/deepseek-v4.1-flash#high"}` y `--standalone`.
   - El ejemplo lo sortea con la decisión 3 (ruta con barras normales en `voice.json`); el defecto sigue en el producto.

3. **El vigilante lee la configuración real de la persona.** `ez watch` lo arranca `hc run` sin entorno, así que `VoiceConfig::load(None)` toma `%APPDATA%\EzAgent\voice.json` del usuario.
   - `rust/crates/ez-ctl/src/local/state.rs:76` y `rust/crates/ez-ctl/src/local.rs:313`.
   - Con `EZ_HC_PATH` en el entorno del núcleo se evitaría.

4. **La pestaña de un hijo en `--show tab` se abre en el espacio que está a la vista, no en el del padre.** Consecuencia: la primera pestaña del hijo se abría en el espacio de la persona (espacio de trabajo de la persona en la corrida 1). El ejemplo lo corrige con `space.select` de su propio espacio (decisión 6) y devuelve el foco al final.

5. **Aviso de «validado» que puede tardar.** En la corrida 1 (con un OpenCode muerto) el aviso de «validado» no llegó al padre en 30 s; con la ventana de 45 s que usa el ejemplo llegó siempre. Tiempos de espera del ejemplo: 45 s para «terminado»/«validado», 15 s para «fallido» (este último, el que exige el brief).

## Pendiente o riesgos
- El paso 9 (humo real con los CLI instalados) queda **implementado pero sin ejecutar**: lo corre el agente principal, previo aviso a la persona. El ejemplo no lanza ningún agente real en el modo normal.
- El pico de memoria de la corrida no se instrumentó: el ejemplo no carga modelos ni datos grandes (solo texto) y sus procesos hijos son `ez.exe` y `fake-ws-agent.exe`. Queda como medición pendiente si el brief lo exige como evidencia.
- El hallazgo 1 (JSON de error de `hc run` en `stderr`) es el que más ruido genera en la salida: el ejemplo lo reporta como aviso, no como fallo, porque §13 exige que el vigilante exista una sola vez, y así se cumple.
- La limpieza dejó la ventana de Herdr de la persona como estaba (foco en `t6h`, solo las 11 terminales originales); los espacios `ez-e2e-*` que quedan en `recent` de `herdr-app` son los cerrados por la prueba.
