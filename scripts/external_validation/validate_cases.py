#!/usr/bin/env python3
"""Strict parser for the pinned external-validation case catalog."""

from __future__ import annotations

import argparse
import json
import re
from dataclasses import dataclass, fields
from pathlib import Path
from typing import Sequence


DEFAULT_ORDER = ("openruyi", "ipe", "bevy")
SHA_PATTERN = re.compile(r"^[0-9a-f]{40}$")
CASE_ID_PATTERN = re.compile(r"^[a-z][a-z0-9-]*$")
MEMORY_PATTERN = re.compile(r"^[1-9][0-9]*(?:m|g)$")
REPOSITORY_PATTERN = re.compile(r"^https://github\.com/[^/\s]+/[^/\s]+(?:\.git)?$")
CI_URL_PATTERN = re.compile(r"^https://github\.com/[^/\s]+/[^/\s]+/actions/runs/[0-9]+/job/[0-9]+$")


class CatalogError(ValueError):
    """The external-validation catalog is unsafe or malformed."""


@dataclass(frozen=True)
class CaseSpec:
    case_id: str
    repository: str
    base_ref: str
    head_sha: str
    ci_url: str
    oracle_argv: list[str]
    required_regex: list[str]
    rejected_regex: list[str]
    memory: str
    timeout_minutes: int
    attempt_timeout_ms: int


CASE_KEYS = frozenset(field.name for field in fields(CaseSpec))


def _require_string_list(case_id: str, key: str, value: object) -> list[str]:
    if not isinstance(value, list) or not value:
        raise CatalogError(f"{case_id}.{key} must be a non-empty string array")
    if any(not isinstance(item, str) or not item or "\0" in item for item in value):
        raise CatalogError(f"{case_id}.{key} contains an invalid string")
    return list(value)


def _parse_case(raw: object) -> CaseSpec:
    if not isinstance(raw, dict):
        raise CatalogError("each case must be an object")
    unknown = set(raw) - CASE_KEYS
    missing = CASE_KEYS - set(raw)
    if unknown:
        raise CatalogError(f"case contains unknown keys: {sorted(unknown)}")
    if missing:
        raise CatalogError(f"case is missing keys: {sorted(missing)}")

    case_id = raw["case_id"]
    if not isinstance(case_id, str) or not CASE_ID_PATTERN.fullmatch(case_id):
        raise CatalogError("case_id must be a lowercase identifier")
    repository = raw["repository"]
    if not isinstance(repository, str) or not REPOSITORY_PATTERN.fullmatch(repository):
        raise CatalogError(f"{case_id}.repository must be an HTTPS GitHub repository URL")
    base_ref = raw["base_ref"]
    if not isinstance(base_ref, str) or not base_ref or any(char.isspace() for char in base_ref):
        raise CatalogError(f"{case_id}.base_ref is invalid")
    head_sha = raw["head_sha"]
    if not isinstance(head_sha, str) or not SHA_PATTERN.fullmatch(head_sha):
        raise CatalogError(f"{case_id}.head_sha must be a pinned 40-character SHA")
    ci_url = raw["ci_url"]
    if not isinstance(ci_url, str) or not CI_URL_PATTERN.fullmatch(ci_url):
        raise CatalogError(f"{case_id}.ci_url must identify a GitHub Actions job")

    oracle_argv = _require_string_list(case_id, "oracle_argv", raw["oracle_argv"])
    required_regex = _require_string_list(case_id, "required_regex", raw["required_regex"])
    rejected_regex = _require_string_list(case_id, "rejected_regex", raw["rejected_regex"])
    for key, patterns in (("required_regex", required_regex), ("rejected_regex", rejected_regex)):
        try:
            for pattern in patterns:
                re.compile(pattern)
        except re.error as error:
            raise CatalogError(f"{case_id}.{key} contains invalid regex: {error}") from error

    memory = raw["memory"]
    if not isinstance(memory, str) or not MEMORY_PATTERN.fullmatch(memory):
        raise CatalogError(f"{case_id}.memory must use a positive m or g suffix")
    timeout_minutes = raw["timeout_minutes"]
    attempt_timeout_ms = raw["attempt_timeout_ms"]
    if isinstance(timeout_minutes, bool) or not isinstance(timeout_minutes, int) or timeout_minutes <= 0:
        raise CatalogError(f"{case_id}.timeout_minutes must be positive")
    if isinstance(attempt_timeout_ms, bool) or not isinstance(attempt_timeout_ms, int) or attempt_timeout_ms <= 0:
        raise CatalogError(f"{case_id}.attempt_timeout_ms must be positive")

    return CaseSpec(
        case_id=case_id,
        repository=repository,
        base_ref=base_ref,
        head_sha=head_sha,
        ci_url=ci_url,
        oracle_argv=oracle_argv,
        required_regex=required_regex,
        rejected_regex=rejected_regex,
        memory=memory,
        timeout_minutes=timeout_minutes,
        attempt_timeout_ms=attempt_timeout_ms,
    )


def load_cases(path: Path, expected_order: Sequence[str] = DEFAULT_ORDER) -> tuple[CaseSpec, ...]:
    try:
        document = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise CatalogError(f"cannot read catalog {path}: {error}") from error
    if not isinstance(document, dict) or set(document) != {"schema_version", "cases"}:
        raise CatalogError("catalog must contain only schema_version and cases")
    if document["schema_version"] != 1:
        raise CatalogError("catalog schema_version must be 1")
    if not isinstance(document["cases"], list):
        raise CatalogError("catalog cases must be an array")
    cases = tuple(_parse_case(raw) for raw in document["cases"])
    identifiers = tuple(case.case_id for case in cases)
    if identifiers != tuple(expected_order):
        raise CatalogError(f"case order must be {tuple(expected_order)}, got {identifiers}")
    if len(set(identifiers)) != len(identifiers):
        raise CatalogError("case identifiers must be unique")
    return cases


def select_case(cases: Sequence[CaseSpec], case_id: str) -> CaseSpec:
    for case in cases:
        if case.case_id == case_id:
            return case
    raise CatalogError(f"unknown case: {case_id}")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("catalog", nargs="?", type=Path, default=Path(__file__).with_name("cases.json"))
    arguments = parser.parse_args()
    cases = load_cases(arguments.catalog)
    print(json.dumps({"schema_version": 1, "case_ids": [case.case_id for case in cases]}))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
