# ReproCut MVP Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (- [ ]) syntax for tracking.

**Goal:** Build a production-shaped ReproCut CLI that proves a stable failure, minimizes a project without touching the original, emits a self-contained report, exposes the oracle through Python, and ships with a real animated demo.

**Architecture:** A Rust workspace separates failure identity, deterministic reduction, process execution, disposable workspace management, reporting, CLI orchestration, and Python bindings. Candidate evaluation is three-valued; hierarchical delta debugging only accepts the exact configured failure. The first public slice reduces files for arbitrary projects and treats ecosystem-specific manifest reduction as the next independently testable slice.

**Tech Stack:** Rust 1.85 MSRV, clap, serde, blake3, tempfile, walkdir, regex, thiserror, wait-timeout, PyO3/maturin, pytest, proptest, loom, cargo-nextest-compatible tests, Miri, HTML/CSS/vanilla JavaScript, Playwright capture, Pillow GIF encoding.

## Global Constraints

- The original project is never used as a candidate workspace.
- Workspace isolation is not marketed as hostile-code containment.
- Inconclusive candidate executions are never accepted as failure preservation.
- Parallel completion order must not change the accepted candidate chain.
- Stdout and stderr capture are bounded.
- Every path selected for mutation is resolved beneath the disposable candidate root.
- Rust owns correctness; Python is an extension surface.
- No AI model participates in the failure oracle.
- No benchmark claim ships without a reproducible fixture.
- Production behavior follows strict red-green-refactor cycles.

## File map

- Cargo.toml: workspace membership, dependency versions, lint policy, release profile.
- rust-toolchain.toml: pinned stable toolchain components.
- crates/reprocut-core/src/model.rs: execution observations and three-valued candidate classification.
- crates/reprocut-core/src/oracle.rs: normalization, baseline fingerprint creation, and matching.
- crates/reprocut-core/src/reducer.rs: deterministic hierarchical delta debugging.
- crates/reprocut-core/src/winner.rs: deterministic concurrent winner primitive and Loom model.
- crates/reprocut-runner/src/lib.rs: bounded child execution, timeout, and process observations.
- crates/reprocut-workspace/src/lib.rs: inventory, safe materialization, and removal validation.
- crates/reprocut-engine/src/lib.rs: baseline stabilization and reducer orchestration.
- crates/reprocut-report/src/lib.rs: self-contained HTML report rendering.
- crates/reprocut-cli/src/main.rs: command-line contract and output emission.
- crates/reprocut-python/src/lib.rs: PyO3 failure-fingerprint API.
- python/reprocut/__init__.py: typed Python import surface.
- python/tests/test_oracle.py: pytest contract tests against the compiled extension.
- tests/fixtures/python_failure/: language-neutral end-to-end fixture project.
- tests/golden/reduction-report.html: reviewed report contract.
- demo/: generated working demonstration artifacts.
- scripts/capture_demo.py: browser frame capture and GIF encoding.
- .github/workflows/ci.yml: fmt, clippy, tests, Miri, Loom, Python, and platform matrix.

---

### Task 1: Workspace and public model

**Files:**
- Create: Cargo.toml
- Create: rust-toolchain.toml
- Create: .gitignore
- Create: crates/reprocut-core/Cargo.toml
- Create: crates/reprocut-core/src/lib.rs
- Create: crates/reprocut-core/src/model.rs
- Test: crates/reprocut-core/tests/model_contract.rs

**Interfaces:**
- Produces: ExecutionObservation, FailureFingerprint, CandidateVerdict, CandidateId, ReductionUnit.

- [ ] **Step 1: Create workspace configuration**

Use resolver 2, Rust 2021, rust-version 1.85, workspace lints that forbid unsafe code, and release settings with thin LTO, one codegen unit, aborting panic, and stripped symbols.

- [ ] **Step 2: Write the failing public-model test**

~~~rust
use reprocut_core::{
    CandidateVerdict, ExecutionObservation, FailureFingerprint,
};

#[test]
fn inconclusive_is_distinct_from_preserved_and_rejected() {
    assert_ne!(CandidateVerdict::Inconclusive, CandidateVerdict::Preserved);
    assert_ne!(CandidateVerdict::Inconclusive, CandidateVerdict::Rejected);
}

#[test]
fn observation_keeps_bounded_stream_metadata() {
    let observation = ExecutionObservation::new(
        Some(1),
        None,
        b"out".to_vec(),
        b"TypeError: currency".to_vec(),
        false,
        false,
    );
    assert_eq!(observation.exit_code(), Some(1));
    assert_eq!(observation.stderr(), b"TypeError: currency");
}

#[test]
fn fingerprint_is_serializable_and_stable() {
    let fingerprint = FailureFingerprint::new(
        Some(1),
        None,
        "TypeError: currency".into(),
    );
    let encoded = serde_json::to_string(&fingerprint).unwrap();
    assert_eq!(
        encoded,
        r#"{"exit_code":1,"signal":null,"anchor":"TypeError: currency"}"#
    );
}
~~~

- [ ] **Step 3: Run the model test and verify RED**

Run: cargo test -p reprocut-core --test model_contract

Expected: compilation fails because the public model types do not exist.

- [ ] **Step 4: Implement the minimal model**

Define immutable structs with private fields, borrowed accessors, serde derives, and explicit constructors. CandidateVerdict has exactly Preserved, Rejected, and Inconclusive.

- [ ] **Step 5: Run the model test and verify GREEN**

Run: cargo test -p reprocut-core --test model_contract

Expected: three tests pass.

- [ ] **Step 6: Commit**

Run:

~~~text
git add Cargo.toml rust-toolchain.toml .gitignore crates/reprocut-core
git commit -m "feat(core): define execution and failure model"
~~~

---

### Task 2: Conservative failure oracle

**Files:**
- Create: crates/reprocut-core/src/oracle.rs
- Modify: crates/reprocut-core/src/lib.rs
- Test: crates/reprocut-core/tests/oracle_contract.rs
- Test: crates/reprocut-core/tests/oracle_properties.rs

**Interfaces:**
- Consumes: ExecutionObservation, FailureFingerprint, CandidateVerdict.
- Produces: FailureOracle::from_baselines, FailureOracle::classify, OracleError.

- [ ] **Step 1: Write failing behavior tests**

~~~rust
use reprocut_core::{CandidateVerdict, ExecutionObservation, FailureOracle};

fn failed(stderr: &str) -> ExecutionObservation {
    ExecutionObservation::new(
        Some(1), None, Vec::new(), stderr.as_bytes().to_vec(), false, false,
    )
}

#[test]
fn stable_baselines_create_an_oracle() {
    let oracle = FailureOracle::from_baselines(&[
        failed("thread 91: TypeError: currency at C:\\tmp\\a.py:84"),
        failed("thread 17: TypeError: currency at C:\\tmp\\a.py:84"),
        failed("thread 02: TypeError: currency at C:\\tmp\\a.py:84"),
    ]).unwrap();
    assert!(oracle.fingerprint().anchor().contains("TypeError: currency"));
}

#[test]
fn unrelated_compile_error_is_rejected() {
    let oracle = FailureOracle::from_baselines(&[
        failed("TypeError: currency"), failed("TypeError: currency"),
        failed("TypeError: currency"),
    ]).unwrap();
    assert_eq!(
        oracle.classify(&failed("ModuleNotFoundError: checkout")),
        CandidateVerdict::Rejected,
    );
}

#[test]
fn truncated_or_timed_out_execution_is_inconclusive() {
    let oracle = FailureOracle::from_baselines(&[
        failed("TypeError: currency"), failed("TypeError: currency"),
        failed("TypeError: currency"),
    ]).unwrap();
    let timed_out = ExecutionObservation::new(
        None, None, Vec::new(), Vec::new(), true, false,
    );
    assert_eq!(oracle.classify(&timed_out), CandidateVerdict::Inconclusive);
}
~~~

- [ ] **Step 2: Run and verify RED**

Run: cargo test -p reprocut-core --test oracle_contract

Expected: FailureOracle is unresolved.

- [ ] **Step 3: Implement minimal conservative normalization**

Normalize CRLF, absolute temporary paths, hexadecimal addresses, PIDs, and long decimal identifiers. Choose the longest stable normalized stderr line shared by all baseline runs. Reject empty anchors and differing exit or signal states.

- [ ] **Step 4: Run and verify GREEN**

Run: cargo test -p reprocut-core --test oracle_contract

Expected: three tests pass.

- [ ] **Step 5: Add property tests**

Use proptest to verify normalization is idempotent, never expands beyond a documented factor, and identical normalized observations classify as Preserved.

- [ ] **Step 6: Run property tests**

Run: cargo test -p reprocut-core --test oracle_properties

Expected: all generated cases pass.

- [ ] **Step 7: Commit**

Run:

~~~text
git add crates/reprocut-core
git commit -m "feat(core): preserve exact failure identity"
~~~

---

### Task 3: Deterministic hierarchical reducer

**Files:**
- Create: crates/reprocut-core/src/reducer.rs
- Modify: crates/reprocut-core/src/lib.rs
- Test: crates/reprocut-core/tests/reducer_contract.rs
- Test: crates/reprocut-core/tests/reducer_properties.rs

**Interfaces:**
- Consumes: CandidateVerdict and ordered ReductionUnit values.
- Produces: reduce(units, evaluator) -> ReductionResult with kept units, attempts, and accepted chain.

- [ ] **Step 1: Write the failing minimality test**

~~~rust
use reprocut_core::{reduce, CandidateVerdict, ReductionUnit};

#[test]
fn removes_every_unit_not_required_for_failure() {
    let units = ["a", "bug.py", "b", "c"]
        .into_iter()
        .enumerate()
        .map(|(id, path)| ReductionUnit::new(id as u32, path.into()))
        .collect::<Vec<_>>();

    let result = reduce(&units, |kept| {
        if kept.iter().any(|unit| unit.path() == "bug.py") {
            CandidateVerdict::Preserved
        } else {
            CandidateVerdict::Rejected
        }
    });

    assert_eq!(
        result.kept().iter().map(|unit| unit.path()).collect::<Vec<_>>(),
        vec!["bug.py"],
    );
    assert!(result.attempts() < 10);
}
~~~

- [ ] **Step 2: Run and verify RED**

Run: cargo test -p reprocut-core --test reducer_contract

Expected: reduce and ReductionUnit are unresolved.

- [ ] **Step 3: Implement hierarchical ddmin**

Use index ranges over a contiguous unit slice. Reuse candidate Vec capacity across attempts. Never accept Inconclusive. Finish with a deterministic one-minimal sweep.

- [ ] **Step 4: Run and verify GREEN**

Run: cargo test -p reprocut-core --test reducer_contract

Expected: the reducer keeps only bug.py.

- [ ] **Step 5: Add differential properties**

Generate universes up to 32 units and literal required sets. Compare the optimized reducer result to a slow sequential reference and assert identical retained IDs.

- [ ] **Step 6: Run properties**

Run: cargo test -p reprocut-core --test reducer_properties

Expected: all generated cases pass.

- [ ] **Step 7: Commit**

Run:

~~~text
git add crates/reprocut-core
git commit -m "feat(core): add deterministic hierarchical reduction"
~~~

---

### Task 4: Safe disposable workspaces and bounded process execution

**Files:**
- Create: crates/reprocut-workspace/Cargo.toml
- Create: crates/reprocut-workspace/src/lib.rs
- Create: crates/reprocut-runner/Cargo.toml
- Create: crates/reprocut-runner/src/lib.rs
- Test: crates/reprocut-workspace/tests/workspace_contract.rs
- Test: crates/reprocut-runner/tests/runner_contract.rs

**Interfaces:**
- Produces: ProjectInventory::scan, CandidateWorkspace::materialize, CandidateWorkspace::remove_units, CommandSpec, ProcessRunner::run.

- [ ] **Step 1: Write failing workspace tests**

Create a real temporary project with nested files and a symlink escape when the platform permits. Assert inventory ordering, ignored .git content, byte-for-byte unchanged source, removal only in the candidate, and rejection of paths escaping the candidate root.

- [ ] **Step 2: Run and verify RED**

Run: cargo test -p reprocut-workspace

Expected: workspace crate API is missing.

- [ ] **Step 3: Implement the minimal workspace backend**

Walk without following links, intern normalized relative paths, sort once, copy into tempfile-managed candidates, and validate removals using normalized relative components before joining with the candidate root.

- [ ] **Step 4: Run workspace tests and verify GREEN**

Run: cargo test -p reprocut-workspace

Expected: workspace tests pass on the current platform; platform-specific symlink test reports an explicit skip only when creation is not permitted.

- [ ] **Step 5: Write failing real-process tests**

Use the current test executable as a child fixture to emit stdout, stderr, non-zero exit, excessive output, and a timeout. Assert exact exit state, bounded capture flags, and elapsed timeout behavior without mocks.

- [ ] **Step 6: Run and verify RED**

Run: cargo test -p reprocut-runner

Expected: ProcessRunner is missing.

- [ ] **Step 7: Implement bounded runner**

Spawn with piped streams, read each stream into a fixed maximum byte budget, retain truncation metadata, enforce timeout with wait-timeout, and kill then reap the child on expiration.

- [ ] **Step 8: Run runner tests and verify GREEN**

Run: cargo test -p reprocut-runner

Expected: all real-process tests pass without leaked children.

- [ ] **Step 9: Commit**

Run:

~~~text
git add crates/reprocut-workspace crates/reprocut-runner Cargo.toml
git commit -m "feat(runtime): isolate candidates and bound child execution"
~~~

---

### Task 5: End-to-end reduction engine

**Files:**
- Create: crates/reprocut-engine/Cargo.toml
- Create: crates/reprocut-engine/src/lib.rs
- Create: tests/fixtures/python_failure/bug.py
- Create: tests/fixtures/python_failure/noise.txt
- Create: tests/fixtures/python_failure/nested/unused.txt
- Test: crates/reprocut-engine/tests/engine_contract.rs

**Interfaces:**
- Consumes: ProjectInventory, CandidateWorkspace, ProcessRunner, FailureOracle, reduce.
- Produces: ReductionEngine::run(ReductionRequest) -> ReductionOutcome.

- [ ] **Step 1: Create the fixture**

bug.py prints a stable “TypeError: currency” line to stderr and exits 1 only while bug.py exists. Noise files do not affect execution.

- [ ] **Step 2: Write the failing end-to-end test**

Run the fixture through the real Python interpreter passed in TEST_PYTHON. Assert three baseline executions, final retention of bug.py, removal of both noise files, unchanged source digest, and a final verification classified Preserved.

- [ ] **Step 3: Run and verify RED**

Run: cargo test -p reprocut-engine --test engine_contract

Expected: ReductionEngine is missing.

- [ ] **Step 4: Implement orchestration**

Stabilize three baselines, build the oracle, evaluate candidates in fresh workspaces, cache verdicts by BLAKE3 of retained unit IDs plus oracle fingerprint, and verify the final candidate three times.

- [ ] **Step 5: Run and verify GREEN**

Run: cargo test -p reprocut-engine --test engine_contract

Expected: the fixture is reduced to bug.py and the source digest remains unchanged.

- [ ] **Step 6: Commit**

Run:

~~~text
git add crates/reprocut-engine tests/fixtures Cargo.toml
git commit -m "feat(engine): minimize a real failing project"
~~~

---

### Task 6: Distinctive self-contained report with golden testing

**Files:**
- Create: crates/reprocut-report/Cargo.toml
- Create: crates/reprocut-report/src/lib.rs
- Create: crates/reprocut-report/assets/report.css
- Create: crates/reprocut-report/assets/report.js
- Create: tests/golden/reduction-report.html
- Test: crates/reprocut-report/tests/report_golden.rs

**Interfaces:**
- Consumes: ReductionOutcome and redacted display paths.
- Produces: render_report(ReportModel) -> String.

- [ ] **Step 1: Lock the visual direction**

Palette:

- Carbon paper #171A1F
- Blueprint blue #2F5BFF
- Cut edge #FF6B4A
- Archive white #F6F7F2
- Signal mint #7CE3C1
- Muted steel #89919D

Typography uses bundled/system fallbacks: Arial Narrow for display, Segoe UI for body, Cascadia Mono for measurements. Layout resembles a physical cutting table: the original project mass sits at left, retained slices contract through the center, and the final reproduction locks into a precise right rail. The signature is a horizontal “cut line” whose removed segments peel away as the reduction progresses.

Self-critique: a generic dark dashboard would obscure the product metaphor. Use archive white as the main field and reserve carbon for the command rail, making the cut line—not cards or gradients—the only visual flourish.

- [ ] **Step 2: Write the failing golden test**

Construct a literal ReportModel with 18 files reduced to 3 and compare render_report output byte-for-byte to tests/golden/reduction-report.html after normalizing line endings.

- [ ] **Step 3: Run and verify RED**

Run: cargo test -p reprocut-report --test report_golden

Expected: render_report is missing.

- [ ] **Step 4: Implement semantic HTML and assets**

Render a single file with escaped content, inline CSS/JS, keyboard-visible focus, responsive rules, reduced-motion support, reduction stages, oracle anchor, exact command, and no external resources.

- [ ] **Step 5: Review and accept the golden file**

Open the generated candidate in a browser, inspect desktop and mobile screenshots, correct visual issues, then copy the reviewed output to the golden path.

- [ ] **Step 6: Run and verify GREEN**

Run: cargo test -p reprocut-report --test report_golden

Expected: golden output matches exactly.

- [ ] **Step 7: Commit**

Run:

~~~text
git add crates/reprocut-report tests/golden Cargo.toml
git commit -m "feat(report): render the reduction as a cut-line report"
~~~

---

### Task 7: CLI and generated reproduction artifact

**Files:**
- Create: crates/reprocut-cli/Cargo.toml
- Create: crates/reprocut-cli/src/main.rs
- Test: crates/reprocut-cli/tests/cli_contract.rs
- Create: README.md

**Interfaces:**
- Consumes: ReductionEngine and render_report.
- Produces: reprocut reduce [OPTIONS] -- COMMAND..., output directory, JSON state, report, and reproduction scripts.

- [ ] **Step 1: Write failing CLI tests**

Use assert_cmd against the compiled binary. Assert help text, missing command error, real fixture reduction, JSON schema fields, generated report, reproduce scripts, and non-overwrite behavior for an existing output directory.

- [ ] **Step 2: Run and verify RED**

Run: cargo test -p reprocut-cli --test cli_contract

Expected: binary target is absent.

- [ ] **Step 3: Implement the CLI**

Use clap derive. Keep stdout machine-readable when --json is active and send progress to stderr. Use atomic directory publication: write into a sibling temporary directory and rename only after final verification and report generation.

- [ ] **Step 4: Run and verify GREEN**

Run: cargo test -p reprocut-cli --test cli_contract

Expected: CLI contracts pass.

- [ ] **Step 5: Write the user README**

Lead with a real before/after command, install methods, safety boundary, supported behavior, architecture link, benchmark methodology, and limitations. Do not claim unsupported ecosystems or measured speedups.

- [ ] **Step 6: Commit**

Run:

~~~text
git add crates/reprocut-cli README.md Cargo.toml
git commit -m "feat(cli): ship one-command project reduction"
~~~

---

### Task 8: Python binding and pytest contract

**Files:**
- Create: crates/reprocut-python/Cargo.toml
- Create: crates/reprocut-python/src/lib.rs
- Create: pyproject.toml
- Create: python/reprocut/__init__.py
- Create: python/tests/test_oracle.py

**Interfaces:**
- Produces: reprocut.FailureOracle.from_baselines and FailureOracle.classify.

- [ ] **Step 1: Write failing pytest tests**

~~~python
from reprocut import FailureOracle


def test_same_failure_is_preserved() -> None:
    oracle = FailureOracle.from_baselines([
        (1, "TypeError: currency"),
        (1, "TypeError: currency"),
        (1, "TypeError: currency"),
    ])
    assert oracle.classify(1, "TypeError: currency") == "preserved"


def test_different_failure_is_rejected() -> None:
    oracle = FailureOracle.from_baselines([
        (1, "TypeError: currency"),
        (1, "TypeError: currency"),
        (1, "TypeError: currency"),
    ])
    assert oracle.classify(1, "ModuleNotFoundError") == "rejected"
~~~

- [ ] **Step 2: Build the unimplemented extension and verify RED**

Run: maturin develop --manifest-path crates/reprocut-python/Cargo.toml
Run: pytest python/tests -q

Expected: import or symbol resolution fails because the binding is absent.

- [ ] **Step 3: Implement minimal PyO3 wrapper**

Translate Python tuples into ExecutionObservation values, delegate every classification to reprocut-core, return literal lowercase verdict strings, and translate OracleError into ValueError.

- [ ] **Step 4: Build and run pytest GREEN**

Run: maturin develop --manifest-path crates/reprocut-python/Cargo.toml
Run: pytest python/tests -q

Expected: two pytest cases pass.

- [ ] **Step 5: Commit**

Run:

~~~text
git add crates/reprocut-python pyproject.toml python Cargo.toml
git commit -m "feat(python): expose the failure oracle"
~~~

---

### Task 9: Concurrency model, Miri, and CI quality gates

**Files:**
- Create: crates/reprocut-core/src/winner.rs
- Test: crates/reprocut-core/tests/loom_winner.rs
- Create: .github/workflows/ci.yml
- Create: deny.toml
- Create: .cargo/config.toml

**Interfaces:**
- Produces: LowestWinner::claim and LowestWinner::load.

- [ ] **Step 1: Write the failing Loom model**

Model two workers claiming candidate IDs 9 and 3 in both possible interleavings and assert the final winner is always 3.

- [ ] **Step 2: Run and verify RED**

Run: RUSTFLAGS="--cfg loom" cargo test -p reprocut-core --test loom_winner

Expected: LowestWinner is absent.

- [ ] **Step 3: Implement the atomic minimum primitive**

Use compare_exchange_weak with Acquire/Release ordering, usize::MAX as the empty sentinel, and cfg(loom) synchronization aliases. Keep this primitive independent from filesystem and process execution.

- [ ] **Step 4: Run Loom and verify GREEN**

Run: RUSTFLAGS="--cfg loom" cargo test -p reprocut-core --test loom_winner

Expected: all explored interleavings pass.

- [ ] **Step 5: Configure CI gates**

CI runs:

- cargo fmt --all -- --check;
- cargo clippy --workspace --all-targets --all-features -- -D warnings;
- cargo test --workspace --all-targets;
- cargo test for the Loom model under cfg loom;
- cargo +nightly miri test -p reprocut-core;
- cargo deny check;
- maturin build and pytest on Python 3.10 through 3.13;
- Windows, macOS, and Linux CLI smoke tests.

- [ ] **Step 6: Run local available gates**

Run each available command and record an explicit blocker for any host policy that prevents execution. A blocked gate is not reported as passing.

- [ ] **Step 7: Commit**

Run:

~~~text
git add crates/reprocut-core .github deny.toml .cargo
git commit -m "test: model concurrency and enforce quality gates"
~~~

---

### Task 10: Working demo, browser QA, and animated GIF

**Files:**
- Create: demo/source/bug.py
- Create: demo/source/checkout.py
- Create: demo/source/noise/
- Create: scripts/capture_demo.py
- Create: output/playwright/
- Create: assets/reprocut-demo.gif
- Modify: README.md

**Interfaces:**
- Consumes: compiled reprocut CLI and generated report.
- Produces: a reproducible demo command, browser screenshots, and assets/reprocut-demo.gif.

- [ ] **Step 1: Create a real demo project**

The demo command fails with a stable currency TypeError while unrelated modules and assets inflate the project. The failure depends on exactly three retained files.

- [ ] **Step 2: Run ReproCut against the demo**

Run: reprocut reduce --output demo/result -- python demo/source/bug.py

Expected: demo/result reproduces the same failure and the report contains measured counts from this run.

- [ ] **Step 3: Browser QA**

Open demo/result/report.html through Playwright CLI, snapshot it, capture desktop at 1440x1000 and mobile at 390x844, inspect both images, and correct clipping, contrast, focus, and reduced-motion behavior.

- [ ] **Step 4: Capture animated frames**

scripts/capture_demo.py launches Chromium, steps the report animation through deterministic data-progress values, captures 24 frames at 1200x675, and encodes an optimized looping GIF with Pillow.

- [ ] **Step 5: Verify the GIF**

Use Pillow to assert GIF format, at least 20 frames, 1200x675 dimensions, finite file size below 8 MiB, and a loop value of zero. Open the GIF for visual inspection.

- [ ] **Step 6: Add the GIF to README**

Place the working demo immediately after the one-sentence product promise and include the exact command used to regenerate it.

- [ ] **Step 7: Commit**

Run:

~~~text
git add demo scripts output/playwright assets README.md
git commit -m "docs: demonstrate ReproCut on a real failure"
~~~

---

### Task 11: Final verification and release-shaped handoff

**Files:**
- Modify: CHANGELOG.md
- Modify: docs/superpowers/specs/2026-08-11-reprocut-design.md

**Interfaces:**
- Consumes: complete workspace.
- Produces: verified MVP checklist and documented known limitations.

- [ ] **Step 1: Run formatting and static analysis**

Run:

~~~text
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo deny check
~~~

- [ ] **Step 2: Run the full Rust and Python suites**

Run:

~~~text
cargo test --workspace --all-targets
RUSTFLAGS="--cfg loom" cargo test -p reprocut-core --test loom_winner
cargo +nightly miri test -p reprocut-core
maturin develop --manifest-path crates/reprocut-python/Cargo.toml
pytest python/tests -q
~~~

- [ ] **Step 3: Run the real CLI acceptance flow**

Run the demo reduction into a fresh directory, execute both the source and reduced commands, compare normalized fingerprints, compare source tree digest before and after, and validate report/GIF artifacts.

- [ ] **Step 4: Audit the plan against the design**

Record every design capability implemented in the MVP and explicitly mark manifest-aware and syntax-aware reduction as post-MVP only if they are not present. Do not imply the full design is complete when only the file-reduction slice exists.

- [ ] **Step 5: Write changelog and known limitations**

Document supported host behavior, lack of hostile-code containment, command determinism requirements, Windows symlink behavior, and the exact local Rust toolchain policy blocker if unresolved.

- [ ] **Step 6: Commit**

Run:

~~~text
git add CHANGELOG.md docs
git commit -m "docs: record MVP verification and limits"
~~~

## Self-review

### Spec coverage

- Exact failure identity: Tasks 1-2.
- Deterministic hierarchical reduction: Task 3.
- Disposable workspace and bounded execution: Task 4.
- Real orchestration and persistence-ready outcome: Task 5.
- Distinctive self-contained report and golden testing: Task 6.
- CLI artifact: Task 7.
- Python extension: Task 8.
- Loom, Miri, proptest, linting, dependency audit, and platform CI: Task 9.
- Real browser QA and GIF: Task 10.
- Evidence-based completion and honest boundary reporting: Task 11.
- Manifest and syntax reducers remain separate independently testable product slices after the first working MVP.

### Placeholder scan

The plan contains no unspecified implementation steps. Deferred manifest and syntax work is explicitly outside this MVP rather than represented by empty tasks.

### Type consistency

ExecutionObservation, FailureFingerprint, CandidateVerdict, FailureOracle, ReductionUnit, ReductionResult, ProjectInventory, CandidateWorkspace, ProcessRunner, ReductionEngine, ReductionRequest, ReductionOutcome, ReportModel, and LowestWinner retain one spelling and ownership role throughout the plan.
