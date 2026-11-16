# TMAP-1 — Mapa de pruebas Python → Rust, grupo 1 (313 pruebas)
Excepción a AGENTS.md: rama de migración a Rust; aplica `rust-migration/AGENTS.rust.md`.
**Antes de nada lee `rust-migration/briefs/_COMMON.md` y `rust-migration/AGENTS.rust.md`.**

## Fuente Python (la especificación, léela completa)
- Archivos de prueba de Python de este grupo: `tests/test_audit_confirm.py`, `tests/test_audit_store.py`, `tests/test_chat_session.py`, `tests/test_chat_store.py`, `tests/test_orchestration.py`, `tests/test_registry.py`, `tests/test_settings_window.py`, `tests/test_swarm_store.py`, `tests/test_swarm_supervisor.py`, `tests/test_web_security.py`. Cada fila de `parity/test-map.csv` cuyo `python_test_id` empiece por uno de ellos es de esta tarea.
- Dónde buscar la cobertura en Rust: las pruebas de todo `rust/crates/**` (unidad, `tests/`, paridad contra `parity/fixtures/*.jsonl` con `cases_run == cases_in_file`, reproducción `parity/replay`, compatibilidad `parity/compat`, robustez `parity/robustness`); los fixtures se generaron llamando a las funciones reales del Python (`parity/capture/**`), así que un caso de fixture cubre la conducta que prueba un test de Python si ejerce la misma función con entradas equivalentes. Desviaciones deliberadas: `rust-migration/python-quirks.md`.

## Archivos que se tocan (solo estos)
- parity/test-map/TMAP-1.csv (nuevo; **no** edites `parity/test-map.csv`, lo fusiona la coordinación)
- pruebas Rust nuevas en `rust/crates/**/tests/` o en los módulos `tests` existentes, **solo** para lo que no esté cubierto (cambios mínimos; si hace falta tocar código de producción, no lo hagas: anótalo como hallazgo)
- rust-migration/reports/TMAP-1-r1.md

Los módulos de tu crate ya existen como esqueleto vacío y están declarados en
su `lib.rs`; reemplaza el contenido. Si un módulo se pasa de 600 líneas,
divídelo en una carpeta con submódulos (`foo.rs` + `foo/bar.rs`).

## Fixtures de referencia (solo para entender formatos; no hay pruebas de paridad en esta fase)
- (ninguno)

## Notas específicas
- Para **cada** prueba de Python de tu grupo, lee la prueba (qué conducta comprueba) y escribe una fila en `parity/test-map/TMAP-1.csv` con columnas `python_test_id,rust_crate,rust_test_path,estado,nota`: `rust_test_path` = ruta y nombre de la prueba Rust que cubre esa conducta (`rust/crates/<crate>/<ruta>.rs::<nombre>` o `parity/fixtures/<archivo>.jsonl#<función o caso>` + la prueba Rust que lo consume); `estado` = `portada` (prueba Rust equivalente), `paridad` (cubierta por casos de fixture o por la reproducción), `nueva` (no estaba cubierta y escribiste la prueba Rust ahora) o `no_aplica`; `nota` = una frase (obligatoria en `no_aplica`: por qué no aplica a Rust, p. ej. detalle de implementación de Python como monkeypatch de un módulo, tkinter, PyInstaller; y en `paridad` qué casos). Corrige `rust_crate` si la conducta vive en otro crate. **Verifica** cada referencia: la prueba Rust debe existir y comprobar de verdad esa conducta (no basta con que el nombre se parezca).
- Si una conducta probada en Python no tiene cobertura en Rust: escribe la prueba Rust (estado `nueva`). Si la prueba nueva falla porque el código Rust se comporta distinto del Python, **no** cambies el código de producción: deja la prueba marcada con `#[ignore = "TMAP: diferencia con Python, ver informe"]` y descríbela en el informe como hallazgo (función, entrada, conducta Python vs Rust). Al final: todas las filas del grupo presentes en tu CSV (ninguna `pendiente`), y una tabla en el informe con los conteos por estado y la lista de hallazgos. `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings` y `cargo test` de los crates que toques, en verde (salvo las `#[ignore]` de hallazgos). Otros cinco workers trabajan en paralelo en otros grupos: no toques sus archivos.

## Criterios de aceptación
- Todo lo público del Python está portado, con los mismos textos y valores por defecto.
- `cargo fmt --all`, `cargo clippy -p <crate> --all-targets -- -D warnings` y
  `cargo test -p <crate>` limpios, ejecutados desde `rust/`.
- Informe en `rust-migration/reports/TMAP-1-r1.md` y un commit en tu rama.
