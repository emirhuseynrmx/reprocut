# ReproCut v0.1.0 Session and Filesystem Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Bind resume/cache to the actual failure and execution environment while making snapshot capture and publication stable under adversarial filesystem changes.

**Architecture:** Baseline stabilization precedes state creation. A versioned execution identity is length-delimited into session v3, and project snapshots become typed entry manifests captured by two stable passes.

**Tech Stack:** Rust 1.85, rusqlite with bundled SQLite, platform filesystem APIs, property/integration tests.

## Global Constraints

- Session contract schema is exactly `3`; v1/v2 journals fail with restart guidance.
- Identity never uses `to_string_lossy`.
- `.git`, state, output, and generated exclusions apply before capture.
- Lockfiles are protected unless `--allow-lockfile-removal` is explicit.
- Every production change follows RED, GREEN, REFACTOR and is committed separately.

---

### Task 1: Execution identity and session v3

**Files:**
- Create: `crates/reprocut-runner/src/identity.rs`
- Modify: `crates/reprocut-runner/src/lib.rs`
- Modify: `crates/reprocut-state/src/lib.rs`
- Modify: `crates/reprocut-state/tests/state_contract.rs`
- Modify: `crates/reprocut-engine/src/lib.rs`

**Interfaces:**
- Produces: `ExecutionIdentity { environment_policy, environment_sha256, program, tools }`.
- Produces: `ProgramIdentity { encoded_path, binary_sha256, version_output_sha256 }`.
- Produces: `SessionContract::new_v3(... failure_fingerprint_sha256, execution_identity, ...)`.

- [ ] Write failing contracts for changed baseline fingerprint, PATH/tool binary, bound env, and non-lossy platform path encoding.
- [ ] Run state/engine contracts and confirm v2 currently accepts incompatible sessions.
- [ ] Resolve executables, hash binaries and bounded version output, hash allowlisted environment values, and implement v3 encoding.
- [ ] Add fingerprint digest to attempts/transitions/cache records and reject old schemas.
- [ ] Run state, runner, engine, protocol, and resume integration tests.
- [ ] Commit with `feat(state): bind sessions to failure and environment`.

### Task 2: Baseline-before-state orchestration

**Files:**
- Modify: `crates/reprocut-engine/src/lib.rs`
- Modify: `crates/reprocut-engine/src/pipeline.rs`
- Modify: `crates/reprocut-cli/src/main.rs`
- Test: `crates/reprocut-engine/tests/session_identity.rs`

**Interfaces:**
- Produces: `StabilizedBaseline { oracle, fingerprint, observations }` before `StateStore::open`.
- Produces: progress callback event `BaselineStable` at the stabilization boundary.

- [ ] Add a failing test that runs equal source/spec with ValueError then TypeError and proves the old journal is never consulted.
- [ ] Add a failing protocol test proving `BaselineStable` precedes search progress.
- [ ] Split stabilization from search, construct v3 after stabilization, and open state only then.
- [ ] Run engine/CLI/protocol tests.
- [ ] Commit with `fix(engine): stabilize failure before opening state`.

### Task 3: Realized structured candidate identity

**Files:**
- Modify: `crates/reprocut-engine/src/lib.rs`
- Modify: `crates/reprocut-state/src/lib.rs`
- Test: `crates/reprocut-engine/tests/cache_identity.rs`

**Interfaces:**
- Produces: cache key fields `{ planned_digest, realized_prepared_snapshot_sha256, failure_fingerprint_sha256 }`.

- [ ] Add a failing test where the preparation tool realizes different bytes for an identical plan and the prior preserved verdict must not cut.
- [ ] Run the focused engine contract and observe the stale hit.
- [ ] Persist realized digest and treat any re-realization mismatch as cache miss plus oracle execution.
- [ ] Run scheduler/state/engine tests and commit `fix(cache): bind verdicts to realized candidates`.

### Task 4: Two-pass typed snapshot

**Files:**
- Modify: `crates/reprocut-workspace/src/lib.rs`
- Create: `crates/reprocut-workspace/src/path_identity.rs`
- Modify: `crates/reprocut-workspace/tests/snapshot_integrity.rs`
- Modify: `crates/reprocut-workspace/tests/snapshot_contract.rs`

**Interfaces:**
- Produces: `SnapshotEntry::{File, Directory, Symlink}`.
- Produces: stable two-pass `ProjectSnapshot::capture` with bounded retries.
- Produces: native-unit path encoding tagged by platform.

- [ ] Add failing tests for second-pass content drift, empty directory, safe relative symlink, escaping/absolute/cyclic symlink, and identity collisions from invalid Unicode.
- [ ] Run workspace contracts and observe missing entry types/stability.
- [ ] Implement no-follow capture, two complete passes, typed digest encoding, safe materialization, and explicit unsupported-path errors.
- [ ] Run workspace tests on Linux/Windows/macOS; add Unix executable-mask/symlink tests.
- [ ] Commit with `feat(workspace): capture stable typed snapshots`.

### Task 5: Inventory and lockfile policy

**Files:**
- Modify: `crates/reprocut-workspace/src/lib.rs`
- Modify: `crates/reprocut-cli/src/main.rs`
- Modify: `crates/reprocut-core/src/protocol.rs`
- Modify: `python/reprocut/client.py`
- Test: `crates/reprocut-workspace/tests/inventory_policy.rs`

**Interfaces:**
- Produces: `LockfilePolicy::{Protect, AllowRemoval}` and exact protected entries.
- Produces: path-segment exclusions for nested generated/dependency dirs.

- [ ] Add failing tests for nested generated dirs, regular-file `.git`, in-root state/output, ordinary `src/`, empty generated dirs, and protected lockfiles.
- [ ] Run workspace/CLI/client tests and observe current gaps.
- [ ] Apply exclusions before snapshot, reject ambiguous overlaps, and expose explicit lockfile removal opt-in on every surface.
- [ ] Run parity and inventory tests; commit `fix(workspace): harden inventory policy`.

### Task 6: Global budgets and shared request validation

**Files:**
- Modify: `crates/reprocut-core/src/protocol.rs`
- Modify: `crates/reprocut-engine/src/lib.rs`
- Modify: `crates/reprocut-cli/src/main.rs`
- Modify: `python/reprocut/client.py`
- Test: `crates/reprocut-core/tests/protocol_contract.rs`
- Test: `python/tests/test_client.py`

**Interfaces:**
- Produces: `ReductionBudget { deadline, cpu_time, max_attempts, max_preparations, max_captured_output, max_disk_bytes }`.

- [ ] Add failing shared fixtures for zero/overflow limits, unsupported mode/ecosystem pairs, and budget exhaustion before publication.
- [ ] Run Rust/Python contract tests and record validation divergence.
- [ ] Centralize typed validation and enforce counters/deadlines in every phase.
- [ ] Run protocol/client/engine tests; commit `feat(engine): enforce global reduction budgets`.

