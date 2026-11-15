# P-SWARM — Paridad de ez-swarm (tablero, protocolo, servicio, vista, incidentes, recuperación y `ezs`) (ronda 1)

## Resultado
- Brief/tarea: P-SWARM
- Estado: completado

## Cambios
- `rust/crates/ez-swarm/tests/swarm_parity.rs`: prueba única `every_swarm_case_matches_python`; carga
  `parity/fixtures/swarm.jsonl`, despacha cada caso por `fn`, compara `output` (JSON estructural,
  texto byte a byte) o `error` (texto exacto), recoge todos los fallos y exige
  `assert_eq!(cases_run, cases.len())` + `assert!(cases.len() >= 644)`.
- `rust/crates/ez-swarm/tests/swarm_parity/support.rs`: directorio temporal por corrida, `BASE`
  (`datetime(2025,1,1,20,0)` en hora local), siembra de tableros con el lenguaje de
  `parity/gen/swarm_board.py` (join/task/claim/finish/reopen/fill/incident/post/reserve/release/
  incident_state/member_state/address/status), volcado normalizado de las 7 tablas de `board.db`
  (id → `<swarm>`, token → `<token>`, HOME → `<tmp>`, sello `YYYYMMDD-HHMMSS` → `<stamp>`, tiempos
  como segundos respecto a `BASE`, filas ordenadas por `json.dumps(sort_keys=True, ensure_ascii=True)`)
  y `diff_json` para informes compactos.
- `rust/crates/ez-swarm/tests/swarm_parity/simple.rs`: 422 casos de `protocol` (138), `models` (133),
  `progress` (20) e `incidents` (131).
- `rust/crates/ez-swarm/tests/swarm_parity/board.rs`: 133 casos de `store.*` (72, tablero directo y
  volcado completo) y `view.*` (61: board, health y summary).
- `rust/crates/ez-swarm/tests/swarm_parity/flow.rs`: 89 casos de `service.handle` (49 transcripciones
  de `ezs` con `board.db` byte a byte), `recovery.resolve` (21, con Herdr falso, traspaso y
  asked/announced) y `cli.*` (19).
- `rust-migration/python-quirks.md`: añade `Q-P-SWARM-01` (minutos `:60` en `parse_reset`).
- `rust-migration/reports/P-SWARM-r1.md`: este informe.

No se modificó ningún archivo de `rust/crates/ez-swarm/src/`: los 644 casos ya coincidían con el
oráculo. Todos los fallos vistos durante el desarrollo fueron defectos del arnés (ver «Decisiones»).

## Decisiones
- **Un solo `#[test]`**: los casos de `cli.*` tocan el entorno del proceso (HOME, APPDATA,
  XDG_CONFIG_HOME, EZS_*) y el oráculo comparte directorio temporal entre casos; en serie no compiten.
- **Carpeta única por caso** (`home/cases/<ruta del caso>`): el generador usa una carpeta por caso y
  `view.health/<id>` y `view.summary/<id>` comparten nombre lógico; reutilizarla contaminaba la base
  (ids de tarea de otro enjambre). Es un defecto del arnés, no del crate.
- **`now` se lee como `f64`**: en los fixtures aparece como `1000.0`/`700.0`; `as_i64` devuelve `None`
  para un número flotante de `serde_json`.
- **`protocol.open_task_line` y `protocol.wake_line`**: el generador siempre usó `command="ezs"` y no
  lo guarda en la entrada; el arnés usa `"ezs"` cuando falta (igual que el oráculo).
- **`protocol.briefing`**: la entrada solo guarda nombres de miembro; el arnés reconstruye
  rol/agente/modelo por caso tal como los arma `parity/gen/swarm.py` (equipo `ana/bo/cy`,
  `solo`, `mismo-rol`, `mixto`, `ajeno`, `otro-rol` y `rol-*`).
- **Resultados de `store.*`**: se aplica `_jsonable` del generador (normalizar todo texto, incluidos
  ids dentro de listas/dicts) y `open_incident` se compara como `[incident, bool]`. El caso
  `create-ok` muestra literalmente `<swarm>` porque el id creado no es el del tablero sembrado.
- **`recovery`**: la carpeta de traspaso es `home/handoff/<id del caso>` (el `id` no viaja en la
  entrada del fixture); `wall` y `clock` congelados en `BASE`; `reset_launches` es un no-op.
- **CLI**: se usan las variantes inyectables (`endpoint_path_from`, `read_port_from`, `main_with`) y,
  alrededor del grupo, se emula el perfil congelado del oráculo (`HOME`/`USERPROFILE` al temporal,
  `APPDATA`/`XDG_CONFIG_HOME`/`EZS_*` por caso); todo se restaura con un guardián.
- **Temp**: los árboles temporales propios se borran con `remove_file`/`remove_dir` recursivo, nunca
  `remove_dir_all`.
- **Submódulos**: el límite de 600 líneas obliga a repartir la prueba en `tests/swarm_parity/*.rs`
  (1855 líneas en total; el mayor, `support.rs`, 543). El punto de entrada sigue siendo
  `tests/swarm_parity.rs`.

## Verificación
- `cargo fmt --all -- --check`: limpio.
- `cargo clippy -p ez-swarm --all-targets -- -D warnings`: limpio.
- `cargo test -p ez-swarm`: **113 passed, 0 failed** (112 unitarias que ya existían + 1 prueba de
  paridad que consume los **644 casos**). Dos corridas seguidas en verde (ids y tokens aleatorios).
- Sin `todo!`, `unimplemented!`, `#[ignore]`, `TODO`, `println!`, `remove_dir_all`, `"cmd"`, `"/c"`,
  `-Command`, `unsafe` ni `.unwrap()` en lo añadido; ningún `.rs` supera 600 líneas ni hay función
  de más de 60 (salvo tablas de `match`, partidas).
- Prueba de mutación manual: cambiar `MAX_BODY` 280 → 279 hace fallar 2 casos y el mensaje señala el
  caso exacto; revertido (el árbol de `src/` queda intacto).

### Por dominio
| Dominio (`fn`) | Casos | Errores esperados | Fallos en la primera corrida medida (arnés) | Defectos de Rust corregidos | Quirks |
|---|---:|---:|---:|---|---|
| `protocol.*` | 138 | 0 | 25 | ninguno | ninguno |
| `models.*` | 133 | 31 | 0 | ninguno | ninguno |
| `progress.check` | 20 | 0 | 0 | ninguno | ninguno |
| `incidents.*` | 131 | 0 | 0 | ninguno | `Q-P-SWARM-01` |
| `store.*` | 72 | 22 | 14 | ninguno | ninguno |
| `view.*` | 61 | 2 | 0 | ninguno | ninguno |
| `service.handle` | 49 | 0 | 0 | ninguno | ninguno |
| `recovery.resolve` | 21 | 4 | 1 | ninguno | ninguno |
| `cli.*` | 19 | 0 | 0 | ninguno | ninguno |
| **Total** | **644** | **59** | **40** | **ninguno** | 1 |

«Fallos en la primera corrida medida» cuenta los 40 casos que fallaron en la primera corrida
completa con el arnés ya compilando: 25 de `protocol` (`command` ausente en `open-task`/`wake` y
roles de `briefing`), 14 de `store` (normalización de `<swarm>` en los resultados, `open_incident`
sin tupla y `create-ok`) y 1 de `recovery` (carpeta de traspaso). Una corrida anterior, sin números
registrados, falló además en `view` por la normalización del texto, la carpeta reutilizada entre
`health` y `summary` y `now` leído como entero. Ninguno se debió al comportamiento del crate: al
corregir el arnés pasaron los 644.

### Fixtures consumidos
- `parity/fixtures/swarm.jsonl`: **644 casos** (59 con `error`), todos ejecutados.

### Pruebas portadas (`parity/test-map.csv`)
Las filas de `test_map.csv` para `ez-swarm` (`tests/test_swarm_*.py`) son pruebas unitarias de Python
y siguen marcadas `pendiente`; este archivo es de solo lectura en la fase de paridad. Esta tarea
cubre el oráculo JSONL completo del dominio `swarm` (644 casos), que es la fuente de verdad indicada
por el brief.

## Pendiente o riesgos
- **Zona horaria**: el oráculo se generó con la hora local del equipo (`TZ=UTC` no aplica en Windows
  porque `time.tzset` no existe). El arnés deriva `BASE` de la misma hora local, así que las
  diferencias se cancelan; solo un huso con cambio de horario entre el 1 y el 2 de enero de 2025
  alteraría `incidents.when`/`describe` y los deltas de `parse_reset`. En este equipo (UTC-5) pasa.
- **`Q-P-SWARM-01`**: el Rust no reproduce el `ValueError` de Python con minutos `:60` (devuelve
  `0.0`, DECISION de F3-12). No hay fixture para ese texto; si se decide reproducirlo, hay que
  tocar `incidents/patterns.rs` y añadir el caso al generador.
- `SwarmService` expone más superficie que la que usa el fixture (`start`, `pending`, `wake_line`,
  `join_line`, `tell`, `pause/resume/stop`); el oráculo solo ejerce `handle` y `issue_token`. Esas
  rutas siguen cubiertas por las pruebas unitarias del crate, no por esta paridad.
- El arnés del CLI prueba las variantes con entorno inyectable, no los envoltorios que leen
  `std::env` (dos líneas sin lógica).
- Las pruebas dejan su carpeta temporal solo si Windows mantiene un `board.db` abierto al terminar;
  en condiciones normales se borra al final del test.
