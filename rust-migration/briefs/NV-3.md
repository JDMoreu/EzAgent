# NV-3 — La app de voz como padre: cuenta las novedades y responde por ellas

**Lee `docs/designs/novedades-subagentes.md` §5 y el informe de DX-2.** Rama desde
`eco/main` con NV-1, NV-2 y DX-2 integrados.

## Qué hace
La app de voz (`ez-app`), cuando lanzó agentes o un enjambre, sigue sus registros
(`ez_herdr::subagents::Registry`, sin tocar Herdr) y: anuncia por voz y en el chat cada
novedad con una frase corta (bloqueado por límite, diálogo, fallido, terminado,
validado); a «sí/dale» tras ofrecer un reintento ejecuta el equivalente de
`ez retry --auto`; a «¿cómo va <nombre>?» responde con el estado del registro. Textos
generados por el código (español neutro), acuses cortos según DX-2, nada del texto del
subagente se ejecuta.

## Pruebas
Con registros falsos y el doble de la app que ya usan sus pruebas: anuncio de cada
estado, respuesta a «¿cómo va …?», reintento ofrecido y aceptado, y que las pruebas no
escriban en los registros reales (`EZAGENT_LOGS_DIR`, DX-2c).

## Criterios
fmt, clippy y test de `ez-app` en verde. Informe `NV-3-r1.md`.
