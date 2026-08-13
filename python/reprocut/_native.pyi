from collections.abc import Sequence
from typing import Literal

Verdict = Literal["preserved", "rejected", "inconclusive"]
Channel = Literal["auto", "stderr", "stdout", "combined"]
Mode = Literal["automatic", "regex", "exit_zero"]

class EvaluationPolicy:
    @classmethod
    def strict(cls) -> EvaluationPolicy: ...
    @classmethod
    def flaky(cls, runs: int = 11, required: int = 9) -> EvaluationPolicy: ...
    @property
    def mode(self) -> Literal["strict", "flaky"]: ...
    @property
    def runs(self) -> int: ...
    @property
    def required(self) -> int: ...

class FailureOracle:
    @classmethod
    def from_baselines(
        cls,
        baselines: Sequence[tuple[int, str] | tuple[int, str, str]],
        *,
        mode: Mode = "automatic",
        channel: Channel = "auto",
        failure_patterns: Sequence[str] | None = None,
        reject_patterns: Sequence[str] | None = None,
    ) -> FailureOracle: ...
    def classify(
        self,
        exit_code: int,
        diagnostic: str,
        *,
        stdout: str = "",
        timed_out: bool = False,
        truncated: bool = False,
    ) -> Verdict: ...
    @property
    def fingerprint(self) -> dict[str, object]: ...
