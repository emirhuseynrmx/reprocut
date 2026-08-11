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
        [(1, "stable stdout", "stable stderr"), (1, "stable stdout", "stable stderr")]
    )
    assert oracle.classify(1, "stable stderr", stdout="stable stdout") == "preserved"
