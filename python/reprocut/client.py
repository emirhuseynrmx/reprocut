"""Typed client for the versioned ReproCut Rust-engine protocol."""

from __future__ import annotations

import contextlib
import json
import os
import queue
import re
import shutil
import subprocess
import sys
import tempfile
import threading
import time
from collections.abc import Callable, Mapping, Sequence
from dataclasses import dataclass
from pathlib import Path
from types import MappingProxyType
from typing import BinaryIO, Literal, Optional, Union, cast

PROTOCOL_VERSION = 1
MAX_EVENT_BYTES = 1024 * 1024
MAX_EVENTS = 10_000
MAX_STDERR_BYTES = 1024 * 1024

Action = Literal["minimize", "resume"]
Ecosystem = Literal["auto", "cargo", "python", "npm", "none"]
Preparation = Literal["none", "offline", "lifecycle_scripts", "isolated_python"]
OracleStream = Literal["auto", "stderr", "stdout", "combined"]
OracleMode = Literal["automatic", "regex", "exit_zero"]
PathArgument = Union[str, os.PathLike[str]]
Executable = Union[PathArgument, Sequence[PathArgument]]


class ReproCutError(RuntimeError):
    """A launch, protocol, reduction, or evidence-validation failure."""

    def __init__(
        self,
        message: str,
        *,
        events: Sequence[ProgressEvent] = (),
        stderr: str = "",
    ) -> None:
        super().__init__(message)
        self.events = tuple(events)
        self.stderr = stderr


@dataclass(frozen=True)
class ReductionRequest:
    """Validated immutable request consumed by the Rust reduction engine."""

    root: Path
    output: Path
    command: tuple[str, ...] = ()
    action: Action = "minimize"
    ecosystem: Ecosystem = "auto"
    preparation: Preparation = "offline"
    timeout_ms: int = 5_000
    max_output_bytes: int = 1_048_576
    oracle_stream: OracleStream = "auto"
    oracle_mode: OracleMode = "automatic"
    failure_patterns: tuple[str, ...] = ()
    reject_patterns: tuple[str, ...] = ()
    python_executable: Optional[Path] = None
    python_wheelhouse: Optional[Path] = None
    python_extras: tuple[str, ...] = ()
    prepare_spec: Optional[Path] = None
    flaky_runs: Optional[int] = None
    flaky_required: Optional[int] = None
    jobs: int = 0
    state: Optional[Path] = None
    restart: bool = False

    def __post_init__(self) -> None:
        object.__setattr__(self, "root", Path(self.root))
        object.__setattr__(self, "output", Path(self.output))
        object.__setattr__(self, "command", tuple(str(part) for part in self.command))
        object.__setattr__(self, "failure_patterns", tuple(sorted(set(self.failure_patterns))))
        object.__setattr__(self, "reject_patterns", tuple(sorted(set(self.reject_patterns))))
        object.__setattr__(
            self,
            "python_extras",
            tuple(sorted({_normalize_extra(extra) for extra in self.python_extras})),
        )
        if self.state is not None:
            object.__setattr__(self, "state", Path(self.state))
        for field in ("python_executable", "python_wheelhouse", "prepare_spec"):
            value = getattr(self, field)
            if value is not None:
                object.__setattr__(self, field, Path(value))
        if self.action not in {"minimize", "resume"}:
            raise ValueError(f"unsupported action: {self.action}")
        if self.ecosystem not in {"auto", "cargo", "python", "npm", "none"}:
            raise ValueError(f"unsupported ecosystem: {self.ecosystem}")
        if self.preparation not in {
            "none",
            "offline",
            "lifecycle_scripts",
            "isolated_python",
        }:
            raise ValueError(f"unsupported preparation: {self.preparation}")
        if self.oracle_stream not in {"auto", "stderr", "stdout", "combined"}:
            raise ValueError(f"unsupported oracle stream: {self.oracle_stream}")
        if self.oracle_mode not in {"automatic", "regex", "exit_zero"}:
            raise ValueError(f"unsupported oracle mode: {self.oracle_mode}")
        if len(self.failure_patterns) > 16 or len(self.reject_patterns) > 16:
            raise ValueError("oracle accepts at most 16 required and 16 reject expressions")
        if any(
            len(pattern.encode("utf-8")) > 4096
            for pattern in (*self.failure_patterns, *self.reject_patterns)
        ):
            raise ValueError("oracle regular expression exceeds 4096 UTF-8 bytes")
        try:
            for pattern in (*self.failure_patterns, *self.reject_patterns):
                re.compile(pattern)
        except re.error as error:
            raise ValueError(f"invalid oracle regular expression: {error}") from error
        if self.oracle_mode == "automatic" and self.failure_patterns:
            raise ValueError("automatic mode does not accept failure_patterns")
        if self.oracle_mode == "regex" and not self.failure_patterns:
            raise ValueError("regex mode requires at least one failure pattern")
        if self.oracle_mode == "exit_zero" and (self.failure_patterns or self.reject_patterns):
            raise ValueError("exit_zero mode does not accept regex patterns")
        isolation_selected = self.preparation == "isolated_python"
        isolation_complete = (
            self.python_executable is not None and self.python_wheelhouse is not None
        )
        isolation_fields = (
            isolation_complete
            or self.python_executable is not None
            or self.python_wheelhouse is not None
            or bool(self.python_extras)
            or self.prepare_spec is not None
        )
        if isolation_selected != isolation_complete or (
            not isolation_selected and isolation_fields
        ):
            raise ValueError(
                "isolated_python requires python_executable and python_wheelhouse, and isolation fields require isolated_python"
            )
        if self.timeout_ms < 1 or self.max_output_bytes < 1 or self.jobs < 0:
            raise ValueError(
                "timeouts and capture limits must be positive; jobs cannot be negative"
            )
        if self.action == "resume" and self.restart:
            raise ValueError("resume requests cannot restart state")
        if self.flaky_runs is not None and not 5 <= self.flaky_runs <= 101:
            raise ValueError("flaky_runs must be within 5..101")
        if self.flaky_runs is not None and self.flaky_runs % 2 == 0:
            raise ValueError("flaky_runs must be odd")
        if self.flaky_required is not None and self.flaky_required < 1:
            raise ValueError("flaky_required must be positive")
        if self.flaky_runs is not None or self.flaky_required is not None:
            effective_runs = self.flaky_runs or 11
            effective_required = self.flaky_required or 9
            if effective_required > effective_runs:
                raise ValueError("flaky_required cannot exceed flaky_runs")
            if effective_required * 3 < effective_runs * 2:
                raise ValueError("flaky mode requires a two-thirds supermajority")

    def to_protocol(self) -> dict[str, object]:
        """Return a detached JSON-serializable V1 request."""
        document: dict[str, object] = {
            "protocol_version": PROTOCOL_VERSION,
            "action": self.action,
            "root": os.fspath(self.root),
            "output": os.fspath(self.output),
            "ecosystem": self.ecosystem,
            "preparation": self.preparation,
            "command": list(self.command),
            "timeout_ms": self.timeout_ms,
            "max_output_bytes": self.max_output_bytes,
            "oracle_stream": self.oracle_stream,
            "oracle_mode": self.oracle_mode,
            "failure_patterns": list(self.failure_patterns),
            "reject_patterns": list(self.reject_patterns),
            "python_extras": list(self.python_extras),
            "jobs": self.jobs,
            "restart": self.restart,
        }
        if self.flaky_runs is not None:
            document["flaky_runs"] = self.flaky_runs
        if self.flaky_required is not None:
            document["flaky_required"] = self.flaky_required
        if self.state is not None:
            document["state"] = os.fspath(self.state)
        if self.python_executable is not None:
            document["python_executable"] = os.fspath(self.python_executable)
        if self.python_wheelhouse is not None:
            document["python_wheelhouse"] = os.fspath(self.python_wheelhouse)
        if self.prepare_spec is not None:
            document["prepare_spec"] = os.fspath(self.prepare_spec)
        return document


def _normalize_extra(name: str) -> str:
    value = str(name)
    if not re.fullmatch(r"[A-Za-z0-9](?:[A-Za-z0-9._-]*[A-Za-z0-9])?", value):
        raise ValueError(f"invalid Python extra name: {value}")
    return re.sub(r"[-_.]+", "-", value).lower()


@dataclass(frozen=True)
class StartedEvent:
    """The request was validated and execution began."""

    action: Action
    root: Path


@dataclass(frozen=True)
class BaselineStableEvent:
    """The original failure produced a stable identity."""

    fingerprint_sha256: str


@dataclass(frozen=True)
class CompletedEvent:
    """Reduction and final same-failure verification completed."""

    output: Path
    evidence: Path
    report: Path
    issue: Path


@dataclass(frozen=True)
class FailedEvent:
    """The Rust engine reached a terminal failure."""

    message: str


ProgressEvent = Union[StartedEvent, BaselineStableEvent, CompletedEvent, FailedEvent]


@dataclass(frozen=True)
class ReductionResult:
    """Verified terminal paths, immutable evidence, and complete event history."""

    output: Path
    evidence_path: Path
    report_path: Path
    issue_path: Path
    fingerprint_sha256: str
    evidence: Mapping[str, object]
    events: tuple[ProgressEvent, ...]


def reduce(
    request: ReductionRequest,
    *,
    progress: Optional[Callable[[ProgressEvent], None]] = None,
    executable: Optional[Executable] = None,
    client_timeout_seconds: Optional[float] = None,
) -> ReductionResult:
    """Run the shared Rust engine and validate its versioned JSONL evidence."""
    if client_timeout_seconds is not None and client_timeout_seconds <= 0:
        raise ValueError("client_timeout_seconds must be positive")
    command = _resolve_command(executable)
    with tempfile.TemporaryDirectory(prefix="reprocut-python-") as temporary:
        request_path = Path(temporary) / "request.json"
        request_path.write_text(
            json.dumps(request.to_protocol(), indent=2) + "\n",
            encoding="utf-8",
        )
        return _run_protocol(
            [*command, "protocol", "run", "--request", os.fspath(request_path)],
            progress,
            client_timeout_seconds,
            request.root,
            request.output,
            request.action,
        )


def _resolve_command(executable: Optional[Executable]) -> tuple[str, ...]:
    if executable is None:
        configured = os.environ.get("REPROCUT_BINARY")
        selected = configured or shutil.which("reprocut")
        if not selected:
            raise ReproCutError(
                "ReproCut Rust binary not found; set REPROCUT_BINARY or run "
                "`cargo install reprocut`"
            )
        command = (selected,)
        _reject_console_recursion(Path(selected))
        return command
    if isinstance(executable, (str, os.PathLike)):
        command = (os.fspath(executable),)
        _reject_console_recursion(Path(command[0]))
        return command
    command = tuple(os.fspath(part) for part in executable)
    if not command:
        raise ValueError("executable command cannot be empty")
    return command


def _reject_console_recursion(candidate: Path) -> None:
    current = Path(sys.argv[0])
    if not candidate.exists() or not current.exists():
        return
    try:
        same = candidate.samefile(current)
    except OSError:
        same = candidate.resolve() == current.resolve()
    if same:
        raise ReproCutError(
            "resolved `reprocut` points to the Python wrapper itself; set REPROCUT_BINARY "
            "to the Rust CLI"
        )


def _run_protocol(
    command: Sequence[str],
    progress: Optional[Callable[[ProgressEvent], None]],
    client_timeout_seconds: Optional[float],
    expected_root: Path,
    expected_output: Path,
    expected_action: Action,
) -> ReductionResult:
    try:
        process = subprocess.Popen(
            command,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )
    except OSError as error:
        raise ReproCutError(f"cannot launch ReproCut: {error}") from error
    assert process.stdout is not None and process.stderr is not None
    stdout_messages: queue.Queue[object] = queue.Queue(maxsize=64)
    stdout_end = object()
    stderr_chunks: list[bytes] = []
    stderr_truncated = [False]
    stdout_thread = threading.Thread(
        target=_read_stdout,
        args=(process.stdout, stdout_messages, stdout_end),
        name="reprocut-stdout",
        daemon=True,
    )
    stderr_thread = threading.Thread(
        target=_drain_stderr,
        args=(process.stderr, stderr_chunks, stderr_truncated),
        name="reprocut-stderr",
        daemon=True,
    )
    stdout_thread.start()
    stderr_thread.start()
    events: list[ProgressEvent] = []
    completed: Optional[CompletedEvent] = None
    baseline: Optional[BaselineStableEvent] = None
    deadline = None if client_timeout_seconds is None else time.monotonic() + client_timeout_seconds
    try:
        while True:
            remaining = None if deadline is None else deadline - time.monotonic()
            if remaining is not None and remaining <= 0:
                raise ReproCutError("ReproCut client timeout expired", events=events)
            try:
                message = stdout_messages.get(timeout=remaining)
            except queue.Empty as error:
                raise ReproCutError("ReproCut client timeout expired", events=events) from error
            if message is stdout_end:
                break
            if isinstance(message, BaseException):
                raise ReproCutError(str(message), events=events) from message
            if len(events) >= MAX_EVENTS:
                raise ReproCutError("protocol emitted more than 10000 events", events=events)
            assert isinstance(message, bytes)
            event = _parse_event(message)
            if not events and not isinstance(event, StartedEvent):
                raise ReproCutError("first protocol event must be started", events=events)
            if isinstance(event, StartedEvent):
                if events:
                    raise ReproCutError("protocol emitted duplicate started events", events=events)
                if event.action != expected_action or not _same_path(event.root, expected_root):
                    raise ReproCutError("started event does not match the submitted request")
            if completed is not None or any(isinstance(item, FailedEvent) for item in events):
                raise ReproCutError(
                    "protocol emitted an event after a terminal event", events=events
                )
            if isinstance(event, BaselineStableEvent) and baseline is not None:
                raise ReproCutError("protocol emitted duplicate baseline events", events=events)
            if isinstance(event, CompletedEvent) and baseline is None:
                raise ReproCutError("completion preceded the stable baseline event", events=events)
            events.append(event)
            if isinstance(event, BaselineStableEvent):
                baseline = event
            elif isinstance(event, CompletedEvent):
                completed = event
            if progress is not None:
                progress(event)
        remaining = None if deadline is None else max(0.0, deadline - time.monotonic())
        try:
            return_code = process.wait(timeout=remaining)
        except subprocess.TimeoutExpired as error:
            raise ReproCutError("ReproCut client timeout expired", events=events) from error
    except BaseException:
        if process.poll() is None:
            process.kill()
        with contextlib.suppress(subprocess.TimeoutExpired):
            process.wait(timeout=5)
        raise
    finally:
        stdout_thread.join(timeout=5)
        stderr_thread.join(timeout=5)
    stderr = b"".join(stderr_chunks).decode("utf-8", errors="replace")
    if stderr_truncated[0]:
        stderr += "\n<stderr truncated by Python client>"
    failed = next((event for event in events if isinstance(event, FailedEvent)), None)
    if failed is not None:
        raise ReproCutError(failed.message, events=events, stderr=stderr)
    if return_code != 0:
        message = stderr.strip() or f"ReproCut exited with status {return_code}"
        raise ReproCutError(message, events=events, stderr=stderr)
    if completed is None or baseline is None:
        raise ReproCutError(
            "successful protocol stream omitted baseline or completion", events=events
        )
    _validate_completed_paths(completed, expected_output)
    evidence = _load_evidence(completed.evidence, baseline.fingerprint_sha256)
    return ReductionResult(
        output=completed.output,
        evidence_path=completed.evidence,
        report_path=completed.report,
        issue_path=completed.issue,
        fingerprint_sha256=baseline.fingerprint_sha256,
        evidence=evidence,
        events=tuple(events),
    )


def _read_stdout(stream: BinaryIO, messages: queue.Queue[object], end: object) -> None:
    try:
        while True:
            line = stream.readline(MAX_EVENT_BYTES + 1)
            if not line:
                return
            if len(line) > MAX_EVENT_BYTES or not line.endswith(b"\n"):
                messages.put(ValueError("protocol event exceeds 1 MiB or lacks a newline"))
                return
            messages.put(line)
    except Exception as error:
        messages.put(error)
    finally:
        messages.put(end)


def _drain_stderr(stream: BinaryIO, chunks: list[bytes], truncated: list[bool]) -> None:
    retained = 0
    while chunk := stream.read(64 * 1024):
        available = MAX_STDERR_BYTES - retained
        if available > 0:
            kept = chunk[:available]
            chunks.append(kept)
            retained += len(kept)
        if len(chunk) > available:
            truncated[0] = True


def _parse_event(raw: bytes) -> ProgressEvent:
    try:
        document = json.loads(raw.decode("utf-8"))
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        raise ReproCutError(f"invalid JSONL protocol event: {error}") from error
    if not isinstance(document, dict):
        raise ReproCutError("protocol event must be a JSON object")
    if document.get("protocol_version") != PROTOCOL_VERSION:
        raise ReproCutError(
            f"unsupported protocol event version: {document.get('protocol_version')!r}"
        )
    kind = document.get("type")
    if kind == "started":
        action = document.get("action")
        if action not in {"minimize", "resume"}:
            raise ReproCutError("started event contains an invalid action")
        return StartedEvent(action=action, root=Path(_required_string(document, "root")))
    if kind == "baseline_stable":
        fingerprint = _required_string(document, "fingerprint_sha256")
        if len(fingerprint) != 64 or any(
            character not in "0123456789abcdef" for character in fingerprint
        ):
            raise ReproCutError("baseline fingerprint must be lowercase SHA-256 hex")
        return BaselineStableEvent(fingerprint_sha256=fingerprint)
    if kind == "completed":
        return CompletedEvent(
            output=Path(_required_string(document, "output")),
            evidence=Path(_required_string(document, "evidence")),
            report=Path(_required_string(document, "report")),
            issue=Path(_required_string(document, "issue")),
        )
    if kind == "failed":
        return FailedEvent(message=_required_string(document, "message"))
    raise ReproCutError(f"unknown protocol event type: {kind!r}")


def _required_string(document: Mapping[str, object], key: str) -> str:
    value = document.get(key)
    if not isinstance(value, str) or not value:
        raise ReproCutError(f"protocol event field {key!r} must be a non-empty string")
    return value


def _same_path(left: Path, right: Path) -> bool:
    try:
        return left.resolve(strict=False) == right.resolve(strict=False)
    except OSError:
        return os.path.abspath(left) == os.path.abspath(right)


def _validate_completed_paths(event: CompletedEvent, expected_output: Path) -> None:
    if not _same_path(event.output, expected_output):
        raise ReproCutError("completed output does not match the submitted request")
    expected_artifacts = {
        "evidence": event.output / "reduction.json",
        "report": event.output / "report.html",
        "issue": event.output / "issue.md",
    }
    actual_artifacts = {
        "evidence": event.evidence,
        "report": event.report,
        "issue": event.issue,
    }
    if not event.output.is_dir():
        raise ReproCutError(f"completed output directory does not exist: {event.output}")
    for name, expected in expected_artifacts.items():
        actual = actual_artifacts[name]
        if not _same_path(actual, expected) or not actual.is_file():
            raise ReproCutError(f"completed {name} artifact is missing or outside output")


def _load_evidence(path: Path, fingerprint: str) -> Mapping[str, object]:
    try:
        document = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise ReproCutError(f"cannot read completed reduction evidence {path}: {error}") from error
    if not isinstance(document, dict) or document.get("schema_version") != 3:
        raise ReproCutError("completed reduction evidence must use schema version 3")
    failure = document.get("failure")
    if not isinstance(failure, dict) or failure.get("same_failure") is not True:
        raise ReproCutError("completed evidence does not prove the same failure")
    if failure.get("fingerprint_sha256") != fingerprint:
        raise ReproCutError("event and evidence fingerprints disagree")
    for label, value in (
        ("source snapshot", document.get("source_snapshot_sha256")),
        ("fingerprint", failure.get("fingerprint_sha256")),
        ("oracle spec", failure.get("oracle_spec_sha256")),
    ):
        if not isinstance(value, str) or not re.fullmatch(r"[0-9a-f]{64}", value):
            raise ReproCutError(f"completed evidence has an invalid {label} SHA-256")
    preparation = document.get("preparation")
    if not isinstance(preparation, dict):
        raise ReproCutError("completed evidence omitted preparation contract")
    preparation_digest = preparation.get("contract_sha256")
    limitations = preparation.get("limitations")
    if preparation_digest is not None and (
        not isinstance(preparation_digest, str)
        or not re.fullmatch(r"[0-9a-f]{64}", preparation_digest)
    ):
        raise ReproCutError("completed evidence has an invalid preparation SHA-256")
    if preparation_digest is None and not (isinstance(limitations, list) and limitations):
        raise ReproCutError("missing preparation digest requires an explicit limitation")
    return cast(Mapping[str, object], _freeze_json(document))


def _freeze_json(value: object) -> object:
    if isinstance(value, dict):
        return MappingProxyType({str(key): _freeze_json(item) for key, item in value.items()})
    if isinstance(value, list):
        return tuple(_freeze_json(item) for item in value)
    return value
