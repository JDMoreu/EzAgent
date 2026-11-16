# PYOUT-2a — Recursos de la app fuera de src/ezagent, empaquetado solo Rust y CI de Rust
Excepción a AGENTS.md: rama de migración a Rust; aplica `rust-migration/AGENTS.rust.md`.
**Antes de nada lee `rust-migration/briefs/_COMMON.md` y `rust-migration/AGENTS.rust.md`.**

## Fuente Python (la especificación, léela completa)
- Decisión del usuario: la versión Python deja de existir; solo queda Rust. Esta tarea (2a) deja la app y su empaquetado sin leer nada de `src/ezagent/`; **no borra** `src/ezagent` ni `tests/` (lo hace 2b, después de la medición de rendimiento contra Python).
- Lo que Rust aún toma del árbol Python: `rust/crates/ez-agent/src/orchestration.rs` (`include_str!` de `src/ezagent/agent/agent_guides/*.guide.md`), `rust/crates/ez-agent/src/skills/files.rs` (`default_skills_dir`, respaldo de desarrollo en `src/ezagent/agent/default_skills`), `rust/crates/ez-hud/src/tokens.rs` y `rust/crates/ez-gfx/src/text/font.rs` (respaldo de desarrollo de las fuentes en `src/ezagent/voice/assets/fonts`), `packaging/build-rust.ps1` (copia fuentes y `default_skills` desde `src\ezagent\...`). El empaquetado Python: `packaging/EzAgent.iss`, `packaging/EzAgent.spec`, `packaging/launcher.py`, `packaging/build.ps1`. CI: `.github/workflows/ci.yml` (hoy `uv sync`, `ruff`, `pytest`).

## Archivos que se tocan (solo estos)
- assets/fonts/**, assets/default_skills/**, assets/agent_guides/** (copias exactas de los archivos de `src/ezagent/...`, mismos bytes; **copiar, no mover**)
- rust/crates/ez-agent/src/orchestration.rs, rust/crates/ez-agent/src/skills/files.rs, rust/crates/ez-hud/src/tokens.rs, rust/crates/ez-gfx/src/text/font.rs (rutas nuevas) y sus pruebas
- packaging/** (el instalador Rust pasa a llamarse `EzAgent.iss` y su script `build.ps1`; se borran `EzAgent.spec`, `launcher.py` y el `EzAgent.iss`/`build.ps1` de Python, usando `git rm`)
- .github/workflows/ci.yml
- rust-migration/reports/PYOUT-2a-r1.md

Los módulos de tu crate ya existen como esqueleto vacío y están declarados en
su `lib.rs`; reemplaza el contenido. Si un módulo se pasa de 600 líneas,
divídelo en una carpeta con submódulos (`foo.rs` + `foo/bar.rs`).

## Fixtures de referencia (solo para entender formatos; no hay pruebas de paridad en esta fase)
- (ninguno)

## Notas específicas
- (1) Copia byte a byte a `assets/` (raíz del repo; ya contiene `icon.png` y otros recursos de la app): `src/ezagent/voice/assets/fonts` → `assets/fonts`, `src/ezagent/agent/default_skills` → `assets/default_skills`, `src/ezagent/agent/agent_guides` → `assets/agent_guides`. Cambia las rutas de Rust para usar las nuevas (junto al ejecutable primero, luego el árbol del repo en desarrollo, como ahora) y `include_str!` de las guías desde `assets/agent_guides`. Comprueba con `sha256` que las copias son idénticas y que **todas** las pruebas de paridad visual del HUD (`hud_*_parity`, `text_mask_parity`, `chat_image_formats`) y de orquestación siguen en verde (las fuentes deben ser las mismas). Después `grep -rn "src/ezagent\|src\\\\ezagent" rust/ packaging/ EzAgent.bat` no debe dar ninguna ruta de código (los comentarios `//! Port of src/ezagent/...` pueden quedar como historia).
- (2) Empaquetado: `git mv packaging/EzAgent-rust.iss packaging/EzAgent.iss` y `git mv packaging/build-rust.ps1 packaging/build.ps1` tras borrar los de Python con `git rm` (también `EzAgent.spec` y `launcher.py`); actualiza las referencias (`EzAgent.bat`, `README`/docs-staging si los nombra, el propio script); el script copia desde `assets/`. Comprueba `powershell -File packaging/build.ps1 -SkipTests` hasta `dist/EzAgent-Setup-0.2.0.exe` (no lo instales). (3) CI: `ci.yml` para Rust en `windows-latest`: toolchain estable, caché de cargo, `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` desde `rust/`; las pruebas que necesitan modelos o DLL descargadas deben omitirse solas si faltan (comprueba que ya lo hacen corriéndolas sin `%LOCALAPPDATA%/EzAgent` con `EZAGENT_HOME` apuntando a una carpeta vacía; si alguna falla en vez de omitirse, haz que se omita con un mensaje). `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` en verde.

## Criterios de aceptación
- Todo lo público del Python está portado, con los mismos textos y valores por defecto.
- `cargo fmt --all`, `cargo clippy -p <crate> --all-targets -- -D warnings` y
  `cargo test -p <crate>` limpios, ejecutados desde `rust/`.
- Informe en `rust-migration/reports/PYOUT-2a-r1.md` y un commit en tu rama.
