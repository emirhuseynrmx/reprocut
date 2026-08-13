#!/usr/bin/env python3
"""Fail closed when the ReproCut 0.1 release contract lacks evidence."""

from __future__ import annotations

import argparse
import json
import re
from dataclasses import asdict, dataclass
from pathlib import Path

from schema_versions import CI_EVIDENCE_SCHEMA, EVIDENCE_SCHEMA, NORMALIZATION_SCHEMA

try:
    import tomllib
except ModuleNotFoundError:  # pragma: no cover - Python 3.10 wheel job
    tomllib = None

VERSION = "0.1.0"
REQUIRED_TARGETS = {
    "x86_64-unknown-linux-gnu",
    "x86_64-unknown-linux-musl",
    "aarch64-unknown-linux-gnu",
    "x86_64-pc-windows-msvc",
    "x86_64-apple-darwin",
    "aarch64-apple-darwin",
}
REQUIRED_CI_GATES = {
    "quality",
    "loom",
    "miri",
    "sanitizer",
    "supply-chain",
    "oci-archive",
    "python-wheel-3.9",
    "python-wheel-3.10",
    "python-wheel-3.11",
    "python-wheel-3.12",
    "python-wheel-3.13",
    "cli-linux",
    "cli-windows",
    "cli-macos",
    "release-benchmark",
    "release-packages",
    "editor",
    "gallery",
    "oracle-adversarial",
    "python-isolation",
    "snapshot-integrity",
    *(f"archive-{target}" for target in REQUIRED_TARGETS),
}
CARGO_GRAPH_COMMAND = re.compile(
    r"\bcargo(?: \+[^ \t]+)? (?:miri )?"
    r"(?:bench|build|clippy|doc|install|metadata|package|publish|run|test)\b"
)
# `maturin build` and `develop` invoke Cargo dependency resolution. `maturin sdist`
# only archives source (including the committed Cargo.lock) and has no --locked option.
MATURIN_GRAPH_COMMAND = re.compile(r"\bmaturin (?:build|develop)\b")


@dataclass(frozen=True)
class Check:
    name: str
    passed: bool
    detail: str


def static_checks(root: Path) -> list[Check]:
    checks: list[Check] = []
    checks.append(check("version", versions_are_consistent(root), "all package surfaces are 0.1.0"))

    evidence = json.loads((root / "demo/result/reduction.json").read_text(encoding="utf-8"))
    demo_ok = (
        evidence.get("schema_version") == EVIDENCE_SCHEMA
        and evidence["failure"]["same_failure"] is True
        and evidence["failure"].get("normalization_schema") == NORMALIZATION_SCHEMA
        and evidence["failure"].get("oracle_mode") in {"automatic", "regex", "exit_zero"}
        and evidence["search"]["final_verifications"] == 3
        and evidence["measurements"]["original"]["files"] == 18
        and evidence["measurements"]["retained"]["files"] == 3
        and len(evidence["failure"]["fingerprint_sha256"]) == 64
        and len(evidence["failure"].get("oracle_spec_sha256", "")) == 64
        and len(evidence.get("source_snapshot_sha256", "")) == 64
        and (
            len(evidence.get("preparation", {}).get("contract_sha256") or "") == 64
            or bool(evidence.get("preparation", {}).get("limitations"))
        )
    )
    checks.append(
        check(
            "demo-evidence",
            demo_ok,
            "schema 4, bound digests, 18->3, same failure, final 3/3",
        )
    )
    attempts = (root / "demo/result/attempts.jsonl").read_text(encoding="utf-8").splitlines()
    checks.append(
        check(
            "attempt-ledger",
            len(attempts) == len(evidence["attempts"]) > 0
            and all(json.loads(line)["event_id"] > 0 for line in attempts),
            "checked-in JSONL matches evidence attempts",
        )
    )

    corpus = json.loads((root / "benchmarks/upstream-corpus.json").read_text(encoding="utf-8"))
    cases = corpus.get("cases", corpus if isinstance(corpus, list) else [])
    checks.append(check("upstream-corpus", len(cases) == 24, "24 pinned upstream cases"))

    required = [
        ".github/workflows/ci.yml",
        ".github/workflows/gallery.yml",
        ".github/workflows/publish-registries.yml",
        ".github/workflows/release.yml",
        "editors/vscode/src/runner.js",
        "gallery/scripts/build.js",
        "scripts/benchmark_release.py",
        "scripts/release/package_binary.py",
        "scripts/release/verify_archive.py",
        "scripts/release/build_manifest.py",
        "docs/RELEASING.md",
        "docs/launch/HACKER_NEWS.md",
    ]
    missing = [path for path in required if not (root / path).is_file()]
    checks.append(
        check(
            "release-surfaces",
            not missing,
            f"missing={missing}" if missing else "present",
        )
    )

    ci_workflow = (root / ".github/workflows/ci.yml").read_text(encoding="utf-8")
    oracle_job = workflow_job(ci_workflow, "oracle-adversarial")
    oracle_targets = {
        "--test oracle_contract",
        "--test oracle_properties",
        "--test oracle_adversarial",
    }
    checks.append(
        check(
            "oracle-ci-coverage",
            oracle_job is not None and all(target in oracle_job for target in oracle_targets),
            "oracle contract, property, and adversarial Cargo targets are explicit",
        )
    )
    checks.append(dependency_lock_check(root))

    release_workflow = (root / ".github/workflows/release.yml").read_text(encoding="utf-8")
    checks.append(
        check(
            "release-targets",
            all(target in release_workflow for target in REQUIRED_TARGETS),
            "all six target triples are declared",
        )
    )
    checks.append(
        check(
            "release-integrity",
            all(
                marker in release_workflow
                for marker in (
                    "smoke_binary.py",
                    "sbom-action",
                    "build_manifest.py",
                    "attest-build-provenance",
                    "environment: release",
                )
            ),
            "smoke, SPDX, checksum, provenance, protected release",
        )
    )

    readme = (root / "README.md").read_text(encoding="utf-8")
    stale = (
        "regular-file-level only",
        "manifest and syntax reducers are not implemented",
    )
    checks.append(
        check(
            "honest-readme",
            not any(value in readme.lower() for value in stale)
            and "currently claims no measured speedup" in readme,
            "no stale scope or unmeasured speed claim",
        )
    )
    return checks


def dependency_lock_check(root: Path) -> Check:
    workflows = sorted((root / ".github/workflows").glob("*.yml"))
    violations: list[str] = []
    lock = root / "Cargo.lock"
    if not lock.is_file() or lock.stat().st_size == 0:
        violations.append("Cargo.lock missing")
    for workflow in workflows:
        content = workflow.read_text(encoding="utf-8")
        if "cargo generate-lockfile" in content:
            violations.append(f"{workflow.name}: regenerates Cargo.lock")
        for number, line in enumerate(content.splitlines(), start=1):
            if (
                CARGO_GRAPH_COMMAND.search(line) or MATURIN_GRAPH_COMMAND.search(line)
            ) and "--locked" not in line:
                violations.append(f"{workflow.name}:{number}: unlocked graph command")
        action = re.compile(
            r"(?ms)^[ \t]*uses: PyO3/maturin-action@[^\r\n]+\r?\n"
            r"(?P<body>.*?)(?=^[ \t]*-[ \t]+(?:name:|uses:)|\Z)"
        )
        violations.extend(
            f"{workflow.name}: maturin-action is not locked"
            for match in action.finditer(content)
            if not re.search(r"(?m)^[ \t]*args:[^\r\n]*--locked", match.group("body"))
        )
    return check(
        "dependency-lock",
        not violations,
        ("committed lock and locked workflow graph" if not violations else "; ".join(violations)),
    )


def workflow_job(workflow: str, name: str) -> str | None:
    match = re.search(
        rf"(?ms)^  {re.escape(name)}:\r?\n(?P<body>.*?)(?=^  [A-Za-z0-9_-]+:\r?\n|\Z)",
        workflow,
    )
    return None if match is None else match.group("body")


def versions_are_consistent(root: Path) -> bool:
    if tomllib is None:
        return True
    cargo = tomllib.loads((root / "Cargo.toml").read_text(encoding="utf-8"))
    python = tomllib.loads((root / "pyproject.toml").read_text(encoding="utf-8"))
    editor = json.loads((root / "editors/vscode/package.json").read_text(encoding="utf-8"))
    gallery = json.loads((root / "gallery/package.json").read_text(encoding="utf-8"))
    return {
        cargo["workspace"]["package"]["version"],
        python["project"]["version"],
        editor["version"],
        gallery["version"],
    } == {VERSION}


def ci_checks(path: Path, expected_commit: str | None) -> list[Check]:
    evidence = json.loads(path.read_text(encoding="utf-8"))
    commit = evidence.get("commit", "")
    statuses = evidence.get("statuses", {})
    schema_ok = evidence.get("schema_version") == CI_EVIDENCE_SCHEMA
    statuses_ok = isinstance(statuses, dict)
    checks = [
        check(
            "ci-schema",
            schema_ok,
            f"schema={evidence.get('schema_version', '<missing>')}",
        )
    ]
    commit_ok = bool(re.fullmatch(r"[0-9a-f]{40}", commit)) and (
        expected_commit is None or commit == expected_commit
    )
    checks.append(check("ci-commit", commit_ok, f"commit={commit or '<missing>'}"))
    for gate in sorted(REQUIRED_CI_GATES):
        status = statuses.get(gate, "missing") if statuses_ok else "invalid statuses object"
        checks.append(check(f"ci:{gate}", status == "success", status))
    return checks


def check(name: str, passed: bool, detail: str) -> Check:
    return Check(name=name, passed=bool(passed), detail=detail)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--repository", type=Path, default=Path.cwd())
    parser.add_argument("--ci-evidence", type=Path)
    parser.add_argument("--expected-commit")
    parser.add_argument("--static-only", action="store_true")
    parser.add_argument("--json", action="store_true")
    arguments = parser.parse_args()
    checks = static_checks(arguments.repository.resolve())
    if arguments.ci_evidence:
        checks.extend(ci_checks(arguments.ci_evidence, arguments.expected_commit))
    elif not arguments.static_only:
        checks.append(check("ci-evidence", False, "pass --ci-evidence from the clean release run"))

    if arguments.json:
        print(
            json.dumps(
                {
                    "schema_version": CI_EVIDENCE_SCHEMA,
                    "checks": [asdict(item) for item in checks],
                },
                indent=2,
            )
        )
    else:
        for item in checks:
            print(f"{'PASS' if item.passed else 'FAIL'}  {item.name}: {item.detail}")
    return 0 if all(item.passed for item in checks) else 2


if __name__ == "__main__":
    raise SystemExit(main())
