# Reglas comunes de la ola HD (subagentes en Herdr)

Excepción a AGENTS.md: rama `eco/main`. La especificación es
`docs/designs/subagentes-herdr.md` (**contrato**, léelo completo antes de empezar; manda
sobre tu brief si discrepan, y en ese caso dilo en el informe). Lo que no contradiga de
`rust-migration/briefs/_COMMON-ECO.md` sigue vigente (alcance, calidad, código, entrega):
léelo también.

## Diferencias con _COMMON-ECO
- `ez-herdr` y `ez-swarm` son puertos de la versión Python: sigue su estilo (resultados
  `Map<String, Value>` con `ok`/`error`/`warnings`, textos en español neutro), no
  reescribas lo que no te piden y **las pruebas existentes de esos crates (incluidas las
  de paridad en `tests/`) deben seguir en verde**.
- En `ez-herdr` la envoltura de entorno ya existe (`powershell_command` + `command_line`):
  **reúsala** para lanzar con entorno; no escribas otra ni otra shell.
- Pruebas de `ez-herdr` sin procesos reales: `Herdr::with_runtime(cfg, runtime)` con un
  `HerdrRuntime` falso que registre cada argv y devuelva respuestas preparadas.
- Nunca lances agentes reales (claude, opencode, codex, qwen, mcode, pi) ni gastes
  cuota. Solo el ejemplo de punta a punta (HD-6) usa `hc` y `herdr-app` reales, y con
  agentes falsos.
- Nunca leas ni escribas la configuración real del usuario (`%APPDATA%\EzAgent`,
  `~/.minimax`, `~/.codex`, `~/.config/opencode`): en pruebas, carpetas temporales y las
  variables `APPDATA`, `MINIMAX_DATA_DIR`, `CODEX_HOME`, `OPENCODE_CONFIG_DIR`.
- Hay `hc` y `herdr-app` instalados y una ventana de Herdr abierta **que usa la persona**:
  no crees, cierres ni enfoques nada en ella salvo en HD-6, y ahí solo lo de la prueba.

## Entrega
Informe en `rust-migration/reports/<ID>-r1.md` (formato de `AGENTS.md` §6), «Estado:
completado» solo con fmt, clippy y test de verdad en verde (pega sus líneas finales).
