"""resolve_safe: ninguna herramienta toca el disco sin pasar por aquí."""

from __future__ import annotations

import os
from pathlib import Path

import pytest

from ezagent.agent.paths import PathDenied, resolve_safe


@pytest.fixture
def sandbox(tmp_path: Path) -> Path:
    root = tmp_path / "work"
    root.mkdir()
    return root


def safe(raw: str, root: Path, *, deny=(), globs=(), roots=None) -> Path:
    return resolve_safe(raw, root, roots if roots is not None else [str(root)], deny, globs)


def test_ruta_dentro_de_la_raiz_se_acepta_y_se_resuelve(sandbox: Path) -> None:
    target = sandbox / "a" / "b.txt"
    assert safe(str(target), sandbox) == target.resolve()


def test_ruta_relativa_se_resuelve_contra_el_workspace(sandbox: Path) -> None:
    assert safe("notas/hoy.md", sandbox) == (sandbox / "notas" / "hoy.md").resolve()


def test_la_propia_raiz_esta_permitida(sandbox: Path) -> None:
    assert safe(str(sandbox), sandbox) == sandbox.resolve()


@pytest.mark.parametrize(
    "raw",
    [
        "../fuera.txt",
        "a/../../fuera.txt",
        "a/b/../../../fuera.txt",
        "..",
    ],
)
def test_el_recorrido_hacia_arriba_no_escapa(sandbox: Path, raw: str) -> None:
    with pytest.raises(PathDenied, match="fuera de las carpetas"):
        safe(raw, sandbox)


def test_otra_carpeta_con_el_mismo_prefijo_no_cuenta_como_dentro(sandbox: Path) -> None:
    sibling = sandbox.parent / "work-evil"
    with pytest.raises(PathDenied, match="fuera de las carpetas"):
        safe(str(sibling / "x.txt"), sandbox)


@pytest.mark.parametrize("raw", ["", "a\x00b.txt"])
def test_vacia_o_con_nul_se_rechaza(sandbox: Path, raw: str) -> None:
    with pytest.raises(PathDenied, match="vacía o inválida"):
        safe(raw, sandbox)


@pytest.mark.parametrize("raw", ["\\\\servidor\\share\\x", "//servidor/share/x", "\\\\.\\C:\\x"])
def test_unc_y_dispositivos_se_rechazan(sandbox: Path, raw: str) -> None:
    with pytest.raises(PathDenied, match="UNC"):
        safe(raw, sandbox)


def test_flujos_de_datos_alternativos_se_rechazan(sandbox: Path) -> None:
    with pytest.raises(PathDenied, match="flujos"):
        safe(str(sandbox / "doc.txt") + ":secreto", sandbox)
    with pytest.raises(PathDenied, match="flujos"):
        safe("doc.txt:Zone.Identifier", sandbox)


@pytest.mark.parametrize("name", ["CON", "nul", "COM1", "lpt9", "aux.txt", "PRN.log"])
def test_nombres_reservados_de_windows_se_rechazan(sandbox: Path, name: str) -> None:
    with pytest.raises(PathDenied):
        safe(str(sandbox / name), sandbox)


def test_carpeta_protegida_y_su_contenido(sandbox: Path) -> None:
    secret = sandbox / "secreto"
    for raw in (secret, secret / "x.txt", secret / "a" / "b" / "c.txt"):
        with pytest.raises(PathDenied, match="protegida"):
            safe(str(raw), sandbox, deny=[str(secret)])


def test_carpeta_protegida_no_afecta_a_hermanas_con_prefijo_comun(sandbox: Path) -> None:
    secret = sandbox / "secreto"
    other = sandbox / "secreto2" / "x.txt"
    assert safe(str(other), sandbox, deny=[str(secret)]) == other.resolve()


def test_la_proteccion_resiste_al_recorrido_con_puntos(sandbox: Path) -> None:
    secret = sandbox / "secreto"
    with pytest.raises(PathDenied, match="protegida"):
        safe(str(sandbox / "otra" / ".." / "secreto" / "x"), sandbox, deny=[str(secret)])


@pytest.mark.parametrize(
    ("pattern", "name", "denied"),
    [
        ("**/.env", ".env", True),
        ("**/.env.*", ".env.local", True),
        ("**/*.pem", "cert.pem", True),
        ("**/*.key", "server.key", True),
        ("**/id_rsa*", "id_rsa.pub", True),
        ("**/*.kdbx", "claves.kdbx", True),
        ("**/.env", "env.txt", False),
        ("**/*.pem", "pem.txt", False),
        ("**/*.key", "keyboard.md", False),
    ],
)
def test_patrones_de_archivos_protegidos(
    sandbox: Path, pattern: str, name: str, denied: bool
) -> None:
    raw = str(sandbox / "proyecto" / name)
    if denied:
        with pytest.raises(PathDenied, match="archivo protegido"):
            safe(raw, sandbox, globs=[pattern])
    else:
        assert safe(raw, sandbox, globs=[pattern]) == Path(raw).resolve()


def test_un_archivo_protegido_lo_es_en_cualquier_profundidad(sandbox: Path) -> None:
    deep = sandbox.joinpath(*"a/b/c/d/e".split("/"), ".env")
    with pytest.raises(PathDenied):
        safe(str(deep), sandbox, globs=["**/.env"])


def test_la_raiz_de_unidad_nunca_se_permite(tmp_path: Path) -> None:
    drive = Path(tmp_path.anchor)
    with pytest.raises(PathDenied, match="raíz de unidad"):
        resolve_safe(str(drive), tmp_path, [str(drive)], [], [])


def test_varias_raices_permitidas(sandbox: Path, tmp_path: Path) -> None:
    other = tmp_path / "otra"
    other.mkdir()
    roots = [str(sandbox), str(other)]
    assert safe(str(other / "x"), sandbox, roots=roots) == (other / "x").resolve()
    with pytest.raises(PathDenied):
        safe(str(tmp_path / "tercera" / "x"), sandbox, roots=roots)


def test_sin_raices_no_se_permite_nada(sandbox: Path) -> None:
    with pytest.raises(PathDenied):
        safe(str(sandbox / "x"), sandbox, roots=[])


def test_tilde_y_variables_de_entorno_se_expanden(
    sandbox: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    monkeypatch.setenv("USERPROFILE", str(sandbox))
    monkeypatch.setenv("HOME", str(sandbox))
    monkeypatch.setenv("EZ_TEST_DIR", str(sandbox / "datos"))
    assert safe("~/nota.txt", sandbox, roots=["~"]) == (sandbox / "nota.txt").resolve()
    var = "%EZ_TEST_DIR%" if os.name == "nt" else "$EZ_TEST_DIR"
    assert safe(f"{var}/a.txt", sandbox) == (sandbox / "datos" / "a.txt").resolve()


def test_una_variable_que_apunta_fuera_tambien_se_rechaza(
    sandbox: Path, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    monkeypatch.setenv("EZ_TEST_OUT", str(tmp_path / "fuera"))
    var = "%EZ_TEST_OUT%" if os.name == "nt" else "$EZ_TEST_OUT"
    with pytest.raises(PathDenied, match="fuera de las carpetas"):
        safe(f"{var}/a.txt", sandbox)


def test_un_enlace_simbolico_no_permite_escapar(sandbox: Path, tmp_path: Path) -> None:
    outside = tmp_path / "outside"
    outside.mkdir()
    link = sandbox / "enlace"
    try:
        link.symlink_to(outside, target_is_directory=True)
    except (OSError, NotImplementedError):
        pytest.skip("este sistema no permite crear enlaces simbólicos")
    with pytest.raises(PathDenied, match="fuera de las carpetas"):
        safe(str(link / "secreto.txt"), sandbox)


def test_los_espacios_alrededor_se_ignoran(sandbox: Path) -> None:
    assert safe(f"  {sandbox / 'a.txt'}  ", sandbox) == (sandbox / "a.txt").resolve()
