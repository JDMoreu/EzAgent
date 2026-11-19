# HD-10b — Informe (r1)

## Resultado
- Brief/tarea: HD-10b — corrección de HD-10: `ez tasks --json` con una sola forma
- Estado: completado

## Cambios
- `rust/crates/ez-ctl/src/local/state.rs`: `--json` escribe **siempre** el array de registros
  válidos en stdout; los ilegibles van a **stderr** como `registro ilegible: <ruta>: <error>` en
  ambos modos (con y sin `--json`), así la tabla de stdout queda solo con la tabla. Código 0.
  Se quita la decisión 1 de HD-10 (la salida ya no cambia de forma).
- `rust/crates/ez-ctl/tests/local_state.rs`: la prueba separa stdout y stderr: sin `--json` la
  tabla en stdout no trae la línea del roto y stderr la nombra; con `--json` stdout se parsea como
  `Vec` de 1 registro, sin `ir-1`, y stderr nombra el roto.

Ningún otro archivo se tocó (solo esos dos, como pedía el brief).

## Decisiones
- Ninguna nueva. Se elimina el `// DECISION` de HD-10 sobre la forma condicional del JSON.

## Verificación
- `cargo fmt --check -p ez-ctl` → sin diferencias (`fmt exit=0`).
- `cargo clippy -p ez-ctl --all-targets -- -D warnings` →
  `Finished dev profile [unoptimized + debuginfo] target(s) in 0.50s`.
- `cargo test -p ez-ctl` → `55 passed; 0 failed`, 0 ignoradas.
- Corrida contra una **copia** de `%LOCALAPPDATA%\EzAgent\subagentes\t6h` en una carpeta temporal
  (`EZ_SUBAGENTS_DIR`) con un `roto.json` añadido; la carpeta real no se modificó. stdout y stderr
  por separado:

```
$ EZ_SUBAGENTS_DIR=<temp>/subagentes ez tasks --parent t6h
--- stdout:
nombre  tipo · modelo                               papel      estado      hace             resumen
ir-1    OpenCode · opencode/hy4-flash-free          developer  fallido     hace 3 h 28 min
nv-1    OpenCode · opencode-go/deepseek-v4.1-flash  developer  en_proceso  hace 3 h 34 min
--- stderr:
registro ilegible: <temp>/subagentes\t6h\roto.json: EOF while parsing a value at line 1 column 17
exit=0
```

```
$ EZ_SUBAGENTS_DIR=<temp>/subagentes ez tasks --parent t6h --json
--- stdout:
[{"name":"ir-1",...,"validation":null,"notified":"fallido"},{"name":"nv-1",...,"validation":null,"notified":"en_proceso"}]
--- stderr:
registro ilegible: <temp>/subagentes\t6h\roto.json: EOF while parsing a value at line 1 column 17
exit=0
```

  (el stdout del `--json` es un array de 2 elementos; aparece tal cual, sin la clave `invalid`).

## Pendiente o riesgos
- El ejemplo de HD-6 (`e2e_delegate`) vuelve a leer el array de siempre y ya no puede romperse por
  la forma del JSON.
