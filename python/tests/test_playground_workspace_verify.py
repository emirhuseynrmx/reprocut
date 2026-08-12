from __future__ import annotations

import importlib.util
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
WORKSPACE_SCRIPT = ROOT / "scripts" / "playground_workspace_verify.py"
SINGLE_FILE_SCRIPT = ROOT / "scripts" / "playground_verify.py"


def load_script(name: str, path: Path):
    spec = importlib.util.spec_from_file_location(name, path)
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def test_read_inlines_multiline_include_str() -> None:
    verifier = load_script("playground_workspace_verify", WORKSPACE_SCRIPT)

    source = verifier.read("scripts/verification/session_integrity_contract.rs")

    assert "include_str!" not in source
    assert "CREATE TABLE sessions" in source


def test_single_file_verifier_inlines_multiline_include_str() -> None:
    verifier = load_script("playground_verify", SINGLE_FILE_SCRIPT)
    contract = ROOT / "scripts" / "verification" / "session_integrity_contract.rs"

    source = verifier.inline_includes(contract, contract.read_text(encoding="utf-8"))

    assert "include_str!" not in source
    assert "CREATE TABLE sessions" in source
