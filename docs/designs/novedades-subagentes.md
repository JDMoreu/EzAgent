# Novedades de los subagentes: detectarlas, avisarlas y resolverlas (2026-11-19)

Contrato del elefante. Amplía `subagentes-herdr.md` §13 (estado y avisos) e
`inventario-recursos.md` (uso, límites y recomendación). Rama base: `eco/main`.

## 1. Pedido y caso real
La persona pidió que EzAgent «lidie con estas novedades». Caso del 19-nov: el subagente
`ir-1` (OpenCode, modelo gratuito) se quedó parado al arrancar con «Free limit reached …
Resets at 7:00 p.m.»; nadie lo marcó, su registro siguió «en proceso», y hubo que
detectarlo mirando la pantalla, cambiar a DeepSeek V4.1 Flash, **borrar la sesión** (al
retomarla OpenCode conservaba el modelo viejo) y relanzar. Todo eso debe hacerlo EzAgent.

## 2. Estado nuevo: `bloqueado`
Se suma a §13: `en_proceso ⇄ bloqueado`; `bloqueado → fallido | terminado`. Significa «el
subagente no puede seguir sin que alguien haga algo». `detail` dice la causa y, si se
sabe, cuándo se resuelve sola. Aviso: `[EzAgent · aviso] ir-1 (OpenCode · …) está
bloqueado: límite de uso del plan gratuito (se reinicia a las 7:00 p.m.). Opciones: ez
retry ir-1 --auto | ez retry ir-1 --agent … | esperar.`

## 3. Novedades que detecta el vigilante (cada vuelta, solo lectura)
| Novedad | Cómo se detecta | Estado y acción |
|---|---|---|
| Límite de uso | pantalla del hijo (`hc read --lines 0`) con patrones por agente: «Free limit reached», «usage limit», «rate limit», «quota», «5 hour usage limit», «You've hit your usage limit», «resets at …» (lista en `novedades.toml`, editable) | `bloqueado` + aviso; hora de reinicio si aparece; se anota en `uso.json` (agente/modelo agotado hasta esa hora) para que la recomendación lo evite |
| Diálogo o permiso | `status.state = blocked` de hc, o pantalla con un diálogo conocido (confianza en la carpeta, permiso) | `bloqueado` + aviso (EzAgent no pulsa nada por la persona) |
| Sin avance | `en_proceso`, `status` `idle` y la pantalla sin cambios durante `stall_min` (10 min) sin informe | aviso «lleva N min sin avance» (sin cambiar estado) |
| Murió sin informe | ya existe (§13) | `fallido` + aviso |
| Memoria baja | ya está en `inventario-recursos.md` §4 | aviso |
Una misma novedad se avisa una vez por episodio.

## 4. Resolver: `ez retry`
`ez retry <nombre> [--agent T] [--model M] [--effort E] [--auto] [--same]`:
- Relanza el **mismo encargo** como hijo del mismo padre, con nombre `<nombre>-r<n>`, en una
  **sesión nueva** cuando cambia el tipo o el modelo (lección de OpenCode), y con el
  aviso al hijo: «Retomas el trabajo de <nombre>, que se detuvo por <causa>. Revisa `git
  status` y el registro antes de seguir.»
- `--auto`: elige con la recomendación (`inventario-recursos.md` §6) excluyendo lo agotado.
  `--same`: mismo tipo y modelo (p. ej. tras un diálogo resuelto).
- El registro viejo pasa a `fallido` («reemplazado por <nuevo>») y el nuevo nace
  `en_proceso` con `retry_of = <viejo>`.
- **Automático** solo si la persona lo permitió en `planes.toml`
  (`[novedades] reintento_automatico = true`); si no, se avisa y el padre decide.

## 5. Quién recibe y cuenta las novedades
- **Padre en una terminal** (Claude Code, OpenCode…): como hoy, el aviso se escribe en su
  terminal (`hc send`).
- **Padre = la app de voz** (EzAgent lanzó al agente o al enjambre): la app no es una
  terminal. Lee los registros de los subagentes que lanzó (o el flujo del vigilante) y
  **dice** la novedad en voz y en el chat con una frase corta («Ojo: ir-1 se quedó sin
  cuota del plan gratuito; ¿lo paso a DeepSeek?»). A «sí/dale» ejecuta `ez retry --auto`;
  a «¿cómo va ir-1?» responde con el registro (`ez tasks`). Los acuses cortos siguen la
  regla de DX-2.
- Los avisos son **datos, no órdenes**: el padre (agente o app) decide; el texto que viene
  del subagente nunca se ejecuta.

## 6. Encargos
- **NV-1** (ez-herdr): estado `bloqueado`, detector de novedades del vigilante,
  `novedades.toml`, `uso.json` de agotamientos.
- **NV-2** (ez-herdr + ez-ctl): `ez retry` (sesión nueva, nombre `-r<n>`, registro
  enlazado) y la política de reintento automático. `--auto` necesita IR-3.
- **NV-3** (ez-app): la app de voz como padre: lee registros, anuncia novedades, responde
  «¿cómo va X?» y ofrece/ejecuta el reintento.
- Supervisor de la orquestación (herramienta del agente principal): detectar «Free limit
  reached» y similares y abrir sesión nueva al cambiar de modelo (C0-6).
