"""Python surface for ReproCut's failure oracle."""

try:
    from ._native import FailureOracle
except ModuleNotFoundError as error:
    if error.name != "reprocut._native":
        raise
    from ._fallback import FailureOracle

    BACKEND = "reference"
else:
    BACKEND = "native"

__all__ = ["BACKEND", "FailureOracle"]
