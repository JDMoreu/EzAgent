"""Pruebas de F5: descubrimiento, validación y ejecución de skills.

Cubren ST-18, ST-19 y ST-20 del documento del núcleo, además del catálogo
máximo, la carga de archivos dentro de una skill, el hash de carpeta y las
siete skills empaquetadas.  Todo se escribe dentro de ``tmp_path``.
"""

from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path
from typing import Any

import pytest

from ezagent.agent import skills as skills_module
from ezagent.agent import tools_skills
from ezagent.agent.config import (
    AgentConfig,
    ConfirmConfig,
    GeneralConfig,
    PathsConfig,
    SkillsConfig,
    ToolsDConfig,
    WebConfig,
)
from ezagent.agent.skills import (
    MAX_CATALOG,
    MAX_CATALOG_DESCRIPTION_CHARS,
    MAX_DESCRIPTION_CHARS,
    MAX_SKILL_CHARS,
    Skill,
    SkillContent,
    SkillError,
    SkillLibrary,
    ensure_default_skills,
    hash_folder,
)
from ezagent.agent.tools_skills import build_skill_tools

yaml = pytest.importorskip("yaml")

_DEFAULT = object()


# ---------------------------------------------------------------------------
# Utilidades
# ---------------------------------------------------------------------------


def make_config(tmp_path: Path, *, allow_scripts: bool = False) -> AgentConfig:
    app_dir = tmp_path / "app"
    workspace = tmp_path / "workspace"
    read_root = tmp_path / "read"
    for directory in (app_dir, workspace, read_root):
        directory.mkdir(parents=True, exist_ok=True)
    config = AgentConfig(
        general=GeneralConfig(),
        paths=PathsConfig(
            read_roots=[str(read_root), str(workspace)],
            write_roots=[str(workspace)],
            deny=[],
            deny_globs=[],
        ),
        confirm=ConfirmConfig(accept=["sí", "confirmo"], reject=["no"]),
        web=WebConfig(),
        skills=SkillsConfig(allow_scripts=allow_scripts, trusted=[]),
        tools_d=ToolsDConfig(allowed_executables=["python"]),
        app_dir=app_dir,
        workspace=workspace,
        permissions_path=app_dir / "permissions.toml",
        system_md_path=app_dir / "system.md",
        memory_dir=app_dir / "memory",
        history_path=app_dir / "memory" / "history.db",
        audit_path=app_dir / "logs" / "audit.jsonl",
        journal_dir=app_dir / "journal",
        skills_dir=app_dir / "skills",
        global_skills_dir=tmp_path / "global_skills",
        tools_d_dir=app_dir / "tools.d",
    )
    config.ensure_dirs()
    return config


def write_skill(
    root: Path,
    folder: str,
    *,
    name: str | None | object = _DEFAULT,
    description: str | None = "Descripción de prueba. Úsala para probar.",
    body: str = "# Cuerpo\n\nTexto de la skill.\n",
    files: dict[str, str] | None = None,
    frontmatter: str | None = None,
) -> Path:
    skill_dir = root / folder
    skill_dir.mkdir(parents=True, exist_ok=True)
    if frontmatter is None:
        lines = ["---"]
        if name is _DEFAULT:
            lines.append(f"name: {json.dumps(folder)}")
        elif name is not None:
            lines.append(f"name: {json.dumps(str(name), ensure_ascii=False)}")
        if description is not None:
            lines.append(
                f"description: {json.dumps(description, ensure_ascii=False)}"
            )
        lines.append("---")
        frontmatter = "\n".join(lines) + "\n"
    (skill_dir / "SKILL.md").write_text(frontmatter + body, encoding="utf-8")
    for relative, content in (files or {}).items():
        target = skill_dir / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(content, encoding="utf-8")
    return skill_dir


def catalog_entries(catalog: str) -> list[str]:
    return [line for line in catalog.splitlines() if line.startswith("- ")]


# ---------------------------------------------------------------------------
# Descubrimiento y catálogo
# ---------------------------------------------------------------------------


def test_descubrimiento_local_primero_y_colisiones(tmp_path: Path) -> None:
    config = make_config(tmp_path)
    write_skill(config.skills_dir, "alfa", description="Local alfa.")
    write_skill(config.skills_dir, "beta", description="Local beta.")
    write_skill(config.global_skills_dir, "gamma", description="Global gamma.")
    write_skill(config.global_skills_dir, "alfa", description="Global alfa.")

    library = SkillLibrary(config)

    assert library.names() == ["alfa", "beta", "gamma"]
    assert library.get("alfa").description == "Local alfa."
    assert library.get("alfa").path.is_relative_to(config.skills_dir)
    assert any("alfa" in collision for collision in library.collisions)

    catalog = library.catalog_text()
    assert catalog.startswith("<available_skills>")
    assert catalog.endswith("</available_skills>")
    assert "alfa: Local alfa." in catalog
    assert "gamma: Global gamma." in catalog


def test_st18_nombre_distinto_solo_avisa_y_carga(tmp_path: Path) -> None:
    config = make_config(tmp_path)
    write_skill(config.skills_dir, "buena", name="otra", description="Válida.")
    library = SkillLibrary(config)

    skill = library.get("buena")
    assert skill is not None
    assert skill.declared_name == "otra"
    assert any(
        "no coincide" in warning for warning in library.warnings
    )
    assert "buena:" in library.catalog_text()


def test_st18_sin_description_se_descarta(tmp_path: Path) -> None:
    config = make_config(tmp_path)
    write_skill(config.skills_dir, "sin-desc", description=None)

    library = SkillLibrary(config)

    assert library.get("sin-desc") is None
    assert library.catalog_text() == ""
    assert any("description" in warning for warning in library.warnings)


def test_yaml_ilegible_se_descarta(tmp_path: Path) -> None:
    config = make_config(tmp_path)
    write_skill(
        config.skills_dir,
        "rota",
        frontmatter="---\nname: [sin cerrar\ndescription: rota\n",
    )

    library = SkillLibrary(config)

    assert library.get("rota") is None
    assert any("YAML" in warning for warning in library.warnings)


def test_sin_name_usa_el_nombre_de_la_carpeta(tmp_path: Path) -> None:
    config = make_config(tmp_path)
    write_skill(config.skills_dir, "sin-name", name=None)

    library = SkillLibrary(config)

    assert library.get("sin-name") is not None
    assert any("no declara" in warning for warning in library.warnings)


def test_nombre_invalido_solo_avisa(tmp_path: Path) -> None:
    config = make_config(tmp_path)
    write_skill(config.skills_dir, "Mayusculas", name="Mayusculas")

    library = SkillLibrary(config)

    assert library.get("Mayusculas") is not None
    assert any("minúsculas" in warning for warning in library.warnings)


def test_descripcion_larga_se_recorta(tmp_path: Path) -> None:
    config = make_config(tmp_path)
    write_skill(config.skills_dir, "larga", description="d" * 1500)

    library = SkillLibrary(config)

    skill = library.get("larga")
    assert skill is not None
    assert len(skill.description) == MAX_DESCRIPTION_CHARS
    assert any("descripción" in warning for warning in library.warnings)


def test_catalogo_maximo_20_y_descripcion_200(tmp_path: Path) -> None:
    config = make_config(tmp_path)
    for index in range(MAX_CATALOG + 5):
        write_skill(
            config.skills_dir,
            f"skill-{index:02d}",
            description="x" * 300,
        )

    library = SkillLibrary(config)
    catalog = library.catalog_text()
    entries = catalog_entries(catalog)

    assert len(entries) == MAX_CATALOG
    for entry in entries:
        description = entry.split(": ", 1)[1].rsplit(" (", 1)[0]
        assert len(description) <= MAX_CATALOG_DESCRIPTION_CHARS
    assert any("20" in warning for warning in library.warnings)


def test_sin_pyyaml_avisa_y_catalogo_vacio(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    config = make_config(tmp_path)
    write_skill(config.skills_dir, "alfa")
    monkeypatch.setitem(sys.modules, "yaml", None)

    library = SkillLibrary(config)

    assert library.catalog_text() == ""
    assert any("PyYAML" in warning for warning in library.warnings)


# ---------------------------------------------------------------------------
# Carga de skills y archivos
# ---------------------------------------------------------------------------


def test_load_skill_sin_file_devuelve_cuerpo_y_archivos(
    tmp_path: Path,
) -> None:
    config = make_config(tmp_path)
    write_skill(
        config.skills_dir,
        "demo",
        body="# Demo\n\nHaz cosas.\n",
        files={
            "scripts/foo.py": "print('foo')\n",
            "references/guia.md": "Guía\n",
            "assets/dato.txt": "Dato\n",
        },
    )
    library = SkillLibrary(config)

    content = library.load_skill("demo")

    assert isinstance(content, SkillContent)
    assert content.text == "# Demo\n\nHaz cosas."
    assert content.file is None
    assert content.truncated is False
    assert set(content.files) == {
        "scripts/foo.py",
        "references/guia.md",
        "assets/dato.txt",
    }
    assert library.get("demo").scripts == ("scripts/foo.py",)


def test_load_skill_file_valido_y_traversal(tmp_path: Path) -> None:
    config = make_config(tmp_path)
    write_skill(
        config.skills_dir,
        "demo",
        files={"references/guia.md": "GUIA DE USO\n"},
    )
    library = SkillLibrary(config)

    content = library.load_skill("demo", "references/guia.md")
    assert content.text == "GUIA DE USO\n"
    assert content.file == "references/guia.md"

    with pytest.raises(SkillError):
        library.load_skill("demo", "../SKILL.md")
    with pytest.raises(SkillError):
        library.load_skill("demo", "..\\..\\secreto.txt")
    with pytest.raises(SkillError):
        library.load_skill("demo", "C:\\Windows\\win.ini")
    with pytest.raises(SkillError):
        library.load_skill("demo", "/etc/passwd")
    with pytest.raises(SkillError):
        library.load_skill("demo", "references/no-existe.md")


def test_load_skill_tope_20000_caracteres(tmp_path: Path) -> None:
    config = make_config(tmp_path)
    write_skill(config.skills_dir, "grande", body="z" * (MAX_SKILL_CHARS + 500))
    library = SkillLibrary(config)

    content = library.load_skill("grande")

    assert len(content.text) == MAX_SKILL_CHARS
    assert content.truncated is True


def test_folder_hash_es_estable_y_cambia(tmp_path: Path) -> None:
    config = make_config(tmp_path)
    folder = write_skill(
        config.skills_dir,
        "demo",
        files={"scripts/foo.py": "print('uno')\n"},
    )
    library = SkillLibrary(config)

    first = library.folder_hash("demo")
    assert first == hash_folder(folder)
    assert first == hash_folder(folder)

    (folder / "scripts" / "foo.py").write_text("print('dos')\n", encoding="utf-8")
    second = library.folder_hash("demo")

    assert second != first


# ---------------------------------------------------------------------------
# Skills empaquetadas
# ---------------------------------------------------------------------------


EXPECTED_DEFAULT_SKILLS = {
    "organizar-carpetas",
    "investigar-web",
    "documentos-word",
    "documentos-pdf",
    "hojas-excel",
    "resumir-documento",
    "notas-rapidas",
}


def test_ensure_default_skills_copia_las_siete(tmp_path: Path) -> None:
    config = make_config(tmp_path)

    copied = ensure_default_skills(config)

    assert set(copied) == EXPECTED_DEFAULT_SKILLS
    for name in EXPECTED_DEFAULT_SKILLS:
        assert (config.skills_dir / name / "SKILL.md").is_file()

    # Una segunda llamada no sobrescribe ni vuelve a copiar.
    target = config.skills_dir / "notas-rapidas" / "SKILL.md"
    original = target.read_text(encoding="utf-8")
    target.write_text(original + "\nEditada por el usuario.\n", encoding="utf-8")

    assert ensure_default_skills(config) == []
    assert "Editada por el usuario" in target.read_text(encoding="utf-8")


def test_default_skills_validas(tmp_path: Path) -> None:
    config = make_config(tmp_path)
    copied = ensure_default_skills(config)
    assert set(copied) == EXPECTED_DEFAULT_SKILLS

    library = SkillLibrary(config)
    assert set(library.names()) == EXPECTED_DEFAULT_SKILLS
    for name in EXPECTED_DEFAULT_SKILLS:
        skill = library.get(name)
        assert skill is not None
        assert skill.description
        assert len(skill.description) <= MAX_DESCRIPTION_CHARS
        assert "## Cuándo usarla" in skill.body
        assert "## Pasos" in skill.body
        assert "## Límites" in skill.body

    source = Path(skills_module.__file__).resolve().parent / "default_skills"
    script_count = 0
    for script in source.rglob("scripts/*.py"):
        source_text = script.read_text(encoding="utf-8")
        compile(source_text, str(script), "exec")
        script_count += 1
    assert script_count == 3


def test_scripts_de_skills_no_importan_dependencias_al_inicio() -> None:
    source = Path(skills_module.__file__).resolve().parent / "default_skills"
    for script in source.rglob("scripts/*.py"):
        text = script.read_text(encoding="utf-8")
        assert "python-docx" in text or "openpyxl" in text or "pypdf" in text
        assert "shell=True" not in text
        assert "os.system" not in text


# ---------------------------------------------------------------------------
# Herramientas de skills: ST-19 y ST-20
# ---------------------------------------------------------------------------


class StubLibrary:
    """Library mínima para probar las herramientas sin YAML ni disco."""

    def __init__(
        self,
        skill: Skill | None = None,
        folder_hash: str = "hash-inicial",
        content: SkillContent | None = None,
    ) -> None:
        self.skill = skill
        self.hash = folder_hash
        self.content = content
        self.loaded: list[tuple[str, str | None]] = []

    def get(self, name: str) -> Skill | None:
        if self.skill is not None and name == self.skill.name:
            return self.skill
        return None

    def folder_hash(self, name: str) -> str:
        if self.skill is None or name != self.skill.name:
            raise SkillError(f"No existe la skill «{name}».")
        return self.hash

    def load_skill(self, name: str, file: str | None = None) -> SkillContent:
        self.loaded.append((name, file))
        if self.content is None:
            return SkillContent(
                name=name,
                text="CUERPO DE LA SKILL",
                files=[],
                path="/tmp/skill",
            )
        return self.content


def make_script_skill(tmp_path: Path) -> tuple[Skill, Path]:
    skill_dir = tmp_path / "app" / "skills" / "demo"
    scripts_dir = skill_dir / "scripts"
    scripts_dir.mkdir(parents=True, exist_ok=True)
    script = scripts_dir / "run.py"
    script.write_text("print('hola')\n", encoding="utf-8")
    skill = Skill(
        name="demo",
        description="Demo",
        path=skill_dir,
        body="CUERPO",
        files=("scripts/run.py",),
        scripts=("scripts/run.py",),
    )
    return skill, script


def test_st19_allow_scripts_false_deniega_siempre(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    config = make_config(tmp_path, allow_scripts=False)
    skill, _script = make_script_skill(tmp_path)
    library = StubLibrary(skill)
    tools = {tool.name: tool for tool in build_skill_tools(config, library)}

    def forbidden(*args: Any, **kwargs: Any) -> Any:
        raise AssertionError("no se debe ejecutar con allow_scripts=false")

    monkeypatch.setattr(subprocess, "run", forbidden)

    result = tools["run_skill_script"].handler(
        {"skill": "demo", "script": "scripts/run.py", "args": []}
    )
    second = tools["run_skill_script"].handler(
        {"skill": "demo", "script": "scripts/run.py", "args": ["x"]}
    )

    assert not result.ok
    assert "deshabilitada" in (result.error or "").lower()
    assert "allow_scripts" in (result.error or "")
    assert not second.ok
    assert tools["run_skill_script"].level == 2
    assert tools["run_skill_script"].describe({}).always_confirm is True


def test_st20_entorno_limpio_cwd_timeout_y_salida_truncada(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    config = make_config(tmp_path, allow_scripts=True)
    skill, script = make_script_skill(tmp_path)
    library = StubLibrary(skill, folder_hash="hash-abc")
    tools = {tool.name: tool for tool in build_skill_tools(config, library)}
    monkeypatch.setenv("OPENAI_API_KEY", "sk-no-debe-pasarse")

    captured: dict[str, Any] = {}

    def fake_run(argv: list[str], **kwargs: Any) -> subprocess.CompletedProcess:
        captured["argv"] = list(argv)
        captured["kwargs"] = dict(kwargs)
        return subprocess.CompletedProcess(
            args=argv,
            returncode=0,
            stdout="s" * 20_000,
            stderr="e" * 100,
        )

    monkeypatch.setattr(subprocess, "run", fake_run)

    result = tools["run_skill_script"].handler(
        {"skill": "demo", "script": "scripts/run.py", "args": ["uno", "dos"]}
    )

    assert result.ok
    argv = captured["argv"]
    kwargs = captured["kwargs"]
    assert argv[0] == (sys.executable or "python")
    assert argv[1] == str(script.resolve())
    assert argv[2:] == ["uno", "dos"]
    assert kwargs["shell"] is False
    assert kwargs["cwd"] == str(config.workspace)
    assert float(kwargs["timeout"]) == 30.0
    assert set(kwargs["env"]) <= {"PATH", "SYSTEMROOT", "TEMP"}
    assert "OPENAI_API_KEY" not in kwargs["env"]
    assert len(result.data["stdout"]) == 8000
    assert len(result.data["stderr"]) == 100

    approvals_path = config.app_dir / "skills_approved.json"
    approvals = json.loads(approvals_path.read_text(encoding="utf-8"))
    assert approvals["demo"] == "hash-abc"


def test_aprobacion_por_hash_se_actualiza_al_cambiar_la_skill(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    config = make_config(tmp_path, allow_scripts=True)
    skill, _script = make_script_skill(tmp_path)
    library = StubLibrary(skill, folder_hash="hash-1")
    tools = {tool.name: tool for tool in build_skill_tools(config, library)}

    def fake_run(argv: list[str], **kwargs: Any) -> subprocess.CompletedProcess:
        return subprocess.CompletedProcess(
            args=argv, returncode=0, stdout="ok", stderr=""
        )

    monkeypatch.setattr(subprocess, "run", fake_run)
    approvals_path = config.app_dir / "skills_approved.json"

    assert tools["run_skill_script"].handler(
        {"skill": "demo", "script": "scripts/run.py"}
    ).ok
    assert json.loads(approvals_path.read_text(encoding="utf-8"))["demo"] == "hash-1"

    library.hash = "hash-2"
    assert tools["run_skill_script"].handler(
        {"skill": "demo", "script": "scripts/run.py"}
    ).ok
    assert json.loads(approvals_path.read_text(encoding="utf-8"))["demo"] == "hash-2"


def test_run_skill_script_timeout(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    config = make_config(tmp_path, allow_scripts=True)
    skill, _script = make_script_skill(tmp_path)
    library = StubLibrary(skill)
    tools = {tool.name: tool for tool in build_skill_tools(config, library)}

    def fake_run(*args: Any, **kwargs: Any) -> Any:
        raise subprocess.TimeoutExpired(cmd="python", timeout=30, output=b"parcial")

    monkeypatch.setattr(subprocess, "run", fake_run)

    result = tools["run_skill_script"].handler(
        {"skill": "demo", "script": "scripts/run.py"}
    )

    assert not result.ok
    assert "tiempo" in (result.error or "").lower()
    assert result.data["stdout"] == "parcial"


def test_run_skill_script_rechaza_traversal(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    config = make_config(tmp_path, allow_scripts=True)
    skill, _script = make_script_skill(tmp_path)
    library = StubLibrary(skill)
    tools = {tool.name: tool for tool in build_skill_tools(config, library)}
    called = False

    def forbidden(*args: Any, **kwargs: Any) -> Any:
        nonlocal called
        called = True
        raise AssertionError("no se debe ejecutar")

    monkeypatch.setattr(subprocess, "run", forbidden)

    result = tools["run_skill_script"].handler(
        {"skill": "demo", "script": "../evil.py"}
    )

    assert not result.ok
    assert "«..»" in (result.error or "")
    assert called is False

    absolute = tools["run_skill_script"].handler(
        {"skill": "demo", "script": "C:\\Windows\\evil.py"}
    )
    assert not absolute.ok
    assert called is False


def test_run_skill_script_solo_py_y_dentro_de_scripts(tmp_path: Path) -> None:
    config = make_config(tmp_path, allow_scripts=True)
    skill, _script = make_script_skill(tmp_path)
    (skill.path / "scripts" / "nota.txt").write_text("hola", encoding="utf-8")
    library = StubLibrary(skill)
    tools = {tool.name: tool for tool in build_skill_tools(config, library)}

    not_py = tools["run_skill_script"].handler(
        {"skill": "demo", "script": "scripts/nota.txt"}
    )
    assert not not_py.ok
    assert ".py" in (not_py.error or "")

    outside_scripts = tools["run_skill_script"].handler(
        {"skill": "demo", "script": "SKILL.md"}
    )
    assert not outside_scripts.ok
    assert "scripts/" in (outside_scripts.error or "")


def test_run_skill_script_valida_args(tmp_path: Path) -> None:
    config = make_config(tmp_path, allow_scripts=True)
    skill, _script = make_script_skill(tmp_path)
    library = StubLibrary(skill)
    tools = {tool.name: tool for tool in build_skill_tools(config, library)}
    handler = tools["run_skill_script"].handler

    assert not handler(
        {"skill": "demo", "script": "scripts/run.py", "args": "no-es-lista"}
    ).ok
    assert not handler(
        {
            "skill": "demo",
            "script": "scripts/run.py",
            "args": ["x"] * (tools_skills.MAX_SCRIPT_ARGS + 1),
        }
    ).ok
    assert not handler(
        {
            "skill": "demo",
            "script": "scripts/run.py",
            "args": ["x" * (tools_skills.MAX_SCRIPT_ARG_CHARS + 1)],
        }
    ).ok


def test_load_skill_tool_nivel_0_y_no_confiable(tmp_path: Path) -> None:
    config = make_config(tmp_path)
    skill, _script = make_script_skill(tmp_path)
    content = SkillContent(
        name="demo",
        text="CONTENIDO",
        files=["scripts/run.py"],
        path=str(skill.path),
    )
    library = StubLibrary(skill, content=content)
    tools = {tool.name: tool for tool in build_skill_tools(config, library)}

    load = tools["load_skill"]
    assert load.level == 0
    assert load.untrusted_output is True
    result = load.handler({"name": "demo", "file": "references/guia.md"})

    assert result.ok
    assert result.data["text"] == "CONTENIDO"
    assert library.loaded == [("demo", "references/guia.md")]


def test_run_skill_script_es_no_confiable(tmp_path: Path) -> None:
    config = make_config(tmp_path, allow_scripts=True)
    skill, _script = make_script_skill(tmp_path)
    library = StubLibrary(skill)
    tools = {tool.name: tool for tool in build_skill_tools(config, library)}

    run = tools["run_skill_script"]
    assert run.level == 2
    assert run.untrusted_output is True
    assert run.timeout_s > 30.0


def test_skill_inexistente_devuelve_error(tmp_path: Path) -> None:
    config = make_config(tmp_path, allow_scripts=True)
    library = StubLibrary(None)
    tools = {tool.name: tool for tool in build_skill_tools(config, library)}

    result = tools["run_skill_script"].handler(
        {"skill": "no-existe", "script": "scripts/run.py"}
    )

    assert not result.ok
    assert "no existe" in (result.error or "").lower()
