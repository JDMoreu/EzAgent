# IR-3b — Corrección de la revisión de IR-3: los agotamientos del vigilante, por modelo

Revisión del agente principal sobre `work/IR-3` (86bde17 … aac3ef9). Bien: `modelos.toml`
con fábrica editable, `ez recommend` con razones legibles, `ez delegate` sin `--agent`,
guarda con código 4 y `--force`, aviso de memoria por episodio con `MemoryProbe`, frase de
mcode. **Mal** (lo dejaste anotado en tus riesgos, y hay que cerrarlo ya):
`ez-scan/src/usage/depleted.rs` lee `uso.json` por agente e ignora el modelo. El vigilante
(NV-1) anota el modelo; si mañana el modelo gratuito de OpenCode da «Free limit reached»,
`ez usage` volvería a marcar **todo** OpenCode como `agotado`: el mismo error que corrigió
IR-2b, por otra puerta. La recomendación lo esquiva, pero `ez usage` (y la app de voz en
NV-3) no.

## Cambios (solo `ez-scan/src/usage/depleted.rs`, `usage.rs` si hace falta, y pruebas)
1. Un agotamiento de `uso.json` con `model` agota solo ese modelo (y lo que IR-2b ya
   decide por plan); sin `model`, al agente entero (compatibilidad). Un agotamiento vencido
   (`until` pasado) no cuenta.
2. El agente queda `agotado` solo con la misma regla de IR-2b (todos sus modelos usables).
3. Pruebas: `uso.json` con `opencode` + `opencode/hy4-flash-free` y uso de hoy de
   `opencode-go/deepseek-v4.1-flash` → OpenCode no agotado, el gratuito sí; sin `model` →
   agente agotado; vencido → no cuenta.

## Entrega
fmt, clippy y test de `ez-scan` y `ez-ctl` en verde; `ez usage --json` real pegado.
Commit en `work/IR-3`, informe `rust-migration/reports/IR-3b-r1.md`. Compila en primer
plano y espera el resultado.
