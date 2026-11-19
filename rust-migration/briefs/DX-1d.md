# DX-1d — La app se cae al abrir el chat con DX-1b (urgente)

**Lee `AGENTS.md`, `rust-migration/reports/DX-1-r1.md` y `rust-migration/reports/DX-1b-r1.md`.**
Rama `work/DX-1d` hecha desde `work/DX-1b` (el código con el fallo).

## Lo que pasó (19-nov, 13:52)
La persona recompiló la app con DX-1b (release, `rust/target/release/ezagent-voice.exe`),
abrió el panel y el chat, y la app **se cerró**: Windows registró `Application Error`
1000, `ezagent-voice.exe` 0.2.0.0, **código de excepción 0xc0000409**, desplazamiento
`0xbe3f2e` en el propio exe. Es el aborto rápido de Rust: casi seguro un **pánico dentro de
un callback llamado por Windows** (WndProc / hilo de ventana / FFI, `extern "system"` no
puede propagar el pánico) o un pánico durante otro pánico. El registro a archivo de DX-1b
terminó en `HUD: chat` sin mensaje de error (el pánico no se registra). DX-1b se revirtió
en `eco/main` (commit 82b25b2) para devolverle a la persona una app que funcione.

## Qué hacer
1. **Reproducir** sin la app de la persona: un ejemplo o prueba que abra el chat con el
   camino real (`ChatPanel::open` → primer cuadro en el hilo de pintura → `render` →
   `compose` con el cursor como parche) y un historial variado: markdown, bloques de
   código, listas, enlaces, emojis, texto largo sin espacios, mensajes vacíos, imágenes,
   35 mensajes. Prueba también el binario `ezagent-replay` si sirve para abrir el chat.
   **No** leas el historial real de la persona (`%APPDATA%\EzAgent\history`): si con
   historiales sintéticos no se reproduce, detente y dilo en el informe.
2. Si hace falta ubicar el pánico: compila release con `CARGO_PROFILE_RELEASE_DEBUG=1` en
   tu rama y usa `rust-migration/tools/orquestacion/symbolize.py` con el desplazamiento, o
   instala un `panic hook` temporal que escriba el mensaje.
3. **Corregir** la causa y **blindar**: ningún pánico debe cruzar un callback de Windows
   (atrapar con `catch_unwind` en los bordes que falten y registrar el error) y un
   `panic hook` permanente que escriba el mensaje y la ubicación en el registro a archivo
   de DX-1b antes de abortar.
4. **Prueba de humo de la app real** (nueva regla para todo cambio de interfaz): el binario
   release arranca con una carpeta `APPDATA` temporal, abre el chat por el mismo camino del
   atajo y sigue vivo 10 s (sin micrófono real si existe un modo para eso; si no, explica
   cómo lo hiciste sin tocar la app de la persona).

## Reglas
No cierres ni toques la app abierta de la persona. Ninguna medición pasa de 1 GB.

## Criterios
fmt, clippy y test de `ez-hud` y `ez-app` en verde, la reproducción antes/después y la
prueba de humo pegadas en `rust-migration/reports/DX-1d-r1.md`.
