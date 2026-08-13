from __future__ import annotations

import pytest
from reprocut import FailureOracle


def test_exit_zero_mode_uses_termination_without_output_identity() -> None:
    oracle = FailureOracle.from_baselines(
        [(0, "", ""), (0, "", "")],
        mode="exit_zero",
    )

    assert oracle.classify(0, "truncated output", truncated=True) == "preserved"
    assert oracle.classify(7, "") == "rejected"
    assert oracle.classify(0, "", timed_out=True) == "inconclusive"
    assert oracle.fingerprint["mode"] == "exit_zero"
    assert oracle.fingerprint["anchors"] == []
    assert oracle.fingerprint["normalization_schema"] == 5


def test_regex_mode_requires_all_patterns_and_applies_reject_veto() -> None:
    oracle = FailureOracle.from_baselines(
        [
            (1, "", "TypeError: invoice 7 currency"),
            (1, "", "TypeError: invoice 8 currency"),
        ],
        mode="regex",
        channel="stderr",
        failure_patterns=(r"TypeError: invoice [0-9]+", "currency"),
        reject_patterns=("secondary failure",),
    )

    assert oracle.classify(1, "TypeError: invoice 9 currency") == "preserved"
    assert oracle.classify(1, "TypeError: invoice 9") == "rejected"
    assert oracle.classify(1, "TypeError: invoice 9 currency\nsecondary failure") == "rejected"
    assert oracle.fingerprint["mode"] == "regex"
    assert oracle.fingerprint["failure_patterns"] == [
        "TypeError: invoice [0-9]+",
        "currency",
    ]


@pytest.mark.parametrize(
    ("mode", "failure_patterns", "reject_patterns", "message"),
    [
        ("regex", (), (), "requires"),
        ("exit_zero", ("x",), (), "patterns"),
        ("automatic", ("x",), (), "patterns"),
        ("regex", ("(",), (), "regular expression"),
        ("regex", ("x" * 4097,), (), "4096"),
        ("regex", tuple(f"p{index}" for index in range(17)), (), "16"),
    ],
)
def test_invalid_oracle_specs_fail_before_baselines(
    mode: str,
    failure_patterns: tuple[str, ...],
    reject_patterns: tuple[str, ...],
    message: str,
) -> None:
    with pytest.raises(ValueError, match=message):
        FailureOracle.from_baselines(
            [(1, "TypeError"), (1, "TypeError")],
            mode=mode,
            failure_patterns=failure_patterns,
            reject_patterns=reject_patterns,
        )
