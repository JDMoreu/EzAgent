# E0 · Higiene y base open source

| Campo | Valor |
|---|---|
| Depende de | — |
| Agente sugerido | DeepSeek V4.1 Flash |
| Esfuerzo | S (1–2 días) |
| Rama | `stage/E0-higiene-oss` |
| Hallazgos | P3, P4, P7, R1, R3, T11 (parcial), U4, U9 |

## Objetivo
Dejar el repositorio publicable y seguro de tocar: licencia, CI, lint, sin
rutas personales ni dependencia de Dicta, sin código muerto evidente y sin las
dos carreras de hilos conocidas.

## Por qué
Ninguna etapa posterior es fiable sin CI y lint. La dependencia de Dicta (P3)
impide que cualquier otra persona arranque el proyecto. Las carreras R1 y U4
provocan cortes de audio y frames corruptos intermitentes.

## Dentro del alcance
1. Archivos de comunidad y licencia.
2. CI en GitHub Actions y configuración de ruff.
3. Localización de whisper sin Dicta.
4. Borrado del camino legado de `voice/llm.py` y de código muerto.
5. Arreglo de `_tts_stop` y del estado de la isla.
6. Sustituir `print` por `loguru` en los archivos tocados.

## Fuera del alcance
- No unificar `agent/loop.py` con `voice/llm.py` (es E3).
- No cambiar comportamiento de VAD, STT, TTS, LLM ni UI.
- No renombrar módulos ni mover archivos (es E5).
- No formatear con `ruff format` archivos que no se tocan.

## Archivos que se tocan
| Archivo | Acción | Notas |
|---|---|---|
| `LICENSE` | crear | Apache-2.0, titular «JDMoreu y colaboradores de EzAgent» |
| `NOTICE` | crear | Atribuciones: Dicta (MIT) para hotkeys, VAD y ventana layered; Geist (OFL); Kokoro (Apache-2.0); whisper.cpp (MIT) |
| `CONTRIBUTING.md` | crear | Requisitos, `uv sync`, pruebas, enlace a `AGENTS.md` y `docs/roadmap/` |
| `CODE_OF_CONDUCT.md` | crear | Contributor Covenant 2.1 (en inglés) |
| `SECURITY.md` | crear | Cómo reportar; alcance: ejecución de agentes, IPC local |
| `.github/workflows/ci.yml` | crear | Ver contrato abajo |
| `.github/ISSUE_TEMPLATE/bug.yml`, `feature.yml` | crear | Campos: versión, Windows, pasos, logs |
| `pyproject.toml` | modificar | `ruff` y `pyright` en `dev`; sección `[tool.ruff]`; `license = "Apache-2.0"` |
| `src/ezagent/voice/stt.py` | modificar | Sustituir `dicta_home`/`whisper_models_dir` por `whisper_home()` |
| `src/ezagent/voice/app.py` | modificar | Solo `ensure_whisper` (≈ l. 186-243), `_tts_stop` y `print` de las zonas tocadas |
| `scripts/start-whisper.ps1` | modificar | Parámetro `-WhisperHome`; sin `..\dicta` |
| `src/ezagent/voice/notch/layered.py` | modificar | Renombrar `_CLASS_NAME` a `"ezagent-layered-window"`, título del hilo `ezagent-…`; docstring sin referencias a dicta |
| `src/ezagent/voice/notch/island.py` | modificar | Lock de estado (ver contrato) y `print` → logger (l. 1736) |
| `src/ezagent/voice/llm.py` | modificar | Borrar `SYSTEM_PROMPT`, `TOOLS` y `_execute` legados (l. 16-265, 557-648); si el runtime falla, se dice un error por voz |
| `src/ezagent/voice/audio.py` | modificar | Borrar `MicCapture.record` (l. 429-476); default `silence_secs` del `Segmenter` = 1.3 |
| `src/ezagent/agent/runtime.py` | modificar | Borrar `note_done` no-op (l. 597-606) y sus llamadas |
| `README.md`, `src/ezagent/voice/README.md`, `packaging/EzAgent.spec`, `packaging/EzAgent.iss` | modificar | Sin rutas personales ni Dicta; whisper en `%LOCALAPPDATA%\EzAgent\whisper` |
| `docs/voice-latency.md`, `docs/voice-architecture.md` | modificar | Corregir las desalineaciones de P7 (marca `first_sentence`, cola de eco 0,25 s, «proveedor compatible») |
| `tests/…` | modificar | Solo las pruebas del código borrado o de `stt.py` |

## Archivos prohibidos
`src/ezagent/agent/loop.py`, `src/ezagent/voice/notch/menu.py`,
`src/ezagent/voice/notch/settings.py`, todo `src/ezagent/agent/tools_*.py`.

## Interfaces y contratos

```python
# src/ezagent/voice/stt.py
def whisper_home() -> Path | None:
    """Carpeta con whisper-server.exe y models/.
    Orden: env EZAGENT_WHISPER_HOME → cfg.whisper_home (voice.json)
    → %LOCALAPPDATA%/EzAgent/whisper → None."""

def whisper_models_dir() -> Path | None:  # whisper_home()/"models" o env EZAGENT_WHISPER_MODELS
```

- Nuevo campo en `VoiceConfig`: `whisper_home: str = ""` (no se muestra en el
  menú todavía).
- Si no se encuentra whisper: `ensure_whisper` registra el error y el notch
  pasa a `error` con el aviso por voz «No encuentro whisper. Revisa la carpeta
  de whisper en la configuración.» — sin excepción no controlada.

```python
# src/ezagent/voice/app.py — corrección de R1
# Un único lock protege la reasignación del evento de parada.
self._tts_lock = threading.Lock()

def _new_tts_stop(self) -> threading.Event:
    with self._tts_lock:
        if self._tts_stop is not None:
            self._tts_stop.set()
        self._tts_stop = threading.Event()
        return self._tts_stop

def _signal_tts_stop(self) -> None:
    with self._tts_lock:
        if self._tts_stop is not None:
            self._tts_stop.set()
```
Todas las apariciones de `if self._tts_stop is not None: self._tts_stop.set()`
pasan a `_signal_tts_stop()` y las reasignaciones (l. 1227-1229, 1248-1250,
1305) a `_new_tts_stop()`.

```python
# src/ezagent/voice/notch/island.py — corrección de U4
# Los setters públicos (draw, set_state, set_mic_level y similares) escriben
# bajo self._state_lock; render_frame toma una instantánea al inicio:
with self._state_lock:
    snap = _IslandSnapshot(state=self._state, level=self._level, mic_level=self._mic_level, ...)
# y el resto del frame solo lee `snap`.
```

```yaml
# .github/workflows/ci.yml (contrato)
on: [push, pull_request]
jobs:
  test:
    runs-on: windows-latest
    strategy: { matrix: { python: ["3.11", "3.13"] } }
    steps: checkout → astral-sh/setup-uv → uv sync --frozen
           → uv run ruff check src tests → uv run pytest -q
```
Las pruebas que necesitan audio, GPU o red real deben saltarse en CI con
`pytest.mark.skipif(os.getenv("CI"), ...)`; listar cuáles en el informe.

```toml
# pyproject.toml
[tool.ruff]
line-length = 100
target-version = "py311"
[tool.ruff.lint]
select = ["E4", "E7", "E9", "F", "I", "B"]
```
Si hay más de 40 avisos no triviales de `B`, quítalo de `select`, déjalo
anotado como `# DECISION` y abre una nota en el informe.

## Pasos
1. Archivos de comunidad, `LICENSE` y `NOTICE`. → verificación: existen y se
   renderizan.
2. `pyproject.toml` (ruff, pyright, license) + `uv lock`; corregir los avisos
   de ruff con `ruff check --fix` y a mano los restantes. → `uv run ruff check src tests`
3. CI. → push a la rama y comprobar el job en verde.
4. `whisper_home()` + `start-whisper.ps1` + `ensure_whisper` + pruebas nuevas en
   `tests/test_voice_core.py` (env var, carpeta por defecto, ausencia). → `pytest -q`
5. Borrar el camino legado de `voice/llm.py`, `MicCapture.record` y
   `note_done`; ajustar pruebas. → `pytest -q`
6. Corrección R1 (`_tts_stop`) con prueba: dos hilos llamando
   `_new_tts_stop`/`_signal_tts_stop` 1 000 veces sin dejar un evento activo sin señalar.
7. Corrección U4 (lock e instantánea) con prueba de que `render_frame` usa la
   instantánea (simular un setter concurrente).
8. Docs: README, voice/README, P7.

## Criterios de aceptación
- [ ] `uv run pytest -q` y `uv run ruff check src tests` en verde, local y en CI.
- [ ] `git grep -n -i dicta -- src scripts packaging README.md` solo devuelve
      líneas de atribución (comentarios «Basado en Dicta (MIT)»); la lista
      completa aparece en `NOTICE`.
- [ ] `git grep -n "JDMoreu" -- README.md src scripts packaging` vacío (el
      autor solo aparece en `LICENSE`/`NOTICE`/`pyproject.toml`).
- [ ] Con whisper en `%LOCALAPPDATA%\EzAgent\whisper` la app arranca sin
      argumentos; sin whisper, la app no se cae y lo dice.
- [ ] `voice/llm.py` tiene menos de 450 líneas.
- [ ] Ninguna prueba eliminada salvo las del código borrado (listarlas).

## Pruebas
- Añadir: `tests/test_voice_core.py::test_whisper_home_*` (3 casos),
  `tests/test_app_confirmation.py::test_tts_stop_is_thread_safe` (o archivo nuevo
  `tests/test_threading.py`), `tests/test_notch_ui.py::test_render_uses_snapshot`.

## Comandos de verificación
```powershell
uv sync
uv run ruff check src tests
uv run pytest -q
git grep -n -i dicta -- src scripts packaging README.md
uv run ezagent-voice   # prueba manual: arranca, escucha, responde
```

## Riesgos y rollback
- Borrar el camino legado deja sin respuesta si el runtime falla → se cubre con
  el mensaje de error por voz y una prueba.
- Ruff `--fix` cambia imports en muchos archivos → hacerlo en un commit
  separado para revertir fácil.

## Definición de hecho
Criterios cumplidos, CI en verde en GitHub, informe de `AGENTS.md` §6.

## Prompt para el agente
```
Lee AGENTS.md y docs/roadmap/etapas/E0-higiene-oss.md completos.
Ejecuta la etapa E0 paso a paso, en el orden indicado, con un commit por paso.
Toca solo los archivos de la tabla «Archivos que se tocan». No toques los
archivos prohibidos. Después de cada paso ejecuta uv run pytest -q.
Al terminar ejecuta los comandos de verificación y responde con el informe
final de AGENTS.md §6, incluyendo la lista de pruebas eliminadas o saltadas en CI.
```
