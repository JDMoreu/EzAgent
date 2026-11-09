# Qwen Code — manual para el orquestador

Fuente: documentación de QwenLM/qwen-code (espejos de la doc oficial). `qwen` no está instalado en este equipo: confirma con `qwen --help`.

## Abrir
- `qwen` abre el TUI interactivo.
- `-m/--model <modelo>`: modelo de esa sesión (por ejemplo `qwen3-coder-plus`). Gana sobre `QWEN_MODEL` y sobre `.qwen/settings.json`.
- `-i/--prompt-interactive "texto"`: abre el TUI con ese primer mensaje. `-p/--prompt` es no interactivo (y está en desuso frente al argumento posicional).
- `--approval-mode plan|default|auto-edit|auto|yolo` (según versión). `--yolo` aprueba todo; úsalo solo si el usuario lo pide.
- NO documenta ningún flag de esfuerzo de razonamiento: si lo piden, díselo al usuario.
- Ejemplo: `qwen --model qwen3-coder-plus`.

## Dentro de la sesión
- `/approval-mode <modo>` o `Shift+Tab` (en Windows `Tab`) cicla el modo de aprobación. `/model` cambia de modelo (sin verificar su sintaxis).
