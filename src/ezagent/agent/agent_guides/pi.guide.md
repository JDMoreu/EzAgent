# pi — manual para el orquestador

Fuente: `pi --help` y la documentación incluida en `pi-coding-agent/docs` (verificado).

## Abrir
- `pi [opciones] [mensajes...]`: los argumentos posicionales son el mensaje inicial.
- `--model <proveedor/id>`: admite sufijo de razonamiento (`proveedor/id:high`). `--provider <nombre>`.
- `--thinking <nivel>`: `off`, `minimal`, `low`, `medium`, `high`, `xhigh`, `max`.
- `-p/--print`: no interactivo. `-c/--continue`, `-r/--resume`, `--name <nombre>`, `--models <patrones>` (modelos para ciclar con Ctrl+P), `--list-models [texto]`.
- `--model` acepta `proveedor/id` con sufijo de razonamiento (`pi --model sonnet:high`) o coincidencia aproximada del id o nombre; `--thinking` pisa el sufijo y se recorta a lo que soporte el modelo; `--provider` restringe la búsqueda (requiere `--model`).
- `pi "mensaje"` abre el TUI con ese primer mensaje (`--` evita que un mensaje que empiece por `-` se tome por flag).
- Ejemplo: `pi --model anthropic/claude-opus --thinking high`.

## Dentro de la sesión
- `/model` (o `Ctrl+L`): selector de modelo. `Ctrl+S` en el selector lo guarda como predeterminado.
- `/thinking`: selector del nivel de razonamiento. `Shift+Tab` cicla el nivel. `Ctrl+S` en el selector lo guarda.
- `Ctrl+P` / `Shift+Ctrl+P` (en Windows `Alt+P`): modelo siguiente/anterior entre los acotados; `/scoped-models` los configura.
- `Ctrl+T` colapsa o expande los bloques de razonamiento.

## Cómo configurar bien
Lo más fiable es lanzar con `--model` y `--thinking`. Para cambiar con la sesión abierta usa `/thinking` o `Shift+Tab` con `keys` y confirma con `read`.

## Verificado en vivo
- `pi --model <id> --thinking high` muestra `<modelo> • high` abajo a la derecha: el esfuerzo se aplica al arrancar.
- Herdr no reconoce `pi` como agente (lo ve como `powershell.exe`); EzAgent detecta que arrancó por la pantalla.
- Un nombre ambiguo como `sonnet` puede resolverse en un proveedor sin clave (por ejemplo Bedrock) y fallar con «No API key found»: usa `proveedor/id` completo (`pi --list-models sonnet` los lista).
