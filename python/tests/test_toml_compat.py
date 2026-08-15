from __future__ import annotations

import sys
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts" / "release"))

from toml_compat import load_toml_module  # noqa: E402


def test_toml_loader_falls_back_for_python_3_9_and_3_10() -> None:
    fallback = object()
    imported: list[str] = []

    def import_module(name: str) -> object:
        imported.append(name)
        if name == "tomllib":
            error = ModuleNotFoundError("No module named 'tomllib'")
            error.name = "tomllib"
            raise error
        assert name == "tomli"
        return fallback

    assert load_toml_module(import_module) is fallback
    assert imported == ["tomllib", "tomli"]


def test_toml_loader_does_not_hide_broken_module_imports() -> None:
    def import_module(name: str) -> object:
        error = ModuleNotFoundError("No module named 'transitive_dependency'")
        error.name = "transitive_dependency"
        raise error

    with pytest.raises(ModuleNotFoundError, match="transitive_dependency"):
        load_toml_module(import_module)
