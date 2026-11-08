# E8 · Instalación y onboarding

| Campo | Valor |
|---|---|
| Depende de | E4, E7 |
| Agente sugerido | DeepSeek V4.1 Flash |
| Esfuerzo | M (3–5 días) |
| Rama | `stage/E8-onboarding` |
| Hallazgos | P3 (cierre), P4 |

## Objetivo
Que una persona con Windows 11 limpio pase de descargar el instalador a dar
su primera orden por voz en menos de 5 minutos, sin abrir una consola.

## Dentro del alcance
- T1 Gestor de descargas (`core/downloads.py`): whisper.cpp (Vulkan y CPU),
  modelo whisper `large-v3-turbo-q5_0`, Kokoro (int8 + voces), Silero VAD.
  Manifest con URL, tamaño y SHA256; reanudación; verificación; progreso por
  eventos.
- T2 Detección de GPU Vulkan (probar el binario Vulkan con un WAV de 0,2 s;
  si falla, CPU).
- T3 Onboarding de 3 pasos en el notch (texto permitido en estos pasos):
  1. Micrófono: elegir dispositivo + medidor + «di algo».
  2. Modelo: preset (OpenCode Go por defecto, enlace para obtener clave) +
     clave + prueba de conexión.
  3. Agentes: detectar `claude`, `opencode`, `codex`, `hc` en el PATH;
     elegir el agente por defecto; ofrecer añadir el proyecto actual.
- T4 Instalador Inno Setup es/en, sin admin, con opción «iniciar con
  Windows»; build portable; ambos generados en CI (`workflow_dispatch` y tags).
- T5 Diagnóstico: comando de bandeja «Copiar diagnóstico» (versiones, rutas,
  estado de whisper/Kokoro/VAD, proveedor sin clave, últimas 100 líneas de log).

## Fuera del alcance
- Firma de código con certificado (se documenta como pendiente).
- Actualizaciones automáticas (se anota para después de E9).

## Archivos que se tocan
`src/ezagent/core/downloads.py`, `src/ezagent/core/manifest.toml`,
`src/ezagent/notch/onboarding.py` (o `ui-web/src/onboarding.ts`),
`src/ezagent/core/session.py` (arranque en modo onboarding),
`src/ezagent/platform/win/tray.py`, `packaging/**`,
`.github/workflows/release.yml`, `scripts/start-whisper.ps1` (o su sustituto
en Python), `README.md`, pruebas nuevas.

## Contratos
```toml
# core/manifest.toml
[[asset]]
id = "whisper-vulkan"
url = "https://github.com/ggml-org/whisper.cpp/releases/download/<versión>/<archivo>.zip"
sha256 = "<verificado por el agente al implementarlo>"
size = 0
dest = "whisper"
unzip = true
```
- El agente **verifica** cada URL y calcula cada SHA256 al implementar; si un
  recurso oficial no existe en el formato esperado, se detiene y lo reporta
  (no busca espejos no oficiales).
- Licencias de cada recurso añadidas a `NOTICE`.
- Descargas solo por HTTPS, a `%LOCALAPPDATA%\EzAgent\`, con archivo `.part` y
  renombrado atómico tras verificar el hash.

## Criterios de aceptación
- [ ] VM limpia de Windows 11 (sin Python, sin uv): instalar → onboarding →
      «abre opencode en <proyecto> y dime qué hay en el README» funciona en
      < 5 min con conexión de 100 Mb/s (cronometrado, documentado en el informe).
- [ ] Sin GPU Vulkan, cae a CPU y lo dice.
- [ ] Hash incorrecto ⇒ se borra el `.part`, se reintenta una vez y se informa.
- [ ] El instalador y el portable se generan en CI desde un tag.
- [ ] `security-review` del gestor de descargas sin hallazgos altos.

## Prompt para el agente
```
Lee AGENTS.md y docs/roadmap/etapas/E8-onboarding.md.
Ejecuta la tarea <T#> de E8. Verifica cada URL oficial y su SHA256 antes de
escribirlos en el manifest; si alguno no existe, detente y repórtalo.
Entrega el informe de AGENTS.md §6.
```
