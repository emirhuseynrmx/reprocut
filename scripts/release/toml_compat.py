"""Load a TOML parser across every Python version supported by ReproCut."""

from __future__ import annotations

from importlib import import_module as default_import_module
from typing import Callable


def load_toml_module(
    import_module: Callable[[str], object] = default_import_module,
) -> object | None:
    """Prefer stdlib tomllib and fall back to tomli on Python 3.9/3.10."""

    try:
        return import_module("tomllib")
    except ModuleNotFoundError as error:
        if error.name != "tomllib":
            raise

    try:
        return import_module("tomli")
    except ModuleNotFoundError as error:
        if error.name != "tomli":
            raise
        return None
