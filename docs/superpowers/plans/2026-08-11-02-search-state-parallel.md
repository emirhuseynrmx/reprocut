# Search, Durable State, and Parallel Frontier Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the flat file list with canonical transformations, hierarchical/subset-complement reduction, crash-safe resume, and deterministic bounded parallel evaluation.

**Architecture:** Core creates ordered candidate plans. Workspace materializes immutable transformations. A single SQLite writer journals attempts and transitions. Workers execute a bounded frontier while the coordinator commits only the earliest eligible preserved rank.

**Tech Stack:** Rust 1.85, serde, SHA-256, rusqlite/SQLite WAL, tempfile, proptest, Loom.

## Global Constraints

- Version remains `0.1.0`; unsafe is forbidden in ReproCut source.
- Source files are immutable and candidate identities are content-addressed.
- Sequential and parallel runs must commit byte-identical transition chains.
- Incomplete/cancelled evidence is historical only and never authorizes a cut.
- Use official Playground contracts where the local Rust toolchain is blocked; SQLite/platform matrices remain explicit CI gates.
- Registry publication remains the user's final action.

---

### Task 1: Canonical transformation model

**Files:**
- Create: `crates/reprocut-core/src/transformation.rs`
- Modify: `crates/reprocut-core/src/lib.rs`
- Create: `crates/reprocut-core/tests/transformation_contract.rs`
- Create: `crates/reprocut-core/tests/transformation_properties.rs`
- Modify: `crates/reprocut-workspace/src/lib.rs`

**Interfaces:**
- Produces: `ProjectPath`, `ByteRange`, `Operation`, `Transformation`, `CandidateRank`, `ContentDigest`.
- Produces: `CandidateWorkspace::materialize(snapshot, transformation_set)`.

- [ ] **Step 1: Write failing canonicalization tests**

```rust
#[test]
fn operation_order_does_not_change_candidate_digest() {
    let a = candidate([delete("b.py"), delete("a.py")]);
    let b = candidate([delete("a.py"), delete("b.py")]);
    assert_eq!(a.digest(), b.digest());
}
```

- [ ] **Step 2: Verify RED because transformation types do not exist**
- [ ] **Step 3: Implement validated paths, non-overlapping ranges, canonical ordering, and stable encoding**
- [ ] **Step 4: Materialize delete and replace operations in descending range order**
- [ ] **Step 5: Run properties for permutations, overlaps, UTF-8 boundaries, and source immutability; commit**

```bash
git add crates/reprocut-core crates/reprocut-workspace
git commit -m "feat(core): model canonical project transformations"
```

### Task 2: Full ddmin and directory hierarchy

**Files:**
- Replace: `crates/reprocut-core/src/reducer.rs`
- Modify: `crates/reprocut-core/tests/reducer_contract.rs`
- Modify: `crates/reprocut-core/tests/reducer_properties.rs`
- Create: `crates/reprocut-workspace/src/hierarchy.rs`
- Create: `crates/reprocut-workspace/tests/hierarchy_contract.rs`

**Interfaces:**
- Produces: `ordered_frontier(active, granularity)` with subset and complement ranks.
- Produces: `DirectoryHierarchy::groups()` and final singleton sweep.

- [ ] **Step 1: Add a non-monotonic verdict map where complement-only reduction gets stuck**
- [ ] **Step 2: Verify the existing reducer fails to find the expected 1-minimal result**
- [ ] **Step 3: Implement ordered subset-plus-complement ddmin with reusable buffers**
- [ ] **Step 4: Build a path trie and emit directory groups before leaves**
- [ ] **Step 5: Exhaust every non-empty required subset through eight units and generated directory trees**
- [ ] **Step 6: Commit**

```bash
git add crates/reprocut-core crates/reprocut-workspace
git commit -m "feat(core): reduce hierarchical and non-monotonic candidates"
```

### Task 3: SQLite session journal and resume validation

**Files:**
- Create: `crates/reprocut-state/Cargo.toml`
- Create: `crates/reprocut-state/src/lib.rs`
- Create: `crates/reprocut-state/src/schema.rs`
- Create: `crates/reprocut-state/migrations/0001.sql`
- Create: `crates/reprocut-state/tests/state_contract.rs`
- Modify: `Cargo.toml`

**Interfaces:**
- Produces: `StateStore::create`, `StateStore::resume`, `WriterHandle`, `SessionContract`, `AttemptRecord`, `TransitionRecord`.
- Consumes canonical digests and evidence types.

- [ ] **Step 1: Write failing atomic-transition and incompatible-resume tests**

```rust
#[test]
fn transition_and_preserved_attempt_are_atomic() {
    let store = fixture_store();
    store.inject_failure_after_attempt_insert();
    assert!(store.accept_transition(attempt(), transition()).is_err());
    assert_eq!(store.transitions().unwrap(), Vec::new());
}
```

- [ ] **Step 2: Verify RED before the state crate exists**
- [ ] **Step 3: Implement schema, WAL/FULL pragmas, migrations, and one-writer command channel**
- [ ] **Step 4: Implement exact session-contract validation and cache rules**
- [ ] **Step 5: Exercise reopen, duplicate messages, crash injection, and migration refusal; commit**

```bash
git add Cargo.toml Cargo.lock crates/reprocut-state
git commit -m "feat(state): journal resumable reduction sessions"
```

### Task 4: Deterministic bounded frontier scheduler

**Files:**
- Create: `crates/reprocut-engine/src/scheduler.rs`
- Create: `crates/reprocut-engine/tests/scheduler_contract.rs`
- Create: `crates/reprocut-core/tests/loom_frontier.rs`
- Modify: `crates/reprocut-engine/src/lib.rs`

**Interfaces:**
- Produces: `FrontierScheduler::evaluate(plans, jobs, evaluator)`.
- Consumes ordered `CandidateRank`, cache lookups, and writer messages.

- [ ] **Step 1: Write a test where a later preserved rank finishes first**
- [ ] **Step 2: Verify a naive race winner would fail the required accepted chain**
- [ ] **Step 3: Implement a bounded queue, atomic work index, ordered result slots, and prefix commitment**
- [ ] **Step 4: Persist late results as cache entries without applying their transitions**
- [ ] **Step 5: Add Loom interleavings for publish/cancel/shutdown and properties across 1/2/4/16 workers**
- [ ] **Step 6: Commit**

```bash
git add crates/reprocut-engine crates/reprocut-core
git commit -m "feat(engine): evaluate a deterministic parallel frontier"
```

### Task 5: Engine resume and interruption contract

**Files:**
- Modify: `crates/reprocut-engine/src/lib.rs`
- Modify: `crates/reprocut-cli/src/main.rs`
- Create: `crates/reprocut-engine/tests/resume_contract.rs`
- Modify: `crates/reprocut-cli/tests/cli_contract.rs`

**Interfaces:**
- Produces CLI: `reprocut resume`, `--state`, `--restart`, `--jobs`.
- Consumes state store, scheduler, and transformation pipeline.

- [ ] **Step 1: Write failing interrupted/resumed equivalence test**
- [ ] **Step 2: Verify RED because the current engine has only memory-local cache**
- [ ] **Step 3: Refactor the engine into checkpointed phase transitions**
- [ ] **Step 4: Handle first and second Ctrl-C semantics with group cancellation and writer drain**
- [ ] **Step 5: Compare uninterrupted and resumed artifacts byte-for-byte; commit**

```bash
git add crates/reprocut-engine crates/reprocut-cli
git commit -m "feat(engine): resume durable reduction sessions"
```
