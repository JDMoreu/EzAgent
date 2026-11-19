# HD-9 — Informe (r1)

## Resultado
- Brief/tarea: HD-9 — corregir los cuatro fallos del producto que encontró HD-6 (B1–B4 + E2E sin atajos)
- Estado: completado

## Cambios
- `rust/crates/ez-herdr/src/herdr/child.rs`: **B1** — `hc_run_json` lee el JSON de `hc run --json` tanto por `stdout` como por `stderr` (con el mensaje «no devolvió nada / algo que no es JSON» intacto) y sigue marcando `name_taken`; **B4** — `place_child` delega la pestaña en el espacio del padre (`keep_child_in_parent_space`).
- `rust/crates/ez-herdr/src/herdr/child/space.rs` (nuevo): **B4** — la adopción por etiquetas: lee `herdr.space` del padre (`hc get`), se lo pone al hijo (`hc label <hijo> herdr.space=<espacio>`, sin `herdr.tab`) y espera a que Herdr abra la pestaña en ese espacio (`herdr-app ls`, 0,5 s entre intentos hasta 15 s). Si el padre no tiene espacio, o Herdr no abre la pestaña, cae al `terminal.keep` de siempre con su aviso.
- `rust/crates/ez-herdr/src/herdr/child/tests.rs`: el doble de Herdr aprende la respuesta de un padre con `herdr.space` y la lista de Herdr (`herdr-app ls`), y la ruta `app:ls`.
- `rust/crates/ez-herdr/src/herdr/child/tests/launch.rs`: pruebas de B1 (el «nombre ocupado» que llega por `stderr`) y de B4 (el hijo hereda el espacio del padre sin `terminal.keep` ni `pane.focus`; sin espacio, aviso y `terminal.keep`).
- `rust/crates/ez-herdr/src/herdr/text.rs`: **B2** — `command_line` pone el operador `&` delante del programa cuando va citado (`& 'C:\ruta\programa.exe' args`); PowerShell trata una cadena citada como texto y el hijo moría.
- `rust/crates/ez-herdr/src/herdr/text/tests.rs` (nuevo): pruebas del texto generado (programa simple, ruta con espacios, ruta con `\`, entorno delante, solo el programa lleva `&`).
- `rust/crates/ez-herdr/src/herdr/delegate/tests.rs`: el doble responde «nombre ocupado» por `stderr` y la prueba comprueba que la delegación reintenta con `n+1` (`dev1-opencode` → `dev2-opencode`).
- `rust/crates/ez-herdr/src/subagents/watch.rs`: **B1** — `ensure_watcher` lee el JSON de error de `stdout` o de `stderr`: `name_taken` = «el vigilante ya corre» (sin aviso espurio) y cualquier otro error dice su mensaje, no el JSON crudo.
- `rust/crates/ez-herdr/src/subagents/watch/tests.rs`: pruebas de `ensure_watcher` (nombre ocupado por `stderr` y por `stdout`, otro error, vigilante nuevo).
- `rust/crates/ez-ctl/src/local/state.rs`: **B3** — `Context::with_hc` resuelve la ruta de `hc` sin leer `VoiceConfig` cuando ya viene dada (`--hc`); `hc_path_with` deja inyectar el cargador para poder contar las lecturas.
- `rust/crates/ez-ctl/src/local/watch.rs`: **B3** — `ez watch` usa la ruta de `hc` del contexto (la del `--hc`) y ya no la busca en la configuración; pruebas nuevas (con la ruta dada no se lee la configuración; sin ella sí; con `APPDATA` en una carpeta sin `voice.json`).
- `rust/crates/ez-ctl/examples/e2e_delegate.rs`: la corrida escribe su JSON en `rust-migration/reports/HD-9-e2e.json` (el de HD-6 no se pisa).
- `rust/crates/ez-ctl/examples/e2e_delegate/setup.rs`: **se quita la decisión 3** — el agente falso se lanza con su ruta de verdad (con `\`), sin el truco de las barras normales.
- `rust/crates/ez-ctl/examples/e2e_delegate/children.rs`: **se quita la decisión 6** (`space.select` antes de la pestaña) y se añade la comprobación de que ningún hijo deja el aviso «No pude arrancar el vigilante».

Ningún otro archivo del producto se tocó. Fuera del alcance del brief se creó **un** archivo nuevo
(`herdr/child/space.rs`) porque `child.rs` llegó a 600 líneas (el tope de `_COMMON-ECO`) con la
lógica de B4; la regla común pide partir en submódulos (`modulo.rs` + `modulo/parte.rs`) y el
módulo sigue el patrón que ya usan `child.rs`/`child/tests.rs`.

## Decisiones
1. `herdr/text.rs::command_line` — el `&` va **solo** delante del programa y **solo** cuando va
   citado: con un programa simple (`claude`) la línea no cambia y el catálogo de fábrica sigue
   igual. Los argumentos citados que no son el programa no llevan `&`.
2. `herdr/child.rs::hc_run_json` — el JSON se busca primero en `stdout` y luego en `stderr`. Si
   `stdout` trae un JSON válido (éxito) manda ese; si no, se prueba `stderr`. Los mensajes de
   error («no devolvió nada», «devolvió algo que no es JSON») se conservan tal cual.
3. `herdr/child/space.rs` — la pestaña del hijo se abre **por la etiqueta `herdr.space`** (la vía
   que dice el brief), no por `terminal.keep`: `terminal.keep --place tab` además de abrir la
   pestaña mueve el foco de la persona al espacio del padre (verificado en vivo). `terminal.keep`
   queda solo como reserva, con aviso, cuando el padre no tiene `herdr.space` o cuando Herdr no
   abre la pestaña en 15 s.
4. `herdr/child/space.rs` — sin `herdr.tab`: Herdr crea una pestaña nueva en ese espacio (con su
   panel) y la deja inactiva; el foco de la persona no se mueve.
5. `herdr/child/space.rs` — la espera de la pestaña mira `herdr-app ls` (no hay etiqueta
   `herdr.tab` en el hijo cuando la pestaña la abre Herdr): se busca la terminal dentro de las
   pestañas del espacio del padre.
6. `ez-ctl/src/local/state.rs` — con `--hc` la ruta dada manda sobre `EZ_HC_PATH` y sobre
   `voice.json`, y `VoiceConfig::load` no se llama: el vigilante corre en una terminal de `hc` sin
   entorno y no tiene por qué leer el `voice.json` de la persona.
7. `ez-ctl/src/local/state.rs` — `hc_path_with` recibe el cargador de la configuración como
   función: así una prueba puede contar cuántas veces se lee (0 con la ruta dada, 1 sin ella).
8. `examples/e2e_delegate.rs` — el JSON de la corrida va a `HD-9-e2e.json`: el de HD-6 es la
   evidencia de aquella prueba y no se pisa.
9. `examples/e2e_delegate/children.rs` — la comprobación del aviso del vigilante se hace por
   hijo (6 comprobaciones nuevas): el texto que se busca es el del producto
   («No pude arrancar el vigilante»).

## Verificación

### Puerta del elefante (desde `rust/`)
```
$ cargo fmt -p ez-herdr -p ez-ctl -- --check
(sin salida) — salida 0

$ cargo clippy -p ez-herdr -p ez-ctl --all-targets -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.54s — salida 0

$ cargo test -p ez-herdr
running 276 tests
test result: ok. 276 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.23s
    Running tests\agent_driver_parity.rs … test result: ok. 1 passed; 0 failed; …
    Running tests\herdr_parity.rs …      test result: ok. 1 passed; 0 failed; …
    Running tests\metrics_parity.rs …    test result: ok. 2 passed; 0 failed; …
    Running tests\permission.rs …        test result: ok. 27 passed; 0 failed; …
    Running tests\tasks_activity_parity.rs … test result: ok. 1 passed; 0 failed; …

$ cargo test -p ez-ctl
test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s   (lib)
test result: ok. 7 passed; 0 failed; …  (cli_errors)      test result: ok. 9 passed; 0 failed; …  (delegate_local)
test result: ok. 16 passed; 0 failed; … (local_state)     test result: ok. 6 passed; 0 failed; …  (roundtrip)
test result: ok. 7 passed; 0 failed; …  (server_errors)   test result: ok. 2 passed; 0 failed; …  (waits)
```
Las pruebas de paridad de `ez-herdr` (`agent_driver_parity`, `herdr_parity`, `metrics_parity`,
`permission`, `tasks_activity_parity`) siguen en verde: 32 en total.

### Prueba de punta a punta (el ejemplo, con Herdr real y agentes falsos)
```
$ cargo run -p ez-ctl --example e2e_delegate
[EzAgent] prueba ez-e2e-18072
[EzAgent] copia: …\rust-migration\reports\HD-9-e2e.json
…
PASS  delegar en developer-opencode                  dev1-opencode (t180) en tab
PASS  el vigilante de plan1-claude arrancó sin avisos
PASS  el argv de dev1-opencode                      ["C:\\Users\\…\\fake-ws-agent.exe", "--echo", …, "--as", "opencode", "--standalone"]
PASS  el OPENCODE_CONFIG_CONTENT de dev1-opencode    {"model":"opencode-go/deepseek-v4.1-flash#high"} (se esperaba opencode-go/deepseek-v4.1-flash#high)
PASS  dev1-opencode tiene pestaña propia             placed=tab; pestaña tb3fbd1fb29b9be8d1407ea57c33f40421
PASS  valqa1-mcode tiene pestaña propia              placed=tab; pestaña tbb22d01e84abc7e290962d0e574d3d681
PASS  el padre recibe el aviso de «terminado»        [EzAgent · aviso] plan1-claude (Claude Code · sonnet) terminó. …
PASS  el padre recibe el aviso de «validado»         [EzAgent · aviso] plan1-claude quedó validado por t17p: «ok».
PASS  el padre recibe el aviso de «fallido»          [EzAgent · aviso] dev2-qwen (Qwen Code · qwen3-coder-plus) falló: …
PASS  el vigilante del padre existe una sola vez     ["t17n"]
PASS  el foco vuelve a donde estaba                  pane p652… / agent t6h; ahora pane p652… / agent t6h
PASS  no queda nada de la prueba en Herdr            []
131 comprobaciones: 131 pasan, 0 fallan.
CÓDIGO DE SALIDA: 0
```
El JSON de la corrida quedó en `rust-migration/reports/HD-9-e2e.json` (`"ok": true`).

### Comprobaciones en vivo (las del brief)
- **B1** — `hc run --bg --json --name hd9-probe -- ping -n 30 127.0.0.1` dos veces: la primera
  devuelve el JSON por `stdout` (código 0); la segunda, **código 1, `stdout` vacío y el JSON de
  error en `stderr`**:
  `{"error":{"hint":"pick another name, or kill the one holding it","kind":"name_taken","message":"a live terminal is already named \"hd9-probe\""}}`.
  Terminal de sonda matado y borrado (`hc kill`/`hc rm`).
- **B2** — a mano, sin dejar código de shell en el repo:
  `powershell.exe -NoProfile -Command "'C:\Windows\System32\ping.exe' -n 1 127.0.0.1"` →
  «Token '-n' inesperado en la expresión o la instrucción» (código 1);
  `powershell.exe -NoProfile -Command "& 'C:\Windows\System32\ping.exe' -n 1 127.0.0.1"` →
  el programa corre y sale con **código 0**.
- **B4** — en un espacio de prueba propio (`space.new --focus false`, terminales `ping`):
  - el padre `t178` tenía `herdr.space=sp5f52…`; el hijo `t17g` recibió `hc label t17g
    herdr.space=sp5f52…` y **Herdr le abrió la pestaña `tb644917…` en el espacio del padre**
    (la lista del espacio: pestañas del padre y del hijo en `sp5f52…`; el hijo bajo su padre en
    `sub_agents`: `["t179","t17c","t17d","t17g"]`);
  - **el foco no cambió**: antes y después, `space=sped17…` (EzAgent), `tab=tbc954…`,
    `focused_pane=p652…`, `agent=t6h`; la pestaña nueva quedó inactiva;
  - con `terminal.keep --place tab` en vez de la etiqueta, en cambio, la ventana **sí** saltaba al
    espacio del padre y enfocaba la pestaña nueva (por eso la adopción va por la etiqueta);
  - limpieza: se cerró el espacio de prueba (`space.close`), se borraron sus terminales y el foco
    quedó donde estaba.

## Pendiente o riesgos
- **El `command_line` de `ez-herdr::session` sigue siendo una copia sin el `&`** (`session/text.rs`,
  fuera del alcance de este brief). Hoy no la usa el producto (solo la prueba de paridad
  `agent_driver_parity`): el camino real —el lanzamiento visible (`actions.rs::launch_visible`) y
  el envoltorio de entorno del lanzamiento en segundo plano (`actions.rs::wrapped_argv`)— pasa por
  `herdr::text::command_line`, que ya lleva el `&`. La copia quedó atrás; conviene un HD-10 que
  unifique las dos (el mismo problema que R4-1 con `valid_name`).
- La adopción por `herdr.space` depende de que Herdr cree la pestaña al ver la etiqueta: si una
  versión de Herdr no lo hiciera, el vigilante caería al `terminal.keep` de siempre **después de
  15 s** (con aviso). Es un coste raro, no un fallo.
- Con la adopción, la pestaña del hijo queda **inactiva**: el padre no ve al hijo al instante
  (antes `terminal.keep` la enfocaba). Es lo que pide el brief («sin mover el foco de la
  persona»); si más adelante se quiere que la persona vea al hijo, hay que pedirlo explícitamente
  (pestaña en segundo plano + aviso).
- El paso 9 del ejemplo (humo real con los CLI instalados) sigue **sin ejecutar**: no lo pide el
  brief y gastaría cuota.
- La medición de memoria (tope de 1 GB) no se instrumentó: las pruebas de esta ola son de texto y
  procesos pequeños (`ez.exe`, `fake-ws-agent.exe`, `hc`, `herdr-app`); el ejemplo corre seis
  subagentes falsos en paralelo.
