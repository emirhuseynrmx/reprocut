# ReproCut 0.1 Integrity Hardening Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship ReproCut `0.1.0` with deterministic failure identity, explicit regex and exit-zero interestingness, immutable metadata-aware source snapshots, and genuinely isolated offline Python dependency reduction.

**Architecture:** `reprocut-core` owns the validated oracle contract and mode-aware fingerprint; `reprocut-workspace` captures a single immutable source truth including executable metadata; `reprocut-runner` owns explicit child environments; and `reprocut-engine` binds all three plus a frozen Python preparation contract into session/cache identity. CLI, protocol, Python, reports, demos, and release gates serialize the same canonical model without reimplementing verdict logic.

**Tech Stack:** Rust 1.85, `regex`, `serde`, `sha2`, `tempfile`, `rusqlite`, PyO3 0.29, Python 3.9-3.13, pytest, offline pip/venv, GitHub Actions, Loom, Miri, sanitizers.

## Global Constraints

- Keep every package and publication surface at exactly `0.1.0`; do not introduce `0.2`, an RC, or a compatibility path for the unreleased unsafe contracts.
- Keep `unsafe_code = "forbid"`, Clippy `all` and `pedantic` at deny, and MSRV `1.85`.
- Fail closed for incomplete observations, weak automatic anchors, invalid/baseline-mismatching regexes, source drift, preparation failure, permission restoration failure, and final verification disagreement.
- Use deterministic exact matching only: no similarity score, embedding, LLM classification, probabilistic oracle, or fallback to exit state alone.
- Never execute shell strings, perform an implicit online install, reread live source bytes after snapshot capture, or silently restore permissions best-effort.
- Automatic/regex patterns are bounded to 16 required plus 16 reject expressions, with each expression at most 4096 UTF-8 bytes.
- Normalization schema is exactly `2`, evidence schema is exactly `3`, and session contract schema is exactly `2`.
- Isolated Python dependency reduction requires an explicit interpreter, a frozen regular-file-only `.whl` wheelhouse, fresh per-run venvs, `pip --no-index`, and scrubbed ambient Python/index variables.
- Snapshot executable metadata is the exact Unix owner/group/other three-bit execute mask; non-Unix capture and restore use zero deterministically.
- Actual crates.io and PyPI publication stays with the user; implementation may prepare and audit artifacts but must not publish, push, or tag.
- Native Rust executables are blocked locally by Windows Application Control; use `scripts/playground_workspace_verify.py`, `scripts/playground_rustfmt.py`, and CI-compatible fixtures for Rust verification.

## File Structure

- `crates/reprocut-core/src/oracle.rs`: validated `OracleSpec`, construction, mode-specific classification, and stable discriminator selection.
- `crates/reprocut-core/src/diagnostic.rs`: schema-2 newline normalization, contextual volatility rules, boilerplate rejection, and line categorization/scoring.
- `crates/reprocut-core/src/model.rs`: `OracleMode`, mode-aware `FailureFingerprint`, and its canonical digest.
- `crates/reprocut-workspace/src/lib.rs`: inventory API and re-exports; no candidate path may copy from the live root.
- `crates/reprocut-workspace/src/snapshot.rs`: drift-checked capture, subset construction, copy-on-write transformations, executable-mask digest and restoration.
- `crates/reprocut-runner/src/lib.rs`: immutable `ChildEnvironment` and `CommandSpec` environment application.
- `crates/reprocut-engine/src/python_isolation.rs`: frozen wheelhouse, interpreter identity, prepare-spec validation, fresh venv construction, and command resolution.
- `crates/reprocut-engine/src/lib.rs`: snapshot-first phase orchestration, preparation/oracle/session binding, classification, final verification, and outcome fields.
- `crates/reprocut-engine/src/pipeline.rs`: Python manifest candidates gated by a complete isolation contract rather than a trust flag.
- `crates/reprocut-state/src/lib.rs`: session-contract schema 2 and exact compatibility failures.
- `crates/reprocut-core/src/protocol.rs`, `crates/reprocut-cli/src/main.rs`, `python/reprocut/client.py`, `python/reprocut/cli.py`: validated user configuration surfaces.
- `crates/reprocut-python/src/lib.rs`, `python/reprocut/_fallback.py`, `python/reprocut/_native.pyi`: native/fallback oracle parity.
- `crates/reprocut-report/src/evidence.rs`, `crates/reprocut-report/src/lib.rs`, `crates/reprocut-report/src/issue.rs`: evidence schema 3 and mode-aware rendering.
- `scripts/build_demo.py`, `scripts/capture_demo.py`, `scripts/benchmark_release.py`, `scripts/release/audit.py`: regenerated proof and release gates.

---

### Task 1: Schema-2 Diagnostic Normalization and Discriminator Selection

**Files:**
- Create: `crates/reprocut-core/src/diagnostic.rs`
- Modify: `crates/reprocut-core/src/lib.rs`
- Modify: `crates/reprocut-core/src/oracle.rs`
- Modify: `crates/reprocut-core/Cargo.toml`
- Test: `crates/reprocut-core/tests/oracle_adversarial.rs`
- Test: `crates/reprocut-core/tests/oracle_properties.rs`
- Test: `scripts/verification/oracle_v2_contract.rs`

**Interfaces:**
- Produces: `pub const NORMALIZATION_SCHEMA: u16 = 2`.
- Produces: `pub fn normalize_diagnostic(input: &str) -> String`.
- Produces internally: `stable_discriminators(channel: DiagnosticChannel, streams: &[&[u8]]) -> Result<Vec<DiagnosticAnchor>, OracleError>`.
- Consumes: existing `DiagnosticAnchor`, `DiagnosticChannel`, and `OracleError`.

- [x] **Step 1: Add adversarial RED fixtures for false-positive prevention**

```rust
#[test]
fn separator_cannot_hide_a_changed_exception() {
    let separator = "-".repeat(80);
    let oracle = FailureOracle::from_baselines(&[
        failed(&format!("{separator}\nValueError: invoice 123")),
        failed(&format!("{separator}\nValueError: invoice 123")),
    ]).expect("discriminative root");
    assert_eq!(oracle.classify(&failed(&format!("{separator}\nKeyError: invoice 123"))), CandidateVerdict::Rejected);
}

#[test]
fn semantic_assertion_numbers_are_not_normalized() {
    let oracle = FailureOracle::from_baselines(&[
        failed("AssertionError: left 123 right 456"),
        failed("AssertionError: left 123 right 456"),
    ]).expect("assertion identity");
    assert_eq!(oracle.classify(&failed("AssertionError: left 999 right 777")), CandidateVerdict::Rejected);
}
```

Also add literal cases for changed pytest node ID, changed Rust compiler code, punctuation-only baselines, stable root with shorter stack, and contextual PID/temp path/port/duration/location changes.

- [x] **Step 2: Run the focused RED contract through the existing Rust verification path**

Run: `python scripts/playground_workspace_verify.py --scope core --append scripts/verification/oracle_adversarial_contract.rs`

Expected: FAIL because `diagnostic.rs`, schema 2, and discriminator intersection do not exist.

- [x] **Step 3: Implement context-qualified normalization in `diagnostic.rs`**

Use compiled `OnceLock<Regex>` rules for newline/space canonicalization, candidate and conventional temporary roots, UUID/ISO timestamp, contextual pointer/PID/thread/port/duration/location tokens. Preserve unqualified decimals, semantic short hex, relative paths, test names, status codes, shapes, amounts, and versions. Export `normalize_diagnostic` from `lib.rs` and remove the blanket `[0-9]+` and all-absolute-path replacement logic from `oracle.rs`.

- [x] **Step 4: Implement deterministic boilerplate filtering and categories**

Define private `DiscriminatorKind::{FailingTest, CompilerDiagnostic, RootFailure, Assertion, Message}` and an `EligibleLine { channel, text, kind, score, baseline_position }`. Reject punctuation-only separators, summary-only pass/fail counts, heading-only traceback/backtrace text, location-only frames, and generic lifecycle exits. Sort by kind priority, descending score, baseline position, then lexical text; retain at most four lines while taking each available category before a duplicate category.

- [x] **Step 5: Replace whole-stream stability with exact eligible-line intersection**

For every selected stream, normalize each baseline independently, build eligible-line sets, intersect exact lines across all baselines, and produce anchors from that intersection. `Auto` includes only streams containing recognized error-bearing lines; `Combined` requires both streams; explicit stdout/stderr requires its selected stream. Return `EmptyAnchor` if no eligible stable discriminator exists.

- [x] **Step 6: Run the focused contract and core property suite GREEN**

Run: `python scripts/playground_workspace_verify.py --scope core --append scripts/verification/oracle_adversarial_contract.rs`

Run: `python scripts/playground_workspace_verify.py --scope core --append scripts/verification/oracle_v2_contract.rs`

Expected: PASS with changed semantic failures rejected and contextual volatility preserved.

- [x] **Step 7: Commit the normalization/discriminator unit**

```powershell
git add crates/reprocut-core scripts/verification/oracle_v2_contract.rs
git commit -m "fix(core): harden automatic failure identity"
```

### Task 2: Validated Oracle Modes and Canonical Fingerprints

**Files:**
- Modify: `crates/reprocut-core/src/model.rs`
- Modify: `crates/reprocut-core/src/oracle.rs`
- Modify: `crates/reprocut-core/src/lib.rs`
- Test: `crates/reprocut-core/tests/oracle_modes.rs`
- Test: `crates/reprocut-core/tests/model_contract.rs`
- Test: `scripts/verification/final_rust_contract.rs`

**Interfaces:**
- Produces: `pub enum OracleMode { Automatic, Regex, ExitZero }`.
- Produces: `pub struct OracleSpec` with `pub fn new(mode, channel, failure_patterns, reject_patterns) -> Result<Self, OracleError>` and read-only accessors.
- Produces: `FailureOracle::from_spec_and_baselines(spec: OracleSpec, baselines: &[ExecutionObservation]) -> Result<Self, OracleError>`.
- Produces: `FailureFingerprint::{mode, termination, anchors, failure_patterns, reject_patterns, normalization_schema, oracle_spec_digest}` and `pub fn digest(&self) -> ContentDigest`.

- [x] **Step 1: Add RED tests for configuration bounds and three verdict contracts**

```rust
#[test]
fn exit_zero_ignores_truncated_output_but_not_timeout() {
    let spec = OracleSpec::new(OracleMode::ExitZero, DiagnosticChannel::Auto, vec![], vec![]).expect("spec");
    let oracle = FailureOracle::from_spec_and_baselines(spec, &[exited(0), exited(0)]).expect("baseline");
    assert_eq!(oracle.classify(&truncated_exit(0)), CandidateVerdict::Preserved);
    assert_eq!(oracle.classify(&exited(9)), CandidateVerdict::Rejected);
    assert_eq!(oracle.classify(&timed_out()), CandidateVerdict::Inconclusive);
}

#[test]
fn reject_pattern_vetoes_required_regex() {
    let spec = OracleSpec::new(OracleMode::Regex, DiagnosticChannel::Stderr, vec!["TypeError: invoice [0-9]+".into()], vec!["secondary failure".into()]).expect("spec");
    let oracle = FailureOracle::from_spec_and_baselines(spec, &[failed("TypeError: invoice 7"), failed("TypeError: invoice 8")]).expect("baseline");
    assert_eq!(oracle.classify(&failed("TypeError: invoice 9\nsecondary failure")), CandidateVerdict::Rejected);
}
```

Add tests for invalid regex, empty regex mode, patterns in exit-zero mode, 17 patterns, 4097-byte patterns, baseline required-pattern mismatch, reject-pattern baseline match, termination mismatch, automatic reject veto, and fingerprint digest changes for every field.

- [x] **Step 2: Run the oracle-mode RED suite**

Run: `python scripts/playground_workspace_verify.py --scope core --append scripts/verification/oracle_modes_contract.rs`

Expected: FAIL because `OracleMode`, `OracleSpec`, and mode-aware fingerprint fields are absent.

- [x] **Step 3: Implement `OracleSpec` validation and canonical encoding**

Compile patterns with Rust `regex::Regex`, deduplicate and sort pattern strings lexically, enforce counts/byte lengths and mode combinations before observations are evaluated, and add precise `OracleError` variants (`InvalidConfiguration`, `InvalidPattern`, `PatternTooLong`, `TooManyPatterns`, `BaselinePatternMismatch`, `BaselineUnexpectedReject`, `ExitZeroBaselineRequired`).

- [x] **Step 4: Implement mode-specific construction and classification**

Automatic uses Task 1 anchors plus reject veto; regex uses newline-canonicalized bounded raw selected text with reject-first/all-required/termination-equal semantics; exit-zero requires baseline `ExitCode(0)`, treats candidate `ExitCode(0)` as preserved, other exit codes as rejected, and timeout/signal/runner failure as inconclusive. Automatic and regex treat truncation as inconclusive; exit-zero deliberately ignores truncation.

- [x] **Step 5: Replace compatibility fingerprint fields with one mode-aware serialized value**

Encode every fingerprint field using fixed tags plus length-delimited bytes under domain `REPROCUT-FINGERPRINT\0`; set `normalization_schema` to `2`; retain `anchor()` only as a display accessor returning the first anchor or an empty string for exit-zero. Include the canonical `OracleSpec` digest in the fingerprint digest.

- [x] **Step 6: Run oracle, model, docs, and format verification GREEN**

Run: `python scripts/playground_workspace_verify.py --scope core --append scripts/verification/oracle_modes_contract.rs`

Run: `python scripts/playground_workspace_verify.py --scope full --append scripts/verification/final_rust_contract.rs`

Run: `python scripts/playground_rustfmt.py`

Expected: PASS, including deterministic digest tests.

- [x] **Step 7: Commit oracle modes and fingerprints**

```powershell
git add crates/reprocut-core scripts/verification/final_rust_contract.rs
git commit -m "feat(core): add explicit oracle modes"
```

### Task 3: Python Fallback and Native Oracle Parity

**Files:**
- Modify: `crates/reprocut-python/src/lib.rs`
- Modify: `python/reprocut/_fallback.py`
- Modify: `python/reprocut/_native.pyi`
- Modify: `python/reprocut/__init__.py`
- Modify: `python/tests/test_oracle.py`
- Modify: `python/tests/test_native_backend.py`
- Create: `python/tests/oracle_cases.json`
- Create: `python/tests/test_oracle_parity.py`

**Interfaces:**
- Produces Python `FailureOracle.from_baselines(baselines, *, mode="automatic", channel="auto", failure_patterns=(), reject_patterns=())`.
- Produces immutable fingerprint dictionaries containing `mode`, `termination`, `anchors`, `failure_patterns`, `reject_patterns`, `normalization_schema`, `oracle_spec_sha256`, and `fingerprint_sha256`.
- Consumes literal cross-language cases from `python/tests/oracle_cases.json`.

- [x] **Step 1: Add shared RED corpus and fallback assertions**

Store cases with `name`, `mode`, `channel`, `failure_patterns`, `reject_patterns`, `baselines`, `candidate`, and `expected`. Include every adversarial case from Task 1 plus regex and exit-zero cases from Task 2. Assert exact fingerprint dictionaries, not only verdict strings.

- [x] **Step 2: Run fallback RED tests**

Run: `$env:PYTHONPATH='.test-deps;python'; & 'C:\Users\emirh\.cache\codex-runtimes\codex-primary-runtime\dependencies\python\python.exe' -m pytest python/tests/test_oracle.py python/tests/test_oracle_parity.py -q --basetemp .tmp/pytest-oracle-red`

Expected: FAIL on missing mode parameters and schema-1 fingerprint data.

- [x] **Step 3: Port the exact deterministic contract to `_fallback.py`**

Mirror Rust regex syntax only where Python `re` accepts the shared corpus, apply the same bounds, contextual normalizers, boilerplate predicates, line categories, ordering, and verdict rules. Use `hashlib.sha256` over the same tag/length-delimited encoding so fallback and native produce byte-identical hex digests.

- [x] **Step 4: Extend PyO3 and type stubs without duplicating Rust verdict logic**

Parse Python sequences into `OracleSpec`, call `FailureOracle::from_spec_and_baselines`, expose exact fingerprint fields/digests, and keep frozen object behavior. Update `_native.pyi` literals for modes and channels.

- [x] **Step 5: Run fallback suite GREEN and mark native parity as CI-required**

Run: `$env:PYTHONPATH='.test-deps;python'; & 'C:\Users\emirh\.cache\codex-runtimes\codex-primary-runtime\dependencies\python\python.exe' -m pytest python/tests/test_oracle.py python/tests/test_oracle_parity.py -q --basetemp .tmp/pytest-oracle-green`

Expected: fallback PASS; native-only test skips only when the local wheel is unavailable and remains mandatory in wheel CI.

- [x] **Step 6: Commit Python oracle parity**

```powershell
git add crates/reprocut-python python/reprocut python/tests
git commit -m "feat(python): mirror oracle v2 contract"
```

### Task 4: Immutable Metadata-Aware Source Snapshots

**Files:**
- Create: `crates/reprocut-workspace/src/snapshot.rs`
- Modify: `crates/reprocut-workspace/src/lib.rs`
- Modify: `crates/reprocut-engine/src/lib.rs`
- Test: `crates/reprocut-workspace/tests/snapshot_contract.rs`
- Create: `crates/reprocut-workspace/tests/snapshot_integrity.rs`
- Modify: `crates/reprocut-engine/tests/engine_contract.rs`
- Create: `scripts/verification/snapshot_integrity_contract.rs`

**Interfaces:**
- Produces: `ProjectSnapshot::capture(inventory: &ProjectInventory, policy: &InventoryPolicy) -> Result<Self, WorkspaceError>`.
- Produces: `ProjectSnapshot::subset<'a>(&self, units: impl IntoIterator<Item=&'a ReductionUnit>) -> Result<Self, WorkspaceError>`.
- Produces: `SnapshotFile::executable_mask() -> u8` and `ProjectSnapshot::measurements()`.
- Produces: `WorkspaceError::SourceDrift { path, reason }` and `WorkspaceError::PermissionRestore { path, source }`.

- [x] **Step 1: Add RED snapshot integrity tests**

```rust
#[test]
fn live_source_changes_do_not_change_a_snapshot_subset() {
    let inventory = ProjectInventory::scan(root.path()).expect("inventory");
    let frozen = ProjectSnapshot::capture(&inventory, &InventoryPolicy::source_only()).expect("capture");
    fs::write(root.path().join("src/lib.rs"), b"changed live bytes").expect("mutate live source");
    let subset = frozen.subset(inventory.units()).expect("subset");
    assert_eq!(subset.file("src/lib.rs"), Some(&b"original bytes"[..]));
}
```

On Unix add exact `0o100`, `0o010`, and `0o001` cases proving distinct snapshot digests and preservation through subset, `with_file_contents`, structured transformation, prepared capture, materialization, and final publication. Add deterministic zero-mask assertions on non-Unix.

- [x] **Step 2: Run snapshot RED verification**

Run: `python scripts/playground_workspace_verify.py --scope workspace --append scripts/verification/snapshot_integrity_contract.rs`

Expected: FAIL because capture rereads live bytes, transformations discard metadata, and digest omits execute masks.

- [x] **Step 3: Implement drift-checked full capture**

For each inventory unit, capture file type, length, modification time, and three-bit execute mask before read; read bytes once; capture metadata again; reject any difference. After all reads, rescan with the same inventory policy and require the same sorted paths. Build source digest and measurements exclusively from captured files.

- [x] **Step 4: Move all snapshot transformations to metadata-preserving constructors**

`SnapshotFile::with_contents` must preserve `executable_mask`; new prepared files capture their mask; replacements and subsets share unchanged `Arc<[u8]>` values. Hash domain `REPROCUT-SNAPSHOT-V2\0`, path, byte length, bytes/content digest, and execute mask.

- [x] **Step 5: Restore exact execute bits after writing bytes**

On Unix read the materialized file mode, replace only `0o111` with the stored three-bit mask, and propagate `set_permissions` errors. On non-Unix require mask zero and perform no permission mutation.

- [x] **Step 6: Remove engine live-source digest and file materialization paths**

At engine entry perform inventory scan followed immediately by `ProjectSnapshot::capture`; use snapshot digest/measurements for session identity; form every file-level candidate via `source_snapshot.subset(kept)` and `CandidateWorkspace::materialize_snapshot`; never call `CandidateWorkspace::materialize` or `inventory_digest` during a reduction; publish only the final verified snapshot.

- [x] **Step 7: Run snapshot/workspace/engine GREEN verification**

Run: `python scripts/playground_workspace_verify.py --scope workspace --append scripts/verification/snapshot_integrity_contract.rs`

Run: `python scripts/playground_workspace_verify.py --scope workspace --append scripts/verification/snapshot_contract.rs`

Expected: PASS with no live source reads after capture.

- [x] **Step 8: Commit immutable snapshots**

```powershell
git add crates/reprocut-workspace crates/reprocut-engine scripts/verification/snapshot_integrity_contract.rs
git commit -m "fix(workspace): freeze source bytes and executable metadata"
```

### Task 5: Explicit Runner Environment Policy

**Files:**
- Modify: `crates/reprocut-runner/src/lib.rs`
- Modify: `crates/reprocut-runner/tests/runner_contract.rs`
- Create: `scripts/verification/runner_environment_contract.rs`

**Interfaces:**
- Produces: `pub struct ChildEnvironment { clear: bool, set: BTreeMap<OsString, OsString>, remove: BTreeSet<OsString>, path_prepend: Vec<PathBuf> }`.
- Produces: `CommandSpec::with_environment(self, environment: ChildEnvironment) -> Self`.
- Produces: immutable accessors used in engine contract hashing.

- [x] **Step 1: Add RED tests for set/remove/PATH behavior**

Run a child helper that prints selected variables. Assert removed `PYTHONPATH`/`PIP_INDEX_URL` are absent, explicit `VIRTUAL_ENV` and `PIP_NO_INDEX=1` are present, and the requested venv directory is the first PATH entry without losing the remaining platform PATH.

- [x] **Step 2: Run runner RED verification**

Run: `python scripts/playground_workspace_verify.py --scope engine --append scripts/verification/runner_environment_contract.rs`

Expected: FAIL because `CommandSpec` cannot describe environment removal.

- [x] **Step 3: Implement immutable environment application**

Apply `env_clear` only when requested; otherwise call `env_remove` for every removal before `envs` for every explicit setting. Construct PATH with `std::env::join_paths`, return a typed `RunnerError::InvalidEnvironment` for invalid entries, and never invoke a shell.

- [x] **Step 4: Run runner contracts GREEN**

Run: `python scripts/playground_workspace_verify.py --scope engine --append scripts/verification/runner_environment_contract.rs`

Run: `python scripts/playground_workspace_verify.py --scope engine --append scripts/verification/process_group_contract.rs`

Expected: PASS without regressing descendant teardown.

- [x] **Step 5: Commit runner environment policy**

```powershell
git add crates/reprocut-runner scripts/verification/runner_environment_contract.rs
git commit -m "feat(runner): make child environments explicit"
```

### Task 6: Frozen Offline Python Preparation Contract

**Files:**
- Create: `crates/reprocut-engine/src/python_isolation.rs`
- Modify: `crates/reprocut-engine/src/lib.rs`
- Modify: `crates/reprocut-engine/src/pipeline.rs`
- Modify: `crates/reprocut-engine/Cargo.toml`
- Create: `crates/reprocut-engine/tests/python_isolation_contract.rs`
- Create: `tests/fixtures/python_isolation/wheels/build_fixture.py`
- Create: `tests/fixtures/python_isolation/project/pyproject.toml`
- Create: `tests/fixtures/python_isolation/project/src/reprocut_fixture/__init__.py`
- Create: `tests/fixtures/python_isolation/project/tests/test_failure.py`
- Create: `scripts/verification/python_isolation_contract.rs`

**Interfaces:**
- Produces: `pub struct PythonIsolationRequest { interpreter: PathBuf, wheelhouse: PathBuf, extras: Vec<String>, prepare_spec: Option<PathBuf> }`.
- Produces internally: `FrozenPythonPreparation::capture(request: &PythonIsolationRequest) -> Result<Self, PythonPreparationError>`.
- Produces: `FrozenPythonPreparation::{digest, prepare(snapshot), command_for(candidate_root, original_program, arguments)}`.
- Consumes: Task 5 `ChildEnvironment` and Task 4 snapshots.

- [x] **Step 1: Build tiny project-owned wheel fixtures without registry access**

The fixture contains two local packages, `required_dep` and `unused_dep`, each built into a deterministic wheel and committed under `tests/fixtures/python_isolation/wheels/`. The failing project imports `required_dep`, declares both dependencies, and exits with a stable exception. The build script exists only to reproduce the checked-in wheels; tests consume the committed wheel bytes and never contact an index.

- [x] **Step 2: Add RED integration contracts**

Assert: host-only modules are invisible; every baseline/candidate/final run has a distinct `sys.prefix`; deleting `required_dep` rejects during preparation/test; deleting `unused_dep` can be preserved; `PIP_INDEX_URL`/`PYTHONPATH` are scrubbed; only the frozen wheelhouse is used; absolute host Python/pytest commands are rejected; non-wheel, symlink, traversal-like, duplicate-case, or changed wheelhouse entries fail closed; changed wheelhouse/prepare-spec changes preparation digest.

- [x] **Step 3: Run Python isolation RED tests**

Run: `python scripts/playground_workspace_verify.py --scope engine --append scripts/verification/python_isolation_contract.rs`

Expected: FAIL because `IsolatedPython` currently trusts the caller environment and has no preparation contract.

- [x] **Step 4: Implement frozen wheelhouse capture**

Canonicalize the caller path once, enumerate regular non-symlink `.whl` files only, validate basename safety and case-insensitive uniqueness, copy into an owned temp directory, then hash sorted filename, byte length, and bytes under `REPROCUT-WHEELHOUSE-V1\0`. Retain the owned directory for the session lifetime; candidates never read the caller directory.

- [x] **Step 5: Implement interpreter and prepare-spec validation**

Run the explicit interpreter once with a fixed `-c` probe returning implementation/version/executable JSON. Validate extras with normalized Python extra-name grammar. Deserialize schema-1 prepare spec with `deny_unknown_fields`, argv arrays only, and only `{python}`, `{candidate}`, `{wheelhouse}` placeholders; reject empty argv, shell strings, and unknown placeholders.

- [x] **Step 6: Implement fresh candidate-local venv preparation**

For every run invoke explicit interpreter `-m venv <owned-venv>` without system site packages; use venv Python `-m pip install --disable-pip-version-check --no-input --no-index --find-links <frozen-wheelhouse> .[extras]`; run optional expanded argv; construct Task 5 environment with `VIRTUAL_ENV`, `PYTHONNOUSERSITE=1`, `PIP_NO_INDEX=1`, `PIP_FIND_LINKS`, and venv PATH first while removing ambient Python/index/user-site variables.

- [x] **Step 7: Resolve candidate command safely**

Map `python`, `python3`, and platform variants to venv Python; resolve pytest and other relative tool names only inside the venv Scripts/bin directory; reject absolute Python/test-runner paths outside the candidate venv. Preserve non-Python project-owned relative executables only when they resolve under candidate root.

- [x] **Step 8: Bind complete preparation identity**

Hash canonical interpreter path/identity, frozen wheelhouse digest, sorted extras, prepare-spec bytes/expanded argv, environment-policy version, install argv, timeout, and capture limit under `REPROCUT-PYTHON-PREP-V1\0`. Expose digest and a non-secret evidence description; never serialize temporary owned paths as identity.

- [x] **Step 9: Run isolation integration GREEN**

Run: `python scripts/playground_workspace_verify.py --scope engine --append scripts/verification/python_isolation_contract.rs`

Expected: PASS entirely offline on Windows/Unix path-layout fixtures.

- [x] **Step 10: Commit Python isolation**

```powershell
git add crates/reprocut-engine tests/fixtures/python_isolation scripts/verification/python_isolation_contract.rs
git commit -m "feat(engine): isolate Python candidates offline"
```

### Task 7: Engine, Cache, and Resume Integrity

**Files:**
- Modify: `crates/reprocut-engine/src/lib.rs`
- Modify: `crates/reprocut-engine/src/pipeline.rs`
- Modify: `crates/reprocut-state/src/lib.rs`
- Modify: `crates/reprocut-state/src/schema.rs`
- Create: `crates/reprocut-state/migrations/0003.sql`
- Modify: `crates/reprocut-engine/tests/engine_contract.rs`
- Modify: `crates/reprocut-state/tests/state_contract.rs`
- Create: `scripts/verification/session_integrity_contract.rs`

**Interfaces:**
- Produces: `ReductionRequest::with_oracle(self, oracle_spec: OracleSpec) -> Self`.
- Produces: `ReductionRequest::with_python_isolation(self, request: PythonIsolationRequest) -> Self`.
- Produces: `SessionContract::new_v2(source_snapshot_digest, command_digest, oracle_spec_digest, preparation_digest, policy_digest, adapter_version, engine_version)`.
- Produces: cache key `SHA256(candidate_snapshot_digest || oracle_spec_digest || preparation_digest)` with domain separation.

- [x] **Step 1: Add RED session/cache tests**

Create two otherwise identical contracts that differ in each of: source execute mask, command argv boundary, oracle mode/channel/pattern, normalization schema, preparation digest, ecosystem, inventory exclusion, timeout, capture budget, and evaluation policy. Assert distinct digests and refuse resume from schema 1 with an actionable incompatibility error. Assert candidate cache misses when oracle/preparation identity changes.

- [x] **Step 2: Run session RED verification**

Run: `python scripts/playground_workspace_verify.py --scope engine --append scripts/verification/session_integrity_contract.rs`

Expected: FAIL because current session identity omits most fields and uses normalization schema 1.

- [x] **Step 3: Implement session contract schema 2**

Persist explicit `contract_schema = 2`; encode every field with domain tags and length prefixes; add migration 0003 only to store the schema marker for new sessions, not to migrate old identity. `resume` must reject any older contract with: `state contract schema 1 is incompatible with ReproCut 0.1 integrity schema 2; restart explicitly`.

- [x] **Step 4: Bind oracle and preparation before opening state**

Capture source snapshot, validate/capture Python preparation when requested, construct/validate `OracleSpec`, then build/open session state. No child process starts before all static contracts succeed. Incomplete isolated-Python configuration disables dependency candidates only when isolation was not requested; an explicitly requested incomplete contract returns a configuration error.

- [x] **Step 5: Apply identical preparation to every phase**

Use one engine helper for baseline, file candidate, structured candidate, final verification, and publication verification: materialize snapshot, prepare fresh environment, resolve command, execute, classify. Baseline preparation failure aborts; candidate preparation failure rejects; preparation timeout/runner failure is inconclusive.

- [x] **Step 6: Replace cache identity and pipeline gating**

Use candidate snapshot + oracle spec + preparation digest, not transformation description or live inventory. Enable Python dependency manifest candidates only when `FrozenPythonPreparation` exists; keep safe script-entry reduction available without it and expose a limitation explaining why dependency entries were skipped.

- [x] **Step 7: Run state, pipeline, and engine GREEN verification**

Run: `python scripts/playground_workspace_verify.py --scope engine --append scripts/verification/session_integrity_contract.rs`

Run: `python scripts/playground_workspace_verify.py --scope pipeline --append scripts/verification/pipeline_contract.rs`

Run: `python scripts/playground_workspace_verify.py --scope engine --append scripts/verification/engine_compile_contract.rs`

Expected: PASS with exact resume refusal and phase parity.

- [x] **Step 8: Commit engine/session integrity**

```powershell
git add crates/reprocut-engine crates/reprocut-state scripts/verification/session_integrity_contract.rs
git commit -m "fix(engine): bind candidates to complete session identity"
```

### Task 8: CLI, Protocol, and Typed Python Request Surfaces

**Files:**
- Modify: `crates/reprocut-core/src/protocol.rs`
- Modify: `crates/reprocut-cli/src/main.rs`
- Modify: `crates/reprocut-cli/tests/cli_contract.rs`
- Modify: `python/reprocut/client.py`
- Modify: `python/reprocut/cli.py`
- Modify: `python/tests/test_client.py`
- Modify: `scripts/verification/cli_contract.rs`
- Modify: `scripts/verification/cli_compile_contract.rs`

**Interfaces:**
- CLI flags: `--oracle-mode`, `--oracle-stream`, repeatable `--failure-regex`, repeatable `--reject-regex`, `--python-executable`, `--python-wheelhouse`, repeatable `--python-extra`, and `--prepare-spec`.
- Protocol V1 additive fields: `oracle_mode`, `failure_patterns`, `reject_patterns`, `python_executable`, `python_wheelhouse`, `python_extras`, and `prepare_spec`.
- Python `ReductionRequest` exposes the same typed fields and validations.

- [x] **Step 1: Add RED parser/protocol tests for valid and invalid combinations**

Assert `--oracle-mode exit-zero -- <command>` parses; regex requires at least one `--failure-regex`; exit-zero rejects regex flags; `--prepare isolated-python` requires interpreter and wheelhouse; Python extras normalize/deduplicate; protocol denies unknown fields and rejects the same invalid combinations before engine execution.

- [x] **Step 2: Run CLI and Python client RED tests**

Run: `python scripts/playground_workspace_verify.py --scope full --append scripts/verification/cli_contract.rs`

Run: `$env:PYTHONPATH='.test-deps;python'; & 'C:\Users\emirh\.cache\codex-runtimes\codex-primary-runtime\dependencies\python\python.exe' -m pytest python/tests/test_client.py -q --basetemp .tmp/pytest-client-red`

Expected: FAIL on absent fields and obsolete “trusts your command environment” semantics.

- [x] **Step 3: Add Clap values and central conversion validation**

Introduce `OracleModeArg`; make pattern flags repeatable; add isolation paths/extras/spec; convert CLI and protocol through one `validated_request` function that constructs `OracleSpec` and `PythonIsolationRequest`. Replace help text with the explicit offline isolation requirements and exit-zero semantics.

- [x] **Step 4: Extend additive protocol V1 and Python serialization**

Default omitted `oracle_mode` to `automatic`, pattern arrays to empty, and isolation fields to `None`/empty. Validate field combinations in Rust `ReductionRequestV1::validate` and Python `ReductionRequest.__post_init__`; serialize canonical values without changing `PROTOCOL_VERSION = 1`.

- [x] **Step 5: Run CLI/client/protocol GREEN verification**

Run: `python scripts/playground_workspace_verify.py --scope full --append scripts/verification/cli_contract.rs`

Run: `$env:PYTHONPATH='.test-deps;python'; & 'C:\Users\emirh\.cache\codex-runtimes\codex-primary-runtime\dependencies\python\python.exe' -m pytest python/tests/test_client.py -q --basetemp .tmp/pytest-client-green`

Expected: PASS with identical early validation in both entry points.

- [x] **Step 6: Commit public configuration surfaces**

```powershell
git add crates/reprocut-core/src/protocol.rs crates/reprocut-cli python/reprocut python/tests/test_client.py scripts/verification/cli_contract.rs scripts/verification/cli_compile_contract.rs
git commit -m "feat(cli): expose validated integrity contracts"
```

### Task 9: Evidence Schema 3 and Mode-Aware Reports

**Files:**
- Modify: `crates/reprocut-report/src/evidence.rs`
- Modify: `crates/reprocut-report/src/lib.rs`
- Modify: `crates/reprocut-report/src/issue.rs`
- Modify: `crates/reprocut-report/assets/report.html`
- Modify: `crates/reprocut-report/assets/report.js`
- Modify: `crates/reprocut-report/tests/evidence_contract.rs`
- Modify: `crates/reprocut-report/tests/issue_contract.rs`
- Modify: `crates/reprocut-report/tests/report_golden.rs`
- Modify: `crates/reprocut-cli/src/main.rs`
- Modify: `python/reprocut/client.py`
- Modify: `tests/golden/protocol-events.jsonl`
- Modify: `tests/golden/reduction-report.html`

**Interfaces:**
- Produces `EVIDENCE_SCHEMA_VERSION: u16 = 3`.
- Produces `FailureEvidence` fields `oracle_mode`, `normalization_schema`, `anchors`, `failure_patterns`, `reject_patterns`, `oracle_spec_sha256`, and `fingerprint_sha256`.
- Produces top-level `source_snapshot_sha256: String` and `preparation: PreparationEvidence { mode, contract_sha256: Option<String>, limitations }`.

- [x] **Step 1: Add RED evidence round-trip and golden assertions**

Assert schema 3 rejects missing/invalid 64-character lowercase digests, exit-zero evidence has no anchors/patterns, regex evidence carries patterns verbatim, automatic evidence carries schema-2 anchors, and preparation/source digests displayed in HTML and issue Markdown match JSON exactly.

- [x] **Step 2: Run report RED verification**

Run: `python scripts/playground_workspace_verify.py --scope report --append scripts/verification/report_contract.rs`

Expected: FAIL because evidence is schema 2 and reports assume one textual anchor.

- [x] **Step 3: Make schema-3 evidence the single publication model**

Populate all fields from `ReductionOutcome`/`FailureFingerprint`/frozen preparation; validate mode-specific invariants in `ReductionEvidence::validate`; update Python client `_load_evidence` to require schema 3 and same-failure/digest agreement.

- [x] **Step 4: Render mode-aware HTML and issue evidence**

Display “Automatic discriminators”, “Required/reject regex”, or “Exit-zero interestingness” according to mode. Show source snapshot and preparation contract hashes, normalization schema, phase limitations, before/after measurements, and final verification count. Do not claim fuzzy similarity or Python isolation when unavailable.

- [x] **Step 5: Regenerate and verify deterministic goldens**

Run: `python scripts/playground_workspace_verify.py --scope report --append scripts/verification/report_golden_contract.rs`

Run: `& 'C:\Users\emirh\.cache\codex-runtimes\codex-primary-runtime\dependencies\node\bin\node.exe' scripts/verification/report_browser.cjs`

Expected: PASS with schema-3 fixture and no unstamped second truth model.

- [x] **Step 6: Commit evidence schema 3**

```powershell
git add crates/reprocut-report crates/reprocut-cli/src/main.rs python/reprocut/client.py tests/golden
git commit -m "feat(report): publish integrity evidence schema 3"
```

### Task 10: Demo, Documentation, and Release Gates

**Files:**
- Modify: `scripts/build_demo.py`
- Modify: `scripts/capture_demo.py`
- Modify: `scripts/benchmark_release.py`
- Modify: `scripts/release/audit.py`
- Modify: `python/tests/test_demo_builder.py`
- Modify: `python/tests/test_demo_assets.py`
- Modify: `python/tests/test_benchmark_release.py`
- Modify: `python/tests/test_release_audit.py`
- Modify: `.github/workflows/ci.yml`
- Modify: `.github/workflows/release.yml`
- Modify: `README.md`
- Modify: `docs/RELEASING.md`
- Modify: `docs/release/0.1.0.md`
- Modify: `docs/launch/DEMO_SCRIPT.md`
- Modify: `docs/launch/HACKER_NEWS.md`
- Regenerate: `demo/result/reduction.json`
- Regenerate: `demo/result/report.html`
- Regenerate: `demo/result/issue.md`
- Regenerate: `demo/result/attempts.jsonl`
- Regenerate: checked-in demo animation metadata/fingerprint comment used by repository assets.

**Interfaces:**
- Produces release-audit gates named exactly `oracle-adversarial`, `python-isolation`, and `snapshot-integrity`.
- Produces demo evidence schema 3 generated from the same core/fallback fixtures.

- [x] **Step 1: Add RED release-audit and demo assertions**

Require schema 3, normalization schema 2, valid source snapshot digest, mode-aware fingerprint, preparation digest or explicit limitation, final verification `3`, and all three new CI gate names. Reject stale schema-2 demo artifacts and any README/release claim based on them.

- [x] **Step 2: Run Python release RED suite**

Run: `$env:PYTHONPATH='.test-deps;python'; & 'C:\Users\emirh\.cache\codex-runtimes\codex-primary-runtime\dependencies\python\python.exe' -m pytest python/tests/test_demo_builder.py python/tests/test_demo_assets.py python/tests/test_release_audit.py python/tests/test_benchmark_release.py -q --basetemp .tmp/pytest-release-red`

Expected: FAIL because checked-in proof uses evidence schema 2 and the new gates are absent.

- [x] **Step 3: Add CI jobs and release evidence requirements**

Add cross-platform adversarial oracle tests, isolated-Python tests using committed wheels with network/index disabled, and snapshot-integrity tests including Unix execute masks. Add their stable gate names to `REQUIRED_CI_GATES`; require the exact release commit in audit evidence.

- [x] **Step 4: Regenerate demo/report/issue/animation proof under schema 3**

Update `build_demo.py` to use OracleSpec semantics and schema-3 evidence; run the builder; rebuild report/issue/attempts and asset fingerprint metadata; run `capture_demo.py` validation. Preserve the honest Playground limitation if the remote host lacks Python, while local final verification still executes the fixture three times.

- [x] **Step 5: Update user documentation with exact modes and isolation contract**

Document automatic, regex, and exit-zero examples; isolated Python wheelhouse workflow; fail-closed limitations; schema incompatibility requiring explicit restart; Unix execute preservation; crates.io/PyPI commands as user-run steps only; and the absence of a measured performance claim.

- [x] **Step 6: Run release Python suite GREEN**

Run: `$env:PYTHONPATH='.test-deps;python'; & 'C:\Users\emirh\.cache\codex-runtimes\codex-primary-runtime\dependencies\python\python.exe' -m pytest python/tests -q --basetemp .tmp/pytest-release-green`

Run: `& 'C:\Users\emirh\.cache\codex-runtimes\codex-primary-runtime\dependencies\python\python.exe' scripts/release/audit.py --repository . --static-only`

Expected: all Python tests pass except the documented local native-wheel skip; every static release check passes.

- [x] **Step 7: Commit release proof and documentation**

```powershell
git add .github README.md docs scripts python/tests demo
git commit -m "test(release): require 0.1 integrity evidence"
```

### Task 11: Full Verification, Audit Record, and Distribution ZIP

**Files:**
- Modify: `docs/verification/2026-08-12-integrity-hardening.md`
- Create: `dist/reprocut-0.1.0-source.zip`

**Interfaces:**
- Consumes the exact committed 0.1 source tree.
- Produces a verification record with commands, UTC timestamps, commit SHA, pass/fail/skip counts, platform limitation, and ZIP SHA-256.

- [x] **Step 1: Run formatting and complete remote Rust workspace verification**

Run: `python scripts/playground_rustfmt.py`

Run: `python scripts/playground_workspace_verify.py`

Expected: all composed workspace contracts pass; no warning/error is omitted from the verification record.

- [x] **Step 2: Run Python and browser/report verification**

Run: `$env:PYTHONPATH='.test-deps;python'; & 'C:\Users\emirh\.cache\codex-runtimes\codex-primary-runtime\dependencies\python\python.exe' -m pytest python/tests -q --basetemp .tmp/pytest-final`

Run: `& 'C:\Users\emirh\.cache\codex-runtimes\codex-primary-runtime\dependencies\node\bin\node.exe' scripts/verification/report_browser.cjs`

Expected: Python suite and browser contract pass; native-wheel skip is recorded, not described as a pass.

- [x] **Step 3: Run static release and repository hygiene audits**

Run: `& 'C:\Users\emirh\.cache\codex-runtimes\codex-primary-runtime\dependencies\python\python.exe' scripts/release/audit.py --repository . --static-only`

Run: `git diff --check`

Run: `git status --short`

Expected: static audit passes, diff check is empty, and status contains only the intended verification record before its commit.

- [x] **Step 4: Commit the verification record**

```powershell
git add docs/verification/2026-08-12-integrity-hardening.md
git commit -m "docs(verify): record 0.1 integrity gates"
```

- [x] **Step 5: Create a clean source ZIP from tracked release files**

Use `git archive --format=zip --prefix=reprocut-0.1.0/ -o dist/reprocut-0.1.0-source.zip HEAD`; verify its entries contain no `.git`, `.reprocut`, temporary pytest data, local wheel caches, secrets, state databases, or unrelated workspace files. Calculate SHA-256 with `Get-FileHash -Algorithm SHA256` and append it to the verification handoff without amending committed source claims.

- [x] **Step 6: Report exact outcome without publishing**

Provide links to the source tree, verification record, and ZIP; list exact pass/skip counts and the Windows native-Rust limitation; state that crates.io/PyPI publication, push, and tag remain for the user.

## Self-Review Result

- Spec coverage: every design section maps to Tasks 1-10; immutable capture and executable masks are Task 4, environment removal is Task 5, true Python isolation is Task 6, session/cache identity is Task 7, public parity is Task 8, schema-3 proof is Task 9, and release gates are Tasks 10-11.
- Placeholder scan: the plan contains no deferred implementation markers; `{python}`, `{candidate}`, and `{wheelhouse}` are the intentionally supported prepare-spec placeholders from the approved design.
- Type consistency: `OracleSpec`, `PythonIsolationRequest`, `FrozenPythonPreparation`, `ChildEnvironment`, mode-aware `FailureFingerprint`, and `ProjectSnapshot::subset` are introduced before downstream consumers and retain the same names/signatures throughout.
