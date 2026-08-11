"""Typed Python surface for ReproCut's oracle and shared Rust reducer."""

from .client import (
    BaselineStableEvent,
    CompletedEvent,
    FailedEvent,
    ProgressEvent,
    ReductionRequest,
    ReductionResult,
    ReproCutError,
    StartedEvent,
    reduce,
)

try:
    from ._native import EvaluationPolicy, FailureOracle
except ModuleNotFoundError as error:
    if error.name != "reprocut._native":
        raise
    from ._fallback import EvaluationPolicy, FailureOracle

    BACKEND = "reference"
else:
    BACKEND = "native"

__all__ = [
    "BACKEND",
    "BaselineStableEvent",
    "CompletedEvent",
    "EvaluationPolicy",
    "FailedEvent",
    "FailureOracle",
    "ProgressEvent",
    "ReductionRequest",
    "ReductionResult",
    "ReproCutError",
    "StartedEvent",
    "reduce",
]
