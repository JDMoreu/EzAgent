"""Genera los briefs de una ola de workers a partir de una tabla."""

import json
import pathlib
import sys

ROOT = pathlib.Path(r"C:\Users\JDMoreu\Documents\EzAgent")
BRIEFS = ROOT / "rust-migration" / "briefs"

TASKS = json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))

for task in TASKS:
    python = "\n".join(f"- {item}" for item in task["python"])
    files = "\n".join(f"- {item}" for item in task["files"])
    fixtures = "\n".join(f"- parity/fixtures/{item}.jsonl" for item in task.get("fixtures", [])) or "- (ninguno)"
    notes = "\n".join(f"- {item}" for item in task.get("notes", [])) or "- (ninguna)"
    text = f"""# {task['id']} — {task['title']}
Excepción a AGENTS.md: rama de migración a Rust; aplica `rust-migration/AGENTS.rust.md`.
**Antes de nada lee `rust-migration/briefs/_COMMON.md` y `rust-migration/AGENTS.rust.md`.**

## Fuente Python (la especificación, léela completa)
{python}

## Archivos que se tocan (solo estos)
{files}

Los módulos de tu crate ya existen como esqueleto vacío y están declarados en
su `lib.rs`; reemplaza el contenido. Si un módulo se pasa de 600 líneas,
divídelo en una carpeta con submódulos (`foo.rs` + `foo/bar.rs`).

## Fixtures de referencia (solo para entender formatos; no hay pruebas de paridad en esta fase)
{fixtures}

## Notas específicas
{notes}

## Criterios de aceptación
- Todo lo público del Python está portado, con los mismos textos y valores por defecto.
- `cargo fmt --all`, `cargo clippy -p <crate> --all-targets -- -D warnings` y
  `cargo test -p <crate>` limpios, ejecutados desde `rust/`.
- Informe en `rust-migration/reports/{task['id']}-r1.md` y un commit en tu rama.
"""
    path = BRIEFS / f"{task['report_id']}.md"
    path.write_text(text, encoding="utf-8", newline="\n")
    print("brief", path.name)
