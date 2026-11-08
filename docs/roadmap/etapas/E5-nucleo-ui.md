# E5 · Separación núcleo/UI (bus de eventos e IPC)

| Campo | Valor |
|---|---|
| Depende de | E2, E3 (E4 recomendable) |
| Agente sugerido | MiniMax M3.1 Flash |
| Esfuerzo | L (1–2 semanas) |
| Rama | `stage/E5-nucleo-ui` |
| Hallazgos | U1, U6, R2, L10 |

## Objetivo
Partir `voice/app.py` en un núcleo sin UI y un puente IPC, y convertir el notch
actual en un cliente que corre **en otro proceso** y habla por WebSocket
local. Resultado: el núcleo funciona con `--headless` y la UI puede
sustituirse (E6) sin tocar el núcleo.

## Por qué
El render del notch comparte proceso y GIL con Kokoro y PyAudio (U1);
`app.py` (2 566 líneas) mezcla estado, turnos, ajustes y UI (U6). Sin esta
separación no se puede comparar ni migrar la UI.

## Dentro del alcance
- T1 `core/events.py` (eventos tipados + `EventBus`) y `core/state.py` (máquina de estados pura).
- T2 `core/session.py` + `core/turn.py`: extraer de `app.py` la lógica de llamada, mute, turno y avisos.
- T3 `voice/pipeline.py` + reubicar los módulos de E2 en la estructura de `02-arquitectura.md` §2.
- T4 `ui/protocol.py` + `ui/bridge.py` (WebSocket, token, límite de 30 Hz para `level`).
- T5 Notch como cliente: `ezagent/notch/client.py` (proceso hijo) que traduce mensajes a la API actual de `Notch`.
- T6 `platform/win/` (hotkeys, bandeja, instancia única) y `app.py` como punto de entrada ≤ 200 líneas.
- T7 Prueba de dependencias entre paquetes y `--headless`.

## Fuera del alcance
- No reescribir el render del notch (es E6/E7).
- No cambiar comportamiento visible: la app debe verse y comportarse igual.

## Archivos que se tocan
| Archivo | Acción |
|---|---|
| `src/ezagent/core/events.py`, `state.py`, `session.py`, `turn.py`, `config.py` | crear |
| `src/ezagent/voice/pipeline.py` | crear |
| `src/ezagent/voice/*` (módulos de E2) | mover a nombres de `02-arquitectura.md` §2 con `git mv` |
| `src/ezagent/ui/protocol.py`, `bridge.py` | crear |
| `src/ezagent/notch/` | `git mv src/ezagent/voice/notch src/ezagent/notch` + `client.py` nuevo |
| `src/ezagent/platform/win/hotkeys.py`, `tray.py`, `single_instance.py` | mover/crear |
| `src/ezagent/app.py` | crear (entrada); `voice/app.py` se borra al final |
| `pyproject.toml` | `ezagent-voice = "ezagent.app:main"`; dependencia `websockets>=13,<16` |
| `packaging/EzAgent.spec`, `packaging/launcher.py` | actualizar rutas |
| `docs/roadmap/protocolo-ui.md` | crear: esquema completo v1 |
| `tests/**` | actualizar imports; nuevas pruebas de bus, estado, puente y dependencias |

## Interfaces y contratos

```python
# core/events.py
@dataclass(frozen=True)
class StateChanged: state: str; muted: bool; listening: bool
@dataclass(frozen=True)
class Level: mic: float; voice: float
@dataclass(frozen=True)
class AgentChanged: id: str; agent: str; project: str; state: str
@dataclass(frozen=True)
class ConfirmRequested: id: str; summary: str; strong: bool
@dataclass(frozen=True)
class ConfirmAnswered: id: str; accepted: bool
@dataclass(frozen=True)
class SettingsChanged: snapshot: dict
@dataclass(frozen=True)
class Toast: kind: str; text: str
@dataclass(frozen=True)
class UiCommand: name: str; args: dict

class EventBus:
    def publish(self, event: object) -> None: ...            # thread-safe
    def subscribe(self, kind: type, fn: Callable[[object], None]) -> Callable[[], None]: ...
    async def stream(self) -> AsyncIterator[object]: ...
```

```python
# core/state.py — función pura (sin hilos ni E/S)
@dataclass(frozen=True)
class CoreState:
    call: bool; listening: bool; muted: bool; phase: str  # idle|listen|think|speak
    agents: tuple[tuple[str, str], ...]                    # (id, state)
    error: str | None
def reduce(state: CoreState, event: object) -> CoreState: ...
def ui_state(state: CoreState) -> str: ...  # prioridad de 03-diseno-y-movimiento.md §5
```

- Protocolo: tabla de `02-arquitectura.md` §6; `docs/roadmap/protocolo-ui.md`
  lo amplía con ejemplos JSON de cada mensaje, códigos de error y versión.
- Puente: `ws://127.0.0.1:<puerto libre>/ui?token=<hex 32>`; el token y el
  puerto se pasan al proceso UI por las variables `EZAGENT_UI_URL` y
  `EZAGENT_UI_TOKEN`. Conexión sin token ⇒ cierre con código 4401.
- El proceso UI se relanza si muere (máx. 3 veces en 60 s); el núcleo sigue
  funcionando y lo registra.

## Pasos
1. T1 con pruebas puras de `reduce` (tabla de transiciones).
2. T2 extraer sesión y turno; `voice/app.py` usa ambos (todavía en proceso).
3. T3 pipeline y movimientos de archivos (un commit solo con `git mv`).
4. T4 puente + pruebas con cliente WebSocket en la prueba.
5. T5 cliente notch en proceso aparte; la UI se comporta igual (prueba manual
   con checklist: arrastrar, acoplar, menú, guardar, confirmación, mute, atajos).
6. T6 plataforma y entrada nueva; borrar `voice/app.py`.
7. T7 prueba de dependencias (`tests/test_imports.py` amplía la de E3) y
   `--headless` (prueba que arranca, procesa un turno falso y sale).

## Criterios de aceptación
- [ ] `uv run ezagent-voice --headless` funciona por voz sin ventana.
- [ ] El notch corre en otro proceso (`tasklist` muestra dos procesos) y la
      checklist manual de T5 pasa completa.
- [ ] Ningún archivo en `src/` supera 800 líneas, salvo `notch/menu.py` e
      `notch/island.py` (se tratan en E6/E7).
- [ ] La regla de dependencias de `02-arquitectura.md` §2 se cumple (prueba).
- [ ] Banco de latencia sin regresión (± 5 %) respecto a E2/E3.
- [ ] `security-review` del puente sin hallazgos altos.

## Comandos de verificación
```powershell
uv run pytest -q
uv run ruff check src tests
uv run ezagent-voice --headless
uv run ezagent-voice
uv run python -m bench.latency --wav-dir bench/latency/fixtures --runs 3
```

## Riesgos y rollback
- Movimientos masivos → hacer los `git mv` en commits sin cambios de
  contenido para conservar el historial y facilitar la revisión.
- Latencia extra del IPC en `level` → el bus limita a 30 Hz y la UI interpola.

## Prompt para el agente
```
Lee AGENTS.md, docs/roadmap/etapas/E5-nucleo-ui.md y docs/roadmap/02-arquitectura.md.
Ejecuta la tarea <T#> de E5. Los movimientos de archivos van en commits
separados sin cambios de contenido. No cambies el comportamiento visible.
Entrega el informe de AGENTS.md §6.
```
