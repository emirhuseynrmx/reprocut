from __future__ import annotations

import json
from pathlib import Path

import pytest
from reprocut import FailureOracle

CASES = json.loads((Path(__file__).with_name("oracle_cases.json")).read_text("utf-8"))
BY_NAME = {case["name"]: case for case in CASES}


@pytest.mark.parametrize("case", CASES, ids=lambda case: case["name"])
def test_public_backend_matches_the_cross_language_oracle_corpus(
    case: dict[str, object],
) -> None:
    oracle = FailureOracle.from_baselines(
        case["baselines"],
        mode=case["mode"],
        channel=case["channel"],
        failure_patterns=case["failure_patterns"],
        reject_patterns=case["reject_patterns"],
    )
    candidate = dict(case["candidate"])
    assert oracle.classify(**candidate) == case["expected"]

    expected = case.get("fingerprint")
    if expected is None:
        expected = BY_NAME[case["fingerprint_ref"]]["fingerprint"]
    assert oracle.fingerprint == expected
