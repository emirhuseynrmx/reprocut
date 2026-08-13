from __future__ import annotations

import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts" / "release"))

from audit import (  # noqa: E402
    REQUIRED_CI_GATES,
    ci_checks,
    demo_artifact_manifest_check,
    dependency_lock_check,
    static_checks,
)


def test_static_release_contract_is_fully_encoded_and_current() -> None:
    assert {
        "oracle-adversarial",
        "python-isolation",
        "snapshot-integrity",
    } <= REQUIRED_CI_GATES
    checks = static_checks(ROOT)

    assert len(checks) >= 8
    assert "oracle-ci-coverage" in {item.name for item in checks}
    assert "dependency-lock" in {item.name for item in checks}
    assert all(item.passed for item in checks), [item for item in checks if not item.passed]


def test_ci_evidence_is_schema_versioned_and_bound_to_the_expected_commit(
    tmp_path: Path,
) -> None:
    commit = "a" * 40
    evidence = tmp_path / "ci-evidence.json"
    evidence.write_text(
        json.dumps(
            {
                "schema_version": 1,
                "commit": commit,
                "statuses": {gate: "success" for gate in REQUIRED_CI_GATES},
            }
        ),
        encoding="utf-8",
    )

    checks = ci_checks(evidence, commit)

    assert all(item.passed for item in checks)


def test_ci_evidence_fails_closed_for_unknown_schema_and_invalid_statuses(
    tmp_path: Path,
) -> None:
    evidence = tmp_path / "ci-evidence.json"
    evidence.write_text(
        json.dumps({"schema_version": 2, "commit": "b" * 40, "statuses": []}),
        encoding="utf-8",
    )

    checks = ci_checks(evidence, "b" * 40)

    assert not next(item for item in checks if item.name == "ci-schema").passed
    assert all(not item.passed for item in checks if item.name.startswith("ci:"))


def test_dependency_lock_check_rejects_missing_lock_and_unlocked_graph_commands(
    tmp_path: Path,
) -> None:
    workflows = tmp_path / ".github" / "workflows"
    workflows.mkdir(parents=True)
    workflow = workflows / "ci.yml"
    workflow.write_text("jobs:\n  quality:\n    steps:\n      - run: cargo test\n")

    assert not dependency_lock_check(tmp_path).passed

    (tmp_path / "Cargo.lock").write_text("# committed lock\n", encoding="utf-8")
    assert not dependency_lock_check(tmp_path).passed

    workflow.write_text(
        "jobs:\n  quality:\n    steps:\n"
        "      - run: cargo test --locked\n"
        "      - run: maturin sdist --out dist\n",
        encoding="utf-8",
    )
    assert dependency_lock_check(tmp_path).passed

    workflow.write_text(
        "jobs:\n  quality:\n    steps:\n"
        "      - run: cargo test --locked\n"
        "      - run: maturin sdist --out dist\n"
        "      - run: maturin build --release\n",
        encoding="utf-8",
    )
    assert not dependency_lock_check(tmp_path).passed


def test_demo_artifact_manifest_rejects_changed_retained_bytes(tmp_path: Path) -> None:
    artifact = tmp_path / "demo" / "result"
    artifact.mkdir(parents=True)
    (artifact / "bug.py").write_bytes(b"before")
    digest = __import__("hashlib").sha256(b"before").hexdigest()
    (artifact / "artifact-manifest.json").write_text(
        json.dumps(
            {
                "schema_version": 1,
                "artifact_id": "a" * 64,
                "members": [
                    {
                        "path": "bug.py",
                        "sha256": digest,
                        "size_bytes": 6,
                        "executable_mask": 0,
                    }
                ],
            }
        ),
        encoding="utf-8",
    )
    assert demo_artifact_manifest_check(tmp_path).passed

    (artifact / "bug.py").write_bytes(b"after")
    assert not demo_artifact_manifest_check(tmp_path).passed
