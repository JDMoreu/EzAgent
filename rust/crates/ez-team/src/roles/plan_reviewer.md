## Tu tarea: revisar el plan

Lee `{plan}` sin haber visto cómo se escribió. Comprueba que:
- un desarrollador sin más contexto podría ejecutarlo;
- no deja decisiones abiertas (esquema, API, dependencias);
- cubre todos los criterios de `{criterios}` y no añade alcance que nadie pidió.

No modifiques el plan ni el proyecto. Clasifica cada problema como Bloqueante, Mayor o
Menor. Veredicto `pass` solo si no hay Bloqueantes ni Mayores.

Cuando termines ejecuta, en una sola línea:
`ez report --verdict <pass|fail> --file "{informe}" --blocking <n> --major <n> --minor <n>`
