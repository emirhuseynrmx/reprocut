from __future__ import annotations

import importlib.util
import json
import re
import sys
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts" / "release"))

from toml_compat import load_toml_module  # noqa: E402

tomllib = load_toml_module()
pytestmark = pytest.mark.skipif(tomllib is None, reason="tomllib or tomli is required")
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

    rust = (ROOT / "crates" / "reprocut-core" / "src" / "schema.rs").read_text(encoding="utf-8")
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
    evidence = json.loads((ROOT / "demo" / "result" / "reduction.json").read_text(encoding="utf-8"))
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
    assert project["license"] == "Apache-2.0"
    assert project["license-files"] == ["LICENSE"]
    assert project["scripts"]["reprocut-py"] == "reprocut.cli:main"
    assert project["urls"]["Repository"].endswith("/reprocut")
    assert pyproject["tool"]["maturin"]["module-name"] == "reprocut._native"
    assert pyproject["tool"]["maturin"]["locked"] is True
    assert pyproject["tool"]["maturin"]["sdist-generator"] == "git"
    assert "tomli==2.4.1; python_version < '3.11'" in project["optional-dependencies"]["test"]


def test_native_wheel_matrix_installs_the_toml_fallback_explicitly() -> None:
    workflow = (ROOT / ".github" / "workflows" / "ci.yml").read_text(encoding="utf-8")

    assert "\"tomli==2.4.1; python_version < '3.11'\"" in workflow


def test_all_project_metadata_uses_the_single_apache_2_license() -> None:
    cargo = tomllib.loads((ROOT / "Cargo.toml").read_text(encoding="utf-8"))
    editor = json.loads((ROOT / "editors" / "vscode" / "package.json").read_text(encoding="utf-8"))
    gallery = json.loads((ROOT / "gallery" / "package.json").read_text(encoding="utf-8"))
    readme = (ROOT / "README.md").read_text(encoding="utf-8")
    release_readme = (ROOT / "release" / "README.md").read_text(encoding="utf-8")

    assert cargo["workspace"]["package"]["license"] == "Apache-2.0"
    assert editor["license"] == "Apache-2.0"
    assert gallery["license"] == "Apache-2.0"
    assert (ROOT / "LICENSE").is_file()
    assert not (ROOT / "LICENSE-MIT").exists()
    assert not (ROOT / "LICENSE-APACHE").exists()
    assert "Licensed under the [Apache License 2.0](LICENSE)." in readme
    assert "README, the Apache-2.0 license, and a version record" in release_readme
    assert "dual licenses" not in release_readme


def test_release_workflows_are_native_safe_restartable_and_preflighted() -> None:
    publish = (ROOT / ".github" / "workflows" / "publish-registries.yml").read_text(
        encoding="utf-8"
    )
    release = (ROOT / ".github" / "workflows" / "release.yml").read_text(encoding="utf-8")
    ci = (ROOT / ".github" / "workflows" / "ci.yml").read_text(encoding="utf-8")

    for workflow in (publish, release):
        assert "macos-13" not in workflow
        assert re.search(
            r"target: x86_64-apple-darwin\s+runner: macos-15-intel"
            r"|runner: macos-15-intel\s+target: x86_64-apple-darwin",
            workflow,
        )
        assert re.search(
            r"target: aarch64-apple-darwin\s+runner: macos-15"
            r"|runner: macos-15\s+target: aarch64-apple-darwin",
            workflow,
        )

    for workflow in (publish, ci):
        assert "--no-verify" not in workflow
        assert "scripts/release/publish_crates.py preflight" in workflow

    assert "scripts/release/publish_crates.py publish" in publish
    assert "--expected-owner emirhuseynrmx" in publish
    assert (ROOT / "scripts" / "release" / "publish_crates.py").is_file()


def test_sdist_is_built_and_smoked_outside_the_repository() -> None:
    workflows = [
        (ROOT / ".github" / "workflows" / name).read_text(encoding="utf-8")
        for name in ("ci.yml", "publish-registries.yml")
    ]
    for workflow in workflows:
        assert "python -m pip wheel" in workflow
        assert "--no-deps" in workflow
        assert "/tmp/reprocut-sdist-wheel" in workflow
        assert "python -m venv /tmp/reprocut-sdist-smoke" in workflow
        assert "REPROCUT_REQUIRE_NATIVE=1" in workflow


def test_registry_readme_links_and_install_contract_are_release_stable() -> None:
    readme = (ROOT / "README.md").read_text(encoding="utf-8")
    pinned = "https://github.com/emirhuseynrmx/reprocut/raw/v0.1.0/"

    assert f"{pinned}assets/reprocut-banner.svg" in readme
    assert f"{pinned}assets/reprocut-demo.gif" in readme
    assert "packages have not been published" not in readme
    assert "cargo install reprocut --version 0.1.0 --locked" in readme
    assert "python -m pip install reprocut==0.1.0" in readme
    assert "The Python package does not bundle the Rust reducer CLI." in readme


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
