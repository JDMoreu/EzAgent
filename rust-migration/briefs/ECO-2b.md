# ECO-2b — `ez-arbiter`: correcciones de la revisión 1 (parte 1 de 2)

**Lee `rust-migration/briefs/_COMMON-ECO.md`, `rust-migration/briefs/ECO-2.md` (la tarea
original, ya integrada), su informe `rust-migration/reports/ECO-2-r1.md` y
`docs/designs/espacio-elefante.md` §8 y §14 (manda §14).** Esta parte cubre §14.1, §14.2
(lado del árbitro), §14.3, §14.6, §14.7 y los fallos de pez de §14.5. La parte 2 (ECO-2c)
hará las huellas de contenido, el ejecutor secuencial, el reinicio, las acciones de pausa
y los eventos tipados: **no** los hagas aquí.

## Archivos que se tocan (solo estos)
- `rust/crates/ez-arbiter/**`

## Cambios
1. **Ids e identidad (§14.1).** Id de tarea `t` + 12 hex de un uuid v4; `task.new`
   devuelve `data.task`. `pub struct FishKey { pub space, pub task, pub fish: String, pub attempt: u32 }`
   (serde) y `Caller::Fish(FishKey)`. Un pez solo actúa si su `FishKey` coincide
   **entera** con un trabajo vigente; si no: «Este pez ya no está activo en esta tarea.»
   (regla R1). `fish_blocked`, `fish_exited` y la nueva `fish_failed(&FishKey, motivo)`
   reciben la clave. Nombre hc `ez-{8 hex de la tarea}-{pez}` y `-a{n}` si `attempt > 1`;
   etiquetas `ez=1`, `ez_space`, `ez_task`, `ez_role`, `ez_fish`, `ez_attempt`.
2. **Tokens (§14.1).** `LaunchRequest` gana `token` (sigue también en `env` como
   `EZ_TOKEN`, porque el proceso lo necesita). `FishLauncher` gana `fn revoke(&self, token: &str)`.
   **Ningún** `ArbiterEvent` ni `ElephantEvent` lleva el token (quita el de `fish_launched`
   y la redacción de la bitácora: ya no hace falta). Se revoca al informar un verificador,
   al salir o fallar un pez, al fallar su trabajo y al terminar, rechazar o detener la tarea
   (todos los de la tarea). Prueba: el token no aparece en `bitacora.jsonl`, `estado.json`
   ni en ningún evento.
3. **Entrega (§14.2).** `FishLauncher::launch(&LaunchRequest) -> Result<Launched, String>`
   con `pub struct Launched { pub terminal: String, pub delivery: Delivery }` y
   `pub enum Delivery { Delivered, Unconfirmed, Failed(String) }`. `Err` o `Failed` → el
   trabajo queda `Failed`, se revoca el token, evento `fish_failed` y rechazo al elefante:
   «No pude entregar el encargo a {pez}: {motivo}». `Unconfirmed` sigue adelante y lo dice
   en el mensaje.
4. **Trabajos (§14.3).** `WorkItem { key: FishKey, round, kind: Launch | Correction(k), report_path, state: Pending | Reported | Failed, started_ts }`
   en el estado de la tarea. Informe en `ronda-<n>/<pez>[-a<n>].informe.md` (lanzamiento) o
   `ronda-<n>/<pez>[-a<n>].informe-c<k>.md` (corrección k); las instrucciones y el encargo
   del pez llevan el mismo sufijo de intento. Línea de corrección exacta:
   `Corrección {k} de la ronda {n}: lee {correccion} y al terminar informa con ez report usando --file "{informe}".`
5. **Informes (§14.3).** `report` exige un trabajo `Pending` de esa clave en la ronda abierta
   y que `file` sea exactamente su `report_path` (comparado tras canonicalizar); si no:
   «El informe debe ir en {ruta}.». `pass` con bloqueantes o mayores > 0 → rechazo:
   «El veredicto pass no puede llevar hallazgos bloqueantes o mayores.». Al aceptarlo se
   copia a `tareas/<tarea>/informes/` (privado) y se guarda su sha256.
6. **Fases (sustituyen R5, §14.3).**
   - Ronda 1: `planner` (uno) → `plan_reviewer` (si está en el equipo; exige el `pass` del
     planner) → `developer` (exige `pass` del planner y, si hay revisor, del revisor) → `qa`
     → `qa_validator`. En rondas siguientes, `planner` y `plan_reviewer` se rechazan:
     «El plan se hace en la ronda 1; para corregirlo usa ez send al planificador.»
     (`ez send` al planner sí vale en cualquier ronda).
   - `qa` exige al menos un `developer` con informe en esta ronda y ninguno pendiente,
     **o** una ronda solo de verificación: ningún trabajo de developer en esta ronda, la
     ronda anterior limpia y la foto de git igual a la de su cierre (ECO-2c cambiará la
     foto por la huella de contenido).
   - Con un `qa` ya lanzado en la ronda, lanzar o corregir un developer → «Ya empezó el QA
     de esta ronda: las correcciones van en la ronda siguiente.»
   - `qa_validator` exige el informe del `qa` de esta ronda.
   - Un trabajo vivo por papel verificador y planner por ronda; si su trabajo falló, se
     puede relanzar con `attempt + 1`, hasta 3 intentos por papel y ronda.
7. **Ronda limpia (§14.3).** `round.close` exige que no haya trabajo pendiente en la ronda
   («Hay trabajo pendiente: {lista}.»). Limpia = criterios pasan ∧ todos los informes de la
   ronda `pass` con 0 bloqueantes y 0 mayores ∧ mutaciones del validador ∧ sin violaciones
   ∧ artefactos intactos. Violaciones: las de ECO-2 más archivos del proyecto cambiados
   durante un trabajo de `planner`, `plan_reviewer` o `qa_validator`.
8. **Artefactos congelados (§14.6).** Al aprobar: copia privada con sha256 de
   `encargo.md`, `criterios.toml` y del equipo aprobado. Antes de lanzar o corregir un
   pez, de aceptar un informe, de verificar y de cerrar ronda o tarea se comparan los de
   `.ez/tareas/<tarea>/`: si difieren, la tarea pasa a `paused` (`artefactos_modificados`)
   y el comando se rechaza con ese motivo. Al reanudar esa pausa se restauran desde la copia.
9. **Rangos (§14.6).** `mutaciones_minimas` 1–20; `rondas_maximas` 2–20; `horas_maximas`
   finito, > 0 y ≤ 24; `segundos` 1–3600; `descripcion` no vacía. `contiene` se busca en
   stdout seguido de stderr. La tarjeta suma la línea
   `Validación: mutaciones mínimas {m}, rondas máximas {r}, horas máximas {h}.`
10. **Rutas (§14.7).** Una sola función `inside(root, path) -> Result<PathBuf, String>`:
    canonicaliza la raíz y el destino (o su padre si no existe) y compara con
    `Path::starts_with`. Úsala para encargo, criterios, encargo de pez, corrección, informe
    y `carpeta` de cada criterio.
11. **Tope de criterios (§14.7).** Al vencer: `taskkill /T /F /PID <pid>` (argumentos en
    lista, sin consola) y luego `kill`; los lectores de salida se abandonan 2 s después
    (canal con vencimiento, nunca `join()` sin límite).
12. **Fallos de pez (§14.5).** `fish_exited` o `fish_failed` antes de informar → trabajo
    `Failed`, token revocado, `ElephantEvent` `fish_failed` con el motivo; se puede relanzar
    (punto 6).

## Pruebas
Adapta las existentes y añade, con git real y dobles: identidad (clave vieja tras relanzar
→ rechazo); token ausente de bitácora, estado y eventos; revocaciones (verificador que
informa, salida, tarea detenida); `Delivery::Failed` y `Err` del lanzador; informe en ruta
equivocada; `pass` con mayores; cada regla de fase (incluida la ronda solo de
verificación y el developer tras el QA); tres intentos y el cuarto rechazado; artefacto
modificado → pausa y restauración al reanudar; cada rango fuera de límites; `inside` con
`..` y prefijo compartido (`proyecto` frente a `proyecto-otro`); tope que vuelve en menos
de 5 s con `ping -n 30 127.0.0.1` y `segundos = 1`.

## Criterios de aceptación
fmt (`-p ez-arbiter`), clippy y test de `ez-arbiter` en verde. Informe en
`rust-migration/reports/ECO-2b-r1.md`.
