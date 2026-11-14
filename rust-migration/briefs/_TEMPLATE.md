# <ID> — <título>
Excepción a AGENTS.md: rama de migración a Rust; aplica `rust-migration/AGENTS.rust.md`.

## Fuente Python (especificación, en py-baseline)
- src/ezagent/... líneas A–B

## Archivos que se tocan
- rust/crates/<crate>/src/<módulo>.rs
- rust/crates/<crate>/tests/<prueba>.rs

## Fixtures y pruebas
- parity/fixtures/<dominio>/*.jsonl (deben consumirse TODOS los casos)
- Filas de parity/test-map.csv: <lista>

## Interfaz pública esperada
- <firmas que fijan las tareas dependientes>

## Criterios de aceptación
- `powershell -File rust/gate.ps1 -Task <ID>` en verde
- <criterios específicos de la tabla de tareas del plan>

## Rondas anteriores
- (el revisor añade aquí los hallazgos de cada rechazo)
