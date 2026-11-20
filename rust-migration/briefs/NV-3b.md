# NV-3b — Corrección de la revisión de NV-3: que la app vea de verdad a sus subagentes

Revisión del agente principal sobre `work/NV-3` (467ebcd … b7d1d64). Bien: registro de los
lanzamientos bajo `voz`, frases por código, una vez por episodio, «¿cómo va X?» por atajo
y herramienta, oferta de reintento, pruebas sin tocar el registro real, humo sin `PÁNICO`.
**Mal**, dos cosas que dejan sin hacer lo central de §5 (las dejaste anotadas en riesgos):

1. **Nadie vigila a los hijos de la app.** El vigilante de NV-1 exige un padre que sea una
   terminal viva, así que para `voz` nadie escribe `bloqueado` (límite de uso, diálogo),
   ni `fallido` cuando un hijo muere sin informe, ni el «sin avance». La app solo anuncia lo
   que escriben `ez report`/`ez validate`. El caso que originó el diseño (un hijo parado por
   «Free limit reached» que nadie marcó) seguiría igual si lo lanza la app.
2. **El reintento no puede funcionar.** `ez retry` lanza con `hc run --parent <padre del
   registro>`; con `HC_TERMINAL_ID=voz` eso es `--parent voz`. Comprobado por el elefante:
   `hc run --parent voz …` → `{"error":{"kind":"not_found","message":"no terminal \"voz\""}}`.

## Cambios
1. **Vigilante dentro de la app para `voz`.** Reutiliza el `Watcher` de
   `ez-herdr/src/subagents/watch*` (no lo dupliques): sepárale **dónde entrega los avisos**
   (hoy `hc send` al padre) con un rasgo pequeño (p. ej. `NoticeSink`), y un modo en que el
   padre no es una terminal (la vida del padre = la de la app). En `ez-app`, un hilo con ese
   vigilante para `voz` cuyos avisos pasan por el mismo camino de anuncios de NV-3 (nada se
   ejecuta del texto). `ez watch` sigue igual para padres-terminal.
2. **Reintento de los hijos de `voz`.** El hijo nuevo se lanza **como la app lanzó al
   original** (sin `--parent` de `hc`, misma colocación), con el registro bajo `voz` y
   `retry_of`. Elige tú si es por biblioteca con el puerto de la app o arreglando el camino
   de `ez retry` para un padre que no es terminal; `# DECISION` y una prueba en cada crate
   que toques. Nunca `--parent voz`.
3. **Pruebas**: un hijo falso **real** en `hc` (de `ez-fakes`) registrado bajo `voz` que
   muestra «Free limit reached» parado → la app anuncia `bloqueado` sin que nadie escriba el
   registro a mano; el mismo hijo muerto sin informe → `fallido` anunciado; reintento
   aceptado → nace `<nombre>-r1` sin `--parent voz` y con `retry_of`. Sin agentes reales.
4. **Humo**: amplía `diag_subagents_smoke` para que use ese hijo falso real (no solo
   registros escritos a mano) y pega la salida literal.

## Archivos
`ez-app` (lo de NV-3), `ez-herdr/src/subagents/watch*.rs` y `subagents/retry*.rs` solo para
el rasgo de entrega, el modo sin padre-terminal y el lanzamiento sin `--parent`; `ez-ctl`
solo si arreglas ahí `ez retry`. Sus pruebas.

## Entrega
fmt, clippy y test de `ez-app`, `ez-herdr` (y `ez-ctl` si lo tocas) en verde. Commit en
`work/NV-3`, informe `rust-migration/reports/NV-3b-r1.md`. Compila en primer plano, sin dos
`cargo` a la vez. No toques la app abierta de la persona ni la ventana de Herdr; cierra
todas las terminales de prueba.
