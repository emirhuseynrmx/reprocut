from __future__ import annotations

import pytest
from reprocut import FailureOracle


def stable_oracle() -> FailureOracle:
    return FailureOracle.from_baselines(
        [
            (1, "TypeError: currency"),
            (1, "TypeError: currency"),
            (1, "TypeError: currency"),
        ]
    )


def test_same_failure_is_preserved() -> None:
    assert stable_oracle().classify(1, "TypeError: currency") == "preserved"


def test_different_failure_is_rejected() -> None:
    assert stable_oracle().classify(1, "ModuleNotFoundError") == "rejected"


def test_incomplete_observation_is_never_accepted() -> None:
    oracle = stable_oracle()
    assert oracle.classify(1, "TypeError: currency", timed_out=True) == "inconclusive"
    assert oracle.classify(1, "TypeError: currency", truncated=True) == "inconclusive"


def test_unstable_or_too_small_baseline_is_a_value_error() -> None:
    with pytest.raises(ValueError, match="at least two"):
        FailureOracle.from_baselines([(1, "TypeError: currency")])
    with pytest.raises(ValueError, match="unstable"):
        FailureOracle.from_baselines([(1, "TypeError: currency"), (2, "TypeError: currency")])


def test_fingerprint_is_an_immutable_plain_value() -> None:
    oracle = stable_oracle()
    assert oracle.fingerprint == {
        "exit_code": 1,
        "signal": None,
        "anchor": "TypeError: currency",
        "anchors": [{"channel": "stderr", "text": "TypeError: currency"}],
        "normalization_schema": 1,
    }
    with pytest.raises(AttributeError):
        oracle.extra = "mutation"  # type: ignore[attr-defined]


def test_volatile_paths_and_ids_do_not_change_failure_identity() -> None:
    oracle = FailureOracle.from_baselines(
        [
            (1, "TypeError: request 10 at /tmp/alpha.py"),
            (1, "TypeError: request 20 at /var/run/beta.py"),
        ]
    )
    assert oracle.classify(1, "TypeError: request 30 at /opt/build/gamma.py") == "preserved"


def test_auto_requires_stable_stdout_and_stderr_when_both_exist() -> None:
    oracle = FailureOracle.from_baselines(
        [(1, "stable stdout", "stable stderr"), (1, "stable stdout", "stable stderr")],
        channel="auto",
    )
    assert oracle.classify(1, "stable stderr", stdout="stable stdout") == "preserved"
    assert oracle.classify(1, "stable stderr", stdout="changed stdout") == "rejected"
    assert oracle.fingerprint["anchors"] == [
        {"channel": "stdout", "text": "stable stdout"},
        {"channel": "stderr", "text": "stable stderr"},
    ]


def test_explicit_stderr_ignores_unstable_stdout_baselines() -> None:
    oracle = FailureOracle.from_baselines(
        [(1, "progress one", "stable stderr"), (1, "progress two", "stable stderr")],
        channel="stderr",
    )
    assert oracle.classify(1, "stable stderr", stdout="anything") == "preserved"
