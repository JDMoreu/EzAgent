# P-IMP-NAMES — Nombres de espacios y terminales mal transcritos: compararlos con lo abierto en Herdr y confirmar «¿te refieres a …?»
Excepción a AGENTS.md: rama de migración a Rust; aplica `rust-migration/AGENTS.rust.md`.
**Antes de nada lee `rust-migration/briefs/_COMMON.md` y `rust-migration/AGENTS.rust.md`.**

## Fuente Python (la especificación, léela completa)
- src/ezagent/agent/tools_herdr.py (cómo el modelo nombra espacios, terminales y agentes en `delegate_to_agent` y demás acciones), src/ezagent/voice/herdr.py (listar espacios, terminales y sus nombres/etiquetas), src/ezagent/agent/confirm.py (confirmaciones por voz) y src/ezagent/voice/app.py (lanzamiento previo de agentes, `No pude abrir … por adelantado`)
- Rust: `rust/crates/ez-agent/src/tools_herdr/**` (`delegate.rs`: `space` y `address`/`terminal`; `port.rs`: `HerdrPort` con `resolve_agent`, `label_for`; `summaries.rs`: textos de confirmación generados por código), `rust/crates/ez-agent/src/confirm.rs` (`ConfirmationManager::ask`, `submit_text`) y cómo llega a las herramientas (`runtime/tools.rs`, `tools_memory.rs` como ejemplo de gate), `rust/crates/ez-herdr/src/herdr*` (listado de espacios y terminales), `rust/crates/ez-app/src/app/activity/tasks.rs` (lanzamiento previo)

## Archivos que se tocan (solo estos)
- rust/crates/ez-herdr/src/** (el resolvedor de nombres, para que lo usen el agente y la app)
- rust/crates/ez-agent/src/tools_herdr/** y rust/crates/ez-agent/src/runtime* solo si hace falta pasar el gate de confirmación a las herramientas de Herdr
- rust/crates/ez-app/src/app/activity/** (lanzamiento previo) y el cableado mínimo en rust/crates/ez-app/src/app/core/** si hace falta
- pruebas nuevas en esos crates, `rust-migration/python-quirks.md` (añadir al final) y rust-migration/reports/P-IMP-NAMES-r1.md

Los módulos de tu crate ya existen como esqueleto vacío y están declarados en
su `lib.rs`; reemplaza el contenido. Si un módulo se pasa de 600 líneas,
divídelo en una carpeta con submódulos (`foo.rs` + `foo/bar.rs`).

## Fixtures de referencia (solo para entender formatos; no hay pruebas de paridad en esta fase)
- (ninguno)

## Notas específicas
- **Pedido del usuario:** «cuando diga un espacio o una terminal y el nombre no se transcriba bien, relaciona lo que se está transcribiendo con lo que tengo abierto y compara el resultado, y si hay duda pídeme confirmación del tipo «¿te refieres a tal?» para que yo diga sí o no». Caso real del log: el usuario dijo el espacio «TEST» y Whisper transcribió «TES», «TESS» y «TETS»; la app respondió `no space "TES"` y el asistente tuvo que preguntar. También «OpenCobe» por «OpenCode» y «Deedseed» por «DeepSeek». Es una **mejora deliberada respecto al Python**: documéntala en `python-quirks.md` y con `// DECISION`.
- **Resolvedor (en `ez-herdr`, reutilizable):** dado un nombre dicho y un tipo (espacio, terminal o agente/perfil), compáralo con los candidatos **vivos** de Herdr (espacios abiertos, terminales con su nombre y etiqueta, perfiles de agente configurados). Normaliza (minúsculas, sin acentos, sin signos ni espacios sobrantes) y puntúa con algo robusto a errores de transcripción: igualdad normalizada, prefijo/contenido, distancia de edición con transposiciones (Damerau-Levenshtein o Jaro-Winkler) y una **clave fonética sencilla** pensada para nombres en inglés dichos en español (dobles letras, b/v, c/k/q, s/z, t/d final, «ss»→«s», etc.). Devuelve: coincidencia exacta → usar; un único candidato muy probable y claramente mejor que el segundo → **usar sin preguntar** y decirlo en el resultado de la herramienta (por ejemplo, «usé el espacio «TEST»»); dudoso (puntuación media o dos candidatos parecidos) → **preguntar**; sin parecido suficiente → error que **lista los nombres disponibles** para que el asistente pregunte. Los umbrales se fijan con las pruebas (TES/TESS/TETS → TEST con confirmación o directo según puntuación; «OpenCobe» → «OpenCode»; nombres muy distintos → sin coincidencia).
- **Confirmación por voz:** cuando haya duda, la herramienta pide confirmación con el `ConfirmationManager` que ya existe (`ask`), con texto **generado por el código, nunca por el modelo** y en español neutro: «¿Te refieres al espacio «TEST»?», «¿Te refieres a la terminal «planner»?». El usuario responde «sí»/«no» por voz o con los botones de la barra (ya funciona así para otras confirmaciones). «Sí» → la acción sigue con el nombre resuelto; «no» o sin respuesta → la herramienta devuelve un error que lista los nombres disponibles. Aplica el resolvedor en **todas** las acciones de Herdr que reciben un espacio, terminal o agente por nombre.
- **Lanzamiento previo** (`activity/tasks.rs`): va en segundo plano y no puede preguntar; usa solo coincidencias muy seguras y, si hay duda, no lanza por adelantado (lo resolverá la herramienta con su confirmación).
- **Pruebas:** unitarias del resolvedor con los casos reales y con candidatos parecidos; de integración de la herramienta con un `HerdrPort` falso y un gate de confirmación falso (sí, no y sin respuesta). `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test -p ez-herdr -p ez-agent -p ez-app` y `cargo test -p ez-app --features replay --test replay_parity` en verde (≈7 min; los escenarios 07-09 delegan a agentes: no deben cambiar, porque ahí los nombres coinciden exactamente).

## Criterios de aceptación
- Todo lo público del Python está portado, con los mismos textos y valores por defecto.
- `cargo fmt --all`, `cargo clippy -p <crate> --all-targets -- -D warnings` y
  `cargo test -p <crate>` limpios, ejecutados desde `rust/`.
- Informe en `rust-migration/reports/P-IMP-NAMES-r1.md` y un commit en tu rama.
