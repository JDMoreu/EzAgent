# Cómo contribuir a EzAgent

Gracias por querer contribuir. EzAgent es una capa de voz local para agentes de
código y el trabajo real lo hacen los agentes delegados (Claude Code, OpenCode,
Codex y otros). Antes de abrir un cambio, lee el contrato del repositorio:
[`AGENTS.md`](AGENTS.md). Es obligatorio para cualquier persona o agente que
modifique el código.

## Requisitos

- Python 3.11, 3.12 o 3.13.
- [uv](https://docs.astral.sh/uv/) para instalar y ejecutar el proyecto.
- Windows 10 u 11 para la interfaz (notch, bandeja del sistema, hotkeys y
  reproducción de audio). El resto del código también funciona en otros
  sistemas, pero la UI y la captura de audio están pensadas para Windows.

La captura de micrófono usa PyAudio, así que necesitas un dispositivo de
entrada y un dispositivo de salida disponibles. Para correr las pruebas no hace
falta micrófono ni altavoz.

## Cómo empezar

```powershell
uv sync                        # instala dependencias y crea el entorno
uv run pytest -q               # ejecuta las pruebas
uv run ruff check src tests    # lint
uv run ezagent-voice           # lanza la aplicación
```

`uv run ezagent-voice` necesita whisper.cpp local y Kokoro. Si whisper no está
configurado, la aplicación no se cae: muestra el error y te dice cómo revisar la
carpeta de whisper en la configuración.

## Flujo de trabajo

1. Crea una rama por tarea, por ejemplo `fix/nombre-corto`.
2. Haz commits pequeños y revisables.
3. Deja las pruebas y el lint en verde antes de abrir el pull request:

   ```powershell
   uv run ruff check src tests
   uv run pytest -q
   ```

4. Abre el pull request contra la rama principal y describe qué resuelve,
   qué archivos toca y cómo lo verificaste.

No mezcles tareas distintas en un mismo pull request. No borres pruebas salvo que el
cambio lo pida o que el código que prueban se elimine en ese mismo cambio.

## Reportar problemas

Usa las plantillas de issue del repositorio:

- **Error**: versión de EzAgent, versión de Windows, pasos para reproducir,
  resultado esperado, resultado real y logs.
- **Propuesta**: problema que resuelve, propuesta, alternativas consideradas y
  si afecta a voz, notch o agentes.

Los logs de la aplicación están en `%APPDATA%\EzAgent\` y en la consola desde la
que la lanzas. Incluye solo las líneas relevantes y quita cualquier clave de API
antes de publicarlas.

Para vulnerabilidades no abras un issue público: sigue
[`SECURITY.md`](SECURITY.md).

## Estilo

- Código y nombres en inglés; textos que ve el usuario en español neutro
  latinoamericano.
- Type hints en todo lo público y `from __future__ import annotations`.
- Nada bloquea el hilo de UI ni los hilos de audio; las llamadas de red y los
  procesos van en workers.
- Logs con `loguru`, nunca `print`.

Al terminar, cierra tu informe con el formato de la sección 6 de
[`AGENTS.md`](AGENTS.md).
