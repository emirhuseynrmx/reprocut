from __future__ import annotations

import json
import sys
from pathlib import Path
from types import SimpleNamespace

import pytest
import reprocut.cli as python_cli
from reprocut.client import (
    BaselineStableEvent,
    CompletedEvent,
    ReductionRequest,
    ReproCutError,
    StartedEvent,
    reduce,
)


def test_typed_client_consumes_the_versioned_engine_protocol(tmp_path: Path) -> None:
    fake = _write_fake_engine(tmp_path, mode="success")
    source = tmp_path / "source"
    source.mkdir()
    events = []
    request = ReductionRequest(
        root=source,
        output=tmp_path / "minimal",
        command=(sys.executable, "bug.py"),
        ecosystem="python",
        state=tmp_path / "state.sqlite3",
    )

    result = reduce(request, executable=[sys.executable, fake], progress=events.append)

    assert [type(event) for event in events] == [
        StartedEvent,
        BaselineStableEvent,
        CompletedEvent,
    ]
    assert result.fingerprint_sha256 == "a" * 64
    assert result.output == request.output
    assert result.evidence["schema_version"] == 4
    assert result.evidence["failure"]["same_failure"] is True
    with pytest.raises(TypeError):
        result.evidence["schema_version"] = 9


@pytest.mark.parametrize("mode", ["missing_manifest", "missing_final_observation"])
def test_typed_client_rejects_incomplete_schema_4_evidence(
    tmp_path: Path, mode: str
) -> None:
    fake = _write_fake_engine(tmp_path, mode=mode)
    request = ReductionRequest(root=tmp_path, output=tmp_path / "minimal")

    with pytest.raises(ReproCutError, match="retained manifest|final observations"):
        reduce(request, executable=[sys.executable, fake])


def test_failed_event_preserves_bounded_stderr_and_event_history(tmp_path: Path) -> None:
    fake = _write_fake_engine(tmp_path, mode="failure")
    request = ReductionRequest(root=tmp_path, output=tmp_path / "minimal")

    with pytest.raises(ReproCutError, match="baseline was unstable") as raised:
        reduce(request, executable=[sys.executable, fake])

    assert "diagnostic context" in raised.value.stderr
    assert raised.value.events[-1].message == "baseline was unstable"


def test_unknown_protocol_version_fails_closed(tmp_path: Path) -> None:
    fake = _write_fake_engine(tmp_path, mode="wrong_version")
    request = ReductionRequest(root=tmp_path, output=tmp_path / "minimal")

    with pytest.raises(ReproCutError, match="unsupported protocol event version"):
        reduce(request, executable=[sys.executable, fake])


def test_completed_event_cannot_redirect_the_client_outside_requested_output(
    tmp_path: Path,
) -> None:
    fake = _write_fake_engine(tmp_path, mode="wrong_output")
    request = ReductionRequest(root=tmp_path, output=tmp_path / "minimal")

    with pytest.raises(ReproCutError, match="does not match"):
        reduce(request, executable=[sys.executable, fake])


def test_request_rejects_resume_restart_and_invalid_flaky_bounds(tmp_path: Path) -> None:
    with pytest.raises(ValueError, match="cannot restart"):
        ReductionRequest(
            root=tmp_path,
            output=tmp_path / "minimal",
            action="resume",
            restart=True,
        )


def test_integrity_contract_is_canonical_and_mode_validated(tmp_path: Path) -> None:
    request = ReductionRequest(
        root=tmp_path,
        output=tmp_path / "minimal",
        oracle_mode="regex",
        failure_patterns=(r"TypeError", r"currency", r"TypeError"),
        reject_patterns=(r"secondary",),
        preparation="isolated_python",
        python_executable=Path(sys.executable),
        python_wheelhouse=tmp_path / "wheels",
        python_extras=("Fast_JSON.parser", "fast-json-parser"),
        prepare_spec=tmp_path / "prepare.json",
    )

    assert request.failure_patterns == (r"TypeError", r"currency")
    assert request.python_extras == ("fast-json-parser",)
    assert request.to_protocol()["oracle_mode"] == "regex"
    assert request.to_protocol()["python_extras"] == ["fast-json-parser"]

    with pytest.raises(ValueError, match="requires at least one"):
        ReductionRequest(
            root=tmp_path,
            output=tmp_path / "minimal",
            oracle_mode="regex",
        )
    with pytest.raises(ValueError, match="does not accept"):
        ReductionRequest(
            root=tmp_path,
            output=tmp_path / "minimal",
            oracle_mode="exit_zero",
            reject_patterns=("wrong",),
        )
    with pytest.raises(ValueError, match="requires python_executable"):
        ReductionRequest(
            root=tmp_path,
            output=tmp_path / "minimal",
            preparation="isolated_python",
        )


def test_python_console_wrapper_uses_an_explicit_rust_binary(
    tmp_path: Path,
    capsys: pytest.CaptureFixture[str],
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    source = tmp_path / "source"
    source.mkdir()
    observed = {}

    def fake_reduce(request: ReductionRequest, **options: object) -> SimpleNamespace:
        observed["request"] = request
        observed["options"] = options
        return SimpleNamespace(
            output=request.output,
            evidence_path=request.output / "reduction.json",
            report_path=request.output / "report.html",
            issue_path=request.output / "issue.md",
            fingerprint_sha256="a" * 64,
        )

    monkeypatch.setattr(python_cli, "reduce", fake_reduce)

    exit_code = python_cli.main(
        [
            "minimize",
            "--root",
            str(source),
            "--output",
            str(tmp_path / "minimal"),
            "--binary",
            "rust-reprocut",
            "--json",
            "--",
            sys.executable,
            "bug.py",
        ]
    )

    captured = capsys.readouterr()
    assert exit_code == 0
    assert json.loads(captured.out)["fingerprint_sha256"] == "a" * 64
    assert observed["request"].command == (sys.executable, "bug.py")
    assert observed["options"]["executable"] == "rust-reprocut"
    with pytest.raises(ValueError, match="odd"):
        ReductionRequest(
            root=tmp_path,
            output=tmp_path / "minimal",
            flaky_runs=10,
        )


def _write_fake_engine(root: Path, *, mode: str) -> Path:
    script = root / f"fake_engine_{mode}.py"
    script.write_text(
        f"""import json
import pathlib
import sys

mode = {mode!r}
request = json.loads(pathlib.Path(sys.argv[-1]).read_text(encoding="utf-8"))
version = 7 if mode == "wrong_version" else 1
started = {{
    "type": "started",
    "protocol_version": version,
    "action": request["action"],
    "root": request["root"],
}}
print(json.dumps(started), flush=True)
if mode == "wrong_version":
    raise SystemExit(0)
if mode == "failure":
    print("diagnostic context", file=sys.stderr, flush=True)
    failed = {{
        "type": "failed",
        "protocol_version": 1,
        "message": "baseline was unstable",
    }}
    print(json.dumps(failed), flush=True)
    raise SystemExit(1)
output = pathlib.Path(request["output"])
output.mkdir()
evidence = {{
    "schema_version": 4,
    "source_snapshot_sha256": "b" * 64,
    "preparation": {{
        "mode": "offline",
        "contract_sha256": "c" * 64,
        "limitations": [],
    }},
    "failure": {{
        "same_failure": True,
        "fingerprint_sha256": "a" * 64,
        "oracle_spec_sha256": "d" * 64,
    }},
    "measurements": {{
        "retained": {{"files": 1, "bytes": 1}},
    }},
    "search": {{"final_verifications": 1}},
    "kept_files": [{{"path": "bug.py"}}],
    "retained_manifest": {{
        "schema_version": 1,
        "entries": [{{
            "path": "bug.py",
            "kind": "regular_file",
            "sha256": "e" * 64,
            "size_bytes": 1,
            "executable_mask": 0,
            "symlink_target": None,
        }}],
        "total_bytes": 1,
        "manifest_sha256": "f" * 64,
    }},
    "final_observations": [{{
        "ordinal": 1,
        "verdict": "preserved",
        "termination": "exit 1",
        "exit_code": 1,
        "signal": None,
        "timed_out": False,
        "streams_truncated": False,
        "containment": "direct_child",
        "stdout_sha256": "1" * 64,
        "stdout_bytes": 0,
        "stderr_sha256": "2" * 64,
        "stderr_bytes": 10,
    }}],
}}
if mode == "missing_manifest":
    evidence.pop("retained_manifest")
if mode == "missing_final_observation":
    evidence["final_observations"] = []
(output / "reduction.json").write_text(json.dumps(evidence), encoding="utf-8")
(output / "report.html").write_text("report", encoding="utf-8")
(output / "issue.md").write_text("issue", encoding="utf-8")
baseline = {{
    "type": "baseline_stable",
    "protocol_version": 1,
    "fingerprint_sha256": "a" * 64,
}}
print(json.dumps(baseline), flush=True)
event_output = output.parent / "elsewhere" if mode == "wrong_output" else output
completed = {{
    "type": "completed",
    "protocol_version": 1,
    "output": str(event_output),
    "evidence": str(output / "reduction.json"),
    "report": str(output / "report.html"),
    "issue": str(output / "issue.md"),
}}
print(json.dumps(completed), flush=True)
""",
        encoding="utf-8",
    )
    return script
