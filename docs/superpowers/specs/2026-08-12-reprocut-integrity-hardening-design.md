# ReproCut 0.1 integrity hardening design

## Objective

ReproCut 0.1 must never authorize a reduction from weak failure identity,
ambient Python packages, live-checkout drift, or lost executable metadata. The
release stays at `0.1.0`; these changes replace unreleased RC contracts rather
than preserving a known-unsafe compatibility surface.

The work has four implementation units that share one session identity:

1. deterministic failure-oracle v2;
2. explicit regex and exit-zero interestingness modes;
3. candidate-local offline Python preparation;
4. immutable, metadata-aware source snapshots.

Registry publication remains blocked until all new native and cross-platform
gates pass on the exact release commit.

## Confirmed root causes

The current implementation was inspected and counterexamples were executed
against its Python parity implementation.

- `anchor_for` selects only the longest non-empty line in each stable stream.
  A long punctuation separator can therefore represent the failure.
- `normalize_diagnostic` replaces every decimal sequence with `<n>`. Assertion
  values, status codes, shapes, sizes, token IDs, and amounts consequently lose
  semantic identity.
- Whole-stream equality is required while stack frames and progress output are
  treated like the root diagnostic. This produces avoidable false negatives.
- `PreparationMode::IsolatedPython` merely unlocks dependency candidates.
  Baselines and candidates do not create a venv or install the edited project.
- File candidates call `CandidateWorkspace::materialize` against the live
  inventory root after the source digest was computed.
- `ProjectSnapshot` stores path, bytes, and content digest only. Its `copy_to`
  path uses `fs::write`, so Unix executable bits do not survive structured
  reduction or publication.

Observed false positives include:

```text
same separator + different exception  -> preserved
same assertion words + new numbers    -> preserved
same pytest summary + new failing test -> preserved
```

## 1. Oracle model

### 1.1 Public configuration

Introduce a closed `OracleSpec` with three modes:

```rust
pub enum OracleMode {
    Automatic,
    Regex,
    ExitZero,
}

pub struct OracleSpec {
    mode: OracleMode,
    channel: DiagnosticChannel,
    failure_patterns: Vec<String>,
    reject_patterns: Vec<String>,
}
```

Construction validates and canonicalizes the configuration before any child is
spawned:

- at most 16 required and 16 reject patterns;
- each pattern is at most 4096 UTF-8 bytes;
- patterns compile with Rust's bounded, non-backtracking `regex` engine;
- duplicate patterns are removed while stable lexical order is retained;
- regex mode requires at least one failure pattern;
- exit-zero mode rejects failure/reject patterns;
- automatic mode permits reject patterns as a final veto.

CLI surface:

```text
--oracle-mode automatic|regex|exit-zero
--oracle-stream auto|stderr|stdout|combined
--failure-regex REGEX       repeatable; requires regex mode
--reject-regex REGEX        repeatable; automatic or regex mode
```

The versioned JSON protocol and typed Python request expose the same fields.
Invalid combinations fail before baseline execution.

### 1.2 Normalization schema 3

Schema 3 performs only context-qualified volatile replacement. It never
replaces an unqualified decimal, short hexadecimal value, or semantic
`word:number` pair. Schema 3 supersedes schema 2 because schema 2 could mistake
values such as `status:404`, `expected:123`, and `shard:12` for source locations.

Normalize:

- CRLF/CR to LF and repeated horizontal whitespace;
- owned `reprocut-candidate-*` roots and conventional OS temporary roots;
- UUIDs and ISO timestamps;
- pointer-like hexadecimal addresses of at least seven digits when marked as
  an address/pointer or emitted in a stack-frame address position;
- PID, process ID, and thread ID only next to their identifying keyword;
- localhost/loopback ports and values following the word `port`;
- durations with an explicit time unit;
- `path:line:column` only when the token contains `/` or `\`, ends with a
  recognized source/manifest extension, or is an internal normalized temporary
  path; plus explicit `line N` and `column N` locations.

Preserve:

- assertion expected/actual values;
- HTTP/process status codes not identified as PIDs;
- array shapes, dimensions, sizes, token IDs, counts, amounts, versions, and
  short hexadecimal domain values;
- relative source paths and failing test names.

`normalize_diagnostic` remains public and returns schema-3 text. Fingerprints
and resume/cache identities containing an older normalization schema are
incompatible and fail closed.

### 1.3 Boilerplate rejection

Lines cannot become anchors when they are:

- empty or only punctuation/separator characters;
- framework headings such as traceback/backtrace/short-summary headers;
- aggregate summaries containing only pass/fail counts and duration;
- location-only stack frames without an exception, panic, compiler code, test
  identity, or assertion message;
- generic lifecycle text such as `process exited with code ...`.

Boilerplate rules are narrow, enumerated, deterministic regexes with dedicated
positive and negative tests. A line is not discarded merely because it starts
with `error` or contains a path.

### 1.4 Stable discriminators

Each selected stream is normalized per baseline and converted to a set of
eligible lines. Anchor candidates are the exact intersection present in every
baseline; unrelated unstable progress and stack-frame lines do not invalidate
the stream.

Eligible lines receive deterministic categories and priority:

1. failing test identity;
2. compiler diagnostic code and message;
3. exception/panic class and root message;
4. assertion message;
5. other discriminative message.

The selector keeps at most four anchors, ordered by category, score, first
baseline position, then lexical value. It keeps every available category before
taking a second line from one category. Scoring rewards alphabetic tokens and
distinct words, not raw line length.

Recognized identities include at minimum:

- Python/Java/.NET `*Error` and `*Exception` root lines;
- Rust `thread ... panicked at` and Go `panic:` roots;
- pytest node IDs, unittest test names, Cargo/Rust test failure names;
- Rust `error[E....]`, common compiler error codes, and fatal compiler messages;
- assertion lines containing expected/actual/left/right or explicit assertion
  classes.

Automatic candidate classification requires:

1. complete observation evidence;
2. identical termination reason;
3. every selected exact normalized anchor in its original stream;
4. no reject pattern in the selected raw diagnostic.

If a high-confidence root line exists, location-only frames are not required.
The same root exception therefore survives a shorter stack trace. If no
eligible stable discriminator exists, oracle construction returns
`EmptyAnchor`; it never falls back to punctuation or termination alone.

`Auto` selects error-bearing streams. It does not require stable stdout progress
unless stdout contains a recognized failure identity. `Combined` explicitly
requires evidence from both streams.

### 1.5 Regex mode

Regex mode is user-owned failure identity, not fuzzy matching.

- Patterns operate on bounded raw diagnostic text after newline canonicalization
  only; semantic numbers remain visible.
- `stderr` and `stdout` inspect exactly that stream. `combined` concatenates
  stdout, a fixed delimiter, and stderr. `auto` uses the same combined view.
- Every required pattern must match every baseline.
- No reject pattern may match a baseline.
- For candidates, a reject match vetoes first; then all required patterns must
  match and termination must equal the baseline termination.
- Invalid regex, baseline mismatch, timeout, truncation, or runner failure can
  never authorize a cut.

Required and reject patterns are stored verbatim in the fingerprint and session
contract.

### 1.6 Exit-zero mode

The command after `--` is the interestingness command.

- Every baseline must exit with code 0.
- Candidate exit 0 means `Preserved`.
- Candidate non-zero exit means `Rejected`.
- Timeout, signal, or runner failure means `Inconclusive`.
- Output truncation does not alter an exit-zero verdict because output is not
  evidence in this mode.
- The fingerprint records exit-zero mode and the command digest, with no textual
  anchors.

This mode never silently activates from a successful automatic baseline; it
requires `--oracle-mode exit-zero`.

### 1.7 Fingerprint and evidence

`FailureFingerprint` becomes a mode-aware value containing:

- oracle mode;
- optional baseline termination;
- normalized exact anchors;
- required and reject patterns;
- normalization schema `2`;
- oracle-spec digest.

Fingerprint SHA-256 includes every field with length-delimited encoding. Evidence
schema advances from 2 to 3 and additionally records:

- frozen source snapshot digest;
- preparation contract digest or `null`;
- oracle mode, normalization schema, patterns, and selected anchors;
- explicit limitations when Python dependency reduction is unavailable.

HTML, issue Markdown, JSONL protocol, Python fingerprint dictionaries, the demo,
gallery metadata, and report goldens all consume this one model.

## 2. Candidate-local Python preparation

### 2.1 Activation contract

Python dependency candidates are available only when all of these are present:

- `--prepare isolated-python`;
- an explicit base interpreter supplied by `--python-executable`;
- a regular, readable `--python-wheelhouse` directory;
- a successful frozen-wheelhouse capture before baselines.

Without this complete contract, dependency entries are excluded while safe
Python script-entry reduction remains available. The CLI explains the missing
requirements and evidence records the limitation. There is no trust-based
`IsolatedPython` shortcut.

Additional repeatable `--python-extra NAME` values request project extras such
as `test`. Values use the Python extra-name grammar and are canonicalized.

An optional `--prepare-spec FILE` accepts a versioned JSON document containing
shell-free argv arrays. It supports only these placeholders:

```text
{python} {candidate} {wheelhouse}
```

No shell interpolation occurs. The complete spec bytes and canonical expanded
argv are bound to the preparation digest. The spec is explicit user authority;
ReproCut does not claim arbitrary commands are an OS network sandbox.

### 2.2 Frozen wheelhouse

At session start, ReproCut scans regular non-symlink `.whl` files only, rejects
unsafe names and all other entries, and copies them into an owned temporary
wheelhouse. The digest includes sorted names, byte lengths, and contents.

Resume captures the supplied wheelhouse again and requires the same digest.
Candidates never read the caller-owned wheelhouse after capture.

### 2.3 Per-candidate environment

Every baseline, file candidate, structured candidate, and final verification in
isolated Python mode performs the same sequence:

1. materialize the frozen project snapshot;
2. create a fresh venv from the explicit interpreter without
   `--system-site-packages`;
3. invoke the venv interpreter's pip with
   `--disable-pip-version-check --no-input --no-index --find-links` against the
   frozen wheelhouse;
4. install `.` or `.[extra1,extra2]`;
5. run optional shell-free preparation argv;
6. run the oracle command with the venv `bin`/`Scripts` directory first in
   `PATH`.

The child environment sets `VIRTUAL_ENV`, `PYTHONNOUSERSITE=1`,
`PIP_NO_INDEX=1`, and the frozen `PIP_FIND_LINKS`; it removes `PYTHONHOME`,
`PYTHONPATH`, index URLs, extra index URLs, and user-site overrides.

Python-like command names resolve to the venv interpreter or its scripts.
Absolute Python/test-runner paths outside the candidate venv are rejected in
isolated mode. This prevents a host interpreter from reintroducing ambient
packages.

Preparation failure rejects the candidate; timeout or runner failure is
inconclusive. A baseline preparation failure aborts the session.

### 2.4 Preparation identity

The preparation digest includes:

- canonical base-interpreter path, version, and implementation;
- frozen wheelhouse digest;
- extras;
- prepare-spec schema and bytes;
- environment policy version;
- install argv and timeout/capture limits.

It is included in session/resume identity, candidate cache keys, final evidence,
and fingerprint hashing.

## 3. Immutable metadata-aware snapshots

### 3.1 Capture boundary

`ProjectInventory` remains a sorted path index. Immediately after inventory,
the engine captures one full `ProjectSnapshot`; source digest and measurements
come from this snapshot. No baseline or candidate reads project bytes from the
live root.

For each source file, capture records metadata before and after reading. A
change in size, modification time, file type, or executable mask aborts with a
dedicated `SourceDrift` error. After all files are read, the path inventory is
rescanned and must match. Once capture succeeds, later checkout edits are
irrelevant because every phase uses the owned snapshot.

File-level reduction creates snapshot subsets by stable unit ID. Manifest and
syntax operations remain copy-on-write snapshot transformations. Final
publication copies the verified final snapshot only.

### 3.2 Executable metadata

Each `SnapshotFile` stores a platform-neutral three-bit Unix executable mask:
owner, group, and other. Non-Unix capture uses zero.

- Snapshot digest schema advances and includes the executable mask.
- Subsets and byte-range replacements preserve the existing mask.
- Prepared-file capture reads the prepared file's resulting mask.
- Materialization writes bytes, then on Unix replaces only the three executable
  permission bits with the captured mask; current read/write bits and umask
  behavior remain intact.
- Non-Unix materialization is a documented no-op for the mask.

This preserves `0100`, `0010`, and `0001` distinctions rather than turning every
executable into `0111`.

## 4. Session and cache integrity

Session contract schema advances from 1 to 2. Its digest includes:

- frozen source snapshot digest including executable metadata;
- exact command argv;
- oracle mode, channel, patterns, normalization schema, and policy;
- preparation digest;
- ecosystem, inventory policy, timeout, capture budget, and engine version.

Candidate cache keys include the candidate snapshot digest, oracle-spec digest,
and preparation digest. A state database created under any older contract is
rejected with an actionable incompatibility message; it is never migrated or
silently reused.

## 5. Failure handling

Fail closed in all ambiguous states:

- no discriminative automatic anchor;
- invalid or baseline-mismatching regex;
- baseline source drift;
- incomplete Python isolation configuration;
- changed wheelhouse on resume;
- preparation timeout/failure;
- truncated diagnostic in automatic/regex mode;
- permission restoration failure;
- final verification disagreement.

None of these states becomes `Preserved` or publishes an artifact.

## 6. Verification matrix

### Oracle adversarial contracts

- same separator, different exception -> rejected;
- same pytest summary, different failing test -> rejected;
- same exit code, different compiler code -> rejected;
- changed semantic assertion numbers -> rejected;
- changed PID/temp root/port/duration/location with same root -> preserved;
- shorter stack trace with same root exception -> preserved;
- punctuation-only baseline -> oracle construction error;
- automatic reject regex veto;
- regex requires every pattern and rejects any veto pattern;
- invalid, oversized, and baseline-mismatching patterns fail before search;
- exit-zero success/nonzero/timeout semantics.

Rust core and Python fallback share literal fixtures and must emit equal
fingerprint dictionaries. Native-wheel CI reruns the same parity corpus.

### Python isolation contracts

- a package installed on the host is invisible in the candidate venv;
- deleting a required dependency causes candidate preparation/test rejection;
- deleting a genuinely unused dependency remains preservable;
- every candidate receives a new venv;
- no index URL is consulted and only the frozen wheelhouse is used;
- absolute host Python/test-runner commands are rejected;
- wheelhouse or prepare-spec change breaks resume compatibility;
- Windows and Unix venv executable layouts both pass.

The integration fixture uses tiny project-owned wheels checked into test assets,
so CI requires no registry/network access.

### Snapshot contracts

- changing the live source after capture cannot affect a candidate;
- path-set or metadata drift during capture aborts;
- snapshot digest changes when executable mask changes;
- Unix owner/group/other executable masks survive file, manifest, syntax, final
  verification, and publication stages;
- byte transformations preserve metadata;
- Windows behavior remains deterministic with a zero executable mask.

### Release gates

Run and require:

- format, Clippy `-D warnings`, workspace/all-target tests, docs;
- Python 3.9-3.13 fallback/native parity;
- Linux, Windows, and macOS CLI/integration tests;
- Loom, Miri, sanitizer, supply-chain, OCI, editor, gallery, archive, and package
  gates already required by 0.1;
- new `oracle-adversarial`, `python-isolation`, and `snapshot-integrity` evidence
  gates in `scripts/release/audit.py`.

The checked-in demo, report, GIF fingerprint comment, README, launch copy, and
release evidence are regenerated under evidence schema 3. The project makes no
0.1 release or same-failure reliability claim from an older artifact.

## 7. Implementation boundaries

Keep responsibilities isolated:

- `reprocut-core`: oracle specification, normalization, discriminator selection,
  fingerprint, verdict semantics;
- `reprocut-workspace`: immutable capture, subsets, executable metadata,
  materialization;
- `reprocut-runner`: explicit child environment and environment removal;
- `reprocut-engine`: phase orchestration, Python preparation, cache/session
  binding;
- `reprocut-adapters`: manifest capabilities only;
- `reprocut-cli`, protocol, Python: validated configuration surfaces;
- `reprocut-report`: schema-3 evidence rendering without a second truth model.

No probabilistic similarity, embedding, LLM classification, shell-string
execution, implicit online install, live source reread, or best-effort permission
restoration enters 0.1.

## References

- Python `venv`: <https://docs.python.org/3/library/venv.html>
- pip offline sources: <https://pip.pypa.io/en/stable/cli/pip_install/>
- Rust filesystem copy permissions: <https://doc.rust-lang.org/std/fs/fn.copy.html>
- Rust regex sets: <https://docs.rs/regex/latest/regex/struct.RegexSet.html>
