from __future__ import annotations

import pytest
from reprocut import EvaluationPolicy, FailureOracle


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
    fingerprint = oracle.fingerprint
    assert fingerprint | {
        "oracle_spec_sha256": "<digest>",
        "fingerprint_sha256": "<digest>",
    } == {
        "mode": "automatic",
        "exit_code": 1,
        "signal": None,
        "termination": {"kind": "exit_code", "value": 1},
        "anchor": "TypeError: currency",
        "anchors": [{"channel": "stderr", "text": "TypeError: currency"}],
        "failure_patterns": [],
        "reject_patterns": [],
        "normalization_schema": 4,
        "oracle_spec_sha256": "<digest>",
        "fingerprint_sha256": "<digest>",
    }
    assert len(fingerprint["oracle_spec_sha256"]) == 64
    assert len(fingerprint["fingerprint_sha256"]) == 64
    with pytest.raises(AttributeError):
        oracle.extra = "mutation"  # type: ignore[attr-defined]


def test_volatile_paths_and_ids_do_not_change_failure_identity() -> None:
    oracle = FailureOracle.from_baselines(
        [
            (1, "PID 10 TypeError: request at /tmp/alpha.py:10:2 port 5001 after 10ms"),
            (
                1,
                "PID 20 TypeError: request at /var/tmp/beta.py:20:4 port 5002 after 20ms",
            ),
        ]
    )
    assert (
        oracle.classify(
            1,
            "PID 30 TypeError: request at /tmp/gamma.py:30:8 port 5003 after 30ms",
        )
        == "preserved"
    )


@pytest.mark.parametrize(
    ("baseline", "candidate"),
    [
        ("HTTPError: status:404", "HTTPError: status:500"),
        (
            "AssertionError: expected:123 actual:456",
            "AssertionError: expected:999 actual:777",
        ),
        ("RuntimeError: shard:12", "RuntimeError: shard:99"),
    ],
)
def test_semantic_colon_numbers_are_not_source_locations(baseline: str, candidate: str) -> None:
    oracle = FailureOracle.from_baselines([(1, baseline), (1, baseline)])

    assert oracle.classify(1, candidate) == "rejected"


def test_recognized_source_line_numbers_remain_volatile() -> None:
    baseline = "TypeError: failed at src/main.rs:12"
    oracle = FailureOracle.from_baselines([(1, baseline), (1, baseline)])

    assert oracle.classify(1, "TypeError: failed at src/main.rs:99") == "preserved"


def test_combined_reserves_an_anchor_for_each_stream() -> None:
    stdout = "\n".join(
        [
            "FAILED tests/a.py::test_x",
            "error[E0425]: missing value",
            "ValueError: invoice failed",
            "expected 12 actual 13",
        ]
    )
    stderr = "fatal: disk exploded"
    oracle = FailureOracle.from_baselines(
        [(1, stdout, stderr), (1, stdout, stderr)], channel="combined"
    )

    channels = {anchor["channel"] for anchor in oracle.fingerprint["anchors"]}
    assert len(oracle.fingerprint["anchors"]) == 4
    assert {"stdout", "stderr"} <= channels
    assert oracle.classify(1, "fatal: totally unrelated", stdout=stdout) == "rejected"


def test_auto_reserves_each_error_bearing_stream_under_anchor_pressure() -> None:
    stdout = "\n".join(
        [
            "FAILED tests/a.py::test_x",
            "error[E0425]: missing value",
            "ValueError: invoice processing failed with detailed context",
            "expected twelve actual thirteen",
        ]
    )
    stderr = "fatal: disk exploded"
    oracle = FailureOracle.from_baselines(
        [(1, stdout, stderr), (1, stdout, stderr)], channel="auto"
    )

    channels = {anchor["channel"] for anchor in oracle.fingerprint["anchors"]}
    assert len(oracle.fingerprint["anchors"]) == 4
    assert {"stdout", "stderr"} <= channels
    assert oracle.classify(1, "fatal: totally unrelated", stdout=stdout) == "rejected"


@pytest.mark.parametrize(
    ("baseline", "candidate"),
    [
        ("HTTPError: GET /api/v1:404", "HTTPError: GET /api/v1:500"),
        ("HTTPError at /api/v1:404", "HTTPError at /api/v1:500"),
        (
            "HTTPError: https://example.com/v1:404",
            "HTTPError: https://example.com/v1:500",
        ),
    ],
)
def test_api_routes_and_urls_retain_semantic_status_values(baseline: str, candidate: str) -> None:
    oracle = FailureOracle.from_baselines([(1, baseline), (1, baseline)])

    assert oracle.classify(1, candidate) == "rejected"


@pytest.mark.parametrize(
    ("baseline", "candidate"),
    [
        ("RuntimeError: support 404", "RuntimeError: support 500"),
        ("RuntimeError: rapid 123", "RuntimeError: rapid 999"),
        ("RuntimeError: pipeline 123", "RuntimeError: pipeline 999"),
        ("RuntimeError: 12msisdn", "RuntimeError: 13msisdn"),
    ],
)
def test_volatile_labels_do_not_match_inside_semantic_words(baseline: str, candidate: str) -> None:
    oracle = FailureOracle.from_baselines([(1, baseline), (1, baseline)])

    assert oracle.classify(1, candidate) == "rejected"


@pytest.mark.parametrize(
    ("baseline", "candidate"),
    [
        ("RuntimeError: port 404", "RuntimeError: port 500"),
        ("RuntimeError: PID 123", "RuntimeError: PID 999"),
        ("RuntimeError: line 12", "RuntimeError: line 99"),
        (
            "RuntimeError: failed after 10 seconds",
            "RuntimeError: failed after 20 seconds",
        ),
    ],
)
def test_lexically_bounded_volatile_values_remain_normalized(baseline: str, candidate: str) -> None:
    oracle = FailureOracle.from_baselines([(1, baseline), (1, baseline)])

    assert oracle.classify(1, candidate) == "preserved"


@pytest.mark.parametrize(
    ("baseline", "candidate"),
    [
        (
            "LookupError: invoice 123e4567-e89b-12d3-a456-426614174000",
            "LookupError: invoice 123e4567-e89b-12d3-a456-426614174999",
        ),
        (
            "ValidationError: effective_at 2026-08-13T10:11:12Z",
            "ValidationError: effective_at 2026-08-14T10:11:12Z",
        ),
        ("TimeoutError: timeout 10ms", "TimeoutError: timeout 20ms"),
        ("HTTPError: error.json:404", "HTTPError: error.json:500"),
        ("HTTPError: /api/error.json:404", "HTTPError: /api/error.json:500"),
        (
            "HTTPError: https://example.test/error.json:404",
            "HTTPError: https://example.test/error.json:500",
        ),
    ],
)
def test_schema_5_preserves_semantic_values(baseline: str, candidate: str) -> None:
    oracle = FailureOracle.from_baselines([(1, baseline), (1, baseline)])

    assert oracle.classify(1, candidate) == "rejected"


@pytest.mark.parametrize(
    ("baselines", "candidate", "expected_anchor"),
    [
        (
            [
                (1, "ValueError: request_id=123e4567-e89b-12d3-a456-426614174000"),
                (1, "ValueError: request_id=123e4567-e89b-12d3-a456-426614174111"),
            ],
            "ValueError: request_id=123e4567-e89b-12d3-a456-426614174222",
            "ValueError: request_id=<uuid>",
        ),
        (
            [
                (1, "2026-08-13T10:11:12Z ERROR ValueError: import failed"),
                (1, "2026-08-13T10:11:13Z ERROR ValueError: import failed"),
            ],
            "2026-08-13T10:11:14Z ERROR ValueError: import failed",
            "<timestamp> ERROR ValueError: import failed",
        ),
        (
            [
                (1, "RuntimeError: import failed; elapsed 10ms"),
                (1, "RuntimeError: import failed; elapsed 20ms"),
            ],
            "RuntimeError: import failed; elapsed 30ms",
            "RuntimeError: import failed; elapsed <duration>",
        ),
    ],
)
def test_schema_5_normalizes_only_recognized_telemetry_context(
    baselines: list[tuple[int, str]], candidate: str, expected_anchor: str
) -> None:
    oracle = FailureOracle.from_baselines(baselines)

    assert oracle.fingerprint["anchors"] == [{"channel": "stderr", "text": expected_anchor}]
    assert oracle.classify(1, candidate) == "preserved"
    assert oracle.fingerprint["normalization_schema"] == 5


@pytest.mark.parametrize(
    ("baseline", "candidate"),
    [
        (
            "RuntimeError: failed at src/module:12",
            "RuntimeError: failed at src/module:99",
        ),
        (
            "RuntimeError: failed at Makefile:12",
            "RuntimeError: failed at Makefile:99",
        ),
    ],
)
def test_explicit_extensionless_source_locations_remain_volatile(
    baseline: str, candidate: str
) -> None:
    oracle = FailureOracle.from_baselines([(1, baseline), (1, baseline)])

    assert oracle.classify(1, candidate) == "preserved"


def test_equal_cross_stream_anchors_use_explicit_channel_order() -> None:
    diagnostic = "ValueError: shared failure"
    oracle = FailureOracle.from_baselines(
        [(1, diagnostic, diagnostic), (1, diagnostic, diagnostic)], channel="auto"
    )

    assert [anchor["channel"] for anchor in oracle.fingerprint["anchors"]] == [
        "stdout",
        "stderr",
    ]


def test_auto_requires_stable_stdout_and_stderr_when_both_exist() -> None:
    oracle = FailureOracle.from_baselines(
        [
            (1, "FAILED tests/a.py::test_total", "TypeError: currency"),
            (1, "FAILED tests/a.py::test_total", "TypeError: currency"),
        ],
        channel="auto",
    )
    assert (
        oracle.classify(1, "TypeError: currency", stdout="FAILED tests/a.py::test_total")
        == "preserved"
    )
    assert (
        oracle.classify(1, "TypeError: currency", stdout="FAILED tests/b.py::test_total")
        == "rejected"
    )
    assert oracle.fingerprint["anchors"] == [
        {"channel": "stdout", "text": "FAILED tests/a.py::test_total"},
        {"channel": "stderr", "text": "TypeError: currency"},
    ]


def test_explicit_stderr_ignores_unstable_stdout_baselines() -> None:
    oracle = FailureOracle.from_baselines(
        [
            (1, "progress one", "TypeError: currency"),
            (1, "progress two", "TypeError: currency"),
        ],
        channel="stderr",
    )
    assert oracle.classify(1, "TypeError: currency", stdout="anything") == "preserved"


def test_python_policy_validates_the_same_supermajority_contract() -> None:
    policy = EvaluationPolicy.flaky(11, 9)
    assert policy.mode == "flaky"
    assert policy.runs == 11
    assert policy.required == 9
    with pytest.raises(ValueError, match="supermajority"):
        EvaluationPolicy.flaky(11, 6)


def test_strict_python_policy_is_immutable() -> None:
    policy = EvaluationPolicy.strict()
    assert (policy.mode, policy.runs, policy.required) == ("strict", 3, 3)
    with pytest.raises(AttributeError):
        policy.runs = 9  # type: ignore[misc]
