## Resultado
- Brief/tarea: ECO-REV0 — revisión estática del diseño, los briefs ECO-1..5 y sus contratos.
- Estado: completado
- Hallazgos: 10 Bloqueantes, 16 Mayores y 6 Menores.
- Alcance: contradicciones, decisiones abiertas, unión entre crates, elusión de la metodología y supuestos sobre hc/CLIs. No se modificó código ni documentación normativa.

## Hallazgos

### Bloqueante

#### B01 — El perfil predeterminado de OpenCode no puede lanzar el modelo elegido
- **Ubicación:** `docs/designs/espacio-elefante.md` §6, «`agentes.toml`»; `rust-migration/briefs/ECO-4.md`, «Módulos y API / launch».
- **Problema:** el valor predeterminado es `opencode --model {modelo}`, pero OpenCode v2.3.0 instalado no declara ese flag; el código existente ya documenta que el TUI lo rechaza (`src/ezagent/voice/config.py:118-128`). Además, `AgentProfile` y `launch::argv` solo devuelven argumentos: no ofrecen las variables de entorno ni el estado aislado que el lanzador actual usa para fijar modelo y variante. ECO-4 puede producir un contrato válido que ECO-6 no puede convertir en un lanzamiento correcto.
- **Propuesta:** hacer que el contrato de lanzamiento devuelva `argv + env` (y el estado aislado necesario), reutilizar el mecanismo ya probado de `OPENCODE_CONFIG_CONTENT`, `--standalone` y preparación de estado de OpenCode, y añadir una prueba de humo con el CLI real. No aceptar como válido un perfil que deje `{modelo}` o `{esfuerzo}` sin aplicar.

#### B02 — La identidad de un pez pierde espacio, tarea e intento al cruzar `ez-ctl` → `ez-arbiter`
- **Ubicación:** `rust-migration/briefs/ECO-3.md`, API `Caller`; `rust-migration/briefs/ECO-2.md`, API `Caller`, `fish_blocked` y `fish_exited`; `docs/designs/espacio-elefante.md` §8 R1 y §9.
- **Problema:** el transporte autentica `Fish { space, fish }`, pero el árbitro recibe solo `Fish { fish }`; las señales de hc también reciben solo `fish`. Los ids (`qa-r1`, `dev-r1-1`…) se repiten en cada tarea y espacio, por lo que el adaptador debe descartar información imprescindible o mantener una correspondencia ambigua. La situación empeora al reutilizar un id en una tarea posterior: `Tokens::revoke` existe, pero ninguna regla liga el token a una generación ni fija su revocación. Un terminal antigua puede conservar una credencial indistinguible de la nueva.
- **Propuesta:** usar en ambos crates `FishIdentity { space, task, fish, attempt }`, incluirlo en reportes y eventos hc, validar que sea la generación activa y revocar sus tokens al salir, ser sustituido y cerrar/detener la tarea.

#### B03 — El token del pez se publica en la bitácora y se registra demasiado tarde
- **Ubicación:** `rust-migration/briefs/ECO-2.md`, «API pública / `ArbiterEvent`», «Persistencia» y «Lanzar un pez»; `docs/designs/espacio-elefante.md` §8 R5/R12; `rust-migration/briefs/_COMMON-ECO.md`, «Seguridad».
- **Problema:** `fish_launched` contiene `token`, `subscribe()` lo difunde y R12/ECO-2 ordenan volcar cada `ArbiterEvent` a `bitacora.jsonl`. Esto contradice «claves y tokens nunca en logs». También hay una carrera: el pez recibe el token y puede ejecutar `ez report` antes de que la ventana reciba el evento y registre esa credencial en `Tokens`.
- **Propuesta:** registrar el token sincrónicamente en `ez-ctl` mediante un puerto/callback **antes** de lanzar el proceso. Quitar el token de `ArbiterEvent`, de la vista y de la bitácora; el evento público solo debe llevar una referencia opaca no reutilizable como credencial.

#### B04 — Se puede cerrar una ronda limpia sin ejecutar la cadena metodológica o con verificaciones obsoletas
- **Ubicación:** `docs/designs/espacio-elefante.md` §8 R5/R9/R10; `rust/crates/ez-ctl/src/skill.md`, «Flujo», pasos 5-7; `rust-migration/briefs/ECO-2.md`, comportamiento R4-R9.
- **Problema:** «todos los desarrolladores con trabajo» es verdadero por vacuidad si no se lanzó ninguno. R9 solo exige QA y validador, así que la primera ronda puede omitir planificador, revisor del plan y desarrolladores. Tampoco exige que `plan.md` exista ni que no queden peces con trabajo pendiente. Después del informe de QA se puede lanzar/corregir un desarrollador y cerrar con QA y validador anteriores a ese último cambio. Dos rondas así alimentan R10. Además, R5 no limita al planner: un segundo `planner` reutiliza exactamente `plan-r<n>`, su nombre hc y sus archivos.
- **Propuesta:** definir una máquina de fases: en la primera ronda exigir plan `pass` (y revisor `pass` si está en el equipo) y al menos un desarrollador; exigir que no haya trabajo pendiente y que `último cambio/informe de desarrollo < informe QA < informe validador`. Invalidar QA/validador si se agenda desarrollo posterior. Permitir una ronda posterior solo de verificación únicamente cuando no hubo cambios ni correcciones desde la ronda limpia anterior. Limitar planner a uno por ronda, igual que los demás papeles no ampliables.

#### B05 — `ronda limpia` acepta hallazgos graves e informes contradictorios
- **Ubicación:** `docs/designs/espacio-elefante.md` §7 `ez report`, §8 R7/R9; `rust/crates/ez-team/src/roles/_comun.md`; `roles/qa.md` y `roles/qa_validator.md`; `rust-migration/briefs/ECO-3.md`, argumentos de `report`.
- **Problema:** los contadores son flags independientes del Markdown y los omitidos valen cero. Un pez puede escribir `### Bloqueante` en su informe y enviar `--verdict pass` sin contadores; el árbitro no contrasta ambas fuentes. R9 tampoco exige cero Bloqueantes/Mayores del validador, ni de planificador/revisor/desarrolladores. Además R7 acepta cualquier archivo dentro del proyecto, no necesariamente el `<pez>.informe.md` asignado ni uno creado para ese trabajo: se puede señalar un informe favorable de otra ronda y el validador puede recibir una ruta `informe_qa` que no existe. El caso normal de corrección ya está roto: un `dev-r1-1` reutilizado en ronda 2 conserva instrucciones con su informe de ronda 1 y R6 no le da otra ruta; al repetir su comando sobrescribe la evidencia anterior y R7 acepta ese archivo para ronda 2.
- **Propuesta:** usar una única fuente estructurada de veredicto, hallazgos y mutaciones (o validar el Markdown y eliminar los flags duplicados); comprobar coherencia y evidencia. Asignar `work_id` y ruta nueva en cada lanzamiento **y corrección**, incluir esa ruta/comando en R6 y exigirla exactamente al informar. Copiar el informe al estado privado con hash al recibirlo y exigir cero Bloqueantes/Mayores en todos los informes de la ronda, incluido el validador.

#### B06 — El árbitro no impide escrituras del elefante ni cambios fuera del papel durante una ronda
- **Ubicación:** `docs/designs/espacio-elefante.md` §2, §8 R7/R9/R11; `rust/crates/ez-team/src/roles/*.md`; `rust/crates/ez-ctl/src/skill.md`, «Reglas».
- **Problema:** R11 solo compara el cierre anterior con el siguiente inicio; una edición del elefante entre `round.start` y `round.close` queda incorporada a la nueva foto y nunca se detecta. Antes de la primera ronda no hay cierre previo que sirva de base, y después de la segunda limpia el elefante puede editar y ejecutar `ez close`: R10 no repite R3/R8 ni compara git. Lo mismo ocurre con cambios de un pez posteriores a su informe, porque R7 toma la foto solo al informar. R9 solo castiga a `developer` por tocar pruebas protegidas: no rechaza código de producción escrito por QA, alteraciones de pruebas antiguas por otro QA ni rutas fuera del encargo del pez. Aceptar un cambio exterior al reanudar tampoco reinicia la racha, y puede concatenar verificaciones de árboles distintos.
- **Propuesta:** guardar una foto al aprobar, reconciliar otra al cierre con todos los escritores y repetir en R10 R3/R8 más la comparación con el último cierre limpio. Todo cambio exterior aceptado reinicia la racha. Declarar `allowed_paths` estructurados y aplicar políticas por papel: elefante sin código; planner solo su artefacto; QA solo pruebas nuevas; validador solo su copia temporal; desarrollador solo su alcance y nunca archivos protegidos. Ningún papel puede debilitar pruebas anteriores sin aprobación explícita.

#### B07 — Las fotos de git no detectan cambios de contenido en casos ordinarios
- **Ubicación:** `rust-migration/briefs/ECO-2.md`, «Git (R4, R7, R9, R11)»; `docs/designs/espacio-elefante.md` §8 R4/R7/R11.
- **Problema:** comparar líneas de `git status --porcelain` más `git diff <head1> <head2>` no es una foto del contenido. Si un archivo ya estaba `M` o `??`, editarlo sin cambiar HEAD ni ese estado produce exactamente la misma foto. Dos escritores sucesivos sobre el mismo archivo también quedan indistinguibles, y una edición después del informe desaparece de la atribución. Así se pueden tocar archivos protegidos o hacer cambios fuera de ronda sin activar R11.
- **Propuesta:** guardar HEAD más una huella de contenido y metadatos de cada ruta rastreada y no rastreada fuera de `.ez/` (con reglas explícitas para renombrados, binarios y enlaces), y comparar huellas en cada transición. Marcar como ambigua cualquier escritura concurrente sobre la misma ruta y fallar de forma segura; usar worktrees por pez si se requiere atribución exacta en paralelo.

#### B08 — Las reglas con E/S fuera del mutex son vulnerables a decisiones duplicadas concurrentes
- **Ubicación:** `rust-migration/briefs/ECO-2.md`, API `Arbiter`, comportamiento R4-R9 y criterio de locks; `rust-migration/briefs/ECO-3.md`, servidor con ocho hilos.
- **Problema:** el servidor admite llamadas simultáneas, pero el árbitro solo dice «estado bajo un Mutex; nunca retenido durante E/S». Sin una reserva atómica, dos `round.start` pueden observar que no hay ronda, dos `fish qa` pueden pasar el máximo antes de lanzar, o dos `round.close` pueden incrementar la racha mientras git/lanzamiento ocurre fuera del lock. Serializar todo con el mutex resolvería la carrera pero violaría la regla de no retenerlo durante E/S.
- **Propuesta:** definir un ejecutor secuencial por espacio (cola y hilo trabajador) o estados de transición/reserva atómicos con generación; toda decisión y reserva queda en el estado antes de liberarlo y el resultado confirma o deshace esa reserva. Añadir pruebas concurrentes para inicio, lanzamiento, corrección, informe y cierre.

#### B09 — ECO-5 exige un shell que las reglas comunes prohíben literalmente
- **Ubicación:** `rust-migration/briefs/ECO-5.md`, «Modo demostración»; `rust-migration/briefs/_COMMON-ECO.md`, «Calidad».
- **Problema:** el demo debe lanzar tres procesos `powershell.exe -NoLogo`, mientras las reglas comunes prohíben en el código añadido `"cmd"`, `"/c"`, `-Command` y «cualquier shell». Un ejecutor no puede cumplir a la vez el demo y la regla común sin decidir por su cuenta cuál violar.
- **Propuesta:** añadir una excepción estrecha y explícita en ECO-5 para `powershell.exe -NoLogo` interactivo solo en `--demo`, sin `-Command` ni evaluación de texto, o sustituir el demo por un binario falso interactivo permitido y ajustar sus criterios.

#### B10 — El encargo que aprueba la persona no queda congelado y es el que reciben los peces
- **Ubicación:** `docs/designs/espacio-elefante.md` §4 y §8 R2/R3/R11; `rust/crates/ez-team/src/contract.rs`, `InstructionContext.brief` y `criteria`.
- **Problema:** R2 copia encargo y criterios a `.ez/tareas/<tarea>/`, pero R3 solo congela criterios y las fotos git excluyen todo `.ez/`. Tras aprobar el encargo A, el elefante puede sustituir el `encargo.md` compartido por B; planner, desarrolladores y QA ejecutan B sin nueva aprobación. Incluso los criterios pueden cambiar entre las comprobaciones R3 para que un pez lea otra versión y restaurarse antes del cierre. El equipo aprobado tampoco recibe una revisión inmutable explícita.
- **Propuesta:** emitir una revisión de aprobación con hashes de encargo, criterios y equipo; hacer que todas las instrucciones lean copias congeladas bajo el árbol privado del árbitro y comprobar la revisión al lanzar cada pez y recibir cada informe. Cualquier cambio exige una nueva tarjeta/aprobación, no solo una pausa al llegar al siguiente `verify`.

### Mayor

#### M01 — Recargar tareas no recupera de forma transaccional el protocolo ni el canal de agentes
- **Ubicación:** `rust-migration/briefs/ECO-2.md`, «Persistencia»; `ECO-3.md`, `Tokens`, `CtlServer::start`; `docs/designs/espacio-elefante.md` §7 y §8.
- **Problema:** hc conserva terminales, pero el servidor nuevo obtiene otro puerto y `Tokens` vuelve vacío. Las terminales conservan `EZ_URL`/`EZ_TOKEN` antiguos, mientras `Arbiter::new` recarga tareas no finales como si pudieran continuar. Tampoco hay orden transaccional entre hc, `estado.json` y `bitacora.jsonl`: una caída tras lanzar y antes de persistir deja un pez huérfano; guardar un informe sin su evento (o al revés) rompe R12. La prueba de recarga no reconstruye esperas ni autorizaciones.
- **Propuesta:** elegir una recuperación completa: pausar no finales al arrancar, invalidar credenciales viejas y relanzar con `FishIdentity` nuevo; o conservar endpoint estable y secretos en almacenamiento privado protegido. Persistir `operation_id`, intención y confirmación en una secuencia, reconciliar hc por etiquetas y reconstruir eventos accionables. Probar cortes inmediatamente antes/después de lanzamiento e informe y un reinicio con `EZ_URL` real.

#### M02 — Los topes son puntos de control sin una transición viable para continuar
- **Ubicación:** `docs/designs/espacio-elefante.md` §5 y §8 R9/R10; `rust-migration/briefs/ECO-2.md`, `resume`; `docs/designs/modo-autonomo-elefante-pez-dorado.md` §2 y §10.13, conservados por la referencia de `espacio-elefante.md`.
- **Problema:** al llegar a `rondas_maximas`, reanudar sin cambiar el límite reproduce la pausa en el siguiente cierre; `horas_maximas` se mide desde la aprobación, por lo que reanudar tras superarla también vuelve a pausar y el tiempo en pausa cuenta. Los criterios congelados no permiten pedir más tiempo/rondas editando el TOML. A la inversa, R10 solo exige dos limpias y no prohíbe cerrar desde `paused`, de modo que el elefante podría saltarse la decisión de la persona al alcanzar un tope.
- **Propuesta:** acciones de persona `continue { extra_rounds, active_seconds }` y `accept`, guardadas como ampliaciones fuera de los criterios congelados; medir tiempo activo excluyendo pausas. R10 debe exigir estado no pausado/`between_rounds` y volver a comprobar R3.

#### M03 — No hay recuperación de peces en error/salida y los verificadores agotan su único intento
- **Ubicación:** `docs/designs/espacio-elefante.md` §8 R5 y «Eventos para el elefante»; `rust-migration/briefs/ECO-2.md`, señales `fish_blocked`/`fish_exited`; `rust/crates/ez-team/src/roles.rs`, verificadores.
- **Problema:** no existe `fish_error`/`fish_lost` aunque `ez-term` produce esos estados. Si QA o el validador sale antes de informar, R9 impide cerrar y el máximo de uno por ronda impide lanzar el reemplazo. El evento `fish_exited` informa al elefante, pero no cambia esta regla. Un proceso que entra en `Status.state=error` ni siquiera tiene una ruta de notificación definida.
- **Propuesta:** añadir transiciones `error/lost/exited_without_report` que pausen o habiliten un intento nuevo, con id de intento distinto, revocación del anterior y tope pequeño de reintentos. El límite de verificadores debe contar informes/peces válidos concurrentes, no impedir toda recuperación de un intento fallido.

#### M04 — Se heredan las reglas de churn, pero ninguna regla o dato las implementa
- **Ubicación:** `docs/designs/espacio-elefante.md` apertura y §8 R9; `docs/designs/modo-autonomo-elefante-pez-dorado.md` §2 y §10.14.
- **Problema:** el diseño v3 conserva las reglas de parada del diseño anterior, que exige escalar cuando un hallazgo corregido reaparece. R9 solo implementa topes y `mismo_error` de criterios; no guarda resoluciones ni compara hallazgos corregidos. Un pez nuevo puede reformular el mismo Mayor indefinidamente hasta agotar las rondas sin activar la pausa de churn.
- **Propuesta:** conservar un historial estructurado de hallazgos y decisiones (archivo, criterio, texto normalizado, ronda y resolución), reutilizar la comparación ya definida en §10.14 y pausar al reaparecer tras corrección, o retirar explícitamente churn de la normativa heredada.

#### M05 — Ocho esperas largas de ocho espacios bloquean también los informes
- **Ubicación:** `rust-migration/briefs/ECO-3.md`, «Servidor» y tiempos del cliente; `docs/designs/espacio-elefante.md` §7 `ez wait`.
- **Problema:** cada `ez wait` retiene un hilo durante hasta 170 s y hay exactamente ocho. Con ocho espacios esperando, `report`, `status` y `send` no tienen worker; su cliente de 120 s puede terminar en error antes de que se libere uno. `stop` también espera los hilos retenidos, con una parada de ventana potencialmente larga.
- **Propuesta:** separar las esperas del pool de comandos y hacerlas cancelables al detener el servidor; usar registros de espera/notificaciones o conexiones independientes, reservando siempre capacidad para `report` y comandos de control. Probar más de ocho esperas concurrentes y cierre durante una espera.

#### M06 — El timeout de criterios no termina necesariamente el árbol de procesos ni libera sus pipes
- **Ubicación:** `rust-migration/briefs/ECO-2.md`, «Verificación (R8)»; `docs/designs/espacio-elefante.md` §8 R8.
- **Problema:** `Child::kill` mata el proceso directo. Un `cargo test`, `npm test` o `python` puede dejar descendientes ejecutándose y manteniendo stdout/stderr abiertos; los lectores que esperan EOF pueden impedir que `verify` vuelva, y esos descendientes pueden seguir modificando el proyecto después de la verificación. La prueba con `ping` no cubre este caso.
- **Propuesta:** ejecutar cada criterio en un Job Object de Windows con terminación del árbol al vencer/cerrar (añadir la dependencia de workspace necesaria) y hacer que la recolección de salida tenga un tope independiente. Probar un comando padre que deja un hijo vivo y un pipe abierto.

#### M07 — El plan no valida estados OSC 7501, entrada inicial ni `ez report` con agentes reales
- **Ubicación:** `docs/designs/espacio-elefante.md` §3, §6, §9 y §12; `rust-migration/briefs/ECO-1.md`, `hc_smoke`; `ECO-4.md`, lanzamiento; `ECO-5.md` y fases posteriores con agentes falsos.
- **Problema:** el esquema de hc define `status` como lo que el programa reporta por OSC 7501; no garantiza que cualquier TUI lanzado con `argv` lo produzca. El smoke solo ejecuta `hostname` y la ola de extremo a extremo usa agentes falsos, así que no comprueba primera línea, transiciones `idle/working/blocked/done`, selección de modelo ni reporte con los tres CLIs. En Codex, además, el código existente ya señala que su sandbox puede impedir ejecutar el comando de protocolo fuera del proyecto (`src/ezagent/voice/config.py:212-214`).
- **Propuesta:** una prueba de compatibilidad por CLI y versión soportada: sesión nueva, primera línea recibida una vez, modelo/esfuerzo efectivos, ejecución real de `ez report`, y estado observado o fallback explícito. Codex necesita un mecanismo permite-`ez` acotado y probado, no `--yolo` implícito; ausencia de estado inicial debe producir una pausa diagnóstica, no una espera indefinida.

#### M08 — El elefante no tiene bootstrap ni comparte el resolvedor de perfiles de los peces
- **Ubicación:** `docs/designs/espacio-elefante.md` §3, §6 «Textos de los peces», §7 `ez --skill` y §11 ECO-6; `rust-migration/briefs/ECO-5.md`, creación de espacios; `ECO-4.md`, `profiles`/`launch`; `rust/crates/ez-ctl/src/skill.md`.
- **Problema:** hay primera línea y plantillas para peces, pero no contrato de arranque del elefante: nada le entrega `skill.md`, espacio, token o reglas. Además `espacios.json` guarda argv independientes y ECO-5 crea `["claude"]`, `["opencode"]` o `["codex"]`, ignorando perfiles personalizados, rutas detectadas, modelos y las correcciones de lanzamiento de `ez-team`. El elefante puede fallar aunque el mismo agente figure disponible para peces.
- **Propuesta:** guardar id de perfil/modelo/esfuerzo y resolver elefante y peces con la misma API de `ez-team`. Añadir a ECO-6 un bootstrap con instrucciones derivadas de `skill.md`, primera línea de una sola línea, `EZ_URL/EZ_TOKEN/EZ_ROLE=elephant`, espacio activo y comprobación de entrega/reanudación.

#### M09 — El id de tarea está abierto y cuatro caracteres no bastan para nombrar terminales globalmente
- **Ubicación:** `docs/designs/espacio-elefante.md` §4 y §9; `rust-migration/briefs/ECO-2.md`, «Lanzar un pez».
- **Problema:** no se especifica cómo se genera `<tarea>`, su charset/unicidad ni lo que devuelve `task.new`, pero se usa en rutas y nombres hc. `ez-{últimos 4 de la tarea}-{pez}` es global para todos los espacios; los CLI de peces pueden seguir vivos después de informar y los ids se repiten cada tarea. Una colisión produce `name_taken` y bloquea el lanzamiento; si el id deriva del título, puede no ser ni aleatorio ni compatible con ruta/nombre.
- **Propuesta:** fijar UUID para tarea y `TaskNewOutcome { task }`, usar su dirección hc como identidad interna y nombres solo decorativos con 8-12 hex más espacio/intento y truncado determinista a 32. Añadir una prueba de dos espacios con el mismo pez y tareas con el mismo título.

#### M10 — Los formatos JSON que cruzan crates y alimentan la vista no son contratos cerrados
- **Ubicación:** `docs/designs/espacio-elefante.md` §4 y §8 R12; `rust-migration/briefs/ECO-2.md`, `Outcome`, `ElephantEvent`, `ArbiterEvent`, `snapshot` y persistencia; `ECO-5.md`, `hub_state`/`card_action`.
- **Problema:** faltan variantes y campos con tipos/nullabilidad de eventos, formato de `ts`, estructura de `fish/timeline/cards` y esquemas de `estado.json`/`verificacion.json`. `card_action { card, action, comment }` no identifica espacio/tarea/tipo y una tarjeta de pausa debería mapear a `resume/stop`, no a approve/reject. `TeamPropose` exige una tarjeta, pero `team::card` necesita `task_title` y el adaptador no tiene getter tipado. Por último, `subscribe` + `snapshot` sin cursor puede perder o duplicar eventos al inicializar. `serde_json::Value` deja todas estas divergencias fuera de las pruebas de crate.
- **Propuesta:** publicar esquemas versionados y ejemplos de oro; definir `Card { id, space, task, kind }`, acciones cerradas (incluidas «Reanudar»/«Detener») y un getter tipado del título activo o dejar que el árbitro construya la tarjeta. Añadir `seq` monotónico y snapshot con `last_seq` para suscribir desde ese punto. Usar tipos concretos y pruebas de serialización en las fronteras estables.

#### M11 — El esfuerzo solicitado se puede descartar silenciosamente, incluso en el ejemplo del manual
- **Ubicación:** `docs/designs/espacio-elefante.md` §6, perfiles y `equipo.toml`; `rust/crates/ez-ctl/src/skill.md`, ejemplo de `equipo.toml`; `rust-migration/briefs/ECO-4.md`, `launch::argv` y validación.
- **Problema:** el ejemplo da Claude `esfuerzo = "high"`, pero su perfil predeterminado no tiene `esfuerzo_args`; `launch::argv` solo lo añade si esa lista existe, sin error ni aviso. Claude instalado sí admite `--effort`, y el launcher Python ya lo configura. Regla similar: un perfil personalizado sin sustitución de esfuerzo puede pasar validación y lanzar otro nivel del aprobado.
- **Propuesta:** añadir `esfuerzo_args = ["--effort", "{esfuerzo}"]` a Claude y hacer que validación/lanzamiento rechacen un `Seat.effort` no consumible. La tarjeta no debe prometer un esfuerzo que el argv/env final no contiene.

#### M12 — `criterios.toml` no define rangos ni toda la semántica de ejecución
- **Ubicación:** `docs/designs/espacio-elefante.md` §5 y §8 R8; `rust-migration/briefs/ECO-2.md`, «Verificación».
- **Problema:** solo se valida comando, id, carpeta y denylist. Nada impide `mutaciones_minimas = 0` (validación limpia con cero mutantes), `rondas_maximas = 0`, horas negativas/NaN o `segundos = 0`. Tampoco se fija si `contiene` examina stdout, stderr o ambos, cómo se intercalan, ni el tipo de cada campo numérico. Dos implementaciones pueden aprobar tareas distintas con el mismo archivo.
- **Propuesta:** publicar un esquema con tipos y rangos (`min_mutations >= 1`, topes finitos positivos, `segundos > 0`, descripción no vacía), y especificar captura y comparación de stdout/stderr con ejemplos. Rechazar valores no finitos y desbordamientos antes de crear la tarea. Mostrar `[validacion]` en la tarjeta de aprobación, que ahora solo muestra encargo/criterios y ocultaría una reducción de mutantes a cero.

#### M13 — La alternativa permitida de `Missed { snapshot: vec![] }` pierde justo los eventos necesarios para arbitrar
- **Ubicación:** `rust-migration/briefs/ECO-1.md`, «Eventos»; `docs/herdr/hc-protocolo.md`, «Eventos»; esquema `SubscribeResult` de hc 0.1.21.
- **Problema:** si no se confirma la forma, el brief permite entregar una foto vacía como si fuera recuperación. En ese hueco puede perderse `fish_exited`, `blocked` o `status`; la siguiente foto vacía no distingue ausencia real de datos desconocidos. Además, hc ordena descartar eventos anteriores al `seq` de la nueva foto, pero ningún `TermEvent` expone ese `seq` ni se exige filtrarlo internamente.
- **Propuesta:** no representar lo desconocido como foto vacía válida: o se parsea la foto con su `seq`, o se emite un evento de resincronización y se llama a `terminal.list` con las mismas etiquetas antes de continuar. Documentar el filtrado de secuencias y probar un `missed` con foto no vacía y eventos antiguos posteriores.

#### M14 — «Dentro de la carpeta» no especifica una validación segura de rutas
- **Ubicación:** `docs/designs/espacio-elefante.md` §5, §7 y §8 R2/R5/R7/R8; `rust-migration/briefs/ECO-2.md`, validaciones y git.
- **Problema:** convertir a absoluta no elimina `..`, enlaces ni junctions de Windows. Una ruta aparentemente interna puede resolver fuera del espacio para leer informes, copiar encargos o ejecutar criterios en otra carpeta. Reutilizar la carpeta como prefijo textual también confunde `proyecto` con `proyecto-otro`. El listado obligatorio de pruebas solo contempla «carpeta fuera», sin variantes canónicas.
- **Propuesta:** canonicalizar raíz y destino existente, validar por componentes, resolver el padre canónico para archivos nuevos y rechazar junctions/symlinks que escapen. Aplicar una única función a encargos, criterios, informes, correcciones, carpeta de criterio y copias; probar `..`, prefijos compartidos, enlaces y junctions.

#### M15 — `cargo fmt --all` contradice el aislamiento de archivos de peces paralelos
- **Ubicación:** `rust-migration/briefs/_COMMON-ECO.md`, «Alcance» y «Calidad»; `docs/designs/espacio-elefante.md` §11-12.
- **Problema:** cada pez solo puede tocar su crate y todos trabajan en paralelo, pero se le ordena ejecutar `cargo fmt --all` (modo escritura) sobre el workspace. Puede reformatear archivos de otros peces o de crates fuera de su brief y hacer que `git diff` incluya cambios sin dueño. El criterio del elefante sí debe comprobar todo el workspace, pero no hace falta que cada pez lo escriba.
- **Propuesta:** en los peces usar `cargo fmt -p <tu-crate>` y reservar `cargo fmt --all --check` para la puerta global; si se conserva `--all` al escribir, limitarlo a una fase sin escritores concurrentes y con autorización de alcance.

#### M16 — La entrega de `first_line` no tiene preparación ni comprobante en el contrato
- **Ubicación:** `docs/designs/espacio-elefante.md` §6 «Textos de los peces» y §9; `rust-migration/briefs/ECO-2.md`, `LaunchRequest`; `ECO-1.md`, `HcClient::run`/`send`; esquema hc `terminal.run`, `RunResult.first_message` y `terminal.send`.
- **Problema:** hc solo envía automáticamente el primer mensaje cuando `argv[0]` nombra un template; los perfiles usan programas normales, así que el adaptador debe hacer otro `send`, pero no se especifica cómo esperar a que el TUI esté listo ni qué hacer con `started = false/null`. Si se elige un template, `HcClient::run` deserializa a `Terminal` y descarta `RunResult.first_message`, eliminando justo el acuse `sent/started` necesario. La regla «mensaje de una línea» reduce el tamaño, pero no demuestra entrega.
- **Propuesta:** elegir una sola transacción de lanzamiento: template preservando y validando `first_message`, o `run → espera de preparación → send → verificación de turno` con timeout, reintento y limpieza. Añadir el resultado al contrato de `FishLauncher`; no marcar `fish_launched`/trabajo pendiente hasta confirmar entrega.

### Menor

#### m01 — El timeout especial de `send` no tiene parámetro de operación
- **Ubicación:** `rust-migration/briefs/ECO-1.md`, «Cliente de peticiones».
- **Problema:** se exige «para `send` y `wait`, el tope de la operación + 10 s», pero `wait` recibe `timeout_s` y `send` no; `call` también recibe un timeout aunque el texto fija 30 s por llamada. El implementador debe elegir si `send` usa 30, 40 o un default de hc.
- **Propuesta:** fijar que wrappers simples usan `call(..., 30 s)`, `wait` usa `timeout_s + 10 s`, y para `send` definir un valor concreto o añadir `timeout_s`/`SendOptions`.

#### m02 — `--json` tiene dos lecturas incompatibles
- **Ubicación:** `docs/designs/espacio-elefante.md` §7 tabla; `rust-migration/briefs/ECO-3.md`, argumentos y salida; `ECO-4.md`, `Inventory::json`.
- **Problema:** la tabla solo anuncia `ez inventory [--json]`; ECO-3 dice que cualquier salida puede ser el `Reply` completo con `--json` y además envía `inventory.args.json = true`. No queda claro si inventory debe imprimir `Inventory::json`, el envelope `Reply` o ambas cosas anidadas, ni si otros comandos aceptan el flag.
- **Propuesta:** elegir un único comportamiento: `--json` global imprime exactamente el `Reply` y `inventory.data` contiene el inventario, o `--json` solo de inventory imprime ese objeto; ajustar parser, servidor y pruebas de todos los comandos.

#### m03 — ECO-4 pide probar «los cuatro ejemplos» de familia, pero §6 solo contiene tres
- **Ubicación:** `rust-migration/briefs/ECO-4.md`, «family» y «Pruebas»; `docs/designs/espacio-elefante.md` §6, regla de familia.
- **Problema:** §6 ejemplifica `deepseek-v4.1-flash`, `qwen4-max` y `gpt-6-luna`; no hay un cuarto ejemplo. La prueba adicional `hy4-flash-free` no convierte retroactivamente la lista en cuatro y queda abierta qué vector falta.
- **Propuesta:** añadir el cuarto ejemplo y su resultado a §6 o cambiar el brief a «los tres ejemplos» y enumerar todos los vectores esperados, incluido `hy4-flash-free → hy`.

#### m04 — La validación de equipo no exige ids/modelos no vacíos
- **Ubicación:** `rust-migration/briefs/ECO-4.md`, `parse` y `validate`; `rust/crates/ez-team/src/contract.rs`, `Seat`.
- **Problema:** «modelo fuera de la lista» solo se comprueba si la lista conocida no está vacía. Con un inventario sin modelos —posible en Codex o si falla `modelos_cmd`— `model = ""` puede pasar; agente vacío y esfuerzo vacío tampoco tienen regla. El resultado puede contener argumentos vacíos o un modelo no aplicado.
- **Propuesta:** exigir `agent` y `model` recortados no vacíos y `effort = Some` no vacío siempre; distinguir inventario desconocido de fallo y exigir confirmación explícita para un modelo no verificable.

#### m05 — La copia temporal del validador tiene alcance y ubicación abiertos
- **Ubicación:** `rust/crates/ez-team/src/roles/qa_validator.md`, pasos 1 y 5.
- **Problema:** se pide copiar «la carpeta del proyecto» excluyendo solo `.ez/`, sin decidir si la copia incluye `.git`, `target`, dependencias o si puede crearse dentro del propio proyecto. Una carpeta temporal interna puede entrar recursivamente en la copia y contaminar/atribuirse a la ronda; incluir artefactos grandes puede hacer inviable la mutación dentro del tope de tiempo.
- **Propuesta:** fijar una raíz temporal fuera del proyecto con nombre único, excluir la propia copia y artefactos/generados enumerados (como mínimo `.git`, `target` y `.ez`), conservar lo que las pruebas necesiten y borrarla en un `finally`.

#### m06 — La barra del elefante puede duplicar bracketed paste y Enter al integrar con hc
- **Ubicación:** `rust-migration/briefs/ECO-5.md`, «Pantalla / Centro»; `docs/designs/espacio-elefante.md` §10; esquema `terminal.send` de hc.
- **Problema:** ECO-5 codifica texto como `ESC[200~ … ESC[201~` más `\r`, mientras el diseño dice que usará `hc terminal.send`, método que ya pega y envía Enter. Reutilizar la codificación del PTY de demo sobre `terminal.send` puede entregar los delimitadores como texto o enviar dos veces; cambiarla sin decidirlo deja dos comportamientos distintos entre demo y producción.
- **Propuesta:** en la integración llamar a `terminal.send` con el texto crudo, sin delimitadores ni `\r` añadidos; reservar la codificación manual para `view_write` en un PTY de demostración. Probar texto de una y varias líneas por ambas rutas.

## Decisiones
- No se eligieron alternativas de implementación: cada hallazgo incluye una propuesta para que el elefante cierre la especificación antes de delegar.
- Se consultó además `docs/designs/modo-autonomo-elefante-pez-dorado.md` porque `espacio-elefante.md` declara vigentes su metodología, hallazgos, paradas y denylist.
- Se usó `src/ezagent/voice/config.py` como evidencia de compatibilidad ya conocida; no se copió su alcance a este trabajo.

## Verificación
- Lectura completa de los 17 archivos solicitados y contraste cruzado con el esquema OpenRPC 0.1.21 y el diseño heredado citado por el propio v3.
- `opencode --version` → `opencode v2.3.0`; `opencode --help` no declara `--model`; `opencode models` sí devolvió ids uno por línea.
- `claude --help` declara `--effort <level>`; no se lanzó ninguna sesión de agente ni se modificó un proyecto.
- Revisión estática completa; no corresponden `pytest`, `ruff`, `cargo clippy` ni `cargo test` porque el único cambio es este informe.

## Pendiente o riesgos
- No se ejecutó una ronda real con hc, los CLIs ni dos espacios; los escenarios de carrera, reconexión, timeout de árbol y agotamiento de workers se deducen de los contratos y deben convertirse en pruebas de integración.
- La clasificación prioriza que el árbitro no declare limpio/done sin evidencia y que los crates se puedan unir sin inventar adaptadores implícitos; no evalúa diseño visual ni coste de modelos.
