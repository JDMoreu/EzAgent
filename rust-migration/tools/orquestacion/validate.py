"""Valida la entrega de un worker: alcance, patrones prohibidos, tamaños, fmt, clippy y tests.

Uso: python validate.py <ID> <crate> [<crate> ...]
"""

import pathlib
import re
import subprocess
import sys

MAIN = pathlib.Path(r"C:\Users\JDMoreu\Documents\EzAgent")
WT = pathlib.Path(r"C:\Users\JDMoreu\Documents\EzAgent-wt")
sys.path.insert(0, str(MAIN / "rust-migration" / "tools"))
import scan_diff  # noqa: E402

task_id, crates = sys.argv[1], sys.argv[2:]
tree = WT / task_id


def run(args, cwd=tree, timeout=1800):
    return subprocess.run(
        args, cwd=cwd, capture_output=True, text=True, encoding="utf-8", errors="replace", timeout=timeout
    )


def allowed_files() -> list[str]:
    brief = (tree / "rust-migration" / "briefs" / f"{task_id}.md").read_text(encoding="utf-8")
    if "## Archivos que se tocan" not in brief:
        crate = re.search(r"rust/crates/([\w-]+)/tests/", brief)
        return [f"rust/crates/{crate.group(1)}/"] if crate else ["rust/crates/"]
    section = brief.split("## Archivos que se tocan", 1)[1].split("##", 1)[0]
    return re.findall(r"^- (rust/\S+)$", section, flags=re.M)


problems: list[str] = []
base = run(["git", "merge-base", "HEAD", "rust/main"]).stdout.strip()
changed = [l.strip() for l in run(["git", "diff", "--name-only", f"{base}..HEAD"]).stdout.splitlines() if l.strip()]
diff = run(["git", "diff", f"{base}..HEAD", "--", "rust/"]).stdout
print(f"commits desde la base: {run(['git', 'rev-list', '--count', f'{base}..HEAD']).stdout.strip()}")
print(f"archivos cambiados: {len(changed)}")

allowed = allowed_files()
ok_prefix = ("rust-migration/reports/", "rust-migration/briefs/", "rust-migration/python-quirks.md")
for path in changed:
    in_scope = path.startswith(ok_prefix) or any(
        path == item or path.startswith(item.rstrip("/") + "/") or item.endswith(".rs") and path.startswith(item[:-3] + "/")
        for item in allowed
    )
    if not in_scope:
        problems.append(f"fuera de alcance: {path}")

problems += scan_diff.check_forbidden(diff)
problems += scan_diff.check_unwrap(diff)
for rs in [tree / p for p in changed if p.endswith(".rs")]:
    lines = len(rs.read_text(encoding="utf-8", errors="replace").splitlines())
    if lines > scan_diff.MAX_LINES:
        problems.append(f"{rs.relative_to(tree)}: {lines} líneas")

report = tree / "rust-migration" / "reports" / f"{task_id}-r1.md"
if not report.exists():
    problems.append("falta el informe rust-migration/reports/" + report.name)

rust = tree / "rust"
fmt = run(["cargo", "fmt", "--all", "--check"], cwd=rust)
if fmt.returncode != 0:
    problems.append("cargo fmt --check falla")
for crate in crates:
    clippy = run(["cargo", "clippy", "-p", crate, "--all-targets", "--", "-D", "warnings"], cwd=rust)
    if clippy.returncode != 0:
        problems.append(f"clippy {crate} falla:\n" + "\n".join(clippy.stderr.splitlines()[-25:]))
    test = run(["cargo", "test", "-p", crate], cwd=rust)
    summary = [l for l in test.stdout.splitlines() if l.startswith("test result")]
    print(f"tests {crate}: {summary}")
    if test.returncode != 0:
        problems.append(f"tests {crate} fallan:\n" + "\n".join((test.stdout + test.stderr).splitlines()[-25:]))

print("PROBLEMAS:" if problems else "VALIDACIÓN OK")
for item in problems:
    print(" -", item)
sys.exit(1 if problems else 0)
