# NV-1b — Corrección de la revisión de NV-1: falsos positivos del límite de uso

Revisión del agente principal sobre `work/NV-1` (3b0d212). Bien: estado `bloqueado`,
transiciones, `uso.json`, diálogo solo si no está `working`, aviso por episodio. **Mal**:
`inspect_running` marca «límite de uso» **aunque el agente esté `working`** y con patrones
de una palabra (`quota`, `rate limit`, `usage limit`). Un subagente que escribe código o
lee un encargo sobre límites (como IR-2) quedaría `bloqueado` y su agente anotado como
agotado en `uso.json`, y la recomendación lo evitaría sin motivo. El diseño (§3 de
`docs/designs/novedades-subagentes.md`) ya está corregido: léelo.

## Cambios (mismo alcance que NV-1)
1. Límite de uso solo si `agent_state != "working"` **y** la pantalla no cambió desde la
   vuelta anterior, buscando en las **últimas 15 líneas** de la pantalla.
2. Patrones de fábrica = frases completas de §3 (sin `quota`, `rate limit` ni `usage
   limit` sueltos); los de `novedades.toml` siguen pudiendo añadir otros.
3. Pruebas: pantalla con «rate limit» / «quota» / el encargo de IR-2 mientras `working` →
   **no** bloquea ni escribe `uso.json`; la misma frase de límite en la parte alta de una
   pantalla larga → no; «Free limit reached … Resets at 7:00 p.m.» al final con `idle` y
   pantalla estable → sí, con la hora.

## Entrega
fmt, clippy y test de `ez-herdr` en verde. Informe `rust-migration/reports/NV-1b-r1.md`.
