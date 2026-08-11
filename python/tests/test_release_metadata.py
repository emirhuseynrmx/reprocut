from __future__ import annotations

import re
from pathlib import Path

import pytest

try:
    import tomllib
except ModuleNotFoundError:
    tomllib = None

pytestmark = pytest.mark.skipif(tomllib is None, reason="stdlib tomllib requires Python 3.11+")

ROOT = Path(__file__).resolve().parents[2]
CRATES = ROOT / "crates"
PUBLISH_ORDER = [
    "reprocut-core",
    "reprocut-report",
    "reprocut-oci",
    "reprocut-workspace",
    "reprocut-runner",
    "reprocut-state",
    "reprocut-syntax",
    "reprocut-adapters",
    "reprocut-engine",
    "reprocut",
]


def test_pypi_metadata_and_console_entrypoint_are_release_complete() -> None:
    pyproject = tomllib.loads((ROOT / "pyproject.toml").read_text(encoding="utf-8"))
    project = pyproject["project"]

    assert project["name"] == "reprocut"
    assert project["version"] == "0.1.0"
    assert project["license"] == "MIT OR Apache-2.0"
    assert project["license-files"] == ["LICENSE-MIT", "LICENSE-APACHE"]
    assert project["scripts"]["reprocut-py"] == "reprocut.cli:main"
    assert project["urls"]["Repository"].endswith("/reprocut")
    assert pyproject["tool"]["maturin"]["module-name"] == "reprocut._native"


def test_every_publishable_path_dependency_has_the_release_version() -> None:
    publishable_names = set()
    for manifest_path in CRATES.glob("*/Cargo.toml"):
        document = tomllib.loads(manifest_path.read_text(encoding="utf-8"))
        package = document["package"]
        if package.get("publish", True) is not False:
            publishable_names.add(package["name"])
            assert package.get("description"), manifest_path
        for dependency in document.get("dependencies", {}).values():
            if isinstance(dependency, dict) and "path" in dependency:
                assert dependency.get("version") == "0.1.0", manifest_path

    assert publishable_names == set(PUBLISH_ORDER)


def test_release_runbook_preserves_dependency_order() -> None:
    runbook = (ROOT / "docs" / "RELEASING.md").read_text(encoding="utf-8")
    commands = re.findall(r"cargo publish -p ([a-z0-9-]+)", runbook)

    assert commands == PUBLISH_ORDER
    assert "reprocut-python` is intentionally `publish = false" in runbook
