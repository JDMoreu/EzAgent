# ECO-2c — `ez-arbiter`: correcciones de la revisión 1 (parte 2 de 2)

**Lee `rust-migration/briefs/_COMMON-ECO.md`, `ECO-2.md`, `ECO-2b.md` (ya integradas),
sus informes en `rust-migration/reports/` y `docs/designs/espacio-elefante.md` §8 y §14
(manda §14).** Esta parte cubre §14.4, el resto de §14.5 y §14.8 (eventos y tarjetas).

## Archivos que se tocan (solo estos)
- `rust/crates/ez-arbiter/**`

## Cambios
1. **Huella de contenido (§14.4).** Sustituye la foto de `git status` por
   `Fingerprint { head: Option<String>, files: BTreeMap<String, String> }` (ruta relativa
   con `/` → sha256 en hex) de los archivos que lista
   `git -C <carpeta> ls-files -co --exclude-standard -z`, sin nada bajo `.ez/`. Los
   archivos que no se pueden leer cuentan con el hash `"ilegible"`. Si hay más de 20 000
   archivos o 500 MB, se calcula igual y se deja un aviso en el log.
2. **Cuándo se toma:** al aprobar la tarea, al lanzar o corregir un pez (inicio de su
   trabajo), al aceptar cada informe, al abrir y cerrar ronda y en `task.close`.
3. **Atribución:** los archivos que cambian entre la huella de inicio de un trabajo y la de
   su informe se atribuyen a su papel (con developers en paralelo, al papel `developer`).
   Las violaciones de ECO-2b se calculan con esta atribución.
4. **Cambios del elefante:** antes de cada comando del elefante, si no hay ningún trabajo
   pendiente, se compara la huella actual con la última registrada; si difiere → `paused`
   (`cambios_fuera_de_ronda`) y el comando se rechaza con ese motivo. Al reanudar esa
   pausa, `rondas_limpias_seguidas` vuelve a 0 y la huella actual pasa a ser la registrada.
5. **Ronda solo de verificación y cierre:** la ronda solo de verificación compara con la
   huella del cierre de la última ronda limpia. `task.close` exige estado `between_rounds`,
   repite R3 y R8 y exige que la huella actual sea igual a la de ese cierre; si no:
   «El proyecto cambió desde la última ronda limpia: hace falta otra ronda.».
6. **Ejecutor secuencial (§14.5).** Un hilo `ez-arbiter-<espacio>` por espacio con su
   cola: `execute`, las acciones de la persona y las señales `fish_*` se encolan y el
   llamador espera su resultado. `wait_for_elephant`, `subscribe` y `snapshot` no pasan
   por la cola. El `Mutex` del estado solo se toma para leer o escribir, nunca durante git,
   verificación o lanzamiento.
7. **Reinicio (§14.5).** `Arbiter::new` pasa las tareas no finales a `paused` (`reinicio`)
   y sus trabajos pendientes a `Failed`, con sus eventos.
8. **Acciones de pausa (§14.5).** `pub fn more_time(&self, space: &str)`: +2 rondas y
   +1 h, guardadas en el estado de la tarea (no en los criterios) y reanuda. El tiempo
   cuenta solo fuera de pausa (guarda inicio y total de pausas). `resume` y `stop` siguen.
9. **Eventos tipados (§14.8).** `ArbiterEvent { seq: u64, ts: f64, space: String, task: Option<String>, kind: EventKind }`
   con `EventKind` enum (serde `tag = "type"`, `snake_case`) y un payload tipado por
   variante: `task_proposed`, `task_approved`, `task_rejected`, `team_proposed`,
   `team_approved`, `team_rejected`, `round_started`, `fish_launched`, `fish_reported`,
   `fish_blocked`, `fish_failed`, `verification`, `round_closed` (con `clean` y motivos),
   `paused`, `resumed`, `task_closed`, `task_stopped`, `rejected` (regla y mensaje),
   `card_opened(Card)` y `card_closed { id }`. `seq` es monotónico en el proceso y también
   se escribe en la bitácora. `snapshot(space)` incluye `last_seq`.
10. **Tarjetas (§14.8).** `Card { id, space, task, kind: Task | Team | Pause, title, body }`.
    Se abre una al proponer tarea o equipo y al pausar; se cierra al decidir. Acciones:
    `approve`/`reject` (Task, Team) y `resume`/`more_time`/`stop` (Pause). Añade
    `pub fn card_action(&self, card: &str, action: &str, comment: Option<&str>) -> Result<Outcome, Rejection>`
    que despacha a la acción correcta y rechaza acciones que no corresponden al tipo.

## Pruebas
Huella: editar un archivo ya modificado se detecta; un archivo sin seguimiento nuevo se
detecta; `.ez/` se ignora; atribución a papel con dos developers. Cambio del elefante
entre trabajos → pausa y racha a 0 al reanudar. `task.close` con el proyecto cambiado →
rechazo. Concurrencia (dos hilos a la vez): dos `round.start`, dos `fish qa`, dos
`round.close` → una sola transición en cada caso. Reinicio → `paused` y trabajos `Failed`.
`more_time` amplía y descuenta pausas. `seq` creciente y `last_seq` en `snapshot`.
`card_action` con cada tipo y una acción inválida.

## Criterios de aceptación
fmt (`-p ez-arbiter`), clippy y test de `ez-arbiter` en verde. Informe en
`rust-migration/reports/ECO-2c-r1.md`.
