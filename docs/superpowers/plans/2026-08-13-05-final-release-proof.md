# ReproCut v0.1.0 Final Release Proof Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Demonstrate on one immutable commit that every locked correctness, security, packaging, and release invariant passes before the user publishes v0.1.0.

**Architecture:** Static audit checks repository-declared invariants; exact-commit CI evidence supplies native and matrix proof; release smoke verifies generated artifacts and clean public-package candidates.

**Tech Stack:** Cargo, Maturin, Twine, pytest, Node test runner, GitHub Actions, OCI runtime, SBOM/provenance tooling.

## Global Constraints

- Missing, skipped, cancelled, stale, or differently committed evidence fails.
- No final tag or registry write occurs in this plan; those remain user-controlled.
- Verification commands run fresh and their outputs are recorded.
- Every discovered regression receives a failing test before its fix.

---

### Task 1: Full fault-injection suite

**Files:**
- Create/modify focused tests under `crates/**/tests`, `python/tests`, and `scripts/verification` named by the release-lock Section 12 cases.
- Modify: `.github/workflows/ci.yml`

**Interfaces:**
- Produces: logical gates `oracle-adversarial`, `artifact-integrity`, `snapshot-integrity`, `session-integrity`, `server-crash-recovery`, `worker-isolation`, and `provider-reconciliation`.

- [ ] Enumerate every Section 12 mutation/crash and map it to an executable test target.
- [ ] Add any missing test first and observe its correct RED failure.
- [ ] Implement only the missing invariant and rerun focused plus neighboring suites.
- [ ] Run the complete fault matrix; commit `test(release): prove v0.1 failure invariants`.

### Task 2: Documentation and archaeology audit

**Files:**
- Modify: `README.md`, `CHANGELOG.md`, `docs/RELEASING.md`, `docs/release/0.1.0.md`, `docs/launch/**`
- Modify: `scripts/release/audit.py`
- Modify/remove local-machine references under `docs/superpowers/**`

**Interfaces:**
- Produces: synchronized version/limitation/security language and no personal runtime paths in public package contents.

- [ ] Add failing audit fixtures for stale schema names, overclaims, local paths, secret guidance, and unsupported ecosystem claims.
- [ ] Update documents from authoritative constants and explicit capability language.
- [ ] Run docs/static audit and commit `docs(release): synchronize final v0.1 contracts`.

### Task 3: Complete local verification

**Files:**
- Produce: `docs/verification/2026-08-13-v0.1.0-release-lock.md`

**Interfaces:**
- Records exact commands, versions, exit codes, and the local Application Control limitation without converting it to success.

- [ ] Run Python 3.9-3.13/fallback/native parity where available, Node editor/gallery, static audit, demo verify, archive tests, and package metadata checks.
- [ ] Run Rust format/Clippy/test/doc/package commands in an environment where Rust binaries execute.
- [ ] Record every result and unresolved limitation; do not mark absent evidence green.
- [ ] Commit `docs(verify): record v0.1 release-lock evidence`.

### Task 4: Exact-commit CI and release artifacts

**Files:**
- Produce: `output/ci-evidence.json`
- Produce: native archives, wheels, sdist, server OCI, SBOMs, checksums, provenance.

**Interfaces:**
- Consumes exact HEAD SHA and CI evidence schema 1.
- Produces a complete dry-run release set without registry upload.

- [ ] Push the implementation branch and wait for every required workflow/matrix gate.
- [ ] Fetch and reconcile exact-commit evidence; reject any missing or stale cell.
- [ ] Run `scripts/release/audit.py --ci-evidence ... --expected-commit ...`.
- [ ] Download artifacts, verify checksums/provenance, clean-install packages, and rerun real failure smoke.
- [ ] Fix regressions only through new RED tests and repeat until exact-commit evidence is complete.

### Task 5: Final handoff

**Files:**
- Update: `docs/release/0.1.0.md`
- Create: final source ZIP outside tracked source after HEAD is fixed.

**Interfaces:**
- Produces: immutable commit SHA, ZIP SHA-256, verified artifact inventory, and manual publication commands.

- [ ] Confirm tracked tree cleanliness and that all release artifacts resolve the exact commit.
- [ ] Create the source ZIP and independently inspect its file list and digest.
- [ ] Present the user with signed-tag, protected-environment, crates.io, and PyPI actions; do not execute irreversible publication without the user's final action.

