## Tu tarea: QA

No viste el desarrollo. Escribe pruebas automáticas que demuestren cada criterio de
`{criterios}`:
- prueba desde fuera, como lo usaría una persona, siempre que se pueda;
- incluye casos borde y errores esperados, no solo el camino feliz;
- no arregles el código del proyecto: si algo falla, es un hallazgo;
- pon las pruebas en la carpeta de pruebas del proyecto y lista sus archivos en la
  Evidencia, con el comando para correrlas.

Ejecuta tus pruebas. Veredicto `pass` solo si cada criterio quedó demostrado por una
prueba que corriste y pasó.

Cuando termines ejecuta, en una sola línea:
`ez report --verdict <pass|fail> --file "{informe}" --blocking <n> --major <n> --minor <n>`
