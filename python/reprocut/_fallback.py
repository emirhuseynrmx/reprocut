"""Readable source-checkout fallback; release wheels use the Rust extension."""

from __future__ import annotations

import re
from collections.abc import Sequence
from typing import Literal, Union

Verdict = Literal["preserved", "rejected", "inconclusive"]
Channel = Literal["auto", "stdout", "stderr", "combined"]
LegacyBaseline = tuple[int, str]
StreamBaseline = tuple[int, str, str]
Baseline = Union[LegacyBaseline, StreamBaseline]

_WINDOWS_PATH = re.compile(r"[A-Za-z]:\\(?:[^\\ \t\r\n:]+\\)*[^\\ \t\r\n:]+")
_UNIX_PATH = re.compile(r"/(?:[^/ \t\r\n:]+/)*[^/ \t\r\n:]+")
_HEX_ADDRESS = re.compile(r"0[xX][0-9a-fA-F]+")
_DECIMAL_ID = re.compile(r"[0-9]+")
_HORIZONTAL_SPACE = re.compile(r"[\t ]+")


class EvaluationPolicy:
    """Immutable strict/flaky execution policy matching the Rust validator."""

    __slots__ = ("_mode", "_required", "_runs")

    def __init__(
        self, mode: str, runs: int, required: int, *, _factory: bool = False
    ) -> None:
        if not _factory:
            raise TypeError("use EvaluationPolicy.strict() or EvaluationPolicy.flaky()")
        object.__setattr__(self, "_mode", mode)
        object.__setattr__(self, "_runs", runs)
        object.__setattr__(self, "_required", required)

    def __setattr__(self, name: str, value: object) -> None:
        del name, value
        raise AttributeError("EvaluationPolicy is immutable")

    @classmethod
    def strict(cls) -> EvaluationPolicy:
        return cls("strict", 3, 3, _factory=True)

    @classmethod
    def flaky(cls, runs: int = 11, required: int = 9) -> EvaluationPolicy:
        if not 5 <= runs <= 101:
            raise ValueError("flaky runs must be between 5 and 101")
        if runs % 2 == 0:
            raise ValueError("flaky runs must be odd")
        if not 1 <= required <= runs:
            raise ValueError("flaky required must be between 1 and runs")
        if required * 3 < runs * 2:
            raise ValueError("flaky required must be at least a two-thirds supermajority")
        return cls("flaky", runs, required, _factory=True)

    @property
    def mode(self) -> str:
        return self._mode

    @property
    def runs(self) -> int:
        return self._runs

    @property
    def required(self) -> int:
        return self._required


def _normalize(diagnostic: str) -> str:
    value = diagnostic.replace("\r\n", "\n").replace("\r", "\n")
    value = _WINDOWS_PATH.sub("<path>", value)
    value = _UNIX_PATH.sub("<path>", value)
    value = _HEX_ADDRESS.sub("<hex>", value)
    value = _DECIMAL_ID.sub("<n>", value)
    lines = (_HORIZONTAL_SPACE.sub(" ", line.strip()) for line in value.splitlines())
    return "\n".join(line for line in lines if line)


class FailureOracle:
    """Reference implementation of the immutable native oracle contract."""

    __slots__ = ("_anchors", "_exit_code")

    def __init__(
        self,
        exit_code: int,
        anchors: tuple[tuple[str, str], ...],
        *,
        _factory: bool = False,
    ) -> None:
        if not _factory:
            raise TypeError("use FailureOracle.from_baselines()")
        object.__setattr__(self, "_exit_code", exit_code)
        object.__setattr__(self, "_anchors", anchors)

    def __setattr__(self, name: str, value: object) -> None:
        del name, value
        raise AttributeError("FailureOracle is immutable")

    @classmethod
    def from_baselines(
        cls, baselines: Sequence[Baseline], *, channel: Channel = "auto"
    ) -> FailureOracle:
        if len(baselines) < 2:
            raise ValueError("at least two baseline observations are required")
        if channel not in {"auto", "stdout", "stderr", "combined"}:
            raise ValueError(f"unsupported diagnostic channel: {channel}")

        observations = tuple(_split_baseline(baseline) for baseline in baselines)
        first_exit = observations[0][0]
        if any(exit_code != first_exit for exit_code, _, _ in observations[1:]):
            raise ValueError("baseline exit states are unstable")

        stdout = _stable_stream(tuple(item[1] for item in observations))
        stderr = _stable_stream(tuple(item[2] for item in observations))
        anchors: list[tuple[str, str]] = []
        if channel == "auto":
            for name, stream in (("stdout", stdout), ("stderr", stderr)):
                if stream[0] == "stable":
                    anchors.append((name, _longest_line(stream[1])))
            if not anchors:
                if stdout[0] == "unstable" or stderr[0] == "unstable":
                    raise ValueError("baseline diagnostics are unstable")
                raise ValueError("baseline diagnostic has no stable non-empty anchor")
        else:
            selected = (
                (("stdout", stdout), ("stderr", stderr))
                if channel == "combined"
                else ((channel, stdout if channel == "stdout" else stderr),)
            )
            for name, stream in selected:
                if stream[0] == "unstable":
                    raise ValueError("baseline diagnostics are unstable")
                if stream[0] == "empty":
                    raise ValueError("baseline diagnostic has no stable non-empty anchor")
                anchors.append((name, _longest_line(stream[1])))
        return cls(first_exit, tuple(anchors), _factory=True)

    def classify(
        self,
        exit_code: int,
        diagnostic: str,
        *,
        stdout: str = "",
        timed_out: bool = False,
        truncated: bool = False,
    ) -> Verdict:
        if timed_out or truncated:
            return "inconclusive"
        if exit_code != self._exit_code:
            return "rejected"
        streams = {"stdout": _normalize(stdout), "stderr": _normalize(diagnostic)}
        matches = all(text in streams[channel].splitlines() for channel, text in self._anchors)
        return "preserved" if matches else "rejected"

    @property
    def fingerprint(self) -> dict[str, object]:
        # Detached nested values prevent callers from mutating oracle state.
        first_anchor = self._anchors[0][1]
        return {
            "exit_code": self._exit_code,
            "signal": None,
            "termination": {"kind": "exit_code", "value": self._exit_code},
            "anchor": first_anchor,
            "anchors": [
                {"channel": channel, "text": text} for channel, text in self._anchors
            ],
            "normalization_schema": 1,
        }


def _split_baseline(baseline: Baseline) -> tuple[int, str, str]:
    if len(baseline) == 2:
        exit_code, diagnostic = baseline
        return exit_code, "", diagnostic
    exit_code, stdout, stderr = baseline
    return exit_code, stdout, stderr


def _stable_stream(values: tuple[str, ...]) -> tuple[str, str]:
    normalized = tuple(_normalize(value) for value in values)
    if any(value != normalized[0] for value in normalized[1:]):
        return "unstable", ""
    if not normalized[0]:
        return "empty", ""
    return "stable", normalized[0]


def _longest_line(diagnostic: str) -> str:
    return max((line for line in diagnostic.splitlines() if line), key=len)
