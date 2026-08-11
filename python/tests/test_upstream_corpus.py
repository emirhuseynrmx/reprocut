from __future__ import annotations

import importlib.util
import json
import sys
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parents[2]
SCRIPT = ROOT / "scripts" / "fetch_upstream_corpus.py"
SPEC = importlib.util.spec_from_file_location("fetch_upstream_corpus", SCRIPT)
assert SPEC is not None and SPEC.loader is not None
CORPUS = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CORPUS)


def test_manifest_pins_24_unique_issue_linked_download_only_cases() -> None:
    manifest = CORPUS.load_manifest(ROOT / "benchmarks" / "upstream-corpus.json")

    assert manifest["case_count"] == 24
    assert manifest["redistribution"] == "download-only"
    assert manifest["upstream_license"] == "GPL-3.0-only"
    assert len(manifest["commit"]) == 40
    assert all(case["issue_url"].startswith("https://") for case in manifest["cases"])
    assert len(CORPUS.selected_members(manifest)) == 95


def test_license_acknowledgement_flag_has_a_stable_python_attribute(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    monkeypatch.setattr(
        sys,
        "argv",
        ["fetch_upstream_corpus.py", "--destination", "corpus", "--accept-gpl-3.0"],
    )

    args = CORPUS.parse_args()

    assert args.accept_gpl_3_0 is True


def test_fetch_copies_only_allowlisted_pinned_files(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    manifest = {
        "commit": "1" * 40,
        "raw_url_template": (
            "https://raw.githubusercontent.com/uw-pluverse/perses/{commit}/{path}"
        ),
        "layouts": {"tiny": ["source.c", "r.sh"]},
        "cases": [
            {
                "id": "case-1",
                "upstream_path": "benchmark/case-1",
                "layout": "tiny",
            }
        ],
    }
    requested: list[str] = []

    def fake_download(url: str, destination: Path) -> int:
        requested.append(url)
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_bytes(destination.name.encode())
        return destination.stat().st_size

    monkeypatch.setattr(CORPUS, "download_file", fake_download)

    staging = tmp_path / "staging"
    staging.mkdir()
    total = CORPUS.fetch_selected_files(manifest, staging)

    assert sorted(path.name for path in (staging / "case-1").iterdir()) == ["r.sh", "source.c"]
    assert total == len(b"r.sh") + len(b"source.c")
    prefix = "https://raw.githubusercontent.com/uw-pluverse/perses/" + "1" * 40
    assert requested == [
        f"{prefix}/benchmark/case-1/r.sh",
        f"{prefix}/benchmark/case-1/source.c",
    ]


def test_download_refuses_an_oversized_content_length(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    class FakeResponse:
        headers = {"Content-Length": str(CORPUS.MAX_MEMBER_BYTES + 1)}

        def __enter__(self) -> FakeResponse:
            return self

        def __exit__(self, *_args: object) -> None:
            return None

    monkeypatch.setattr(CORPUS.urllib.request, "urlopen", lambda *_args, **_kwargs: FakeResponse())

    with pytest.raises(CORPUS.CorpusError, match="8 MiB"):
        CORPUS.download_file("https://example.invalid/source.c", tmp_path / "source.c")
    assert not (tmp_path / "source.c").exists()


def test_manifest_rejects_duplicate_identifiers(tmp_path: Path) -> None:
    manifest = json.loads((ROOT / "benchmarks" / "upstream-corpus.json").read_text())
    manifest["cases"][1]["id"] = manifest["cases"][0]["id"]
    target = tmp_path / "manifest.json"
    target.write_text(json.dumps(manifest), encoding="utf-8")

    with pytest.raises(CORPUS.CorpusError, match="unique"):
        CORPUS.load_manifest(target)
