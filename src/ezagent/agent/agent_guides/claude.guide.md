# Claude Code — manual para el orquestador

Fuente: `claude --help` y los comandos registrados en el binario instalado (verificado). Lo marcado «sin verificar» viene de uso habitual.

## Abrir
- `claude` abre el TUI. El primer argumento posicional es el mensaje inicial.
- `--model <alias|nombre>`: alias `fable`, `opus`, `sonnet`, `haiku` o nombre completo.
- `--effort <nivel>`: `low`, `medium`, `high`, `xhigh`, `max`.
- `--permission-mode <modo>`: `acceptEdits`, `auto`, `bypassPermissions`, `manual`, `dontAsk`, `plan`.
- `--name <nombre>`, `-c/--continue`, `--resume <id>`, `--agent <agente>`, `--add-dir <dir>`.
- Ejemplo: `claude --model opus --effort high`.

## Comandos de barra (verificados que existen)
`/model <modelo>` cambia de modelo, `/effort <nivel>` cambia el esfuerzo (admite `auto`), `/plan` modo plan, `/agents`, `/clear`, `/compact`, `/context`, `/resume`, `/rewind`, `/permissions`, `/config`, `/status`, `/usage`, `/memory`, `/mcp`, `/init`, `/fast`, `/help`, `/exit`.

## Buenas prácticas
- Para planificar sin tocar archivos, abre con `--permission-mode plan` o usa `/plan`.
- Para auditar: modelo `opus` con esfuerzo `high` o `max`.
- Para cambiar modelo o esfuerzo con la sesión abierta, escribe `/model <modelo>` o `/effort <nivel>` y comprueba la pantalla con `read`.
- Pulsar `Esc` interrumpe lo que esté haciendo; `Shift+Tab` cicla el modo de permisos (sin verificar).
