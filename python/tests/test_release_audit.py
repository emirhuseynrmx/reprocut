from __future__ import annotations

import sys
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts" / "release"))

from audit import REQUIRED_CI_GATES, ci_checks, static_checks


def test_static_release_contract_is_fully_encoded_and_current() -> None:
    checks = static_checks(ROOT)

    assert len(checks) >= 8
    assert all(item.passed for item in checks), [
        item for item in checks if not item.passed
    ]


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
