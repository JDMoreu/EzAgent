# DX-1d — Nota de relevo (mcode sin créditos → OpenCode)

Retomas DX-1d (`rust-migration/briefs/DX-1d.md`) en la rama `work/DX-1d`. El subagente
anterior dejó **sin commit**: `rust/crates/ez-core/src/panics.rs` (hook de pánico con
registro a archivo), `rust/crates/ez-app/examples/diag_chat_open.rs` (abre el chat por el
camino real con historial sintético) y `rust/crates/ez-app/examples/diag_chat_smoke.rs`
(humo de la app). Revísalos, consérvalos si sirven y haz commit pronto.

## Lo que ya se sabe
- Con **solo DX-1b** la reproducción y el humo **no** fallan; el camino de contenido y
  composición está limpio: el pánico debe estar en el camino de ventana/callbacks.
- **Corrección del agente principal**: la app que se cayó NO tenía DX-1c. Era `eco/main`
  en el commit `7ce0bbb` = **DX-1b + DX-2 juntos** (DX-2 cambia avisos, estado del habla y
  el camino rápido). La persona hizo **«HUD: panel»** y 3 s después **«HUD: chat»** (registro
  `ezagent.log.2026-11-19`, 18:52:12Z y 18:52:15Z) con la app recién arrancada y en llamada
  («Llamada iniciada», «Escuchando al iniciar»).

## Qué hacer ahora
1. Reproduce sobre esa combinación: crea un worktree propio en `7ce0bbb` (o fusiona
   `work/DX-2` en tu rama, solo para reproducir), con el hook de pánico activo, y repite la
   secuencia panel → chat en la app (o en tu humo) con la llamada activa. Busca el mensaje
   del pánico en el registro.
2. Corrige la causa en tu rama; mantén el blindaje (catch_unwind en todos los callbacks de
   Windows de ez-hud y de la bandeja, hook permanente).
3. El humo final se hace con la combinación DX-1b + DX-2 (como irá a la app de la persona).
Nada pasa de 1 GB; no toques la app abierta de la persona ni su `whisper-server`.
