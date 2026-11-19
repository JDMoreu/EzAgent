# IR-3c — Un agotamiento sin hora de reinicio también vence

Revisión del agente principal sobre `b6cbba3`. Bien: por modelo, compatibilidad sin
`modelo`, vencidos fuera. **Mal**: «una entrada con `hasta: null` sigue vigente hasta que
el vigilante la reemplace». El vigilante solo escribe cuando ve un límite nuevo: un modelo
anotado sin hora (p. ej. los créditos de mcode, que no dan hora) quedaría `agotado` **para
siempre** y la recomendación lo evitaría aunque la persona recargue. IR-2b ya fijó la
regla para los avisos sin hora; `uso.json` debe usar la misma.

## Cambio (solo `ez-scan/src/usage/depleted.rs` y pruebas)
Una entrada sin `hasta` vence como en IR-2b, contando desde el momento en que se anotó:
plan gratuito (modelo `*-free`) a las 24 h; el resto, la ventana de `planes.toml` del
agente o 5 h. Si la entrada no trae el momento en que se anotó, usa la fecha de
modificación de `uso.json`. Pruebas: sin `hasta` y reciente → cuenta; sin `hasta` y más
viejo que la ventana → no cuenta; `*-free` sin `hasta` a las 23 h → cuenta, a las 25 h → no.

## Entrega
fmt, clippy y test de `ez-scan` en verde. Commit en `work/IR-3`, informe
`rust-migration/reports/IR-3c-r1.md`. Compila en primer plano.
