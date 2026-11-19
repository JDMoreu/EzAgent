# Reglas comunes de los peces del espacio de trabajo (briefs ECO-*)

Excepción a AGENTS.md: rama `eco/main` (espacio de trabajo con elefante). Aplica este
archivo; lo que no contradiga de `AGENTS.md` sigue vigente. **No** aplica
`AGENTS.rust.md`: aquí no hay Python que portar; la especificación es
`docs/designs/espacio-elefante.md` (léelo completo antes de empezar; manda sobre tu
brief si discrepan, y en ese caso dilo en el informe).

## Alcance
- Una tarea: la de tu brief. Toca **solo** los archivos que lista. Si necesitas otro,
  no lo toques: resuélvelo dentro de tu crate y explícalo en el informe.
- Los archivos marcados como **contrato** (escritos por el elefante) son de solo lectura.
- Dependencias: solo las de `[workspace.dependencies]` que tu brief permite. En el
  `Cargo.toml` de tu crate puedes añadir `[[bin]]`, `[[example]]`, `[dev-dependencies]`
  y esas dependencias. No toques el `Cargo.toml` raíz. `Cargo.lock` cambia solo como
  efecto de compilar tu crate.
- Otros peces trabajan en paralelo en otros crates: no toques nada fuera del tuyo, no
  mates procesos que no lanzaste y no uses otras carpetas `EzAgent-wt`.

## Calidad (la puerta del elefante)
Desde `rust/`, con tu crate:
```
cargo fmt --all
cargo clippy -p <tu-crate> --all-targets -- -D warnings
cargo test -p <tu-crate>
```
- Prohibido en el código que añadas: `todo!`, `unimplemented!`, `#[ignore]`, la palabra
  `TODO`, `println!`/`eprintln!` (usa `tracing`; la salida de un programa se escribe en
  el `Write` que recibe), `remove_dir_all` fuera de carpetas temporales propias, `"cmd"`,
  `"/c"`, `-Command`, cualquier shell, `unsafe`, y `.unwrap()` fuera de pruebas (usa `?`
  o `.expect("motivo")`).
- Ningún `.rs` supera **600 líneas** (cuenta después de `cargo fmt`) ni una función
  **60 líneas**: parte en submódulos (`modulo.rs` + `modulo/parte.rs`).
- Pruebas propias (`#[cfg(test)]` o `tests/`) para todo lo público. Sin red salvo
  `127.0.0.1`, sin agentes reales y sin depender de que `hc` esté instalado (salvo los
  ejemplos que tu brief pida). Carpetas temporales con nombre único bajo
  `std::env::temp_dir()`, borradas al final.

## Código
- Identificadores y comentarios técnicos en inglés; textos que lee una persona o un
  agente, en español neutro latinoamericano (sin «vosotros» ni «vale»).
- Tipos públicos documentados. Errores con `Result` y mensajes útiles.
- Concurrencia con hilos (`std::thread` con nombre), `crossbeam-channel` y
  `parking_lot`/`std::sync`. Sin `tokio` ni `async` propios (solo `ez-hub` usa el
  runtime de Tauri por su API). Ningún lock retenido durante E/S, un subproceso o una
  espera.
- Subprocesos: `std::process::Command` con argumentos en lista, sin shell, y en Windows
  con `CREATE_NO_WINDOW` (`std::os::windows::process::CommandExt::creation_flags(0x0800_0000)`)
  para que no abran consolas.
- Seguridad: claves y tokens nunca en logs; servidores solo en `127.0.0.1`.
- Si algo no está especificado: lo más simple y seguro, marcado con
  `// DECISION: <motivo>`, y listado en el informe.

## Entrega
- Commits frecuentes en tu rama, en español, que digan qué paso cerraron.
- Informe en `rust-migration/reports/<ID>-r1.md` con el formato de `AGENTS.md` §6.
  Escribe **«Estado: completado»** solo si fmt, clippy y test pasaron de verdad; copia
  sus líneas finales en «Verificación». Nunca dejes una verificación como «pendiente».
- Lista cada `// DECISION` y cualquier desviación del brief o del diseño.
