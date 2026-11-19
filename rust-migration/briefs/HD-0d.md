# HD-0d — Corrección urgente del supervisor: mata trabajadores sanos por un `\r`

Encontrado por el agente principal en uso real (19-nov, IR-1 con DeepSeek): **8 intentos en
~25 min, cada trabajador matado a los 2-3 min**. Causa comprobada: en Windows, Python
escribe `\r\n`; `terminal_state` recibe `"live\r"`, no coincide con `live` ni `finished`,
lo trata como «desconocido» (C0-3), a los 2 min pasa al intento siguiente y `free_name`
**mata la terminal viva**. Las pruebas de HD-0c no lo vieron porque corrían en una
terminal de hc con otro entorno. Es también la causa de los «intento 1 terminó sin informe»
de la mañana (versión anterior).

## Archivos
`rust-migration/tools/orquestacion/supervise3.sh` y `pruebas/`.

## Cambios
1. Quita `\r` de **toda** salida que el script compare o use como dato (`field`,
   `terminal_state`, `launch`, `free_name`, `session_id`, `report_summary`, `hc read`…):
   un ayudante único (`| tr -d '\r'`) en vez de arreglos sueltos.
2. **Nunca matar una terminal que hc reporte viva por un estado «desconocido»**: tras 2 min
   desconocido, sigue esperando (y lo dice cada 5 min); solo `free_name` antes de lanzar
   puede retirar una terminal **terminada**; una **viva** con el mismo nombre significa que
   el trabajador sigue: no se lanza otro intento.
3. **C0-6** (de la revisión): si la pantalla muestra un aviso de límite de uso («Free limit
   reached», «usage limit», «rate limit», «quota»), registra `fallido` con ese motivo y
   avisa al padre; no reintentes con el mismo modelo. Si `EZ_MODEL` cambió respecto de la
   sesión existente, abre una sesión nueva (no `--session` de la vieja).

## Pruebas (amplía `probar.sh`)
Un caso con un `python` falso que imprime `\r\n` (o forzando `PYTHON` a un envoltorio que
lo añada): el trabajador falso sigue vivo, el supervisor **no** lo mata y termina bien con
su informe. Un caso de límite de uso con pantalla falsa. Ejecuta `probar.sh` **también
desde Git Bash fuera de hc** (el entorno donde falló) y pega ambas salidas.

## Entrega
`bash -n` y `probar.sh` en verde en los dos entornos. Informe `HD-0d-r1.md`.
