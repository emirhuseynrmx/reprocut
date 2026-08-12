"""Readable source-checkout fallback; release wheels use the Rust extension."""

from __future__ import annotations

import hashlib
import re
import struct
from collections.abc import Iterable, Sequence
from typing import Literal, Union

Verdict = Literal["preserved", "rejected", "inconclusive"]
Channel = Literal["auto", "stdout", "stderr", "combined"]
Mode = Literal["automatic", "regex", "exit_zero"]
LegacyBaseline = tuple[int, str]
StreamBaseline = tuple[int, str, str]
Baseline = Union[LegacyBaseline, StreamBaseline]

NORMALIZATION_SCHEMA = 3
MAX_PATTERNS = 16
MAX_PATTERN_BYTES = 4096
COMBINED_DELIMITER = "\n--- REPROCUT STREAM ---\n"

_UUID = re.compile(
    r"[0-9A-Fa-f]{8}-[0-9A-Fa-f]{4}-[1-5][0-9A-Fa-f]{3}-"
    r"[89ABab][0-9A-Fa-f]{3}-[0-9A-Fa-f]{12}"
)
_TIMESTAMP = re.compile(
    r"[0-9]{4}-[0-9]{2}-[0-9]{2}[T ][0-9]{2}:[0-9]{2}:[0-9]{2}"
    r"(?:\.[0-9]+)?(?:Z|[+-][0-9]{2}:[0-9]{2})?"
)
_UNIX_TEMP = re.compile(r"/(?:tmp|var/tmp)(?:/[^ \t\r\n:]+)*")
_WINDOWS_TEMP = re.compile(
    r"[A-Za-z]:\\(?:[Tt][Mm][Pp]|[Tt][Ee][Mm][Pp]|"
    r"[Uu]sers\\[^\\ \t\r\n:]+\\[Aa]pp[Dd]ata\\[Ll]ocal\\[Tt]emp)"
    r"(?:\\[^ \t\r\n:]+)*"
)
_ADDRESS = re.compile(
    r"(?:address|addr|pointer|ptr|Address|Pointer)[ \t]*[:=]?[ \t]*"
    r"0x[0-9A-Fa-f]{7,}"
)
_PROCESS_ID = re.compile(
    r"(pid|PID|process[ \t]+[Ii][Dd]|thread[ \t]+[Ii][Dd]|thread|Thread)"
    r"[ \t]*[:=#]?[ \t]*[0-9]+"
)
_LOOPBACK_PORT = re.compile(r"(localhost|LOCALHOST|127\.0\.0\.1|\[::1\]):[0-9]{1,5}")
_NAMED_PORT = re.compile(r"(?:port|Port|PORT)[ \t]*[:=]?[ \t]*[0-9]{1,5}")
_DURATION = re.compile(
    r"[0-9]+(?:\.[0-9]+)?[ \t]*(?:seconds|second|minutes|minute|secs|sec|"
    r"mins|min|ms|ns|us|s|m)"
)
_PATH_LOCATION = re.compile(r"(?P<token>[^ \t\r\n:]+):[0-9]+(?::[0-9]+)?")
_SOURCE_EXTENSIONS = frozenset(
    {
        "bash",
        "c",
        "cc",
        "cjs",
        "cpp",
        "cs",
        "cts",
        "cxx",
        "fish",
        "go",
        "h",
        "hh",
        "hpp",
        "hxx",
        "java",
        "js",
        "json",
        "jsx",
        "kt",
        "kts",
        "mjs",
        "mts",
        "php",
        "py",
        "pyi",
        "rb",
        "rs",
        "scala",
        "sh",
        "swift",
        "toml",
        "ts",
        "tsx",
        "yaml",
        "yml",
        "zsh",
    }
)
_EXTENSIONLESS_SOURCE_FILES = frozenset(
    {"BUILD", "Dockerfile", "Makefile", "WORKSPACE"}
)
_CHANNEL_ORDER = {"stdout": 0, "stderr": 1}
_NAMED_LOCATION = re.compile(r"([Ll]ine|[Cc]olumn)[ \t]+[0-9]+")
_HORIZONTAL_SPACE = re.compile(r"[\t ]+")
_PYTEST = re.compile(r"^(?:failed|error)[ \t]+[^ \t\r\n]+(?:::[^ \t\r\n]+)+")
_COMPILER = re.compile(
    r"(?:error\[[a-z][0-9]{2,}\]|(?:fatal )?error[ \t]+[a-z]{1,5}[0-9]{2,})"
)
_ROOT = re.compile(
    r"(?:[a-z_][a-z0-9_.]*(?:error|exception)|panicked at|^panic:|^fatal:)"
)
_ASSERTION = re.compile(r"(?:assert(?:ion)?|expected|actual|left.*right)")
_MESSAGE = re.compile(r"(?:error|failed|failure|panic|exception|fatal)")
_SUMMARY = re.compile(
    r"^[^A-Za-z0-9_]*(?:[0-9]+[ \t]+(?:failed|passed|skipped|error)s?)"
    r"(?:[^A-Za-z0-9_]|$).*(?:(?:ms|s|sec|seconds?))?[^A-Za-z0-9_]*$"
)
_LOCATION = re.compile(
    r'^(?:at[ \t]+[^ \t\r\n]+|file[ \t]+"[^"]+",[ \t]+line[ \t]+'
    r"<location>|[ \t]*-->[ \t]+[^ \t\r\n]+)(?::?[0-9<>]+)*[ \t]*$"
)
_LIFECYCLE = re.compile(
    r"^(?:process|command|child)[ \t]+(?:exited|failed)[ \t]+with[ \t]+"
    r"(?:code|status)[ \t]+[-0-9]+[^A-Za-z0-9_]*$"
)


class EvaluationPolicy:
    """Immutable strict/flaky execution policy matching the Rust validator."""

    __slots__ = ("_mode", "_required", "_runs")

    def __init__(
        self, mode: str, runs: int, required: int, *, _factory: bool = False
    ) -> None:
        if not _factory:
            raise TypeError("use EvaluationPolicy.strict() or EvaluationPolicy.flaky()")
        object.__setattr__(self, "_mode", mode)
        object.__setattr__(self, "_runs", runs)
        object.__setattr__(self, "_required", required)

    def __setattr__(self, name: str, value: object) -> None:
        del name, value
        raise AttributeError("EvaluationPolicy is immutable")

    @classmethod
    def strict(cls) -> EvaluationPolicy:
        return cls("strict", 3, 3, _factory=True)

    @classmethod
    def flaky(cls, runs: int = 11, required: int = 9) -> EvaluationPolicy:
        if not 5 <= runs <= 101:
            raise ValueError("flaky runs must be between 5 and 101")
        if runs % 2 == 0:
            raise ValueError("flaky runs must be odd")
        if not 1 <= required <= runs:
            raise ValueError("flaky required must be between 1 and runs")
        if required * 3 < runs * 2:
            raise ValueError(
                "flaky required must be at least a two-thirds supermajority"
            )
        return cls("flaky", runs, required, _factory=True)

    @property
    def mode(self) -> str:
        return self._mode

    @property
    def runs(self) -> int:
        return self._runs

    @property
    def required(self) -> int:
        return self._required


class FailureOracle:
    """Reference implementation of the immutable native oracle contract."""

    __slots__ = (
        "_anchors",
        "_channel",
        "_exit_code",
        "_failure_patterns",
        "_mode",
        "_oracle_spec_digest",
        "_reject_patterns",
        "_required_regex",
        "_reject_regex",
    )

    def __init__(
        self,
        exit_code: int,
        mode: Mode,
        channel: Channel,
        anchors: tuple[tuple[str, str], ...],
        failure_patterns: tuple[str, ...],
        reject_patterns: tuple[str, ...],
        oracle_spec_digest: bytes,
        *,
        _factory: bool = False,
    ) -> None:
        if not _factory:
            raise TypeError("use FailureOracle.from_baselines()")
        object.__setattr__(self, "_exit_code", exit_code)
        object.__setattr__(self, "_mode", mode)
        object.__setattr__(self, "_channel", channel)
        object.__setattr__(self, "_anchors", anchors)
        object.__setattr__(self, "_failure_patterns", failure_patterns)
        object.__setattr__(self, "_reject_patterns", reject_patterns)
        object.__setattr__(self, "_oracle_spec_digest", oracle_spec_digest)
        object.__setattr__(
            self, "_required_regex", tuple(map(re.compile, failure_patterns))
        )
        object.__setattr__(
            self, "_reject_regex", tuple(map(re.compile, reject_patterns))
        )

    def __setattr__(self, name: str, value: object) -> None:
        del name, value
        raise AttributeError("FailureOracle is immutable")

    @classmethod
    def from_baselines(
        cls,
        baselines: Sequence[Baseline],
        *,
        mode: Mode = "automatic",
        channel: Channel = "auto",
        failure_patterns: Sequence[str] = (),
        reject_patterns: Sequence[str] = (),
    ) -> FailureOracle:
        canonical_failure, canonical_reject = _validate_spec(
            mode, channel, failure_patterns, reject_patterns
        )
        if len(baselines) < 2:
            raise ValueError("at least two baseline observations are required")
        observations = tuple(_split_baseline(baseline) for baseline in baselines)
        first_exit = observations[0][0]
        if mode == "exit_zero":
            if any(exit_code != 0 for exit_code, _, _ in observations):
                raise ValueError(
                    "exit-zero mode requires every baseline to exit with code zero"
                )
            anchors: tuple[tuple[str, str], ...] = ()
        else:
            if any(exit_code != first_exit for exit_code, _, _ in observations[1:]):
                raise ValueError("baseline exit states are unstable")
            required = tuple(map(re.compile, canonical_failure))
            reject = tuple(map(re.compile, canonical_reject))
            if mode == "regex":
                for observation in observations:
                    diagnostic = _diagnostic_view(
                        channel, observation[1], observation[2]
                    )
                    if any(pattern.search(diagnostic) for pattern in reject):
                        raise ValueError(
                            "a reject expression matches an original baseline"
                        )
                    if not all(pattern.search(diagnostic) for pattern in required):
                        raise ValueError(
                            "a required expression does not match every baseline"
                        )
                anchors = ()
            else:
                for observation in observations:
                    diagnostic = _diagnostic_view(
                        channel, observation[1], observation[2]
                    )
                    if any(pattern.search(diagnostic) for pattern in reject):
                        raise ValueError(
                            "a reject expression matches an original baseline"
                        )
                anchors = _stable_discriminators(channel, observations)
                if not anchors:
                    raise ValueError(
                        "baseline diagnostic has no stable discriminative anchor"
                    )
        spec_digest = _spec_digest(mode, channel, canonical_failure, canonical_reject)
        return cls(
            first_exit,
            mode,
            channel,
            anchors,
            canonical_failure,
            canonical_reject,
            spec_digest,
            _factory=True,
        )

    def classify(
        self,
        exit_code: int,
        diagnostic: str,
        *,
        stdout: str = "",
        timed_out: bool = False,
        truncated: bool = False,
    ) -> Verdict:
        if timed_out:
            return "inconclusive"
        if self._mode == "exit_zero":
            return "preserved" if exit_code == 0 else "rejected"
        if truncated:
            return "inconclusive"
        if exit_code != self._exit_code:
            return "rejected"
        raw = _diagnostic_view(self._channel, stdout, diagnostic)
        if any(pattern.search(raw) for pattern in self._reject_regex):
            return "rejected"
        if self._mode == "regex":
            return (
                "preserved"
                if all(pattern.search(raw) for pattern in self._required_regex)
                else "rejected"
            )
        streams = {"stdout": _normalize(stdout), "stderr": _normalize(diagnostic)}
        matches = all(
            text in streams[anchor_channel].splitlines()
            for anchor_channel, text in self._anchors
        )
        return "preserved" if matches else "rejected"

    @property
    def fingerprint(self) -> dict[str, object]:
        anchors = [
            {"channel": channel, "text": text} for channel, text in self._anchors
        ]
        fingerprint_digest = _fingerprint_digest(
            self._mode,
            self._exit_code,
            self._anchors,
            self._failure_patterns,
            self._reject_patterns,
            self._oracle_spec_digest,
        )
        return {
            "mode": self._mode,
            "exit_code": self._exit_code,
            "signal": None,
            "termination": {"kind": "exit_code", "value": self._exit_code},
            "anchor": self._anchors[0][1] if self._anchors else "",
            "anchors": anchors,
            "failure_patterns": list(self._failure_patterns),
            "reject_patterns": list(self._reject_patterns),
            "normalization_schema": NORMALIZATION_SCHEMA,
            "oracle_spec_sha256": self._oracle_spec_digest.hex(),
            "fingerprint_sha256": fingerprint_digest.hex(),
        }


def _validate_spec(
    mode: str,
    channel: str,
    failure_patterns: Sequence[str],
    reject_patterns: Sequence[str],
) -> tuple[tuple[str, ...], tuple[str, ...]]:
    if mode not in {"automatic", "regex", "exit_zero"}:
        raise ValueError(f"unsupported oracle mode: {mode}")
    if channel not in {"auto", "stdout", "stderr", "combined"}:
        raise ValueError(f"unsupported diagnostic channel: {channel}")
    if len(failure_patterns) > MAX_PATTERNS or len(reject_patterns) > MAX_PATTERNS:
        raise ValueError("oracle accepts at most 16 required and 16 reject expressions")
    if any(
        len(pattern.encode("utf-8")) > MAX_PATTERN_BYTES
        for pattern in (*failure_patterns, *reject_patterns)
    ):
        raise ValueError("oracle regular expression exceeds 4096 UTF-8 bytes")
    if mode == "regex" and not failure_patterns:
        raise ValueError("regex mode requires at least one failure pattern")
    if mode in {"automatic", "exit_zero"} and failure_patterns:
        raise ValueError(f"{mode} mode does not accept failure patterns")
    if mode == "exit_zero" and reject_patterns:
        raise ValueError("exit_zero mode does not accept patterns")
    try:
        for pattern in (*failure_patterns, *reject_patterns):
            re.compile(pattern)
    except re.error as error:
        raise ValueError(f"invalid oracle regular expression: {error}") from error
    return tuple(sorted(set(failure_patterns))), tuple(sorted(set(reject_patterns)))


def _normalize(diagnostic: str) -> str:
    value = diagnostic.replace("\r\n", "\n").replace("\r", "\n")
    value = _UUID.sub("<uuid>", value)
    value = _TIMESTAMP.sub("<timestamp>", value)
    value = _WINDOWS_TEMP.sub("<temp>", value)
    value = _UNIX_TEMP.sub("<temp>", value)
    value = _ADDRESS.sub("address <address>", value)
    value = _PROCESS_ID.sub(r"\1 <id>", value)
    value = _LOOPBACK_PORT.sub(r"\1:<port>", value)
    value = _NAMED_PORT.sub("port <port>", value)
    value = _DURATION.sub("<duration>", value)
    value = _PATH_LOCATION.sub(_normalize_source_location, value)
    value = _NAMED_LOCATION.sub(r"\1 <location>", value)
    lines = (_HORIZONTAL_SPACE.sub(" ", line.strip()) for line in value.splitlines())
    return "\n".join(line for line in lines if line)


def _normalize_source_location(match: re.Match[str]) -> str:
    token = match.group("token")
    basename = re.split(r"[/\\]", token)[-1]
    extension = token.rpartition(".")[2].lower()
    if (
        token == "<temp>"
        or _has_explicit_source_context(match)
        or basename in _EXTENSIONLESS_SOURCE_FILES
        or extension in _SOURCE_EXTENSIONS
    ):
        return f"{token}:<location>"
    return match.group(0)


def _has_explicit_source_context(match: re.Match[str]) -> bool:
    line_prefix = match.string[: match.start()].rsplit("\n", 1)[-1].rstrip(" \t")
    return line_prefix.endswith("-->") or line_prefix.rsplit(maxsplit=1)[-1:] == ["at"]


def _stable_discriminators(
    channel: Channel, observations: Sequence[tuple[int, str, str]]
) -> tuple[tuple[str, str], ...]:
    streams = ("stdout", "stderr") if channel in {"auto", "combined"} else (channel,)
    candidates: list[tuple[int, int, int, str, str]] = []
    for stream in streams:
        values = [item[1] if stream == "stdout" else item[2] for item in observations]
        first = _eligible_lines(stream, _normalize(values[0]))
        intersections = [
            {line[4] for line in _eligible_lines(stream, _normalize(value))}
            for value in values[1:]
        ]
        candidates.extend(
            line for line in first if all(line[4] in lines for lines in intersections)
        )
    if channel == "combined" and not all(
        any(candidate[3] == stream for candidate in candidates)
        for stream in ("stdout", "stderr")
    ):
        return ()
    if channel == "auto":
        candidates = [candidate for candidate in candidates if candidate[0] < 4]
    candidates.sort(
        key=lambda line: (line[0], -line[1], line[2], _CHANNEL_ORDER[line[3]], line[4])
    )
    selected: list[tuple[int, int, int, str, str]] = []
    if channel == "combined":
        selected.extend(
            next(candidate for candidate in candidates if candidate[3] == stream)
            for stream in ("stdout", "stderr")
        )
        for candidate in candidates:
            if len(selected) == 4:
                break
            if candidate not in selected:
                selected.append(candidate)
        return tuple((line[3], line[4]) for line in selected)
    if channel == "auto":
        for stream in streams:
            candidate = next((item for item in candidates if item[3] == stream), None)
            if candidate is not None:
                selected.append(candidate)
    categories: set[int] = set()
    categories.update(candidate[0] for candidate in selected)
    for candidate in candidates:
        if candidate[0] not in categories and candidate not in selected:
            categories.add(candidate[0])
            selected.append(candidate)
            if len(selected) == 4:
                break
    for candidate in candidates:
        if len(selected) == 4:
            break
        if candidate not in selected:
            selected.append(candidate)
    return tuple((line[3], line[4]) for line in selected)


def _eligible_lines(
    stream: str, diagnostic: str
) -> list[tuple[int, int, int, str, str]]:
    result = []
    for position, line in enumerate(diagnostic.splitlines()):
        kind = _discriminator_kind(line)
        if kind is not None:
            result.append((kind, _score(line), position, stream, line))
    return result


def _discriminator_kind(line: str) -> int | None:
    if _is_boilerplate(line):
        return None
    lowercase = line.lower()
    if _PYTEST.search(lowercase):
        return 0
    if _COMPILER.search(lowercase):
        return 1
    if _ROOT.search(lowercase):
        return 2
    if _ASSERTION.search(lowercase):
        return 3
    if _MESSAGE.search(lowercase):
        return 4
    return None


def _is_boilerplate(line: str) -> bool:
    stripped = line.strip()
    if not stripped or not any(character.isalnum() for character in stripped):
        return True
    lowercase = stripped.lower()
    if lowercase in {
        "traceback (most recent call last):",
        "stack backtrace:",
        "backtrace:",
        "short test summary info",
        "failures",
    }:
        return True
    return bool(
        _SUMMARY.search(lowercase)
        or _LOCATION.search(lowercase)
        or _LIFECYCLE.search(lowercase)
    )


def _score(line: str) -> int:
    tokens = {
        token.lower()
        for token in re.split(r"[^A-Za-z0-9_]", line)
        if any(character.isalpha() for character in token)
    }
    return len(tokens) * 16 + sum(character.isalpha() for character in line)


def _split_baseline(baseline: Baseline) -> tuple[int, str, str]:
    if len(baseline) == 2:
        exit_code, diagnostic = baseline
        return exit_code, "", diagnostic
    exit_code, stdout, stderr = baseline
    return exit_code, stdout, stderr


def _diagnostic_view(channel: Channel, stdout: str, stderr: str) -> str:
    stdout = stdout.replace("\r\n", "\n").replace("\r", "\n")
    stderr = stderr.replace("\r\n", "\n").replace("\r", "\n")
    if channel == "stdout":
        return stdout
    if channel == "stderr":
        return stderr
    return stdout + COMBINED_DELIMITER + stderr


def _spec_digest(
    mode: Mode,
    channel: Channel,
    required: tuple[str, ...],
    reject: tuple[str, ...],
) -> bytes:
    value = bytearray(b"REPROCUT-ORACLE-SPEC-V2\0")
    value.append({"automatic": 0, "regex": 1, "exit_zero": 2}[mode])
    value.append({"auto": 0, "stderr": 1, "stdout": 2, "combined": 3}[channel])
    _encode_strings(value, required)
    _encode_strings(value, reject)
    return hashlib.sha256(value).digest()


def _fingerprint_digest(
    mode: Mode,
    exit_code: int,
    anchors: tuple[tuple[str, str], ...],
    required: tuple[str, ...],
    reject: tuple[str, ...],
    spec_digest: bytes,
) -> bytes:
    value = bytearray(b"REPROCUT-FINGERPRINT-V2\0")
    value.append({"automatic": 0, "regex": 1, "exit_zero": 2}[mode])
    value.append(0)
    value.extend(struct.pack("<i", exit_code))
    value.extend(struct.pack("<H", NORMALIZATION_SCHEMA))
    value.extend(spec_digest)
    value.extend(struct.pack("<Q", len(anchors)))
    for channel, text in anchors:
        value.append({"auto": 0, "stderr": 1, "stdout": 2, "combined": 3}[channel])
        _encode_text(value, text)
    _encode_strings(value, required)
    _encode_strings(value, reject)
    return hashlib.sha256(value).digest()


def _encode_strings(value: bytearray, strings: Iterable[str]) -> None:
    strings = tuple(strings)
    value.extend(struct.pack("<Q", len(strings)))
    for item in strings:
        _encode_text(value, item)


def _encode_text(value: bytearray, text: str) -> None:
    encoded = text.encode("utf-8")
    value.extend(struct.pack("<Q", len(encoded)))
    value.extend(encoded)
