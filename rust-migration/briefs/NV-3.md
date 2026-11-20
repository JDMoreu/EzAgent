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

## Adenda del agente principal (antes de lanzar, 19-nov noche)
- **Lee también** `AGENTS.md`, `_COMMON-HD.md`, `_COMMON-ECO.md`,
  `docs/designs/subagentes-herdr.md` §13 y los informes de DX-2/DX-2b/DX-2c y NV-2.
- **Base**: `eco/main` ya tiene NV-1, NV-2 (`ez_herdr::subagents::retry`), IR-3
  (`ez_scan::recommend`), HD-10 (`Registry::list_lenient`: úsala, un archivo dañado no
  puede callar a la app) y DX-1b/DX-1d (chat y blindaje de pánicos).
- **Qué subagentes son de la app**: averigua cómo lanza hoy la app los agentes. Si no deja
  registro, haz que lo deje con un padre fijo y válido (p. ej. `voz`, `# DECISION`) usando
  el mismo camino de `ez-herdr` que `ez delegate`. La app solo anuncia los suyos.
- **«¿cómo va X?»**: camino rápido en código para las formas directas («¿cómo va ir-2?»,
  «¿cómo van los agentes?») como los acuses de DX-2, y una herramienta para el LLM para las
  paráfrasis; la frase la escribe siempre el código con los datos del registro.
- **Reintento**: tras «sí/dale» a una oferta de reintento, usa la biblioteca
  (`ez_herdr::subagents::retry` + `ez_scan::recommend`). Si eso obliga a duplicar la lógica
  de `ez-ctl`, puedes invocar el `ez` compilado con `ez retry <nombre> --auto` como lista de
  argumentos, sin shell (`# DECISION`). Nunca ejecutes texto que venga del subagente.
- **Archivos**: `ez-app` (y `ez-app/Cargo.toml` si necesita `ez-scan`); nada de `ez-hud`
  (DX-1c lo tiene abierto), `ez-stt` (DX-3) ni `ez-herdr`/`ez-ctl` salvo que sea
  imprescindible (dilo en el informe).
- **Regla de interfaz**: además de las pruebas, una **prueba de humo de la app real** sin
  micrófono, como `ez-app/examples/diag_chat_smoke.rs` (crea un modo nuevo o un ejemplo
  hermano): `APPDATA` y `EZ_SUBAGENTS_DIR` temporales con registros falsos que cambian de
  estado (en proceso → bloqueado por límite → terminado), la app viva, los anuncios en el
  chat y sin `PÁNICO` en el registro temporal. Pega la salida literal.
- Compila en primer plano y espera el resultado. No toques la app abierta de la persona.
