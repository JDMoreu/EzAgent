# E4 · Recorte de alcance, i18n y ajustes simples

| Campo | Valor |
|---|---|
| Depende de | E3 |
| Agente sugerido | DeepSeek V4.1 Flash |
| Esfuerzo | M (3–5 días) |
| Rama | `stage/E4-recorte` |
| Hallazgos | P1, P5, P6, T3, T10, T13 |

## Objetivo
Eliminar todo lo que no es orquestación de agentes, pasar los textos a i18n
(es neutro + en) y reducir el menú a 3 grupos básicos más «Avanzado».

## Por qué
El código del asistente general ya no se usa tras E3, pero sigue pesando en
dependencias, tamaño del ejecutable, pruebas y confusión. El menú actual
contradice «fácil y sencillo».

## Dentro del alcance
- T1 Borrar módulos y dependencias del asistente general.
- T2 Simplificar política y confirmaciones.
- T3 i18n de todos los textos de usuario (menú, voz, bandeja, errores).
- T4 Menú: VOZ · MODELO · AGENTES · AVANZADO (plegado) y presets de proveedor.
- T5 Docs: README, `PRODUCT.md`.

## Fuera del alcance
- No cambiar el estilo visual del menú (colores, tipografía): es E7.
- No mover módulos a `core/` (es E5).

## Archivos que se tocan
| Archivo | Acción | Tarea |
|---|---|---|
| `src/ezagent/agent/tools_fs.py`, `journal.py`, `tools_web.py`, `search.py`, `skills.py`, `tools_skills.py`, `tools_d.py`, `tools_memory.py`, `memory.py`, `default_skills/**`, `paths.py` (si queda sin uso), `registry.py` (si queda sin uso) | borrar | T1 |
| `tests/test_agent_files.py`, `test_agent_web.py`, `test_agent_skills.py`, `test_agent_memory.py` y partes de `test_agent_system.py` | borrar / reducir | T1 |
| `pyproject.toml`, `uv.lock` | modificar | T1: quitar `ddgs`, `beautifulsoup4`, `lxml`, `send2trash`, `pyyaml` (si nadie lo usa) y el extra `office` |
| `packaging/EzAgent.spec` | modificar | T1: quitar `collect_all` de los paquetes eliminados |
| `src/ezagent/agent/policy.py`, `confirm.py`, `config.py`, `system_prompt.py` | modificar / borrar | T2: política mínima (ver contrato); `system_prompt.py` desaparece (lo sustituye `llm/prompt.py`) |
| `src/ezagent/core/i18n/es.toml`, `en.toml` | modificar | T3 |
| `src/ezagent/voice/notch/settings.py`, `menu_layout.py`, `menu.py` | modificar | T3, T4: textos vía `t()`, grupos nuevos, sección plegable |
| `src/ezagent/voice/tray.py`, `src/ezagent/voice/app.py` | modificar | T3: textos vía `t()` |
| `src/ezagent/llm_providers.py` | modificar | T4: presets |
| `src/ezagent/voice/config.py` | modificar | T3: campo `language: str = "es"` |
| `README.md`, `PRODUCT.md` | modificar | T5 |

## Archivos prohibidos
`src/ezagent/agents/**` y `src/ezagent/llm/**` salvo para usar `t()`;
`src/ezagent/voice/notch/eyes.py`, `island.py`, `silhouette.py`, `tokens.py`.

## Interfaces y contratos

### Política mínima (T2)
| Acción | Decisión |
|---|---|
| `run_agent`, `agent_status`, `voice_control`, `send_to_agent` a agente `idle` | Ejecutar y narrar |
| `send_to_agent` a agente `working` | Confirmar («¿Interrumpo a {agent}?») |
| `stop_agent` | Confirmar siempre |
| `remember` | Confirmar siempre (texto generado por código) |
| Cualquier acción tras leer salida de un agente en el mismo turno | Confirmar |
Se conserva la auditoría (`agent/audit.py`) para estas decisiones.

### i18n (T3)
```python
# src/ezagent/core/i18n/__init__.py
def set_language(lang: str) -> None: ...      # "es" | "en"
def t(key: str, **params: object) -> str: ... # KeyError en pruebas, clave literal en producción + log
```
- Español neutro latinoamericano: «tú», sin «vale», «vosotros» ni «ordenador»
  («computadora» solo si hace falta; mejor evitarlo).
- Prueba: todas las claves de `es.toml` existen en `en.toml` y viceversa.
- Prueba: ningún literal de texto de usuario en español queda en
  `settings.py`, `menu_layout.py`, `tray.py` (búsqueda de cadenas con tildes o
  palabras frecuentes).

### Menú (T4)
| Grupo | Filas |
|---|---|
| VOZ | Micrófono · Medidor con umbral · Voz (con ▶ prueba) · Velocidad |
| MODELO | Proveedor (preset) · Modelo · Clave (con validación ✓/✗ al guardar) |
| AGENTES | Agente por defecto · Avisos con sonido · Abrir `agents.toml` · Abrir `projects.toml` |
| AVANZADO (plegado) | Silencio/endpoint · Pausas TTS · Motor STT · Variante Kokoro · URL base · Temperatura · Tokens · Razonamiento · Idioma · Atajos · Abrir `voice.json` |

Presets (`llm_providers.py`): OpenCode Go (DeepSeek V4.1 Flash, por defecto),
OpenCode Go (MiniMax M3.1 Flash), Groq, OpenRouter, Anthropic, OpenAI,
Ollama local (`http://127.0.0.1:11434/v1`, sin clave), Personalizado.
Elegir un preset rellena URL y modelo; la clave se pide solo si hace falta.

## Pasos
1. T1 borrar módulos y pruebas; `uv lock`; pruebas en verde.
2. T2 política mínima + pruebas.
3. T3 i18n (primero el catálogo y `t()`, luego archivo por archivo).
4. T4 menú y presets; capturas antes/después en el informe.
5. T5 docs.

## Criterios de aceptación
- [ ] Líneas en `src/` (sin contar `notch/`) < 50 % de las de `eb3dd1d`
      (`git ls-files src | xargs wc -l` antes/después en el informe).
- [ ] `uv run python -c "import ddgs"` falla en el entorno nuevo (dependencia
      eliminada) y la app arranca.
- [ ] Menú básico sin scroll a 100 % y 125 % de escala; «Avanzado» plegado por defecto.
- [ ] Con `language="en"`, la voz, el menú y la bandeja están en inglés.
- [ ] Set dorado sin regresión respecto a E3 (± 1 punto).
- [ ] Pruebas y ruff en verde.

## Comandos de verificación
```powershell
uv sync
uv run pytest -q
uv run ruff check src tests
uv run python -m evals.intents --provider opencode --model deepseek-v4.1-flash --tools v2
uv run ezagent-voice
```

## Riesgos y rollback
- Borrar demasiado: todo está en git (`eb3dd1d`); el informe lista cada
  archivo borrado.
- Usuarios con `permissions.toml` antiguo: se ignora con un aviso en el log.

## Prompt para el agente
```
Lee AGENTS.md y docs/roadmap/etapas/E4-recorte.md completos.
Ejecuta la tarea <T#> de E4. Antes de borrar un archivo, comprueba con
git grep que nada fuera de la lista lo importa; si algo lo importa, detente y
repórtalo. Entrega el informe de AGENTS.md §6 con la lista de archivos borrados
y el recuento de líneas antes/después.
```
