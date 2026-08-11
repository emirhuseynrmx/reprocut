from collections.abc import Sequence
from typing import Literal

Verdict = Literal["preserved", "rejected", "inconclusive"]

class FailureOracle:
    @classmethod
    def from_baselines(
        cls, baselines: Sequence[tuple[int, str]]
    ) -> FailureOracle: ...

    def classify(
        self,
        exit_code: int,
        diagnostic: str,
        *,
        timed_out: bool = False,
        truncated: bool = False,
    ) -> Verdict: ...

    @property
    def fingerprint(self) -> dict[str, int | str | None]: ...
