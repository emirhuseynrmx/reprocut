from __future__ import annotations

import importlib.util
import json
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

LOCKED_CONTRACT_VERSIONS = {
    "NORMALIZATION_SCHEMA": 5,
    "EVIDENCE_SCHEMA": 4,
    "SESSION_SCHEMA": 3,
    "CI_EVIDENCE_SCHEMA": 1,
    "ARTIFACT_MANIFEST_SCHEMA": 1,
    "SERVER_DATABASE_SCHEMA": 1,
}


def test_v0_1_contract_versions_have_one_machine_readable_authority() -> None:
    versions_path = ROOT / "scripts" / "release" / "schema_versions.py"
    assert versions_path.is_file(), "release schema authority is missing"
    spec = importlib.util.spec_from_file_location("reprocut_schema_versions", versions_path)
    assert spec is not None and spec.loader is not None
    versions = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(versions)

    assert {
        name: getattr(versions, name, None) for name in LOCKED_CONTRACT_VERSIONS
    } == LOCKED_CONTRACT_VERSIONS

    rust = (ROOT / "crates" / "reprocut-core" / "src" / "schema.rs").read_text(
        encoding="utf-8"
    )
    for field, value in (
        ("normalization", 5),
        ("evidence", 4),
        ("session", 3),
        ("ci_evidence", 1),
        ("artifact_manifest", 1),
        ("server_database", 1),
    ):
        assert re.search(rf"pub {field}: u16,", rust)
        assert re.search(rf"{field}: {value},", rust)


def test_checked_in_release_surfaces_match_the_locked_contract_versions() -> None:
    evidence = json.loads(
        (ROOT / "demo" / "result" / "reduction.json").read_text(encoding="utf-8")
    )
    report = (ROOT / "demo" / "result" / "report.html").read_text(encoding="utf-8")
    readme = (ROOT / "README.md").read_text(encoding="utf-8")
    changelog = (ROOT / "CHANGELOG.md").read_text(encoding="utf-8")
    release = (ROOT / "docs" / "release" / "0.1.0.md").read_text(encoding="utf-8")

    assert evidence["schema_version"] == 4
    assert evidence["failure"]["normalization_schema"] == 5
    assert "Normalization schema</dt><dd><code>5</code>" in report
    assert "schema-4 evidence" in readme
    assert "schema-5 normalized" in readme
    assert "Evidence schema 4" in changelog
    assert "normalization schema 5" in changelog
    assert "session contract schema 3" in changelog
    assert "evidence schema 4" in release
    assert "normalization schema 5" in release


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
    commands = re.findall(r"cargo publish --locked -p ([a-z0-9-]+)", runbook)

    assert commands == PUBLISH_ORDER
    assert "reprocut-python` is intentionally `publish = false" in runbook
