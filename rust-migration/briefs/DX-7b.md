# DX-7b — Los humos no deben aparecer en la pantalla principal

Continúa en `work/DX-7`; lee `AGENTS.md` y `DX-7.md`. La persona exige que ninguna prueba abra ventanas en la pantalla principal. Esto prevalece sobre la opción de DX-7 de usar la principal cuando no hay secundaria.

Los humos que crean ventanas deben comprobar, antes de crear cualquier ventana o proceso que pueda crearla, que el monitor elegido existe y no es el principal. Si la secundaria no está disponible o no se puede verificar, falla cerrado y explica el motivo. No debe bastar con comprobar el rectángulo después de mostrar la ventana. Cubre también el guion PowerShell que inicia la app; no agregues una ruta de prueba que abra en la principal. Mantén intacto el comportamiento normal de la app sin la variable de prueba. Agrega pruebas de la validación con monitores simulados; si los archivos permitidos no bastan, informa antes de ampliar el alcance.

No abras ventanas, no ejecutes pruebas ni compilaciones todavía y no toques la app de la persona. Deja commit e informe `rust-migration/reports/DX-7b-r1.md` con las verificaciones pendientes.
