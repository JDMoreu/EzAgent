# 05 · Skills y flujo de delegación

## 1. Roles

| Rol | Quién | Responsabilidad |
|---|---|---|
| Responsable de producto | JDMoreu | Aprueba etapas, decide en E6 y E9, prueba con su voz |
| Constructor | DeepSeek V4.1 Flash / MiniMax M3.1 Flash (OpenCode u otro CLI) | Ejecuta un brief o tarea siguiendo `AGENTS.md` |
| Revisor | Claude Code (u otro modelo fuerte) | Revisión de diff, seguridad y diseño al cerrar cada etapa |

**Regla de oro:** el constructor nunca revisa su propio trabajo. Cada etapa se
cierra con una revisión hecha por un modelo distinto o por una persona.

## 2. Cómo trocear para modelos flash

Los modelos flash rinden bien con tareas cerradas y mal con tareas abiertas.

- Cada tarea: **1 objetivo, ≤ 5 archivos, ≤ 300 líneas de diff**, con una
  prueba que demuestre que está hecha.
- Da siempre las firmas exactas (copiadas del brief) y el archivo de ejemplo
  a imitar.
- Pide primero la prueba y después la implementación cuando el contrato está
  claro (TDD ligero).
- Si una tarea falla dos veces, **no** la reintentes igual: pártela más o
  pásala a MiniMax o a un modelo fuerte.
- Contexto: pega el brief de la etapa + `AGENTS.md`. No pegues la auditoría
  entera; el brief ya cita lo necesario.

Usa [`plantillas/brief-tarea.md`](plantillas/brief-tarea.md) para cada tarea.

## 3. Skills recomendadas

### Disponibles en Claude Code (revisor)

| Skill | Cuándo | Cómo |
|---|---|---|
| `impeccable` | E7 (y revisión de E4 por el menú) | `shape` para fijar la dirección desde `03-diseno-y-movimiento.md`; `animate` para la coreografía; `critique` y `audit` sobre capturas del notch; `polish` al final |
| `design-taste-frontend` | E9 | Landing y página de documentación públicas |
| `code-review` | Cierre de **cada** etapa | Sobre el diff de la rama `stage/E<n>-…` |
| `simplify` | Cierre de E3, E4 y E5 | Reutilización y recortes tras los refactors grandes |
| `security-review` | E3 (ejecución de agentes), E5 (IPC), E8 (descargas) | Sobre el diff de la etapa |
| `run` | Cierre de E2, E5, E7 y E8 | Lanzar la app y comprobar el cambio real, no solo las pruebas |
| `skill-creator` | Tras E1 | Crear las skills de proyecto de la sección 4 |

### Skills de proyecto para los constructores (crear en `.agents/skills/`)

OpenCode y otros CLIs cargan skills con `SKILL.md`. Tres skills pequeñas
reducen errores repetidos:

| Skill | Contenido |
|---|---|
| `ezagent-conventions` | Resumen de `AGENTS.md`, mapa de módulos y formato del informe final |
| `ezagent-bench` | Cómo ejecutar `bench.latency`, leer el JSON y comparar con la línea base |
| `ezagent-evals` | Cómo ejecutar `evals.intents`, añadir casos y leer el informe |

## 4. Checklist de cierre de etapa

Copia esta lista en la descripción del PR de la etapa.

```
- [ ] Todos los criterios de aceptación del brief cumplidos (con evidencia)
- [ ] uv run pytest -q en verde
- [ ] uv run ruff check src tests en verde
- [ ] Métricas registradas en docs/roadmap/metricas.md (si aplica)
- [ ] code-review sin hallazgos de severidad alta abiertos
- [ ] security-review (si la etapa lo exige)
- [ ] Probado con la app real (skill run o a mano) — describir qué se probó
- [ ] Docs del brief actualizadas (README, AGENTS.md si cambia el mapa)
- [ ] Estado actualizado en docs/roadmap/README.md
```

## 5. Ramas y commits

- Una rama por etapa: `stage/E<n>-<slug>`; una rama por tarea si se trocea:
  `stage/E<n>-T<m>-<slug>`.
- Commits en español, en imperativo, con el ID: `E2-T3: Reproduce TTS con
  stream persistente`.
- Merge a `main` solo tras la checklist. Squash por tarea.
- Las etapas paralelas (E2 ‖ E3) se integran en orden E3 → E2; el conflicto
  esperado está en `voice/app.py` y lo resuelve el revisor.
