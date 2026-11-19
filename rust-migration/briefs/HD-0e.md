# HD-0e — Corrección de la revisión de HD-0d: falsos positivos del límite de uso

Revisión del agente principal sobre `work/HD-0d`: bien el `\r` (`strip_cr`), no matar
terminales vivas por «desconocido», engancharse a una viva con el mismo nombre y la sesión
nueva al cambiar de modelo (probado fuera de hc, 11 casos). **Mal**: C0-6 detecta el límite
con palabras sueltas («usage limit», «rate limit», «quota») en cualquier parte de la
pantalla y aunque el agente esté trabajando, y **cierra la terminal**. Un subagente que lee
o escribe sobre límites (IR-2 leerá el informe de IR-0, lleno de esas palabras) quedaría
`fallido` y cerrado. Es el mismo defecto que se corrigió en NV-1b; el criterio está en
`docs/designs/novedades-subagentes.md` §3 (léelo).

## Cambios (mismo alcance que HD-0d)
1. El límite solo cuenta si hc **no** reporta `working` (status `idle`, `blocked` o sin
   estado), la pantalla **no cambió** entre dos lecturas seguidas (≥ 15 s) y la frase está
   en las **últimas 15 líneas**.
2. Solo frases completas: «Free limit reached», «5 hour usage limit reached», «You've hit
   your usage limit», «Usage limit reached», «rate limit reached», «quota exceeded»
   (sin distinguir mayúsculas). Fuera las palabras sueltas.
3. Pruebas: el trabajador falso imprime «rate limit», «quota» y «usage limit» mientras está
   `working` (o con la pantalla cambiando) → no se toca; la frase de límite arriba de una
   pantalla larga → no; la frase al final con `idle` y pantalla estable → `fallido` y aviso.
   Ejecuta `probar.sh` dentro y fuera de hc.

## Entrega
`bash -n` y `probar.sh` en verde en los dos entornos. Informe `HD-0e-r1.md`.
