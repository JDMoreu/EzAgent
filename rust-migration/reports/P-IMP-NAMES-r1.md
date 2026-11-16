# P-IMP-NAMES — Nombres de espacios y terminales mal transcritos (ronda 1)

## Resultado
- Brief/tarea: P-IMP-NAMES
- Estado: completado

Mejora sobre el Python: lo que dicta el usuario (espacio, terminal o agente) se
compara con lo que hay abierto en Herdr antes de usarlo. Una coincidencia exacta
se usa tal cual; una muy probable se usa y se cuenta en el resultado; una dudosa
se pregunta por voz con el `ConfirmationManager` ya existente («¿Te refieres al
espacio «TEST»?»); sin parecido suficiente la herramienta devuelve un error que
lista los nombres vivos. Se aplica en todas las acciones de Herdr que reciben un
nombre por parámetro.

## Cambios
- `rust/crates/ez-herdr/src/herdr/names.rs`: resolvedor del dominio (no existía en Python): normalización (minúsculas, sin tildes ni signos, espacios colapsados), clave fonética para nombres ingleses dichos en español, Damerau-Levenshtein, prefijo/contenido y umbrales; `Herdr::name_candidates(kind, fresh)` saca los candidatos vivos (espacios y terminales de `hc ls`, con la caché de 1,5 s primero; agentes de `agents`/`agent_profiles`) y `Herdr::resolve_safely` resuelve sin preguntar (para el lanzamiento previo). Corregidos los dos errores de compilación que dejó la pasada anterior y los avisos de clippy.
- `rust/crates/ez-herdr/src/herdr/names/tests.rs`: nuevo; 12 pruebas con los casos reales («TES»/«TESS»/«TETS» → «TEST» en duda, «OpenCobe» → «OpenCode» sin preguntar, alias «cloud code» → «claude»), candidatos parecidos, nombres distintos, espacios y terminales sacados de un runtime falso y `resolve_safely`.
- `rust/crates/ez-herdr/src/herdr.rs`: re-exporta `candidate_labels` (además de lo que ya dejó la pasada anterior).
- `rust/crates/ez-agent/src/tools_herdr/names.rs`: nuevo; `Names::pick` (exacta / muy probable con nota «usé el espacio «TEST»» / dudosa con confirmación / error con la lista) y `Names::attach`, que deja la nota en el resultado como `name_note`. Sin candidatos se usa el texto tal cual (comportamiento del Python y de los dobles de prueba).
- `rust/crates/ez-agent/src/tools_herdr/names/tests.rs`: nuevo; resolución exacta, muy probable y dudosa, y la herramienta completa con un `HerdrPort` falso y un gate falso respondiendo **sí**, **no** y **sin respuesta** (puerto que registra la dirección usada, para probar que «TES» acaba en `t3`).
- `rust/crates/ez-agent/src/tools_herdr/port.rs`: `HerdrPort::name_candidates(kind, fresh)` con implementación por defecto vacía (los dobles siguen viendo el comportamiento del Python) y la real sobre `Herdr`.
- `rust/crates/ez-agent/src/tools_herdr/args.rs`: `NameConfirm` y `HerdrCallbacks::confirm_name` (opcional; sin él la duda se rechaza y se listan los nombres).
- `rust/crates/ez-agent/src/tools_herdr/delegate.rs`: `get`, `read`, `send`, `focus`, `kill`, `watch`, `keys`, `configure`, `open_terminal`, `guide` y `run` resuelven `address`/`space`/`agent`; la validación de `keys`, `model`/`effort`, `lines` y `then` sigue ocurriendo antes de resolver (no se pregunta por una llamada inválida) y `kill` compara `last_address` con la dirección resuelta.
- `rust/crates/ez-agent/src/tools_herdr/delegate/run.rs` y `run/tests.rs`: nuevos; el lanzamiento y los encadenados salen de `delegate.rs` (se pasaba de 600 líneas al añadir la resolución) y `run` resuelve `agent`, los `agent` de `then`, `space` e `input_from`.
- `rust/crates/ez-agent/src/tools_herdr/delegate/tests.rs`: se queda con las validaciones de argumentos; dos textos esperados cambian porque el resolvedor sustituye «Agente desconocido: …» por la lista de agentes.
- `rust/crates/ez-agent/tests/tools_herdr_parity/fake.rs`: el doble de callbacks declara `confirm_name: None` (los fixtures son del Python y no tienen nombres dudosos).
- `rust/crates/ez-app/src/app/core/parts.rs`: `confirm_name` del grupo de herramientas se engancha al `ConfirmationManager` del runtime; el candado de `agents.agent` se suelta antes de esperar la respuesta (que puede tardar el plazo completo).
- `rust/crates/ez-app/src/app/activity/tasks.rs`: el lanzamiento previo solo usa coincidencias seguras del espacio; con duda no abre nada (lo resolverá la herramienta) y el lanzamiento de una cadena usa el nombre resuelto cuando es seguro.
- `rust-migration/python-quirks.md`: entrada Q-P-IMP-NAMES-01 (desviación deliberada pedida por el usuario, con el caso real del log).
- `rust-migration/reports/P-IMP-NAMES-r1.md`: este informe.

## Decisiones
- `// DECISION`: los umbrales del resolvedor (0,60 duda / 0,86 muy probable / 0,10 de ventaja / 0,82 prefijo / 0,90 fonética idéntica) se fijan con las pruebas. «TES», «TESS» y «TETS» quedan en duda (0,75–0,82) y preguntan; «OpenCobe» es muy probable (0,875) y se usa sin preguntar.
- `// DECISION`: la caché de agentes de 1,5 s se usa solo para el atajo de la coincidencia exacta; antes de preguntar o de rendirse se consulta `hc ls` en vivo, porque la terminal recién abierta puede no estar en la caché.
- `// DECISION`: sin candidatos (Herdr caído o doble de prueba) se usa el texto tal cual, igual que el Python; el resolvedor solo mejora cuando hay con qué comparar.
- `// DECISION`: con «no» o sin respuesta al nombre dudoso, la herramienta devuelve el mismo error que «sin parecido»: «No encontré la terminal «TES». Terminales abiertas: «planner», «TEST».» El nombre usado sin coincidencia exacta se cuenta en el resultado con la clave `name_note`.
- `// DECISION`: el texto del error de agente desconocido en `run` y `guide` pasa de «Agente desconocido: 'nope'. Usa action=agents…» a «No encontré el agente «nope». Agentes disponibles: …»: es la misma mejora (lista para preguntar) aplicada al parámetro `agent`.
- `// DECISION`: `voice_control` no resuelve su `address` (no habla con Herdr: solo registra la vigilancia en la app y esta olvida la terminal que no existe); ver «Pendiente o riesgos».
- `// DECISION`: el lanzamiento de una cadena que no puede preguntar usa el nombre resuelto si es seguro y, si no, el texto tal cual (como el Python).

## Verificación
- `cargo fmt --all -- --check`: limpio.
- `cargo clippy -p ez-herdr --all-targets -- -D warnings`: limpio.
- `cargo clippy -p ez-agent --all-targets -- -D warnings`: limpio.
- `cargo clippy -p ez-app --all-targets -- -D warnings`: limpio (solo los avisos de `vendor/freetype-sys`, ajenos a esta tarea).
- `cargo test -p ez-herdr`: 151 pasadas, 0 fallos (12 nuevas de `herdr::names`).
- `cargo test -p ez-agent`: 172 de la biblioteca y toda la paridad de integración en verde (10 nuevas de `tools_herdr::names`); `tools_herdr_parity` sigue pasando sin tocar casos.
- `cargo test -p ez-app`: 294 pasadas, 0 fallos.
- `cargo test -p ez-app --features replay --test replay_parity`: PENDIENTE_REPLAY
- pytest / ruff: no aplica (no se toca Python; es la especificación, de solo lectura).

### Fixtures consumidos
- Ninguno: el brief no define fixtures de paridad para esta mejora (`parity/fixtures/` no tiene un dominio de nombres). Los casos reales del log están codificados en las pruebas unitarias.
- `tests/tools_herdr_parity.rs` (dominio de P-AGENT-C) se ejecuta de nuevo: sus dobles no devuelven candidatos, así que todas sus salidas siguen siendo las del Python.

### Pruebas portadas (filas de `parity/test-map.csv`)
- Ninguna nueva: `parity/test-map.csv` no tiene filas de `tools_herdr.py` (ese dominio se cubre con `tests/tools_herdr_parity.rs`, de P-AGENT-C).

## Pendiente o riesgos
- `voice_control` con `action=close_and_watch` recibe un `address` que no pasa por el resolvedor: el resolvedor vive en el puerto de Herdr y ese manejador no lo recibe. Si el modelo pasa ahí un nombre dictado, la vigilancia apunta a una dirección que no existe y la app deja de vigilarla al primer `hc get`. Portarlo exige cambiar la firma de `handle_voice` (y su arnés de paridad); se dejó fuera del alcance del brief.
- Los umbrales están calibrados con los casos del log y con candidatos sintéticos: si aparecen transcripciones nuevas conviene añadirlas a las pruebas antes de mover las constantes.
- El resumen de confirmación de la política (`describe_fn`) sigue usando la dirección tal cual: si el modelo pasa un nombre dictado, la tarjeta dice «el agente indicado» (comportamiento del Python). La pregunta del resolvedor sí lleva el nombre real.
- `resolve_safely` y `Names::pick` pueden lanzar un `hc ls` extra por llamada cuando la caché no tiene el nombre; con la coincidencia exacta y la caché caliente (3 s, la misma antigüedad que el refresco del contexto) no se lanza ningún proceso.
