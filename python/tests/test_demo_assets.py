from __future__ import annotations

import hashlib
import json
import os
import subprocess
import sys
import xml.etree.ElementTree as ET
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parents[2]


def tree_digest(root: Path) -> str:
    digest = hashlib.sha256()
    for path in sorted(candidate for candidate in root.rglob("*") if candidate.is_file()):
        if "__pycache__" in path.parts or path.suffix in {".pyc", ".pyo"}:
            continue
        digest.update(path.relative_to(root).as_posix().encode("utf-8"))
        digest.update(b"\0")
        digest.update(path.read_bytes())
        digest.update(b"\0")
    return digest.hexdigest()


def execute_demo(root: Path) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [sys.executable, "bug.py"],
        cwd=root,
        check=False,
        capture_output=True,
        text=True,
        encoding="utf-8",
        env={**os.environ, "PYTHONDONTWRITEBYTECODE": "1"},
    )


def test_checked_in_demo_is_measured_and_reproducible() -> None:
    result = ROOT / "demo" / "result"
    metadata = json.loads((result / "reduction.json").read_text(encoding="utf-8"))

    assert metadata["schema_version"] == 4
    assert metadata["failure"]["normalization_schema"] == 5
    assert metadata["failure"]["oracle_mode"] == "automatic"
    assert len(metadata["source_snapshot_sha256"]) == 64
    assert len(metadata["failure"]["oracle_spec_sha256"]) == 64
    assert len(metadata["preparation"]["contract_sha256"]) == 64
    assert metadata["measurements"]["original"]["files"] == 18
    assert metadata["measurements"]["retained"]["files"] == 3
    assert metadata["search"]["final_verifications"] == 3
    assert metadata["search"]["inconclusive_attempts"] == 0
    kept_files = [entry["path"] for entry in metadata["kept_files"]]
    assert kept_files == ["bug.py", "checkout.py", "fixtures/order.json"]
    assert metadata["failure"]["same_failure"] is True
    assert len(metadata["failure"]["fingerprint_sha256"]) == 64
    assert (
        sorted(
            path.relative_to(result / "project").as_posix()
            for path in (result / "project").rglob("*")
            if path.is_file()
        )
        == kept_files
    )

    attempts = [
        json.loads(line)
        for line in (result / "attempts.jsonl").read_text(encoding="utf-8").splitlines()
    ]
    assert attempts
    assert len(attempts) == len(metadata["attempts"])
    assert [attempt["event_id"] for attempt in attempts] == sorted(
        attempt["event_id"] for attempt in attempts
    )
    issue = (result / "issue.md").read_text(encoding="utf-8")
    assert metadata["failure"]["fingerprint_sha256"] in issue
    assert "attempts.jsonl" in issue
    assert "{{" not in (result / "report.html").read_text(encoding="utf-8")


def test_demo_gif_contract() -> None:
    Image = pytest.importorskip("PIL.Image")
    gif_path = ROOT / "assets" / "reprocut-demo.gif"
    evidence = json.loads((ROOT / "demo" / "result" / "reduction.json").read_text())
    assert 0 < gif_path.stat().st_size < 8 * 1024 * 1024

    with Image.open(gif_path) as animation:
        assert animation.format == "GIF"
        assert animation.size == (1200, 675)
        assert animation.n_frames == 24
        assert animation.info.get("loop") == 0
        assert evidence["failure"]["fingerprint_sha256"].encode() in animation.info["comment"]


def test_banner_is_static_accessible_and_evidence_bound() -> None:
    banner = ROOT / "assets" / "reprocut-banner.svg"
    evidence = json.loads((ROOT / "demo" / "result" / "reduction.json").read_text(encoding="utf-8"))
    source = banner.read_text(encoding="utf-8")
    root = ET.fromstring(source)
    namespace = {"svg": "http://www.w3.org/2000/svg"}
    text = " ".join("".join(root.itertext()).split())
    states = [
        node.attrib["data-file-state"] for node in root.iter() if "data-file-state" in node.attrib
    ]

    assert root.attrib["viewBox"] == "0 0 1600 600"
    assert root.find("svg:title", namespace) is not None
    assert root.find("svg:desc", namespace) is not None
    assert len(states) == evidence["measurements"]["original"]["files"] == 18
    assert states.count("retained") == evidence["measurements"]["retained"]["files"] == 3
    assert states.count("rejected") == 15
    assert len([node for node in root.iter() if node.attrib.get("data-role") == "cut-trace"]) == 1
    assert str(evidence["search"]["attempts"]) in text
    assert "STRICT 3 / 3" in text
    assert evidence["failure"]["fingerprint_sha256"][:16] in text
    assert all(path in text for path in ("bug.py", "checkout.py", "fixtures/order.json"))
    assert "<script" not in source.lower()
    assert "<filter" not in source.lower()
    assert "<animate" not in source.lower()
    assert "http://" not in source.replace("http://www.w3.org/2000/svg", "")
    assert "https://" not in source
    assert "href=" not in source.lower()


def test_reduced_demo_preserves_the_stabilized_source_failure() -> None:
    from reprocut import FailureOracle

    source = ROOT / "demo" / "source"
    reduced = ROOT / "demo" / "result" / "project"
    before = tree_digest(source)
    source_runs = [execute_demo(source) for _ in range(3)]
    oracle = FailureOracle.from_baselines([(run.returncode, run.stderr) for run in source_runs])
    reduced_runs = [execute_demo(reduced) for _ in range(3)]

    assert all(run.returncode != 0 for run in source_runs)
    assert all(oracle.classify(run.returncode, run.stderr) == "preserved" for run in reduced_runs)
    assert tree_digest(source) == before
