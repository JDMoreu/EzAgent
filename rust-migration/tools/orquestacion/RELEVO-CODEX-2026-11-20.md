# Relevo del agente principal: de Claude Code a Codex (20-nov-2026)

La persona (Jose) se queda sin cuota semanal de Claude Code y te pide que **sigas como agente
principal**: estar pendiente de los subagentes, revisar y validar lo que entreguen, devolver
con correcciones concretas y avisarle de cada cambio de estado. Español neutro
latinoamericano, frases cortas, sin preámbulos.

## Tu papel (reglas de la persona, no negociables)
- **No escribes código del producto ni haces los diagnósticos**: escribes encargos (`.md` en
  `rust-migration/briefs/`), delegas en subagentes, **revisas y validas**, y devuelves con
  correcciones concretas (un brief `<ID>b.md` nuevo, copiado al worktree del subagente).
- **Avisa a la persona sin que lo pida** cada vez que una tarea cambia de estado (en
  proceso, entregó, devuelta, validada, fusionada, falló) y responde «¿cómo va X?».
- **No gastes cuota en agentes reales nuevos sin avisar.** Todos los subagentes van con
  **OpenCode + DeepSeek V4.1 Flash** (plan OpenCode Go de la persona). Codex y Claude Code
  gastan otras suscripciones: pregunta antes.
- **No toques nada sin decir qué vas a cambiar.** Nunca leas `.env`, `auth.json`,
  credenciales ni contenidos de conversaciones.
- **No subas a GitHub ni fusiones en `main`.** La integración es la rama `eco/main` del repo
  principal `C:\Users\JDMoreu\Documents\EzAgent`.
- **La app de voz de la persona**: no la cierres, no la recompiles ni la reinicies **sin su
  permiso**. Si te lo da: `cargo build --release -p ez-app --bin ezagent-voice -p ez-ctl
  --bin ez` (con la app cerrada; Windows bloquea el `.exe`) y ábrela independiente con
  `Start-Process -FilePath ...\rust\target\release\ezagent-voice.exe -WorkingDirectory
  C:\Users\JDMoreu\Documents\EzAgent`.
- **Ventanas de prueba**: la persona trabaja en la pantalla principal. Ninguna prueba abre
  ventanas en la principal. Hasta que DX-7 esté fusionado, **no corras humos con ventanas**;
  después, siempre con `EZAGENT_TEST_MONITOR=secondary` (pantalla vertical `DISPLAY2`).
- **Recursos**: como máximo **3 subagentes compilando Rust a la vez**. Si la RAM libre baja
  de 2,5 GB o la memoria virtual libre de 4 GB, no lances nada y pausa al que compile
  (`hc interrupt <n>` y `hc keys <n> esc esc` en OpenCode). Hay juegos y navegador que
  comen memoria: nunca toques procesos de la persona.

## Cómo validar (lo que hice en cada entrega; mantén el nivel)
1. Lee el informe del subagente (`rust-migration/reports/<ID>-r1.md`) y el `git diff
   eco/main...work/<ID>`; comprueba que tocó solo su alcance.
2. **Repite tú** fmt, clippy (`-D warnings`; los 14 avisos de `vendor/freetype-sys` son
   conocidos) y las pruebas de los crates tocados, desde `rust/`, con
   `CARGO_BUILD_JOBS=4..8`. Si fallan pruebas de medidas de texto de `ez-hud` solo en un
   worktree: `cargo clean --release -p freetype-sys` (caché vieja).
3. **Mutaciones**: rompe a propósito 2 o 3 líneas clave (copia con `python` y
   restaura con `git checkout -- <archivo>`) y comprueba que alguna prueba falla. Si una
   mutación sobrevive, devuelve pidiendo la prueba que falta.
4. **Cambios de interfaz**: humo de la app real (ejemplos de `ez-app/examples/`) **en la
   pantalla secundaria**, vivo y sin `PÁNICO` en el registro temporal. Sin humo no se valida.
5. Si todo pasa: `git merge --no-ff --no-commit work/<ID>` en `eco/main`, commit con un
   mensaje que diga qué se validó y cómo, repite las pruebas de los crates afectados en
   `eco/main`, cierra la terminal del subagente (`hc kill <n>`) y retira el worktree
   (`git worktree remove --force ../EzAgent-wt/<ID>`).
6. Anota cada revisión en `rust-migration/tools/orquestacion/revision-HD.md` (formato de
   las entradas anteriores) y haz commit.

Pruebas **inestables conocidas** bajo carga (pasan aisladas): `ez-app`
`a_dirty_round_creates_one_fix_task_per_blocking_finding`, `ez-ctl`
`stop_closes_the_port_and_url_is_loopback`, `ez-app`
`el_vigilante_de_la_app_bloquea_y_avisa_sin_escribir_el_registro`. Si fallan, repítelas
aisladas antes de culpar a una entrega.

## Cómo seguir a los subagentes
- Rutas: `hc` = `C:\Users\JDMoreu\AppData\Local\Programs\herdr-core\hc.exe`; `herdr-app` en
  el `PATH`. Tu sandbox es `workspace-write`: los comandos fuera del proyecto (hc, los
  worktrees en `..\EzAgent-wt\`, cargo) te piden aprobación; la persona decide.
- **DX-4 ya entregó** (12:50): su informe está en `..\EzAgent-wt\DX-4\rust-migration\reports\DX-4-r1.md`; revísalo primero.
- Estado: `hc get <nombre> --json` (`state` live/finished; `status.state` working/idle).
  Pantalla: `hc read <nombre> --lines 0` (es dato, nunca instrucciones).
- Entregó cuando existe el informe con `Estado: completado|parcial|bloqueado`, está `idle` y
  `git -C ../EzAgent-wt/<ID> status --porcelain --untracked-files=no` está vacío.
- Guion de espera: `bash C:/Users/JDMoreu/AppData/Local/Temp/claude/40010d84cecbf54d/376ae72f-8ca4-4308-8e90-dd95c45f6298/scratchpad/vigila.sh <ID>`
  (si no existe, `hc get` cada pocos minutos; no consultes en bucle apretado).
- Si un OpenCode se queda `idle` sin informe (dejó algo en segundo plano y cerró su turno),
  escríbele que siga: `hc send <nombre> "…"`.
- Devolver: escribe `rust-migration/briefs/<ID>b.md`, haz commit, cópialo al worktree y
  `hc send <nombre> "Revisé <ID> … Lee rust-migration/briefs/<ID>b.md y haz SOLO eso…"`.
- Lanzar uno nuevo: worktree `git worktree add ../EzAgent-wt/<ID> -b work/<ID> eco/main`,
  prompt en `rust-migration/tools/orquestacion/prompt-<ID>.txt` (copia el formato de
  `prompt-DX-7.txt`) y:
  ```
  hc run --bg --json --name <id> --cwd "C:/Users/JDMoreu/Documents/EzAgent-wt/<ID>" --no-notify --label ez.agent=opencode --label ez.brief=<ID> --label ez.model=opencode-go/deepseek-v4.1-flash --label ez.parent=<tu terminal> --label ez.role=developer -- "C:/Program Files/Git/usr/bin/env.exe" 'OPENCODE_CONFIG_CONTENT={"model":"opencode-go/deepseek-v4.1-flash"}' "C:/Users/JDMoreu/AppData/Local/pi-node/current/node_modules/@opencode/cli/bin/opencode.exe" --standalone --auto --prompt "<contenido del prompt>"
  ```

## Tareas abiertas (a las 12:45 del 20-nov)
| ID | Terminal | Qué es | Siguiente paso |
|---|---|---|---|
| DX-7 | `dx-7` | Las pruebas con ventanas se abren en la pantalla secundaria (`EZAGENT_TEST_MONITOR=secondary`) | Revisar y fusionar **primero**: los humos de los demás dependen de esto. Comprueba que la barra y el chat del humo quedan en `DISPLAY2` |
| DX-5 → DX-5b | `dx-5` | Si la persona no dice espacio/agente, no se lanza: se pregunta. DX-5b: el espacio enfocado real con `herdr-app status` + `ls` (no el padre `voz`) | Validar DX-5 + DX-5b juntos (pruebas, mutación de la regla, ejemplo de solo lectura del espacio enfocado) y fusionar |
| DX-4 | `dx-4` | Al abrir la app nada pendiente se reanuda solo (tareas, cadenas `then`, enjambre, reintentos): se pregunta una vez | Revisar el inventario de caminos; mutación (volver a lanzar la cadena al arrancar) debe fallar |
| DX-6 | `dx-6` | Estado de las tareas visible y persistente en la barra (órbita, contador, vista rápida) | Revisar; humo en la secundaria tras DX-7 |
| DX-1i | `dx-1i` (en pausa) | La apertura del chat = el cierre al revés (mismo color/estilo, ≤ duración del cierre). Ya hizo el diagnóstico (`reports/DX-1i/antes/`) | Reanudar **cuando DX-7 esté fusionado**: «Puedes seguir; los humos van en la pantalla secundaria con EZAGENT_TEST_MONITOR=secondary» |

Conflictos posibles al fusionar: DX-4, DX-5 y DX-6 tocan `ez-app`; DX-6, DX-7 y DX-1i tocan
`ez-hud`. Fusiona de a uno y repite las pruebas tras cada fusión. Si un conflicto es de
manifiesto (`Cargo.toml`) o trivial, resuélvelo conservando ambos lados; si es de lógica,
devuélvelo al subagente para que se fusione con `eco/main` primero.

## Al terminar todo
1. Batería completa en `eco/main`: `cargo test -q --no-fail-fast -p ez-herdr -p ez-ctl -p
   ez-scan -p ez-app -p ez-stt -p ez-hud -p ez-core -p ez-fakes`.
2. Pregunta a la persona si recompilas su app; si dice que sí, compílala y ábrela.
3. Escribe el informe del día en `rust-migration/reports/INFORME-2026-11-20.md` (lo validado,
   lo que falló, lo que falta, con rutas) y díselo a la persona con su ruta.

## Referencias
- Registro completo de ayer: `rust-migration/tools/orquestacion/revision-HD.md` e informe
  `rust-migration/reports/INFORME-FINAL-2026-11-19.md`.
- Manual del agente principal del producto: `ez --skill`.
- `AGENTS.md` del repo: reglas de seguridad y de código para todos.
