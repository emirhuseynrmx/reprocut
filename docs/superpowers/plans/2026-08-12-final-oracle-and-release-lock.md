# ReproCut Final Oracle and Release Lock Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use test-driven-development and
> verification-before-completion for every behavioral task. This plan is
> executed inline because the user requested immediate implementation and no
> subagent delegation is authorized in this session.

**Goal:** Close the final automatic-oracle, normalization-parity, dependency-lock,
and changelog blockers without changing version `0.1.0`.

**Architecture:** `reprocut-core` and the Python fallback retain one equivalent
oracle model. Stream-aware selection and contextual source locations are small
private helpers. Release workflows consume one committed `Cargo.lock`, and the
static audit enforces that contract.

**Tech Stack:** Rust 1.85, `regex`, Python 3.9-3.13, pytest, Cargo, maturin,
GitHub Actions YAML, official Rust Playground verification.

## Global Constraints

- Keep package version `0.1.0`.
- Keep evidence schema 3, normalization schema 3, session contract schema 2.
- Do not publish, tag, push, or contact registries.
- Do not add runtime dependencies or broaden normalization beyond the approved
  contexts.
- Every behavior change begins with a regression observed failing.

---

### Task 1: Automatic stream coverage and deterministic ranking

**Files:**
- Modify: `crates/reprocut-core/tests/oracle_adversarial.rs`
- Modify: `crates/reprocut-core/src/diagnostic.rs`
- Modify: `python/tests/test_oracle.py`
- Modify: `python/reprocut/_fallback.py`

**Interfaces:**
- Consumes: existing `FailureOracle` baseline/classification API.
- Produces: private stream-aware selection with a maximum of four anchors and
  rank key `(kind, Reverse(score), position, channel_order, text)`.

- [ ] Add Rust and Python tests where stdout fills four discriminator
  categories and stderr contains `fatal: disk exploded`; assert both channels
  are fingerprinted and changed stderr is rejected.
- [ ] Run both focused suites and record that the new tests fail because stderr
  is absent.
- [ ] Add `select_auto_anchors` in Rust and the equivalent quota branch in
  Python. Reserve each non-empty eligible stream before category/global fill.
- [ ] Add a Rust private ranking test using reversed equal candidates and a
  Python public fingerprint-order contract for equal cross-stream lines.
- [ ] Add the explicit channel order to both rank keys and rerun focused tests.

### Task 2: Contextual source locations and duration parity

**Files:**
- Modify: `crates/reprocut-core/tests/oracle_adversarial.rs`
- Modify: `crates/reprocut-core/src/diagnostic.rs`
- Modify: `python/tests/test_oracle.py`
- Modify: `python/reprocut/_fallback.py`
- Modify: `python/tests/oracle_cases.json`

**Interfaces:**
- Consumes: `normalize_diagnostic` and Python `_normalize` through real oracle
  fingerprints.
- Produces: source-location predicate receiving token plus explicit-context
  state; longest-first duration normalization.

- [ ] Add literal route and URL tests requiring `404` to differ from `500`.
- [ ] Add literal positive tests for `at src/module:12` versus line 99,
  `src/main.rs`, and an extensionless `Makefile`.
- [ ] Add a seconds entry to the parity corpus with expected anchor
  `RuntimeError: failed after <duration>`.
- [ ] Run the Python and remote Rust tests and record the route/URL and Rust
  seconds failures.
- [ ] Capture optional `at ` / `--> ` context in the location regex; accept
  only recognized extension, `<temp>`, explicit context, or conventional
  build-file basename.
- [ ] Reorder the Rust duration alternatives to match Python longest-first.
- [ ] Regenerate literal parity digests and rerun parity/oracle suites.

### Task 3: Committed dependency graph and locked workflows

**Files:**
- Create: `Cargo.lock`
- Modify: `.github/workflows/ci.yml`
- Modify: `.github/workflows/release.yml`
- Modify: `.github/workflows/publish-registries.yml`
- Modify: `scripts/release/audit.py`
- Modify: `python/tests/test_release_audit.py`

**Interfaces:**
- Consumes: pinned Rust 1.85 workspace manifests.
- Produces: checked-in lockfile and `dependency-lock` release-audit check.

- [ ] Add a release-audit regression that requires `dependency-lock` and run it
  RED while `Cargo.lock` is absent.
- [ ] Generate `Cargo.lock` with official Cargo 1.85 in an isolated container
  and validate it using `cargo metadata --locked`.
- [ ] Remove all workflow `cargo generate-lockfile` calls and runtime lockfile
  artifact transfer.
- [ ] Put `--locked` on every Cargo/maturin graph-consuming workflow command.
- [ ] Implement the audit by scanning actual workflow command lines; reject
  missing lockfiles, regeneration, and unlocked graph commands.
- [ ] Parse all four YAML workflows and rerun the release audit tests.

### Task 4: Release language and regenerated evidence

**Files:**
- Modify: `CHANGELOG.md`
- Regenerate: `demo/result/*`, `assets/reprocut-demo.gif`,
  `assets/reprocut-banner.svg`, `tests/golden/reduction-report.html` only when
  their measured fingerprint changes.

**Interfaces:**
- Consumes: final oracle implementation and demo fixture.
- Produces: dated 0.1.0 changelog with unambiguous schema labels and internally
  consistent checked-in proof.

- [ ] Replace `[Unreleased]` with `[0.1.0] - 2026-08-12`.
- [ ] State evidence schema 3, normalization schema 3, and session contract
  schema 2 separately; remove both stale `schema-2 evidence` phrases.
- [ ] Rebuild demo/report/golden assets, compare fingerprints, and retain only
  generated changes justified by the final implementation.

### Task 5: Final verification and handoff

**Files:**
- Create: `docs/verification/2026-08-12-final-oracle-and-lock.md`
- Regenerate: `dist/reprocut-0.1.0-source.zip`

**Interfaces:**
- Consumes: clean tracked `HEAD`.
- Produces: exact verification record, source ZIP, SHA-256, and no publication.

- [ ] Run the complete Python suite and bytecode compilation.
- [ ] Run Rust adversarial, oracle-mode parity, report, golden, and full CLI
  contracts through the available official Rust toolchain service.
- [ ] Run rustfmt over every modified Rust file, all static release gates,
  workflow YAML parsing, generated gallery tests/build, and `git diff --check`.
- [ ] Commit production/release changes, then write and commit the verification
  record bound to that source commit.
- [ ] Create the ZIP with `git archive`, read every entry, reject extra roots,
  traversal, symlinks, `.tmp`, `dist`, `target`, and `__pycache__`, then report
  size and SHA-256.

