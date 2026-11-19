# ECO-3b — `ez-ctl`: correcciones de la revisión 1

**Lee `rust-migration/briefs/_COMMON-ECO.md`, `rust-migration/briefs/ECO-3.md` (la tarea
original, ya integrada) y `docs/designs/espacio-elefante.md` §7 y §14 (manda §14).**

## Archivos que se tocan (solo estos)
- `rust/crates/ez-ctl/**`, salvo `src/skill.md` (contrato).

## Cambios
1. **Identidad del pez (§14.1).** `Caller::Fish { space, task, fish, attempt: u32 }`.
   `Tokens` sigue igual; añade `revoke_all_for(&self, predicate)`: revoca los tokens cuyo
   `Caller` cumple el predicado (para cerrar una tarea).
2. **Esperas aparte (§14.8).** Hasta 8 hilos para comandos; los `wait` van a hilos aparte,
   como máximo 32 a la vez; el 33.º recibe 503 con
   `{"ok":false,"message":"Demasiadas esperas.","data":null}`. Un `report` o `status` debe
   atenderse aunque haya 32 esperas en curso (prueba con un `Handler` que retiene `wait`).
   `stop` deja de aceptar peticiones, espera a los hilos de comandos y **no** espera a los
   de `wait` (terminan solos cuando vuelve el `Handler`).
3. **`--json` (§14.8, m02).** `--json` es global: imprime el `Reply` entero en cualquier
   comando. `inventory` ya no envía `args.json` (sus `args` son `{}`); el inventario viaja
   en `data`. Actualiza la ayuda y las pruebas.
4. **`report` (§14.3).** `--blocking`, `--major` y `--minor` son obligatorios (si falta
   alguno: uso incorrecto, salida 2). Las mutaciones siguen siendo opcionales.

## Pruebas
Las existentes adaptadas, más: 32 esperas retenidas + `report` atendido; 33.ª espera →
503; `stop` con esperas retenidas vuelve en menos de 2 s; `--json` en `status` e
`inventory`; `report` sin `--major` → 2; `revoke_all_for`.

## Criterios de aceptación
fmt (`-p ez-ctl`), clippy y test de `ez-ctl` en verde. Informe en
`rust-migration/reports/ECO-3b-r1.md`.
