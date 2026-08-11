"""Python surface for ReproCut's failure oracle."""

try:
    from ._native import EvaluationPolicy, FailureOracle
except ModuleNotFoundError as error:
    if error.name != "reprocut._native":
        raise
    from ._fallback import EvaluationPolicy, FailureOracle

    BACKEND = "reference"
else:
    BACKEND = "native"

__all__ = ["BACKEND", "EvaluationPolicy", "FailureOracle"]
