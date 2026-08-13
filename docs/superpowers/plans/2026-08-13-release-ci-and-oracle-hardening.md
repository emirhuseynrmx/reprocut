# ReproCut v0.1 Release CI and Oracle Hardening Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the v0.1 failure identity, state journal, native Python wheel, OCI export, and release CI contracts correct and reproducibly green without weakening safety invariants.

**Architecture:** Failure normalization remains mirrored in Rust and the pure-Python fallback under a schema-versioned contract. State transitions carry material snapshot identity while candidate digests remain evaluation/cache identity; accepted transitions must form a strict, non-no-op chain. Environment-specific tooling is installed only by the CI job that needs it, and fixtures retain strict offline boundaries.

**Tech Stack:** Rust 1.85, SQLite/rusqlite, regex, PyO3/maturin, Python 3.9-3.13, pytest, GitHub Actions, Docker Buildx, Miri, AddressSanitizer.

## Global Constraints

- Keep the public release version at `0.1.0`.
- Keep `transitions_by_output`; do not hide duplicate material outputs in SQLite.
- Keep wheelhouse validation fail-closed; wheelhouse directories contain only `.whl` files.
- Keep Rust and Python fallback normalization behavior byte-for-byte compatible.
- Keep Python `>=3.9` support while using Pillow only in a compatible dedicated demo job.
- Write and observe a failing regression test before each production behavior change.
- Do not modify or remove the existing untracked `.tmp/` and `dist/` directories.

---

### Task 1: Lexically Sound Failure Normalization

**Files:**
- Modify: `crates/reprocut-core/tests/oracle_adversarial.rs`
- Modify: `python/tests/test_oracle_contract.py`
- Modify: `crates/reprocut-core/src/diagnostic.rs`
- Modify: `python/reprocut/_fallback.py`
- Modify: `crates/reprocut-core/tests/model_contract.rs`

**Interfaces:**
- Consumes: existing `FailureOracle` normalization and classification APIs.
- Produces: normalization schema `4` with bounded labels, bounded duration units, and source-aware `at` context.

- [ ] **Step 1: Add literal adversarial tables in Rust and Python**

  Add rejected pairs for `support 404/500`, `rapid 123/999`, `pipeline 123/999`, `12msisdn/13msisdn`, and `/api/v1:404` versus `/api/v1:500`. Add preserved pairs for `port 404/500`, `PID 123/999`, `src/main.rs:12/99`, `line 12/99`, and `10 seconds/20 seconds`.

- [ ] **Step 2: Run focused Rust and Python tests and record the expected false-preservation failures**

  Run `cargo test -p reprocut-core --test oracle_adversarial` and `PYTHONPATH=python python -m pytest python/tests/test_oracle_contract.py -q`.

- [ ] **Step 3: Implement lexical boundaries in both engines**

  Require a non-word boundary before recognized labels, a numeric/unit boundary after values, remove ambiguous bare `m`, and accept `at PATH:LINE` only for recognized source-tree paths or recognized source extensions. Preserve compiler-arrow and explicit file/line contexts.

- [ ] **Step 4: Advance and document normalization schema `4`**

  Update exact serialization expectations in `model_contract.rs` and ensure the public schema constant has API documentation.

- [ ] **Step 5: Run Rust/Python parity and adversarial contracts**

  Run all oracle contract/property/adversarial tests and the Python fallback/native parity corpus.

### Task 2: Material Snapshot Transition Journal

**Files:**
- Modify: `crates/reprocut-state/tests/state_contract.rs`
- Modify: `crates/reprocut-engine/tests/engine_contract.rs`
- Modify: `crates/reprocut-engine/src/lib.rs`

**Interfaces:**
- Consumes: `TransitionRecord::new(ordinal, from, to, attempt_candidate, accepted_size)`.
- Produces: strict transition chains where `from` and `to` are `ProjectSnapshot::digest()` values and `attempt_candidate` is the cache/evaluation digest.

- [ ] **Step 1: Add state and engine regressions**

  Assert that a duplicate output under a new ordinal fails closed and rolls back evidence. Run a real structured reduction that previously triggered `transitions_by_output`, assert completion, assert every transition begins at the preceding material output, and assert `from != to`.

- [ ] **Step 2: Run the regressions and observe the UNIQUE failure or digest mismatch**

  Run focused `reprocut-state` and `reprocut-engine` tests with `--nocapture`.

- [ ] **Step 3: Separate file-frontier material digests from cache digests**

  Store both digests per frontier slot. Rank/cache/attempt lookup uses the cache digest; transition `to` and the next transition `from` use the material snapshot digest and accepted bytes use snapshot bytes.

- [ ] **Step 4: Reject structured no-op realizations before scheduling them as preserved winners**

  Compare realized material digest with the current snapshot digest. A no-op cannot produce an accepted transition or restart the structured fixpoint.

- [ ] **Step 5: Verify state causality, resume, CLI fixture, and benchmark**

  Run state/engine/CLI contracts, then build the release CLI and run `scripts/benchmark_release.py` once before the final five-run gate.

### Task 3: Reproducible Rust and OCI CI Environments

**Files:**
- Modify: `rust-toolchain.toml`
- Modify: `.github/workflows/ci.yml`
- Modify: `.github/workflows/publish-registries.yml`
- Modify: `crates/reprocut-engine/src/lib.rs`

**Interfaces:**
- Consumes: GitHub-hosted runners and Docker Buildx.
- Produces: minimal default Rust toolchain, explicit job-local components, and a container-driver Buildx builder capable of OCI export.

- [ ] **Step 1: Remove global `clippy` and `rustfmt` component mutation**

  Keep `channel = "1.85.0"` and `profile = "minimal"`; install components only in quality/registry jobs that invoke them.

- [ ] **Step 2: Configure Docker Buildx with the `docker-container` driver**

  Add `docker/setup-buildx-action@v3` before the real OCI test and retain the real `type=oci` exporter and archive validation.

- [ ] **Step 3: Apply canonical Rust formatting**

  Run `cargo fmt --all`, including the engine module order reported by CI.

- [ ] **Step 4: Validate toolchain and OCI workflow syntax**

  Run formatting checks locally and inspect workflow diffs; the real OCI contract remains a GitHub/Linux verification gate if local Docker is unavailable.

### Task 4: Python Isolation and Native Wheel Matrix

**Files:**
- Move: `tests/fixtures/python_isolation/wheels/build_fixture.py` to `tests/fixtures/python_isolation/build_fixture.py`
- Modify: `crates/reprocut-engine/tests/python_isolation_contract.rs`
- Modify: `crates/reprocut-python/src/lib.rs`
- Modify: `python/tests/test_native_backend.py`
- Modify: `pyproject.toml`
- Modify: `.github/workflows/ci.yml`
- Modify: `.github/workflows/publish-registries.yml`

**Interfaces:**
- Consumes: strict committed-wheel isolation and `FailureOracle.classify(..., stdout="")`.
- Produces: a wheel-only wheelhouse, PyO3-compatible borrowed default `stdout`, and Python 3.9-3.13 native smoke coverage.

- [ ] **Step 1: Add a default-stdout native binding regression**

  Call `classify(exit_code, diagnostic)` without `stdout` and require the same public result as explicit empty stdout.

- [ ] **Step 2: Build the native extension and observe the PyO3 default-type compiler error**

  Run `maturin build --locked` or the focused cargo check for `reprocut-python`.

- [ ] **Step 3: Borrow `stdout` at the PyO3 boundary**

  Accept `stdout: &str`, copy it only when constructing `ExecutionObservation`, and leave the public Python call signature unchanged.

- [ ] **Step 4: Move fixture generation tooling outside the wheelhouse**

  Keep the unsafe-wheel rejection logic unchanged and update fixture documentation/references to the new generator path.

- [ ] **Step 5: Split demo-image validation from the native compatibility matrix**

  Run core/native Python tests without Pillow on every Python 3.9-3.13 entry. Run demo asset tests under Python 3.12 with Pillow `12.3.0`. Update extras and publishing validation consistently.

### Task 5: Stale Contracts and Warning-Zero Quality Gate

**Files:**
- Modify: `crates/reprocut-workspace/tests/snapshot_contract.rs`
- Modify: Rust public APIs/tests reported by Clippy or rustdoc
- Modify: Python files reported by Ruff

**Interfaces:**
- Consumes: workspace lint configuration and all-target quality commands.
- Produces: formatting-, Clippy-, rustdoc-, and Ruff-clean source without blanket allowances on production code.

- [ ] **Step 1: Fix the snapshot test iterator type**

  Pass `kept.iter().copied()` to `ProjectSnapshot::from_inventory` so the iterator item is `&ReductionUnit`, not `&&ReductionUnit`.

- [ ] **Step 2: Run Clippy with the CI flags**

  Run `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings`; document public production items and remove/reuse dead production members. Use narrowly scoped test-crate documentation allowances only where integration test crates inherit `missing_docs`.

- [ ] **Step 3: Run rustdoc and Ruff gates**

  Run `RUSTDOCFLAGS=-D warnings cargo doc --locked --workspace --no-deps`, `ruff format --check`, and `ruff check`.

### Task 6: Regenerate Schema-Bound Evidence and Public Cleanup

**Files:**
- Modify: `demo/result/reduction.json`
- Modify: `demo/result/attempts.jsonl`
- Modify: `demo/result/report.html`
- Modify: `demo/result/issue.md`
- Modify: `assets/reprocut-demo.gif`
- Modify: schema-dependent Python/Rust golden files
- Modify: `README.md`
- Delete or consolidate: obsolete `docs/superpowers/plans/*`

**Interfaces:**
- Consumes: release CLI with normalization schema `4`.
- Produces: reproducible evidence whose fingerprint and visible artifacts agree with the shipped oracle.

- [ ] **Step 1: Regenerate demo reduction and derived visual assets using repository scripts**

- [ ] **Step 2: Update literal schema/fingerprint assertions from regenerated evidence**

- [ ] **Step 3: Remove stale local-machine troubleshooting prose and obsolete internal plans**

  Keep this release hardening plan and the current complete design; remove superseded implementation drafts and broken README links.

- [ ] **Step 4: Run demo asset and golden parity tests**

### Task 7: Full Release Verification and Push

**Files:**
- Create: a fresh release archive under `dist/` without deleting pre-existing untracked content.

**Interfaces:**
- Consumes: all prior task outputs.
- Produces: verified commit on `main`, pushed to `origin`, with a green GitHub Actions run.

- [ ] **Step 1: Run local release gate**

  Run Rust format, Clippy, workspace tests, docs, Python formatting/lint/tests, release build, five-run benchmark, crates packaging, wheel/sdist build, and archive-content inspection.

- [ ] **Step 2: Review the complete diff and repository status**

  Confirm no secrets, local absolute paths, generated caches, `.tmp/`, or unrelated `dist/` contents are staged.

- [ ] **Step 3: Commit the scoped changes and push `main`**

- [ ] **Step 4: Watch the resulting GitHub Actions run**

  Inspect failed job logs rather than guessing. If a newly unmasked defect appears, reproduce it, add a regression where behavioral, fix it, rerun local gates, and push the follow-up commit.

- [ ] **Step 5: Produce the final zip and report exact verification evidence**
