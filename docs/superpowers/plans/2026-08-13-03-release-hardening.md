# ReproCut v0.1.0 Release Hardening Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make publication, privacy, OCI, CI evidence, dependencies, and ecosystem adapters meet the locked final-release contract.

**Architecture:** All public outputs originate from verified immutable artifacts. Release evidence is generated and reconciled for one exact commit; dependency and workflow identities are pinned and audited.

**Tech Stack:** Rust 1.85, Python, GitHub Actions/API, OCI image-layout, Cargo/Maturin/Twine.

## Global Constraints

- Bundled SQLite startup/version floor is `3.51.3` or a documented fixed backport.
- Actions use full commit SHAs and nightly toolchains use dates.
- OCI bases use immutable digest references.
- Registry writes remain manual and user-approved.
- Every production change follows RED, GREEN, REFACTOR and is committed separately.

---

### Task 1: Atomic no-replace output

**Files:**
- Create: `crates/reprocut-cli/src/publish.rs`
- Modify: `crates/reprocut-cli/src/main.rs`
- Test: `crates/reprocut-cli/tests/publication_race.rs`

**Interfaces:**
- Produces: `publish_no_replace(staged: &Path, destination: &Path) -> Result<(), PublishError>`.

- [ ] Add a failing concurrent-process test requiring exactly one winner and intact destination bytes.
- [ ] Run it against the existing check-then-rename path.
- [ ] Implement platform no-replace reservation/rename, file/directory sync, and explicit unsupported errors.
- [ ] Run race tests on every OS; commit `fix(cli): publish outputs without clobber races`.

### Task 2: Privacy and derivative redaction

**Files:**
- Create: `crates/reprocut-report/src/redact.rs`
- Modify: `crates/reprocut-cli/src/main.rs`
- Modify: `crates/reprocut-report/src/evidence.rs`
- Test: `crates/reprocut-report/tests/redaction_contract.rs`

**Interfaces:**
- Produces: `RedactionPolicy`, `VerifiedShareBundle`, and CLI `reprocut redact OUTPUT --to DESTINATION`.

- [ ] Add failing tests with token-like argv, absolute paths, diagnostics, and parent identity.
- [ ] Run tests and confirm private metadata leaks.
- [ ] Store relative paths, hash bound env values, implement creation-time flags and immutable derivative redaction.
- [ ] Run report/CLI/gallery tests; commit `feat(privacy): add verified share redaction`.

### Task 3: OCI immutable base and descriptor verification

**Files:**
- Modify: `crates/reprocut-oci/src/lib.rs`
- Modify: `crates/reprocut-oci/tests/oci_builder.rs`
- Modify: `crates/reprocut-cli/src/main.rs`

**Interfaces:**
- Produces: `ResolvedBaseImage { reference, digest }`.
- Produces: deep `verify_oci_archive` over index/manifest/config/layers.

- [ ] Add failing tests for mutable tag, changed descriptor bytes/size/media type, missing blobs, unsafe tar paths, and wrong parent artifact label.
- [ ] Run the OCI contract and observe shallow acceptance.
- [ ] Require digest resolution and verify the entire descriptor graph.
- [ ] Run real ignored OCI smoke in CI; commit `fix(oci): verify immutable image graphs`.

### Task 4: SQLite safe bundled version

**Files:**
- Modify: `Cargo.toml`
- Modify: `Cargo.lock`
- Modify: `crates/reprocut-state/src/lib.rs`
- Test: `crates/reprocut-state/tests/sqlite_version.rs`

**Interfaces:**
- Produces: startup assertion `sqlite_version_number() >= 3_051_003` while retaining Rust 1.85 compatibility.

- [ ] Add a failing version contract against current bundled SQLite 3.49.1.
- [ ] Resolve a Rust-1.85-compatible rusqlite/libsqlite source with SQLite >=3.51.3 in a trusted CI environment and commit the lockfile.
- [ ] Add runtime assertion and WAL/FULL/foreign-key/busy-timeout configuration.
- [ ] Run MSRV metadata, state concurrency, and package tests; commit `fix(state): require WAL-safe bundled sqlite`.

### Task 5: Exact-commit CI evidence

**Files:**
- Create: `scripts/release/fetch_ci_evidence.py`
- Modify: `scripts/release/audit.py`
- Modify: `.github/workflows/ci.yml`
- Modify: `.github/workflows/release.yml`
- Test: `python/tests/test_release_audit.py`
- Test: `python/tests/test_ci_evidence.py`

**Interfaces:**
- Produces: CI evidence schema 1 with run/workflow/job/matrix identities.
- Produces: final `release-evidence` job using `if: always()` and expected cardinalities.

- [ ] Add failing fixtures for wrong SHA, run attempt, workflow digest, missing/cancelled/skipped cell, duplicate logical gate, and stale artifact.
- [ ] Run focused pytest and confirm the current summary can be forged structurally.
- [ ] Implement API collector/reconciliation and exact-tag-commit release gate.
- [ ] Run static audit and fixture tests; commit `feat(release): collect exact commit CI evidence`.

### Task 6: Supply-chain pins

**Files:**
- Modify: `.github/workflows/*.yml`
- Modify: `scripts/release/audit.py`
- Test: `python/tests/test_release_audit.py`

**Interfaces:**
- Produces: full-SHA action references and dated Miri/sanitizer toolchains recorded in evidence.

- [ ] Add failing audit tests for floating action majors, `ubuntu-latest`, undated nightly, mutable images, and unlocked graph commands.
- [ ] Pin every action/runner/tool/image and add identity collection.
- [ ] Run YAML/static audits; commit `build(ci): pin release supply chain`.

### Task 7: Ecosystem adapter completion

**Files:**
- Modify: `crates/reprocut-adapters/src/lib.rs`
- Add golden fixtures under: `crates/reprocut-adapters/tests/fixtures/**`
- Modify: `crates/reprocut-adapters/tests/adapter_contract.rs`

**Interfaces:**
- Produces conservative Cargo target/workspace, Poetry, PDM, Hatch, and npm workspace-object candidates.

- [ ] Add failing real-tool and round-trip fixtures for every shape named in the release-lock spec.
- [ ] Run adapter tests and classify unsupported dynamic cases as retained limitations.
- [ ] Implement static candidates without deleting unknown keys/comments.
- [ ] Run Cargo/Python/npm parser smoke and commit `feat(adapters): cover locked manifest shapes`.

