from __future__ import annotations

import os

import pytest
import reprocut


@pytest.mark.skipif(
    os.environ.get("REPROCUT_REQUIRE_NATIVE") != "1",
    reason="native wheel smoke test is enabled in its dedicated CI job",
)
def test_wheel_uses_the_native_backend() -> None:
    assert reprocut.BACKEND == "native"
    assert reprocut.FailureOracle.__module__ == "reprocut._native"
    assert reprocut.EvaluationPolicy.flaky(11, 9).required == 9
    oracle = reprocut.FailureOracle.from_baselines(
        [
            (1, "currency=EUR", "TypeError: invoice 41"),
            (1, "currency=EUR", "TypeError: invoice 42"),
        ],
        mode="regex",
        channel="combined",
        failure_patterns=[r"TypeError: invoice [0-9]+", "currency"],
        reject_patterns=["PermissionError"],
    )
    assert oracle.classify(1, "TypeError: invoice 99", stdout="currency=EUR") == "preserved"
    stderr_oracle = reprocut.FailureOracle.from_baselines(
        [(1, "ValueError: default stdout"), (1, "ValueError: default stdout")]
    )
    assert stderr_oracle.classify(1, "ValueError: default stdout") == "preserved"
    assert oracle.fingerprint["fingerprint_sha256"] == (
        "023335eba27dab590b959df01b03863c4452ab17c8459b4978408ad238c12cc6"
    )
