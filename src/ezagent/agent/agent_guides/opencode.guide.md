# OpenCode (v2.2.6) — manual para el orquestador

Fuentes: opencode.ai/docs (modelos y CLI), `opencode --help` y el binario instalado. Verificado salvo lo marcado.

## Abrir CON modelo y esfuerzo (rápido, sin menús)
El TUI normal NO acepta `--model` ni `--effort` (imprime la ayuda y se cierra). Se hace con una variable de entorno y el servidor privado:

```
$env:OPENCODE_CONFIG_CONTENT='{"model":"opencode-go/deepseek-v4.1-flash#high"}'; opencode --standalone
```
- Formato del modelo: `proveedor/modelo` y, opcional, `#variante` (= esfuerzo). El id es de catálogo (distingue mayúsculas): `opencode models` los lista, por ejemplo `opencode-go/deepseek-v4.1-flash`.
- Variantes típicas: `low`, `high`, `max`, pero NO existen en todos los modelos; si el nombre no existe el modelo no se resuelve (error). Comprobado: `opencode debug config --standalone` muestra `model.variant` leído de esa variable.
- Hace falta `--standalone`: sin él, el TUI usa un servicio en segundo plano que no lee la variable.
- EzAgent lo hace solo con `run` + `model` + `effort`; no hace falta abrir menús.

## Otros flags del TUI
`--continue`, `--session <id>`, `--prompt <texto>`, `--auto` (aprueba permisos). El argumento posicional es un DIRECTORIO, nunca un mensaje. `opencode run --model proveedor/modelo#variante "msg"` es no interactivo; `opencode mini --model proveedor/modelo` es otra interfaz.

## Cambiar con el agente ya abierto (más lento: usa menús)
| Comando | Qué hace |
|---|---|
| `/models` | Selector «Select model» con buscador: escribir parte del nombre (con espacios) y Enter. Avisa «Switched model to …». |
| `/effort` (alias `/variants`, `/thinking`) | «Select variant». Si el modelo no tiene variantes avisa «No variants available» y no abre nada. |
| `Ctrl+T` | Cicla la variante; cada pulsación avisa «Switched variant to <nombre>» (incluye `default`). |
| `/agents`, `/new`, `/sessions`, `/compact`, `/undo`, `/redo`, `/status`, `/usage`, `/help`, `/exit` | Sesión e información. |
| `Shift+Tab` | Siguiente agente interno. `Esc` cierra un selector. |

## Reglas
- Si hay que cambiar algo con el agente abierto, comprueba la pantalla con `read` antes de escribir más: con un selector abierto el texto va al buscador, y sin selector va a la tarea.
- Si `/effort` dice «No variants available», ese modelo no tiene esfuerzo: díselo al usuario.
