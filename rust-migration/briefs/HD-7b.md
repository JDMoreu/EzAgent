# HD-7b — Corrección de la revisión de HD-7

Revisión del agente principal sobre `work/HD-7` (470cafd..d0b5942): el módulo está bien
(201 pruebas en verde; la mutación «avisar también lo que hizo el padre» se detecta).
Falta cubrir los estados finales: **permitir `validado` → `terminado` no hace fallar
ninguna prueba**. Mismo alcance de archivos que HD-7 (solo pruebas, salvo que encuentres
un error real).

## R7-1. Tabla completa de transiciones
Una prueba que recorra los 16 pares `(desde, hacia)` de `TaskState` y exija que
`transition` acepte **solo** `en_proceso→terminado`, `en_proceso→fallido`,
`terminado→validado` y `terminado→fallido`, y que todo lo demás (incluidos los de
`validado` y `fallido`, que son finales, y los de un estado a sí mismo) devuelva
`Invalid` con el texto «<nombre> está «<estado>»; no puede pasar a «<nuevo>».» sin
cambiar el archivo (compara el JSON antes y después).

## R7-2. Estado inicial de `create`
Prueba de que `create` rechaza un registro que nace `terminado` o `validado`.

## Entrega
fmt, clippy y test de `ez-herdr` en verde. Informe en `rust-migration/reports/HD-7b-r1.md`.
