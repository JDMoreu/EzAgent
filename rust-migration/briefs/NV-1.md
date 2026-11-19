# NV-1 — Estado `bloqueado` y detector de novedades del vigilante

**Lee `rust-migration/briefs/_COMMON-HD.md`, `docs/designs/subagentes-herdr.md` §13 y
`docs/designs/novedades-subagentes.md` §1–§3.** Rama desde `eco/main` **con HD-9
integrado** (toca `subagents/watch.rs`).

## Archivos
`rust/crates/ez-herdr/src/subagents.rs` y `subagents/registry.rs` (estado nuevo y
transiciones), `subagents/watch.rs` (+ nuevo `subagents/watch/novelties.rs`) y sus pruebas.

## Qué hace
1. `TaskState::Blocked` («bloqueado»): `en_proceso ⇄ bloqueado`, `bloqueado → fallido |
   terminado`; tabla completa de transiciones actualizada (como HD-7b) y aviso de §2.
2. El vigilante lee la pantalla visible de cada hijo `en_proceso`/`bloqueado`
   (`hc read <t> --lines 0`, con tope de 4 000 caracteres) y aplica §3:
   - **límite de uso** con los patrones de `%APPDATA%\EzAgent\novedades.toml` (valores de
     fábrica incluidos en el código: los de §3), sin distinguir mayúsculas; extrae la hora
     de reinicio si aparece («Resets at 7:00 p.m.», «resets in 2h») → `bloqueado` y anota en
     `<raíz del registro>/uso.json` `{agente, modelo, hasta, visto_ms}` (escritura atómica);
   - **diálogo**: `status.state = blocked` → `bloqueado`;
   - **sin avance**: `stall_min` (10) con pantalla igual y `idle` → solo aviso;
   - al volver a trabajar (`status working` o pantalla que cambia sin el patrón) →
     `en_proceso` con aviso «siguió trabajando».
   Cada novedad se avisa una vez por episodio. El texto de la pantalla es dato: solo se
   compara con patrones, nunca se ejecuta ni se copia entero al aviso.

## Pruebas
Con hc y reloj falsos: pantallas de ejemplo de OpenCode («Free limit reached … Resets at
7:00 p.m.»), Codex y Claude Code → `bloqueado` + `uso.json` con la hora; diálogo; sin
avance; vuelta a `en_proceso`; un solo aviso por episodio; transiciones.

## Criterios
fmt, clippy y test de `ez-herdr` en verde. Informe `rust-migration/reports/NV-1-r1.md`.
