# mcode (MiniMax Code) — manual para el orquestador

Fuentes: `mcode --help`, su `config.yaml` y su código (verificado en vivo con v0.7.6).

## Abrir CON modelo y esfuerzo
- Modelo: `mcode --model proveedor/modelo`. Distingue mayúsculas: `minimax/MiniMax-M3.1-Flash`, `minimax/MiniMax-M3` (EzAgent los lee de `~/.minimax/config.yaml` y acepta `minimax-m3.1-flash` en minúsculas).
- Esfuerzo: el TUI NO tiene flag. Se lee de `defaultModelThinking.effort` en `config.yaml`; EzAgent lo escribe justo antes de abrir (con copia `config.yaml.ezagent-bak`). Valores: `default`, `low`, `medium`, `high`, `xhigh`, `max`. La barra inferior lo muestra: `M3.1-Flash · Effort low`. Queda como valor por defecto hasta que se pida otro.
- La «variante» (`modelo#thinking` / `#none-thinking`) es otra cosa: activa o apaga el razonamiento. No es el esfuerzo.
- `mcode [prompt]`: el argumento posicional es la tarea inicial. Otros flags: `--mode standard|lightweight`, `--tui-mode regular|fullscreen`, `--session [id]`, `-c/--continue`, `--system-prompt`, `--append-system-prompt`.
- Sin TUI: `mcode exec "prompt"` (también con `--model`); `mcode provider` gestiona proveedores y claves; `mcode login`, `mcode init`, `mcode mcp`, `mcode plugin`, `mcode update`.

## Dentro de la sesión
`/model [filtro]` abre el selector de modelo (y esfuerzo). Otros comandos no verificados: prueba `/help` y comprueba con `read`.
