## Resultado
- Brief/tarea: IR-4
- Estado: completado

## Cambios
- `rust/crates/ez-ctl/Cargo.toml`: `[[example]] e2e_inventory` y `[dev-dependencies]` con `chrono` y `rusqlite` de `[workspace.dependencies]` (solo para las fixtures SQLite y las fechas RFC 3339 del ejemplo).
- `rust/Cargo.lock`: efecto de compilar: `ez-ctl` gana `chrono` y `rusqlite`; no entra ningún crate nuevo al espacio de trabajo.
- `rust/crates/ez-ctl/examples/e2e_inventory.rs`: raíz del ejemplo (documentación, módulos, flujo y códigos 0/1/2).
- `rust/crates/ez-ctl/examples/e2e_inventory/setup.rs`: sandbox temporal (HOME/USERPROFILE/APPDATA/LOCALAPPDATA/CODEX_HOME/MINIMAX_DATA_DIR/EZ_SUBAGENTS_DIR falsos), copias de `fake-ws-agent` por tipo, `voice.json`, `modelos.toml`, `config.yaml` de mcode y caché de Codex.
- `rust/crates/ez-ctl/examples/e2e_inventory/fixtures.rs`: datos inventados de cada lector (`rate_limits` de Codex vigente/vencido, `message.usage` de Claude y Pi, `session_message` + aviso Free de OpenCode, SQLite de mcode), `uso.json` del vigilante y el `developer` previo del padre falso.
- `rust/crates/ez-ctl/examples/e2e_inventory/init.rs`: paso 1, `entorno.json` atómico, `planes.toml` y su segunda corrida.
- `rust/crates/ez-ctl/examples/e2e_inventory/usage.rs`: paso 2, estados y tokens por modelo de los cinco lectores.
- `rust/crates/ez-ctl/examples/e2e_inventory/recommend.rs`: paso 3, `developer`, `qa_validator` (familia distinta) y `planner`.
- `rust/crates/ez-ctl/examples/e2e_inventory/parent.rs`: padre falso en `hc`, ocupación del nombre del vigilante y registro previo.
- `rust/crates/ez-ctl/examples/e2e_inventory/delegate.rs`: paso 4, `ez delegate developer` sin `--agent`, su registro y el primer mensaje.
- `rust/crates/ez-ctl/examples/e2e_inventory/guard.rs`: paso 5, código 4 con el texto de §4 y `--force` con aviso.
- `rust/crates/ez-ctl/examples/e2e_inventory/watch.rs`: paso 6, `ez watch` con umbrales absurdos y un solo aviso en dos vueltas.
- `rust/crates/ez-ctl/examples/e2e_inventory/depleted.rs`: paso 7, `uso.json` del modelo gratuito sin agotar todo OpenCode.
- `rust/crates/ez-ctl/examples/e2e_inventory/cleanup.rs`, `checks.rs`, `proc.rs`, `run.rs`, `out.rs`: limpieza por etiquetas, tabla `OK`/`FALLO`, subprocesos sin shell y salida por `Write`.

## Decisiones
- `# DECISION` en `setup.rs`: el `modelos.toml` de la prueba cita los nombres con punto (`["deepseek-v4.1-flash"]`). TOML lee `[deepseek-v4.1-flash]` sin comillas como una tabla anidada; el propio ejemplo del producto tiene ese problema (ver «Fallos del producto»). Se usa la forma que funciona, sin tocar el producto.
- `# DECISION` en `parent.rs`: el padre falso se abre con `hc run --bg --no-parent --no-notify` y el nombre del vigilante (`ez-watch-a-<padre>`) se ocupa antes con un agente falso. `hc run` no propaga el entorno del cliente, así que un vigilante real leería el `voice.json` de la persona; el paso 6 ejecuta `ez watch` explícitamente con el entorno temporal. Esto no prueba el arranque automático del vigilante (lo cubren HD-6 e IR-3).
- `# DECISION` en `setup.rs`: el PATH conserva el original (con las copias falsas primero) para que `hc` pueda arrancar PowerShell y sus ayudantes; las ocho claves del catálogo apuntan a fakes, así que el producto no puede lanzar un agente real.
- El registro `dev0-opencode` se siembra como `terminado` y ya avisado: solo sirve para que `qa_validator` vea la familia del último `developer`; el vigilante lo ignora.
- El resultado JSON se deja en `%TEMP%\ez-ir4-<id>.json` (fuera de la carpeta que se borra) como evidencia; la carpeta de la prueba y todas las terminales se borran.
- No se añadió ningún `[[bin]]` ni se tocó el `Cargo.toml` raíz.

## Verificación
Desde `rust/`:

```
$ cargo fmt --check -p ez-ctl
fmt=0

$ cargo clippy -p ez-ctl --all-targets -- -D warnings
    Checking ez-ctl v0.1.0 (C:\Users\JDMoreu\Documents\EzAgent-wt\IR-4\rust\crates\ez-ctl)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.77s

$ cargo test -p ez-ctl
test result: ok. 12 passed; 0 failed  (lib)
test result: ok. 0 passed; 0 failed   (bin ez)
test result: ok. 7 passed; 0 failed   (cli_errors)
test result: ok. 10 passed; 0 failed  (delegate_local)
test result: ok. 17 passed; 0 failed  (local_state)
test result: ok. 6 passed; 0 failed   (roundtrip)
test result: ok. 3 passed; 0 failed   (scan_init)
test result: ok. 7 passed; 0 failed   (server_errors)
test result: ok. 2 passed; 0 failed   (waits)
Total: 64 passed; 0 failed.
```

`cargo run -q -p ez-ctl --example e2e_inventory`, salida literal (código 0):

```text
[EzAgent] prueba ez-ir4-4344-36300
[EzAgent] resultado: C:\Users\JDMoreu\AppData\Local\Temp\ez-ir4-4344-36300.json
OK     `ez init --json` termina bien                             ok
OK     entorno.json queda en %APPDATA%                           C:\Users\JDMoreu\AppData\Local\Temp\ez-ir4-4344-36300\appdata\EzAgent\entorno.json (se esperaba C:\Users\JDMoreu\AppData\Local\Temp\ez-ir4-4344-36300\appdata\EzAgent\entorno.json)
OK     entorno.json trae el sistema                              Windows 11 Home · AMD Ryzen 5 5500GT with Radeon Graphics
OK     entorno.json trae los recursos                            RAM 28562 MB · 12 núcleos lógicos · disco 217995 MB
OK     el agente falso claude está en el inventario              instalado=true · C:\Users\JDMoreu\AppData\Local\Temp\ez-ir4-4344-36300\bin\claude.exe
OK     el agente falso opencode está en el inventario            instalado=true · C:\Users\JDMoreu\AppData\Local\Temp\ez-ir4-4344-36300\bin\opencode.exe
OK     el agente falso mcode está en el inventario               instalado=true · C:\Users\JDMoreu\AppData\Local\Temp\ez-ir4-4344-36300\bin\mcode.exe
OK     el agente falso codex está en el inventario               instalado=true · C:\Users\JDMoreu\AppData\Local\Temp\ez-ir4-4344-36300\bin\codex.exe
OK     el agente falso pi está en el inventario                  instalado=true · C:\Users\JDMoreu\AppData\Local\Temp\ez-ir4-4344-36300\bin\pi.exe
OK     el agente falso qwen está en el inventario                instalado=true · C:\Users\JDMoreu\AppData\Local\Temp\ez-ir4-4344-36300\bin\qwen.exe
OK     entorno.json dice qué falta declarar                      7 avisos
OK     entorno.json se escribe sin dejar temporales              []
OK     `ez init` crea planes.toml                                C:\Users\JDMoreu\AppData\Local\Temp\ez-ir4-4344-36300\appdata\EzAgent\planes.toml creado=true
OK     planes.toml propone cada agente detectado                 6 agentes · 839 bytes
OK     la segunda corrida no crea planes.toml                    creado=false
OK     la segunda corrida no pisa planes.toml                    878 bytes
OK     `ez usage --json` termina bien                            ok
OK     Codex vigente: 42 % y estado libre                        libre · 42 %
OK     Codex vigente: el límite es el porcentaje observado       unidad «percent»
OK     Codex vigente: el reinicio está en el futuro              resets_ms 1795138277000
OK     Codex vigente: la ventana observada es de 5 h             5 h
OK     Codex vigente: avisa de la antigüedad del dato            Último dato de cuota: hace 5 min, en 1 archivo(s) revisado(s).
OK     Codex vencido: el estado queda desconocido                desconocido
OK     Codex vencido: explica que la ventana reinició            El dato de Codex es de hace 5 min y su ventana ya reinició; no lo presento como actual.
OK     Claude Code: 540 tokens en 2 mensajes                     540 tokens · 2 mensajes
OK     Claude Code: el plan sale de ~/.claude.json               plan «claude_pro»
OK     Claude Code: los tokens de sonnet                         180 (se esperaban 180)
OK     Claude Code: los tokens de opus                           360 (se esperaban 360)
OK     Claude Code: estado desconocido sin límite declarado      desconocido
OK     OpenCode: 280 tokens y 2 mensajes de session_message      280 tokens · 2 mensajes
OK     OpenCode: el modelo Go no está agotado                    desconocido · 180 tokens
OK     OpenCode: el modelo gratuito sí está agotado              agotado · 100 tokens
OK     OpenCode: el agente no queda agotado por el aviso Free    desconocido
OK     OpenCode: el plan detectado es OpenCode Go                OpenCode Go
OK     OpenCode: avisa del aviso Free vigente                    Aviso vigente de Free: agota los modelos gratuitos (*-free).
OK     OpenCode: dice que solo el modelo está agotado            El modelo opencode/hy4-flash-free está agotado; el agente no, porque otros modelos de la ventana siguen disponibles.
OK     OpenCode: costo de la ventana                             0.5 USD
OK     mcode: 280 tokens en 2 filas                              280 tokens · 2 filas
OK     mcode: los tokens por modelo                              280
OK     mcode: el costo de la ventana                             0.75 USD
OK     mcode: estado desconocido (no hay límite local)           desconocido
OK     Pi: 280 tokens en 2 mensajes                              280 tokens · 2 mensajes
OK     Pi: los tokens por modelo                                 280
OK     Pi: el costo de la ventana                                0.6 USD
OK     Qwen Code: instalado y sin lector de uso                  instalado=true · desconocido
OK     el padre falso se abre en hc                              t20d (ez-ir4-4344-36300-p)
OK     el nombre del vigilante queda ocupado por la prueba       t20f (ez-watch-a-t20d)
OK     el registro del developer anterior está sembrado          dev0-opencode con opencode-go/deepseek-v4.1-flash
OK     developer: elige el modelo medio de OpenCode              opencode · opencode-go/deepseek-v4.1-flash
OK     developer: nivel medio, costo plan y esfuerzo high        medio · plan · high
OK     developer: la recomendación no deja avisos                []
OK     qa_validator: elige Claude Code · opus                    claude · opus
OK     qa_validator: nivel alto y esfuerzo high                  alto · high
OK     qa_validator: la familia es distinta a la del último developer  familia anthropic · La familia anthropic es distinta a la del último developer del padre (deepseek).
OK     planner: elige OpenCode · deepseek-v4-pro                 opencode · opencode-go/deepseek-v4-pro
OK     planner: nivel alto y esfuerzo high                       alto · high
OK     planner: explica el papel y el costo                      ["El papel «planner» pide como mínimo nivel alto y esfuerzo high.", "OpenCode · opencode-go/deepseek-v4-pro es de nivel alto, costo plan y familia deepseek.", "No puedo saber el uso de OpenCode; lo penalicé frente a uno libre.", "planes.toml le da prioridad normal."]
OK     `ez delegate developer` sin --agent termina bien          código 0
OK     la salida dice qué eligió y por qué                       Elegí OpenCode · opencode-go/deepseek-v4.1-flash · high porque El papel «developer» pide como mínimo nivel medio y esfuerzo medium. OpenCode · opencode-go/deepseek-v4.1-flash es de nivel medio, costo 
OK     el hijo elegido es OpenCode · deepseek-v4.1-flash · high  opencode · opencode-go/deepseek-v4.1-flash · high
OK     el hijo se llama dev1-opencode y cuelga del padre         dev1-opencode · padre t20d
OK     el hijo queda en el árbol y el encargo se entrega         placed=tree · task_sent=true
OK     la delegación no deja avisos                              []
OK     el registro del hijo está en_proceso bajo el padre        en_proceso · t20d · t20g
OK     el registro del hijo guarda tipo, modelo y esfuerzo       opencode · opencode-go/deepseek-v4.1-flash · high
OK     `hc get` confirma el padre y el nombre del hijo           dev1-opencode · padre t20d
OK     las etiquetas del hijo lo asocian al padre y al papel     {"agent":"opencode","ez.agent":"opencode","ez.effort":"high","ez.model":"opencode-go/deepseek-v4.1-flash","ez.parent":"t20d","ez.role":"developer"}
OK     el primer mensaje del hijo trae el papel                  1 línea(s) recibidas
OK     el primer mensaje del hijo nombra a su padre y el encargo  padre t20d · marca IR4-ez-ir4-4344-36300-1
OK     el hijo de OpenCode arranca con --standalone              ["C:\\Users\\JDMoreu\\AppData\\Local\\Temp\\ez-ir4-4344-36300\\bin\\opencode.exe", "--echo", "C:\\Users\\JDMoreu\\AppData\\Local\\Temp\\ez-ir4-4344-36300\\registros", "--standalone"]
OK     el hijo de OpenCode fija el modelo y el esfuerzo por entorno  {"model":"opencode-go/deepseek-v4.1-flash#high"}
OK     la guarda rechaza con código 4                            código 4: Recursos justos: RAM libre 13665 MB (mínimo 999999999), memoria virtual libre 13453 MB (mínimo 999999999). Espera a que termine alguno o usa --force.
OK     la guarda usa el texto de §4                              Recursos justos: RAM libre 13665 MB (mínimo 999999999), memoria virtual libre 13453 MB (mínimo 999999999). Espera a que termine alguno o usa --force.
OK     el rechazo no abre ninguna terminal                       1 antes, 1 después
OK     `--force` lanza con el aviso de la guarda                 --force: me salté la guarda de recursos. Recursos justos: RAM libre 13661 MB (mínimo 999999999), memoria virtual libre 13449 MB (mínimo 999999999). Espera a que termine alguno o usa --force.
OK     `--force` abre el hijo dev2-qwen bajo el padre            dev2-qwen · qwen · padre t20d
OK     el registro de dev2-qwen queda en_proceso                 en_proceso (C:\Users\JDMoreu\AppData\Local\Temp\ez-ir4-4344-36300\subagentes\t20d\dev2-qwen.json)
OK     `ez watch` arranca con el padre falso                     [EzAgent · aviso] Memoria baja: RAM libre 13674 MB, virtual libre 13491 MB. El subagente que más usa es dev1-opencode (69 MB).
OK     el aviso de memoria llega una sola vez en dos vueltas     1 aviso(s); seguía vivo=true
OK     el aviso usa el texto de §4                               [EzAgent · aviso] Memoria baja: RAM libre 13674 MB, virtual libre 13491 MB. El subagente que más usa es dev1-opencode (69 MB).
OK     uso.json agota el modelo gratuito                         agotado
OK     uso.json no agota a todo OpenCode                         desconocido
OK     el modelo Go sigue disponible                             desconocido
OK     `ez recommend` no descarta OpenCode por el modelo gratuito  opencode · opencode-go/deepseek-v4.1-flash
OK     no quedan terminales de la prueba en hc                   []
OK     la carpeta de la prueba se borra                          C:\Users\JDMoreu\AppData\Local\Temp\ez-ir4-4344-36300
86/86 OK
```

## Fallos del producto

### 1. `modelos.toml`: los nombres con punto hay que citarlos (documentación y parser discrepan)
- Dónde: `rust/crates/ez-scan/src/recommend/modelos.rs` (ejemplo de la cabecera, líneas 5–10) y `Catalog::parse`.
- Qué pasa: TOML lee `[deepseek-v4.1-flash]` como la tabla `deepseek-v4` con la subclave `1-flash`; el parser busca `nivel`/`costo` en `deepseek-v4`, avisa y salta la entrada. Si no queda ninguna entrada válida, `Catalog::load` cae a los valores de fábrica: la preferencia declarada por la persona se ignora en silencio.
- Reproducción (con agentes falsos y `APPDATA` temporal):

```text
$ cat %APPDATA%\EzAgent\modelos.toml
[deepseek-v4.1-flash]
agente = "opencode"
nivel = "medio"
costo = "plan"
familia = "deepseek"

$ ez recommend developer --json
Aviso: modelos.toml: «deepseek-v4» necesita nivel alto, medio o bajo; lo salto.
{"ok":true,"recommendation":{"role":"developer",...,"agent":"qwen","model":"qwen3-coder-plus","level":"medio","cost":"gratis",...},"warnings":["modelos.toml: «deepseek-v4» necesita nivel alto, medio o bajo; lo salto."]}
```

- Mitigación en la prueba: citar las claves (`["deepseek-v4.1-flash"]`), que es la forma que sí funciona. No se tocó el producto.

### 2. La guarda de recursos mide con el `hc` del PATH, no con `hc_path` de `voice.json`
- Dónde: `rust/crates/ez-scan/src/real.rs`, `fn herdr() -> Herdr { Herdr::new(VoiceConfig::new()) }` (usa la configuración de fábrica, no la cargada).
- Qué pasa: si `hc` no está en el PATH pero sí en `hc_path`, la medición falla; `resource_guard` decide no bloquear («No pude medir… Sigo adelante») y la delegación sigue aunque los umbrales sean absurdos. La guarda falla abierta.
- Reproducción (padre falso real en `hc`; `voice.json` con `hc_path` absoluto y `min_free_ram_mb = 999999999`; PATH sin `hc`):

```text
$ ez delegate developer --agent qwen --model qwen3-coder-plus --brief brief.md --json
{"ok":true,"terminal":"t20b","name":"dev1-qwen",...,"warnings":["No pude medir los recursos: No pude listar los subagentes: No encuentro `hc`. Revisa voice.json.. Sigo adelante."],...}
código 0
```

Esperado por §4: código 4 con «Recursos justos: …». El hijo de prueba se cerró después; no se lanzó ningún agente real.

### 3. Aviso espurio al completar un modelo explícito sin esfuerzo
- Dónde: `rust/crates/ez-ctl/src/local/choice.rs`, `fill`: con `--agent` y `--model` explícitos pero sin `--effort`, intenta la recomendación; si el agente no tiene entrada en `modelos.toml`, deja el modelo pedido y añade «No pude completar el modelo o el esfuerzo». El modelo no faltaba.
- Reproducción: `ez delegate developer --agent qwen --model qwen3-coder-plus --force …` con umbrales absurdos y un `modelos.toml` sin `qwen`. El resultado trae `"warnings":["No pude completar el modelo o el esfuerzo: No encontré un modelo de nivel medio o más …","--force: …"]` y lanza bien. Es solo texto engañoso; no se tocó.

## Pendiente o riesgos
- `entorno.json` no incluye la sección «Uso y límites» que pide §3 del diseño: `Entorno` solo tiene `version`, `generated_ms`, `system`, `agents` y `missing`. El paso 1 del brief pide sistema, recursos y agentes, y eso es lo que comprueba; queda como desviación del contrato de IR-1.
- El arranque automático del vigilante no se prueba aquí: el nombre se ocupa con un fake para no leer el `voice.json` real desde `hc run` (que no propaga el entorno). El vigilante se ejecuta a mano en el paso 6. HD-6 e IR-3 cubren el arranque automático.
- La prueba depende de `hc` real (v0.1.21 aquí) y de que `herdr-app`/`hc` estén instalados; sin `hc` sale con código 2 y un mensaje claro. No usa ni enfoca la ventana de Herdr.
- Los datos de uso son inventados; el escaneo de `ez init` sí mide CPU/GPU/RAM reales (solo lectura).
- El JSON de resultado queda en `%TEMP%\ez-ir4-<id>.json` como evidencia; la carpeta temporal de la prueba y todas sus terminales se borran.
