# ReproCut 0.1.0-alpha.1 Launch Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship a verified alpha branch containing the shared-path `doctor` preflight and an English launch asset set, without tagging or publishing packages.

**Architecture:** `doctor` reuses the engine's baseline setup and oracle stabilization used by `reduce`, then renders either human text or one bounded JSON document. Launch assets are rendered deterministically from the checked-in demo evidence, so displayed counts and fingerprints cannot drift from the artifact they describe.

**Tech Stack:** Rust 1.85 workspace, Clap CLI, Serde JSON, Python 3.10+, Pillow 12.3, Node's built-in test runner, GitHub Actions.

## Global Constraints

- Version remains exactly `0.1.0-alpha.1` in Cargo, Python, workflow, and copy surfaces.
- All launch copy and embedded image text is English.
- Public message: “Same failure. Less project.” and “Diagnose → Reduce → Verify.”
- `doctor` and `reduce` share request construction, preparation, baseline observation, and oracle stabilization.
- `doctor` writes no result directory and opens no persistent session state.
- No release tag, crates.io upload, PyPI upload, or GitHub Release is created.
- Existing evidence limitations and the zero independent-validation count remain visible.

---

### Task 1: Complete the doctor preflight contract

**Files:**
- Create: `crates/reprocut-engine/src/preflight.rs`
- Create: `crates/reprocut-cli/src/doctor.rs`
- Modify: `crates/reprocut-engine/src/lib.rs`
- Modify: `crates/reprocut-cli/src/main.rs`
- Test: `crates/reprocut-engine/tests/engine_contract.rs`
- Test: `crates/reprocut-cli/tests/cli_contract.rs`

**Interfaces:**
- Produces: `ReductionEngine::preflight(&ReductionRequest) -> Result<Preflight, EngineError>`.
- Produces: `PreflightVerdict::{Ready, PreparationRejected, CommandSucceeded, NoStableOracle}`.
- Produces: CLI `reprocut doctor [reduce arguments] [-- command ...]` and `--json` schema version 1.

- [ ] Run the focused preflight and CLI tests with `TEST_PYTHON` pointing to a real interpreter and record the current result.
- [ ] Inspect failures against `ReductionEngine::run`; add a failing regression test before any behavior change.
- [ ] Make only the minimal implementation change needed for each failing contract.
- [ ] Run all engine and CLI contract tests and confirm the source tree and `.reprocut` state remain untouched by `doctor`.
- [ ] Commit the preflight implementation and documentation together.

### Task 2: Exercise doctor against real failure shapes

**Files:**
- Modify when a missing case is found: `crates/reprocut-cli/tests/cli_contract.rs`
- Create: `tests/fixtures/doctor/unstable.py` only if the existing test helpers cannot express the unstable case.

**Interfaces:**
- Consumes: `reprocut doctor` from Task 1.
- Produces: evidence for exit codes 0, 1, and 2 and bounded JSON observations.

- [ ] Create disposable projects for a stable exception, successful command, unstable identifier, and invalid regex.
- [ ] Run the installed/debug CLI against each project and capture stdout, stderr, exit code, and filesystem before/after digest.
- [ ] If a case violates the documented contract, write the smallest failing CLI test and run it red.
- [ ] Implement the minimal fix and run the focused test green.
- [ ] Run `doctor --json` twice on the stable fixture and compare the semantic report fields while excluding timing.
- [ ] Commit any regression fix independently.

### Task 3: Redesign the deterministic launch assets

**Files:**
- Modify: `assets/reprocut-banner.svg`
- Modify: `assets/reprocut-demo.gif`
- Create: `assets/reprocut-launch.png`
- Modify: `scripts/capture_demo.py`
- Modify: `python/tests/test_demo_assets.py`

**Interfaces:**
- Consumes: `demo/result/reduction.json` schema 4 and its measured 18-to-3 reduction.
- Produces: 1600×600 SVG banner, 1920×1080 PNG launch image, and 800×450 animated GIF.

- [ ] Extend `test_demo_assets.py` to require the launch PNG dimensions, dark English banner copy, GIF dimensions, frame count, loop mode, and evidence fingerprint binding; run it and observe failure against current assets.
- [ ] Change the SVG to the approved navy/violet/orange visual system with exact English text and evidence-derived 18 → 3, 24 candidates, and strict 3/3 facts.
- [ ] Refactor `capture_demo.py` around shared palette, typography, logo, evidence, and stage-rendering helpers.
- [ ] Render the 1920×1080 launch image with “Same failure. Less project.” and “Diagnose → Reduce → Verify.”
- [ ] Render the GIF as four readable stages: doctor ready, project reduction, final 3/3 verification, verified artifact.
- [ ] Run the asset tests green and inspect the first, middle, and final animation frames at full resolution.
- [ ] Commit the asset renderer, tests, and generated files.

### Task 4: Align the launch documentation

**Files:**
- Modify: `README.md`
- Modify: `CHANGELOG.md`
- Modify: `release/README.md` if its artifact examples do not use alpha filenames.
- Test: `python/tests/test_release_metadata.py`

**Interfaces:**
- Consumes: assets and CLI behavior from Tasks 1–3.
- Produces: an English alpha landing path whose commands and asset links resolve on the feature branch and after merge.

- [ ] Add a failing metadata assertion for the launch image and exact alpha command examples.
- [ ] Update README asset references, doctor quick start, release copy, and factual demo captions.
- [ ] Remove or correct any remaining `0.1.0` stable filename or version example that conflicts with `0.1.0-alpha.1`.
- [ ] Run the metadata tests green and manually check every public URL in README.
- [ ] Commit the documentation alignment.

### Task 5: Run the complete alpha release gate

**Files:**
- Modify only when a gate exposes a real defect, with a failing regression test first.

**Interfaces:**
- Consumes: the completed branch.
- Produces: fresh local verification evidence and a pushed feature branch.

- [ ] Run `cargo fmt --all -- --check`.
- [ ] Run `cargo clippy --workspace --all-targets --all-features -- -D warnings`.
- [ ] Run `cargo test --workspace --all-targets` with an explicit working `TEST_PYTHON` on Windows.
- [ ] Run `PYTHONPATH=python python -m pytest python/tests -q`.
- [ ] Run `node --test editors/vscode/test/*.test.js` and `node --test gallery/test/*.test.js`.
- [ ] Build the sdist/wheel and run the repository release audit and local package/archive checks.
- [ ] Run the checked-in 18-file fixture through `doctor`, `reduce`, and `verify`; confirm 18 → 3 and strict 3/3 final verification.
- [ ] Run `git diff --check`, inspect the final diff, and confirm no registry credential or generated secret is present.
- [ ] Commit any final evidence-only changes and push `feat/external-ci-validation` to `origin`.
- [ ] Report every platform-only, signing, provenance, and registry check that was not executed locally.
