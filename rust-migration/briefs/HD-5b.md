# HD-5b — Corrección de la revisión de HD-5

Revisión del agente principal sobre `work/HD-5` (ef5960c..184172f): campos, etiquetas y
paso a `RunAgent` están bien (ez-swarm 134 en verde; quitar el padre hace fallar dos
pruebas). Pero la regla de la raíz invierte el árbol al reabrir la raíz. Mismo alcance de
archivos que HD-5.

## R5-1. La raíz es siempre el primer miembro
Hoy la raíz es «el primer miembro con dirección que no sea el que se lanza». Si se
reabre (o reemplaza) el **primer** miembro, la regla elige al segundo como padre y el
árbol queda al revés; y si el primero aún no abrió, el tercero cuelga del segundo.
Regla nueva (actualiza también el comentario de §9 en el código):
- Raíz = el **primer miembro en orden de trabajo**, tenga o no dirección.
- Si el que se lanza **es** la raíz → sin padre (como hoy la raíz).
- Si no, padre = dirección de la raíz si la tiene; si no la tiene todavía → sin padre
  (y el `info!` actual).
Pruebas nuevas: reabrir la raíz → sin padre; con la raíz sin dirección, el tercero no
cuelga del segundo; y que la mutación «la raíz puede ser el propio miembro» haga fallar
alguna prueba.

## Entrega
fmt, clippy y test de `ez-swarm` en verde (y `ez-app` compila). Informe en
`rust-migration/reports/HD-5b-r1.md`.
