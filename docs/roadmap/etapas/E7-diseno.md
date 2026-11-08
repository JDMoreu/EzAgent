# E7 · Sistema de diseño, marca y animaciones

| Campo | Valor |
|---|---|
| Depende de | E6 (decisión de UI) |
| Agente sugerido | DeepSeek V4.1 Flash para implementar; revisión con Claude Code + skill `impeccable` |
| Esfuerzo | L (1–2 semanas) |
| Rama | `stage/E7-diseno` |
| Hallazgos | D1, D2, D3, D4, U6, U7, U8, P7 |

## Objetivo
Implementar la dirección de arte aprobada (propuesta actual: el hilo, `07-marca-hilo.md`; anterior: «la mirada que delega») de
`03-diseno-y-movimiento.md` en la UI elegida en E6: tokens únicos, paleta
nueva validada, estados de agentes, springs, coreografías, earcons
integrados y un `DESIGN.md` nuevo que describa lo construido.

## Dentro del alcance
- T0 (revisor, antes de delegar) Validar la dirección con `impeccable`:
  `shape` con `03-diseno-y-movimiento.md` como brief → ajustar paleta y
  geometría → tabla de contrastes. Lo aprobado se escribe en
  `design/tokens.json`. **Sin T0 aprobado no empieza T1.**
- T1 `design/tokens.json` + `scripts/gen_tokens.py` + archivos generados.
- T2 Eliminar constantes visuales sueltas (`menu.py:147-300`,
  `menu_layout.py:19-35`, `eyes.py:50-60`, `island.py:1821-1840`) o su
  equivalente en la UI web.
- T3 Springs físicos (`notch/motion.py` o `ui-web/src/motion.ts`) con pruebas
  de convergencia y rebote.
- T4 Estados nuevos de ojos: `heard`, `dispatch`, `agent_working`,
  `agent_done`, `agent_blocked` (tabla §5 del doc 03) conectados a
  `AgentChanged`.
- T5 Coreografías §6 (abrir/cerrar menú, despachar, terminó, confirmación) y
  movimiento reducido.
- T6 Estilo del menú simplificado de E4 con la paleta nueva, ambos temas.
- T7 Partir `menu.py` (si sigue la UI Python): `menu/rows/*.py`,
  `menu/primitives.py`, `menu/layout.py`, `menu/input.py`; ningún archivo
  > 600 líneas.
- T8 Regenerar `docs/design/notch-animation.html` desde los tokens; reescribir
  `DESIGN.md` (lo construido, no intenciones) con el agente
  `impeccable-documenter`.
- T9 Iconos: `assets/icon.svg`, `packaging/icon.ico` y el icono de la bandeja
  con la paleta nueva.

## Fuera del alcance
- Núcleo, protocolo y pipeline (salvo añadir un mensaje al protocolo si E6 lo
  anotó como necesario, con versión `v` incrementada).
- Landing y web (es E9).

## Archivos que se tocan
Según la decisión de E6:
- **UI Python:** `src/ezagent/notch/**`, `design/tokens.json`,
  `scripts/gen_tokens.py`, `assets/icon.svg`, `packaging/icon.ico`,
  `src/ezagent/platform/win/tray.py` (solo el icono), `DESIGN.md`,
  `docs/design/notch-animation.html`, `tests/test_notch_*.py`.
- **UI web:** `ui-web/**` (proyecto Tauri), mismos archivos de diseño y
  `packaging/` para incluir el binario.

## Archivos prohibidos
`src/ezagent/core/**` (salvo `ui_state` si cambia la prioridad),
`src/ezagent/agents/**`, `src/ezagent/llm/**`, `src/ezagent/voice/**`.

## Contratos
- Valores visuales, estados, springs, duraciones y earcons: los aprobados en
  T0 dentro de `design/tokens.json`. El agente constructor **no** elige
  colores ni tiempos.
- Prueba anti-deriva: ningún literal hexadecimal de color ni duración en ms
  fuera de los archivos generados (búsqueda por regex en
  `tests/test_tokens.py`).
- Springs: prueba de que `panel` no supera el valor objetivo (rebote 0) y de
  que `launch` rebota entre 8 y 15 %.

## Criterios de aceptación
- [ ] `impeccable` `critique` + `audit` sobre capturas de todos los estados (ambos
      temas) sin hallazgos de severidad alta.
- [ ] Contraste: texto ≥ 4,5:1 e iconos/ojos ≥ 3:1 en ambos temas (tabla en `DESIGN.md`).
- [ ] Umbrales de E6 (fps, frame largo, CPU, RAM) se mantienen con todos los
      estados y coreografías nuevos.
- [ ] Movimiento reducido verificado (sin springs, sin órbita, fundidos de 120 ms).
- [ ] Los cinco earcons suenan en los momentos de §6 y se pueden desactivar.
- [ ] `DESIGN.md` y `notch-animation.html` coinciden con `tokens.json` (prueba
      o script de comprobación).
- [ ] Ningún archivo de UI > 600 líneas.

## Comandos de verificación
```powershell
uv run python scripts/gen_tokens.py --check
uv run pytest -q
uv run ruff check src tests
uv run ezagent-voice
```

## Prompt para el agente
```
Lee AGENTS.md, docs/roadmap/etapas/E7-diseno.md, docs/roadmap/03-diseno-y-movimiento.md,
docs/roadmap/decision-ui.md y design/tokens.json.
Ejecuta la tarea <T#> de E7. No inventes colores, tamaños ni tiempos: todos
salen de design/tokens.json. Adjunta capturas de los estados afectados (oscuro
y claro) al informe de AGENTS.md §6.
```
