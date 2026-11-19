# IR-0 — Investigación: uso, límites, versión y modelos de cada agente sin gastar cuota

**Encargo de investigación. No cambias código del producto.** Lee `AGENTS.md` y
`docs/designs/inventario-recursos.md`. Rama `work/IR-0` desde `eco/main`.

## Qué averiguar, por agente instalado en esta máquina
Agentes: Claude Code (`claude`), OpenCode (`opencode`), MiniMax Code (`mcode`), Codex
(`codex`), Qwen Code (`qwen`), Pi (`pi`). Para cada uno:
1. **Ejecutable nativo**: ruta real (cuidado con los guiones de npm sin extensión; ver
   `rust-migration/reports/HD-0c-r1.md`) y cómo obtener la versión (`--version`).
2. **Modelos y esfuerzos disponibles sin gastar**: comando local o archivo
   (`opencode models`, `~/.codex/models_cache.json`, `~/.minimax/config.yaml`, alias de
   Claude Code…).
3. **Uso y límites observables en archivos locales**: dónde se guardan (rutas exactas),
   formato (nombres de campos y unidades), qué dicen (p. ej. porcentaje usado de la
   ventana de 5 h, fecha de reinicio, tokens por mensaje, aviso de límite alcanzado) y qué
   tan reciente es. Pistas: sesiones de Codex (`rate_limits`), `~/.claude/projects/**/*.jsonl`
   (`usage`), `opencode stats` y los registros de OpenCode (aviso «5 hour usage limit»),
   carpetas de datos de mcode, Qwen Code y Pi.
4. **Plan o suscripción** si aparece en archivos locales (sin leer claves ni tokens).

## Reglas (no negociables)
- **Nunca** ejecutes un agente con un mensaje, abras una sesión con modelo ni llames a una
  API. Solo `--version`, listados locales que no consultan modelos (si dudas, no lo
  ejecutes y dilo) y lectura de archivos.
- **Nunca** copies al informe claves, tokens, correos, contenido de conversaciones ni
  rutas de proyectos de la persona: solo nombres de campos, tipos, unidades y números de
  ejemplo inventados o agregados (p. ej. «used_percent: número 0–100»).
- **Privacidad al leer**: varias de esas carpetas guardan conversaciones de la persona.
  No abras ni muestres su contenido: inspecciona solo la **estructura** con un comando que
  imprima nombres de campos y tipos (p. ej. `python -I -c` que cargue una línea JSON y
  escriba sus claves y el tipo de cada valor, sin los valores; para números de uso, solo
  agregados como máximo, mínimo o recuento). Nunca leas `auth.json`, `.env`, `llm.json`,
  credenciales ni archivos de claves.
- No modifiques ningún archivo fuera de tu rama; no toques la app de voz ni la ventana de
  Herdr. Ningún proceso tuyo pasa de 1 GB.

## Entrega
`rust-migration/reports/IR-0-r1.md`: una sección por agente con 1–4 (con evidencia:
ruta, comando y forma de los datos), una tabla resumen «qué se puede saber sin gastar» y
la propuesta de lector para cada fuente (qué leer, cómo calcular uso en la ventana,
tolerancia a formatos que cambian). Línea «Estado: completado» y commit en tu rama.
