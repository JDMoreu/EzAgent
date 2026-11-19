# ECO-1b — `ez-term`: correcciones de la revisión 1

**Lee `rust-migration/briefs/_COMMON-ECO.md`, `rust-migration/briefs/ECO-1.md` (la tarea
original, ya integrada), `docs/herdr/hc-protocolo.md` y `docs/designs/espacio-elefante.md`
§9 y §14 (manda §14).**

## Archivos que se tocan (solo estos)
- `rust/crates/ez-term/**`

## Cambios
1. **Secuencia (§14.9, M13).** Cada `TermEvent` que viene de una notificación `event`
   lleva su `seq: u64` (campo `params.seq`). `Missed { snapshot: Option<Vec<Terminal>>, seq: Option<u64> }`:
   si la notificación trae la foto, `Some` con su `seq`; si no se puede leer, `None` (el
   consumidor resincroniza con `HcClient::list`). Tras un `missed` con `seq`, el hilo de
   eventos **descarta** los eventos con `seq` ≤ ese valor. La foto inicial de la
   suscripción también guarda su `seq` (`EventStream::snapshot_seq()`).
2. **Topes (§14.8, m01).** Las llamadas simples usan 30 s; `send` usa 75 s; `wait` usa
   `timeout_s + 10 s`. Documenta los tres en `HcClient`.
3. **Lanzamiento completo de un pez (§14.2).** Añade
   `pub fn launch_and_deliver(&self, spec: &RunSpec, first_line: &str, ready_timeout: Duration) -> Result<(Terminal, Delivery), TermError>`
   con la transacción de §14.2: `run` → `wait` hasta que no esté `working` (tope
   `ready_timeout`) → `send` de `first_line` → si `started == Some(false)`, `read` (para el
   log, recortado a 500 caracteres), espera 5 s y reenvía una vez.
   `pub enum Delivery { Delivered, Unconfirmed, Failed(String) }` (`Unconfirmed` cuando
   `started == None`). Si `run` falla, devuelve el error; si falla después, `Failed` con el
   motivo y la terminal ya creada (el llamador decide si la mata).

## Pruebas
Con el hc falso: eventos con `seq`; `missed` con foto (descarta eventos viejos y deja
pasar los nuevos) y sin foto (`None`); topes por método (con un reloj o topes cortos
inyectables solo en pruebas); `launch_and_deliver` en los cuatro caminos (entregado,
`started` nulo, reintento que funciona, reintento que falla).

## Criterios de aceptación
fmt (`-p ez-term`), clippy y test de `ez-term` en verde; el ejemplo `hc_smoke` sigue
funcionando contra el hc real (pega su salida). Informe en `rust-migration/reports/ECO-1b-r1.md`.
