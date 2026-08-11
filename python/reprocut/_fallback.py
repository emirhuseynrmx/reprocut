"""Readable source-checkout fallback; release wheels use the Rust extension."""

from __future__ import annotations

import re
from collections.abc import Sequence
from types import MappingProxyType
from typing import Literal


Verdict = Literal["preserved", "rejected", "inconclusive"]
Baseline = tuple[int, str]

_WINDOWS_PATH = re.compile(r"[A-Za-z]:\\(?:[^\\ \t\r\n:]+\\)*[^\\ \t\r\n:]+")
_UNIX_PATH = re.compile(r"/(?:[^/ \t\r\n:]+/)*[^/ \t\r\n:]+")
_HEX_ADDRESS = re.compile(r"0[xX][0-9a-fA-F]+")
_DECIMAL_ID = re.compile(r"[0-9]+")
_HORIZONTAL_SPACE = re.compile(r"[\t ]+")


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

    __slots__ = ("_exit_code", "_anchor")

    def __init__(self, exit_code: int, anchor: str, *, _factory: bool = False) -> None:
        if not _factory:
            raise TypeError("use FailureOracle.from_baselines()")
        object.__setattr__(self, "_exit_code", exit_code)
        object.__setattr__(self, "_anchor", anchor)

    def __setattr__(self, name: str, value: object) -> None:
        del name, value
        raise AttributeError("FailureOracle is immutable")

    @classmethod
    def from_baselines(cls, baselines: Sequence[Baseline]) -> FailureOracle:
        if len(baselines) < 2:
            raise ValueError("at least two baseline observations are required")
        first_exit, first_diagnostic = baselines[0]
        if any(exit_code != first_exit for exit_code, _ in baselines[1:]):
            raise ValueError("baseline exit states are unstable")
        normalized = _normalize(first_diagnostic)
        if any(_normalize(diagnostic) != normalized for _, diagnostic in baselines[1:]):
            raise ValueError("baseline diagnostics are unstable")

        anchor = ""
        for line in normalized.splitlines():
            if line and len(line) >= len(anchor):
                anchor = line
        if not anchor:
            raise ValueError("baseline diagnostic has no stable non-empty anchor")
        return cls(first_exit, anchor, _factory=True)

    def classify(
        self,
        exit_code: int,
        diagnostic: str,
        *,
        timed_out: bool = False,
        truncated: bool = False,
    ) -> Verdict:
        if timed_out or truncated:
            return "inconclusive"
        if exit_code != self._exit_code:
            return "rejected"
        return "preserved" if self._anchor in _normalize(diagnostic).splitlines() else "rejected"

    @property
    def fingerprint(self) -> dict[str, int | str | None]:
        # A new value prevents callers from mutating oracle state.
        return dict(MappingProxyType({
            "exit_code": self._exit_code,
            "signal": None,
            "anchor": self._anchor,
        }))
