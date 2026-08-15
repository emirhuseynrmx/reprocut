from __future__ import annotations

import hashlib
import importlib.util
import io
import subprocess
import sys
import tarfile
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parents[2]
SCRIPT = ROOT / "scripts" / "release" / "publish_crates.py"
EXPECTED_ORDER = (
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
)


def load_publish_crates():
    assert SCRIPT.is_file(), "restartable crates.io publisher is missing"
    spec = importlib.util.spec_from_file_location("reprocut_publish_crates", SCRIPT)
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


class FakeRegistry:
    def __init__(self, versions: dict[str, dict[str, object] | None], owner: str) -> None:
        self.versions = versions
        self.owner = owner
        self.lookups: list[str] = []

    def version(self, package: str, version: str):
        assert version == "0.1.0"
        self.lookups.append(package)
        return self.versions.get(package)

    def owners(self, package: str) -> set[str]:
        return {self.owner}


def write_archives(root: Path, module) -> dict[str, str]:
    package_dir = root / "target" / "package"
    package_dir.mkdir(parents=True)
    license_contents = b"Apache License\n"
    (root / "LICENSE").write_bytes(license_contents)
    checksums = {}
    for package in module.PUBLISH_ORDER:
        archive = package_dir / f"{package}-{module.VERSION}.crate"
        write_crate_archive(archive, package, license_contents)
        checksums[package] = hashlib.sha256(archive.read_bytes()).hexdigest()
    return checksums


def write_crate_archive(archive: Path, package: str, license_contents: bytes | None) -> None:
    archive.parent.mkdir(parents=True, exist_ok=True)
    with tarfile.open(archive, "w:gz") as package_archive:
        manifest = b'[package]\nname = "fixture"\nversion = "0.1.0"\n'
        manifest_info = tarfile.TarInfo(f"{package}-0.1.0/Cargo.toml")
        manifest_info.size = len(manifest)
        package_archive.addfile(manifest_info, io.BytesIO(manifest))
        if license_contents is not None:
            license_info = tarfile.TarInfo(f"{package}-0.1.0/LICENSE")
            license_info.size = len(license_contents)
            package_archive.addfile(license_info, io.BytesIO(license_contents))


def test_preflight_verifies_every_package_in_dependency_order(tmp_path: Path) -> None:
    module = load_publish_crates()
    commands: list[list[str]] = []
    license_contents = b"Apache License\n"
    (tmp_path / "LICENSE").write_bytes(license_contents)

    def fake_run(command: list[str], _repository: Path) -> None:
        commands.append(command)
        package = command[command.index("-p") + 1]
        archive = tmp_path / "target" / "package" / f"{package}-{module.VERSION}.crate"
        write_crate_archive(archive, package, license_contents)

    result = module.preflight(tmp_path, run=fake_run)

    assert module.PUBLISH_ORDER == EXPECTED_ORDER
    assert tuple(result) == EXPECTED_ORDER
    assert [command[command.index("-p") + 1] for command in commands] == list(EXPECTED_ORDER)
    assert all(command[:3] == ["cargo", "package", "--locked"] for command in commands)
    assert all("--no-verify" not in command for command in commands)
    for command in commands:
        package = command[command.index("-p") + 1]
        patched = {
            item.split(".", 2)[2].split(".", 1)[0]
            for item in command
            if item.startswith("patch.crates-io.")
        }
        assert patched == set(module.PACKAGE_DEPENDENCIES[package])


def test_preflight_rejects_missing_or_mismatched_crate_license(tmp_path: Path) -> None:
    module = load_publish_crates()
    package = EXPECTED_ORDER[0]
    archive = tmp_path / "target" / "package" / f"{package}-{module.VERSION}.crate"
    (tmp_path / "LICENSE").write_bytes(b"canonical Apache license\n")

    write_crate_archive(archive, package, None)
    with pytest.raises(module.PublishError, match="LICENSE"):
        module.ensure_archive(tmp_path, package)

    write_crate_archive(archive, package, b"different license\n")
    with pytest.raises(module.PublishError, match="LICENSE"):
        module.ensure_archive(tmp_path, package)


def test_every_publishable_cargo_package_embeds_license() -> None:
    module = load_publish_crates()

    for package in EXPECTED_ORDER:
        result = subprocess.run(
            [
                module.cargo_executable(),
                "package",
                "--locked",
                "--allow-dirty",
                "-p",
                package,
                "--list",
                *module.cargo_patch_arguments(package),
            ],
            cwd=ROOT,
            check=True,
            capture_output=True,
            text=True,
        )
        assert "LICENSE" in result.stdout.splitlines(), f"{package} package omits LICENSE"


def test_publish_skips_only_an_identical_owned_registry_version(tmp_path: Path) -> None:
    module = load_publish_crates()
    checksums = write_archives(tmp_path, module)
    registry = FakeRegistry(
        {package: {"checksum": checksum} for package, checksum in checksums.items()},
        "emirhuseynrmx",
    )

    results = module.publish(
        tmp_path,
        registry=registry,
        expected_owner="emirhuseynrmx",
        run=lambda *_: pytest.fail("an identical published package must be skipped"),
        sleep=lambda _: None,
    )

    assert [item.status for item in results] == ["skipped"] * len(EXPECTED_ORDER)
    assert registry.lookups == list(EXPECTED_ORDER)


def test_publish_waits_for_new_version_then_verifies_checksum_and_owner(tmp_path: Path) -> None:
    module = load_publish_crates()
    checksums = write_archives(tmp_path, module)
    first = EXPECTED_ORDER[0]
    registry = FakeRegistry(
        {package: {"checksum": checksum} for package, checksum in checksums.items()},
        "emirhuseynrmx",
    )
    registry.versions[first] = None
    commands: list[list[str]] = []

    def fake_run(command: list[str], _repository: Path) -> None:
        commands.append(command)
        registry.versions[first] = {"checksum": checksums[first]}

    results = module.publish(
        tmp_path,
        registry=registry,
        expected_owner="emirhuseynrmx",
        run=fake_run,
        sleep=lambda _: None,
    )

    assert commands == [["cargo", "publish", "--locked", "-p", first]]
    assert results[0].status == "published"
    assert all(item.status == "skipped" for item in results[1:])


def test_publish_fails_closed_for_checksum_or_owner_mismatch(tmp_path: Path) -> None:
    module = load_publish_crates()
    checksums = write_archives(tmp_path, module)
    versions = {package: {"checksum": checksum} for package, checksum in checksums.items()}
    versions[EXPECTED_ORDER[0]] = {"checksum": "0" * 64}

    with pytest.raises(module.PublishError, match="checksum"):
        module.publish(
            tmp_path,
            registry=FakeRegistry(versions, "emirhuseynrmx"),
            expected_owner="emirhuseynrmx",
            run=lambda *_: None,
            sleep=lambda _: None,
        )

    with pytest.raises(module.PublishError, match="owner"):
        module.publish(
            tmp_path,
            registry=FakeRegistry(
                {package: {"checksum": checksum} for package, checksum in checksums.items()},
                "someone-else",
            ),
            expected_owner="emirhuseynrmx",
            run=lambda *_: None,
            sleep=lambda _: None,
        )
