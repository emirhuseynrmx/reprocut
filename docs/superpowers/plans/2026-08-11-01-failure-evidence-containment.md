# Failure Evidence and Containment Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add fail-closed stdout/stderr failure identity, statistically reported flaky execution, portable termination reasons, and whole-process-tree teardown.

**Architecture:** `reprocut-core` owns immutable evidence and aggregation policy. `reprocut-runner` owns process containment and bounded capture. `reprocut-engine` composes repeated observations without platform conditionals leaking into the oracle.

**Tech Stack:** Rust 1.85, regex, serde, command-group, proptest, Loom, PyO3, pytest.

## Global Constraints

- Keep every workspace package on version `0.1.0`, edition 2021, and Rust 1.85.
- Forbid handwritten unsafe code in ReproCut crates.
- Source checkouts remain read-only and incomplete evidence never authorizes a cut.
- Preserve the explicit legacy behavior through `--oracle-stream stderr`.
- The local Rust executable is blocked by Windows Application Control; use official Playground contracts for portable red/green evidence and leave platform jobs as explicit CI gates.
- Final crates.io/PyPI publication is performed by the user, not by implementation tasks.

---

### Task 1: Portable observations and multi-stream fingerprints

**Files:**
- Modify: `crates/reprocut-core/src/model.rs`
- Modify: `crates/reprocut-core/src/oracle.rs`
- Modify: `crates/reprocut-core/src/lib.rs`
- Modify: `crates/reprocut-core/tests/model_contract.rs`
- Modify: `crates/reprocut-core/tests/oracle_contract.rs`
- Modify: `crates/reprocut-core/tests/oracle_properties.rs`
- Modify: `python/reprocut/_fallback.py`
- Modify: `python/tests/test_oracle.py`

**Interfaces:**
- Produces: `TerminationReason`, `DiagnosticChannel`, `DiagnosticAnchor`, `FailureFingerprint::anchors()`, `FailureOracle::from_baselines(channel, observations)`.
- Preserves: `ExecutionObservation::new(...)` through a compatibility constructor while internal state moves to `TerminationReason`.

- [ ] **Step 1: Write failing channel-selection contracts**

```rust
#[test]
fn auto_requires_every_stable_non_empty_channel() {
    let oracle = FailureOracle::from_baselines(
        DiagnosticChannel::Auto,
        &[obs(1, "stable out", "stable err"), obs(1, "stable out", "stable err")],
    ).unwrap();
    assert_eq!(oracle.classify(&obs(1, "changed", "stable err")), CandidateVerdict::Rejected);
    assert_eq!(oracle.classify(&obs(1, "stable out", "stable err")), CandidateVerdict::Preserved);
}
```

- [ ] **Step 2: Run the remote core contract and confirm it fails because `DiagnosticChannel` and multi-anchor fingerprints do not exist**

Run: `python scripts/playground_verify.py scripts/verification/oracle_v2_contract.rs --run`

- [ ] **Step 3: Implement the immutable evidence model**

```rust
pub enum DiagnosticChannel { Auto, Stderr, Stdout, Combined }
pub enum TerminationReason { ExitCode(i32), UnixSignal(i32), TimedOut, RunnerFailure }
pub struct DiagnosticAnchor { channel: DiagnosticChannel, text: String }
pub struct FailureFingerprint {
    termination: TerminationReason,
    anchors: Vec<DiagnosticAnchor>,
    normalization_schema: u16,
}
```

- [ ] **Step 4: Implement fail-closed baseline selection and exact candidate classification**

Normalize each stream independently. `Auto` selects every stable, complete, non-empty stream; `Combined` requires both; explicit channels reject empty or unstable baselines. Candidate classification requires exact termination and every selected anchor.

- [ ] **Step 5: Mirror pure semantics in the Python reference backend and run Python tests**

Run: `PYTHONPATH=python python -m pytest python/tests/test_oracle.py -q`

- [ ] **Step 6: Run oracle properties and commit**

```bash
git add crates/reprocut-core python/reprocut/_fallback.py python/tests/test_oracle.py scripts/verification/oracle_v2_contract.rs
git commit -m "feat(core): identify failures across output channels"
```

### Task 2: Strict and flaky aggregation policies

**Files:**
- Create: `crates/reprocut-core/src/policy.rs`
- Modify: `crates/reprocut-core/src/lib.rs`
- Create: `crates/reprocut-core/tests/policy_contract.rs`
- Create: `crates/reprocut-core/tests/policy_properties.rs`
- Modify: `crates/reprocut-engine/src/lib.rs`

**Interfaces:**
- Consumes: `FailureOracle`, `CandidateVerdict`.
- Produces: `EvaluationPolicy::{Strict, Flaky}`, `AggregateEvidence`, `AggregateDecision`, `wilson_interval`.

- [ ] **Step 1: Write failing strict/flaky contracts**

```rust
#[test]
fn flaky_policy_stops_when_nine_of_eleven_is_decided() {
    let policy = EvaluationPolicy::flaky(11, 9).unwrap();
    let evidence = policy.aggregate([CandidateVerdict::Preserved; 9]);
    assert_eq!(evidence.decision(), AggregateDecision::Preserved);
    assert!(evidence.wilson_95().is_some());
}
```

- [ ] **Step 2: Verify RED with a standalone Playground contract**

Run: `python scripts/playground_verify.py scripts/verification/policy_contract.rs --run`

- [ ] **Step 3: Implement validated policies and integer decision boundaries**

Strict uses `3/3`. Flaky defaults to `9/11`, accepts odd run counts `5..=101`, requires a strict supermajority, stops once the required count is reached or can no longer be reached, and computes the 95% Wilson interval for display only.

- [ ] **Step 4: Integrate aggregation into baseline, candidate, and final verification loops**

The engine supplies observations lazily so early-stop avoids unnecessary process execution. Incomplete observations remain counted separately.

- [ ] **Step 5: Run exhaustive generated verdict-sequence properties and commit**

```bash
git add crates/reprocut-core crates/reprocut-engine scripts/verification/policy_contract.rs
git commit -m "feat(core): aggregate deterministic and flaky evidence"
```

### Task 3: Whole-process-tree containment

**Files:**
- Modify: `Cargo.toml`
- Modify: `crates/reprocut-runner/Cargo.toml`
- Modify: `crates/reprocut-runner/src/lib.rs`
- Modify: `crates/reprocut-runner/tests/runner_contract.rs`
- Create: `crates/reprocut-runner/tests/fixtures/spawn_descendant.py`
- Create: `scripts/verification/process_group_contract.rs`

**Interfaces:**
- Produces: `ContainmentMechanism`, `ProcessRunner::run` with group ownership and group teardown.
- Consumes: `TerminationReason` from core.

- [ ] **Step 1: Write a descendant-marker regression test**

The fixture spawns a child that writes `descendant-survived` after the parent timeout. The test waits beyond that delay and asserts the marker does not exist.

- [ ] **Step 2: Run the current runner and verify the regression fails because only the direct child is killed**

Run the Unix fixture in the official Playground and retain Windows as a CI-only RED gate.

- [ ] **Step 3: Add `command-group` with a Rust-1.85-compatible exact minor version**

Use `group_spawn`, retain the group handle through both capture threads, call group kill on timeout/cancellation/drop, then wait/reap the root process.

- [ ] **Step 4: Preserve bounded concurrent drains and expose the active containment mechanism**

The runner must continue draining after reaching each stream budget and set `streams_truncated` without deadlocking the child.

- [ ] **Step 5: Run timeout, capture, Unix descendant, and Windows CI contracts; commit**

```bash
git add Cargo.toml Cargo.lock crates/reprocut-runner scripts/verification/process_group_contract.rs
git commit -m "feat(runner): contain complete candidate process trees"
```

### Task 4: CLI and Python compatibility surface

**Files:**
- Modify: `crates/reprocut-cli/src/main.rs`
- Modify: `crates/reprocut-python/src/lib.rs`
- Modify: `python/reprocut/_native.pyi`
- Modify: `python/tests/test_native_backend.py`
- Modify: `crates/reprocut-cli/tests/cli_contract.rs`

**Interfaces:**
- Produces CLI flags: `--oracle-stream`, `--flaky`, `--flaky-runs`, `--flaky-required`.
- Produces Python enums/configuration matching Rust policy validation.

- [ ] **Step 1: Write failing CLI parsing and invalid-policy tests**
- [ ] **Step 2: Verify RED in the flattened CLI Playground contract**
- [ ] **Step 3: Thread policy/channel configuration into `ReductionRequest` without global state**
- [ ] **Step 4: Expose typed PyO3 constructors and update `.pyi` signatures**
- [ ] **Step 5: Run CLI, Python reference, and native-wheel CI contracts; commit**

```bash
git add crates/reprocut-cli crates/reprocut-python python
git commit -m "feat(cli): configure stable and flaky failure evidence"
```
