# NV-3c — El humo de la app como padre, con el `hc` real

Revisión del agente principal sobre `0c69f21 … 8dd2b63`. Bien: `NoticeSink` y modo sin
padre-terminal en el `Watcher` (sin duplicarlo), hilo en la app para `voz`, latido y sink
comparten `announced`, reintento sin `--parent` cuando el padre no es una terminal, 327 +
369 pruebas. **Falta** lo que pedía NV-3b §3–4: el humo usa `fake-hc`, no el `hc` real. Con
el `hc` real cambian justo las cosas que este modo nuevo toca: `hc ls --all` con las
terminales de la persona, el estado `idle`/`working` real, la pantalla real y el
lanzamiento sin `--parent`. Además el reintento desde la app (el `ez` compilado) no se
ejecutó nunca.

## Cambio (solo `ez-app/examples/diag_subagents_smoke.rs` y, si hace falta, su `Cargo.toml`)
Un modo `--real-hc` (el actual con `fake-hc` se conserva):
1. `APPDATA`, `EZAGENT_LOGS_DIR`, `EZ_SUBAGENTS_DIR` temporales; `voice.json` con el `hc`
   real; agentes falsos de `ez-fakes` en el `PATH` del proceso (como `e2e_inventory`), así
   ningún tipo del catálogo puede abrir un agente real.
2. Dos hijos falsos **en `hc` real**, con nombre único y registrados bajo `voz`: uno que
   imprime «Free limit reached. Resets at 7:00 p.m.» y se queda parado; otro que sale sin
   informe. La app debe anunciar `bloqueado` y `fallido` **sin que nadie escriba el
   registro**.
3. Aceptar la oferta de reintento («sí») con el `ez` compilado real: nace `<nombre>-r1` en
   `hc` **sin** `--parent voz` (compruébalo con `hc get`), registro con `retry_of`, y la
   terminal vieja cerrada.
4. Al final, ninguna terminal de la prueba viva (por nombre y etiqueta) y la carpeta
   temporal borrada, aunque algo falle.

## Entrega
fmt y clippy de `ez-app` en verde; salida literal del modo `--real-hc` (y del de
`fake-hc`). Commit en `work/NV-3`, informe `rust-migration/reports/NV-3c-r1.md`. No toques
la ventana de Herdr de la persona ni ninguna de sus terminales; sin agentes reales.
