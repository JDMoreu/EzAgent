# DX-5b — «Estás en <espacio>» tiene que ser el espacio donde está la persona

Revisión del agente principal sobre `work/DX-5` (f80b516 … 3efe3b2). Bien: la regla vive
en el código (`place.rs`), pregunta armada por código con datos estructurados, tolerancia
de nombres, `PendingPlace` sin ida y vuelta infinita, nunca crea espacios, 12 pruebas.
**Mal**: la decisión «el espacio enfocado de la app es el de su terminal padre `voz`».
`voz` **no es una terminal** (NV-3: `hc run --parent voz` da `not_found`), así que la
pregunta nunca sabría dónde está la persona, que es justo su queja («estoy en un espacio y
me creas el agente en otro»). Y no es cierto que Herdr no exponga el foco; comprobado por
el elefante, de solo lectura y sin tocar la ventana:

```
herdr-app status   → {"space": "sped17256b0e…", "tab": "…", "focused_pane": "…", "agent": "t6h", …}
herdr-app ls       → {"spaces": [{"id": "sp…", "name": "prueba-Spinning-center", "active": false, "tabs": […]}, …]}
```

## Cambio
1. `focused_space` de la app = el nombre del espacio cuyo `id` es `status.space` (con `ls`
   para el nombre), leído en el momento de preguntar. Por el adaptador de `herdr-app` que ya
   existe en `ez-herdr` (puedes tocar lo mínimo ahí, solo lectura: `status` y `ls`). Si
   `herdr-app` no responde, vacío: la pregunta lista los espacios sin «Estás en…».
2. «aquí / en este / el actual» resuelve a ese espacio enfocado.
3. Pruebas: con `status`/`ls` falsos → la pregunta empieza por el espacio enfocado real y
   «aquí» lo elige; sin `herdr-app` → pregunta sin «Estás en» y no lanza.
4. Corrida real de solo lectura: un ejemplo o prueba ignorable que imprima el espacio
   enfocado que ve la app en esta máquina (debe coincidir con `herdr-app status`). Nada de
   ventanas ni de cambiar el foco.

## Entrega
fmt, clippy y test de `ez-agent`, `ez-app` (y `ez-herdr` si lo tocas). Commit en
`work/DX-5`, informe `rust-migration/reports/DX-5b-r1.md`. Compila en primer plano.
