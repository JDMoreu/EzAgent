# DX-5d — Distinguir pedidos iguales separados

Trabaja sobre `work/DX-5`; conserva las reglas y el alcance de DX-5 y DX-5c. Sin ventanas, agentes reales, credenciales ni cambios de foco.

La huella de DX-5c no identifica el pedido: dos pedidos con el mismo texto, o con los mismos primeros 160 caracteres, comparten elecciones aunque la persona haya abandonado la pregunta. Además depende de la tarea que escriba el LLM. El informe de DX-5c reconoce ambos casos; la autorización anterior no debe trasladarse a un pedido nuevo.

Vincula las elecciones al ciclo real de la pregunta y sus respuestas, mediante estado o identidad de turno controlados por la aplicación, no mediante similitud del texto generado por el LLM. Invalida lo pendiente al abandonar o sustituir el pedido, aunque una delegación posterior sea idéntica. Conserva respuestas cortas y la continuación legítima. Agrega pruebas que reproduzcan abandono seguido de pedido idéntico, pedidos con el mismo prefijo de 160 caracteres y continuación legítima; ninguno de los nuevos pedidos debe lanzar sin autorización propia. Si el alcance existente no permite identificar el ciclo de forma fiable, detente e informa qué archivos harían falta; no inventes una señal.

Ejecuta fmt, clippy y pruebas de los crates afectados; deja commit e informe `rust-migration/reports/DX-5d-r1.md`.
