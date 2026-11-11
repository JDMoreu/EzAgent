"""VoiceApp: todo atributo ``self._x`` que se usa existe en la clase.

Las pruebas de la app construyen ``VoiceApp`` con ``__new__`` y no pasan por
``__init__``; un método borrado que solo se referencia al cablear callbacks
rompe el arranque real sin que ninguna otra prueba falle.
"""

from __future__ import annotations

import ast
from pathlib import Path

import ezagent.voice.app as app_module


def _voice_app_class() -> ast.ClassDef:
    tree = ast.parse(Path(app_module.__file__).read_text(encoding="utf-8"))
    for node in tree.body:
        if isinstance(node, ast.ClassDef) and node.name == "VoiceApp":
            return node
    raise AssertionError("No encuentro la clase VoiceApp")


def _is_self_attribute(node: ast.AST) -> bool:
    return (
        isinstance(node, ast.Attribute)
        and isinstance(node.value, ast.Name)
        and node.value.id == "self"
    )


def test_every_self_attribute_used_by_voice_app_is_defined() -> None:
    cls = _voice_app_class()
    defined = {
        node.name for node in cls.body if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef))
    }
    used: dict[str, int] = {}
    for node in ast.walk(cls):
        targets: list[ast.AST] = []
        if isinstance(node, ast.Assign):
            targets = list(node.targets)
        elif isinstance(node, (ast.AnnAssign, ast.AugAssign)):
            targets = [node.target]
        for target in targets:
            for item in ast.walk(target):
                if _is_self_attribute(item):
                    defined.add(item.attr)  # type: ignore[union-attr]
        if _is_self_attribute(node) and isinstance(node.ctx, ast.Load):  # type: ignore[union-attr]
            used.setdefault(node.attr, node.lineno)  # type: ignore[union-attr]
    missing = {
        name: line
        for name, line in used.items()
        if name not in defined and not hasattr(app_module.VoiceApp, name)
    }
    assert not missing, f"VoiceApp usa atributos que no existen: {missing}"
