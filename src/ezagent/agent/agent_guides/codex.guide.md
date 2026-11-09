# Codex CLI — manual para el orquestador

Fuente: referencia de la CLI de Codex (learn.chatgpt.com/docs/developer-commands) y pruebas en vivo con codex-cli 0.167.0.

## Abrir CON modelo y esfuerzo
- `-m/--model <modelo>`; `-c/--config clave=valor` sobrescribe la configuración solo en esa ejecución (el valor se interpreta como TOML).
- Esfuerzo: `-c model_reasoning_effort=high`. Los valores válidos los declara cada modelo en el catálogo que Codex descarga con la cuenta (`~/.codex/models_cache.json`, `supported_reasoning_levels`): `low`, `medium`, `high`, `xhigh`, `max` (y `ultra` solo en `gpt-5.6-terra`). EzAgent lee ese catálogo y rechaza antes de abrir un nivel o modelo que no existe. Las guías de terceros que citan `minimal` o `none` no coinciden con esta versión.
- El catálogo no marca restricciones por plan: el TUI acepta todos los niveles, pero el servidor puede rechazar los altos al enviar un mensaje si tu plan no los incluye (no verificable sin ese plan).
- Ejemplo: `codex -m <modelo> -c model_reasoning_effort=high`.
- `codex [PROMPT]`: el argumento posicional abre el TUI con ese primer mensaje; sin él, el TUI queda a la espera.
- Otros: `-p/--profile <nombre>` (capa `$CODEX_HOME/<nombre>.config.toml`), `-s/--sandbox read-only|workspace-write|danger-full-access`, `-a/--ask-for-approval on-request|never`. No actives `danger-full-access` ni `never` si el usuario no lo pide.

## Verificado en vivo (0.167.0)
- `codex -c model_reasoning_effort=low` y `=medium` abren el TUI y la barra inferior lo muestra: `GPT-6-Luna low` / `GPT-6-Luna medium`. Pegar un mensaje y Enter funciona (respondió «ok»).
- Con `-c` Codex avisa «1 warning · f2» (corre sin el servidor compartido, modo embebido): es inofensivo; `Esc` lo cierra.
- El ejecutable está en `~/.codex/packages/standalone/current/bin/codex.exe`; en las terminales de Herdr basta escribir `codex`.

## Dentro de la sesión (sin verificar)
`/model` abre un selector (modelo y esfuerzo) que se maneja con flechas y Enter: usa `keys` y `read`. Otros: `/approvals`, `/new`, `/compact`, `/diff`, `/status`, `/init`, `/quit`.
