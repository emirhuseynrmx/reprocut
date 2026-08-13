from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "scripts"))

from build_demo import artifact_manifest, format_summary


def test_terminal_summary_is_portable_to_the_windows_code_page() -> None:
    summary = format_summary(output="demo/result", original_files=18, retained_files=3, attempts=19)

    assert "18 -> 3 files" in summary
    summary.encode("cp1254", errors="strict")


def test_artifact_manifest_binds_member_bytes_and_excludes_its_envelope(tmp_path: Path) -> None:
    (tmp_path / "project").mkdir()
    member = tmp_path / "project" / "bug.py"
    member.write_bytes(b"raise ValueError()\n")

    first = artifact_manifest(tmp_path)
    (tmp_path / "artifact-manifest.json").write_text("ignored envelope", encoding="utf-8")
    assert artifact_manifest(tmp_path) == first

    member.write_bytes(b"raise TypeError()\n")
    second = artifact_manifest(tmp_path)
    assert first["artifact_id"] != second["artifact_id"]
