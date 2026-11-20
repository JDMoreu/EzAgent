# DX-5 — Si la persona no dice dónde ni quién, no se elige: se pregunta

**Lee `AGENTS.md`, `rust-migration/briefs/_COMMON-ECO.md`,
`ez-agent/src/text/orchestration_hint.txt` y `default_system_md.txt`.** Rama `work/DX-5`
desde `eco/main`.

## Pedido de la persona (20-nov)
«Cuando te delegue una tarea a un espacio o a un agente y yo no te diga de forma verbal o
textual cuál, no elijas por tu cuenta: mira qué espacios y qué agentes disponibles hay y
pregúntame si quiero que la haga alguno de ellos o un agente específico. A veces estoy en
un espacio de trabajo y me creas el agente en otro espacio y se me daña el flujo.»

## Lo que hace hoy (visto por el agente principal; confírmalo)
`ez-agent/src/tools_herdr/delegate.rs` `pick_space`: sin `space` en los argumentos queda
`None` y el lanzador decide; con `space`, el LLM puede haberlo **inventado**. El agente
(`agent`) también puede ponerlo el LLM sin que la persona lo haya dicho.

## Qué se hace
1. **La regla vive en el código, no solo en el prompt.** Al delegar (`run`/lanzamientos de
   `tools_herdr`, también `then` y el enjambre si eligen sitio): el espacio y el agente
   cuentan como «dichos» solo si aparecen en lo que la persona dijo o escribió en este
   pedido (`last_user_text` existe como gancho) o en su respuesta a la pregunta de abajo
   (con tolerancia: mayúsculas, tildes, «open code» = OpenCode, el nombre de un espacio o de
   una terminal viva). Si falta alguno, **no se lanza nada**.
2. **La pregunta la arma el código** con lo que hay ahora: el espacio **en el que está la
   persona** (el enfocado en Herdr) primero, los demás espacios, los agentes vivos (con su
   tipo y espacio) y los tipos instalados que podrían abrirse. Frase breve, en español
   neutro, para voz y chat, p. ej.: «¿Dónde lo hago? Estás en <espacio>. Puedo dárselo a
   <agente vivo> o abrir un agente nuevo ahí (OpenCode, Claude Code…). ¿O prefieres otro
   espacio?». La herramienta devuelve al LLM un resultado que obliga a preguntar (sin
   lanzar), con los datos estructurados para resolver la respuesta.
3. **Respuestas cortas**: «aquí / en este / en el actual» = el espacio enfocado; «con
   OpenCode» = un agente nuevo de ese tipo en el espacio elegido; el nombre de un agente
   vivo = enviárselo a él. Si la respuesta sigue siendo ambigua, se vuelve a preguntar una
   vez con las opciones que quedan.
4. **Nunca** se crea un espacio nuevo sin que la persona lo pida.
5. Ajusta `orchestration_hint.txt`/`default_system_md.txt` para que el LLM pregunte en vez de
   suponer (la regla del código manda aunque el prompt falle).
6. **Pruebas** con el puerto de Herdr falso: sin espacio ni agente dichos → no se lanza y la
   pregunta nombra el espacio enfocado, los otros y los agentes vivos; el LLM inventa un
   espacio que la persona no dijo → no se lanza, se pregunta; «aquí» → espacio enfocado;
   nombre de un agente vivo → se le envía a él; espacio y agente dichos desde el principio →
   se lanza sin preguntar; nunca se crea un espacio.

## Archivos
`ez-agent/src/tools_herdr/*`, los textos de `ez-agent/src/text/`, y en `ez-app` solo el
gancho del espacio enfocado/último texto si hace falta; sus pruebas. Nada de `ez-hud`
(DX-1i) ni del arranque de la app (DX-4 lo tiene abierto).

## Reglas
**Prohibido abrir ventanas**: nada de lanzar la app ni ejemplos con ventanas hasta que el
agente principal lo autorice (la persona está en una entrevista). Nunca toques la ventana
de Herdr de la persona: sus espacios se leen, no se cambian. Sin agentes reales. Compila en
primer plano, sin dos `cargo` a la vez.

## Entrega
fmt, clippy y test de `ez-agent` (y `ez-app` si lo tocas) en verde. Informe
`rust-migration/reports/DX-5-r1.md`, commit en `work/DX-5`.
