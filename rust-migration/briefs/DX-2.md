# DX-2 — Propuesta: respuestas cortas y naturales a «ok», «listo», «bueno»

**Encargo de análisis y propuesta. No cambies el código del producto.** Rama base:
`eco/main`.

## Pedido de la persona
Cuando EzAgent le avisa de algo (un agente terminó, un recordatorio, una pregunta) y ella
responde con algo corto como «ok», «listo», «bueno», «dale», «perfecto», «gracias», la
respuesta de voz debe ser **breve y natural** («listo, quedo atento», «me avisas si
necesitas algo») y **sin repetir** lo que ya se dijo. La conversación debe sentirse fluida
y adaptarse a respuestas cortas.

## Qué revisar
- Cómo llegan los avisos a la conversación y cómo se habla de ellos: `ez-app/src/app/`
  (`announce*`, `watch*`, `events*`, `call*`, `chat*`), y los textos del sistema en
  `ez-agent/src/text/` (`default_system_md.txt`, `voice_flow_hint.txt`,
  `execution_hint.txt`) y `ez-agent/src/system_prompt.rs`.
- Si existe un camino rápido sin LLM para frases simples (`ez-app/src/fast.rs`) y cómo
  decide.
- Qué contexto recibe el LLM justo después de un aviso (¿el aviso queda como mensaje del
  asistente?, ¿se le pide resumir?) y por qué hoy tiende a repetir.
- Los registros reales `%APPDATA%\EzAgent\logs\latency.jsonl` y `audit.jsonl` (solo
  tiempos y nombres) para ver cuánto tarda hoy un turno corto.

## Entrega
`rust-migration/reports/DX-2-r1.md`:
1. Cómo funciona hoy (con archivo:línea) y por qué la respuesta a «ok» sale larga o
   repetida.
2. Propuesta concreta, con el texto exacto de cada cambio:
   - camino rápido determinista (sin LLM) para acuses cortos justo después de un aviso
     o de una respuesta del asistente: lista de frases que cuentan como acuse (español
     neutro latinoamericano, con variantes de STT como «okey», «oc», «va»), ventana de
     tiempo, respuestas variadas y breves generadas por el código, y cuándo **no**
     aplica (si la frase trae algo más: «ok, pero cambia X» va al LLM);
   - regla en el prompt de voz para respuestas cortas y sin repetir;
   - cómo se mide (latencia antes/después) y las pruebas que lo cubrirían.
3. Riesgos (cortar una orden real, sonar robótico) y cómo se evitan.
Haz commit del informe en tu rama.
