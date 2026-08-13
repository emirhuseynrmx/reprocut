# ReproCut v0.1.0 Correctness and Evidence Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship normalization schema 5, evidence schema 4, artifact manifests, and an independently executable `reprocut verify` publication gate.

**Architecture:** The Rust core owns normalization and fingerprint identity; the Python fallback mirrors one shared literal corpus. `reprocut-report` owns canonical evidence/artifact structures, while `reprocut-cli` performs filesystem verification and exposes verified-artifact-gated commands.

**Tech Stack:** Rust 1.85, regex, serde/serde_json, sha2, Python 3.9+, pytest.

## Global Constraints

- Version remains exactly `0.1.0`.
- Normalization schema is `5`; reduction evidence schema is `4`; artifact manifest schema is `1`.
- No probabilistic matching, shell evaluation, or silent compatibility migration.
- Only `Preserved` authorizes a cut; ambiguous observations fail closed.
- Every production change follows RED, GREEN, REFACTOR and is committed separately.

---

### Task 1: Contextual normalization schema 5

**Files:**
- Modify: `crates/reprocut-core/src/diagnostic.rs`
- Modify: `python/reprocut/_fallback.py`
- Modify: `python/tests/oracle_cases.json`
- Modify: `crates/reprocut-core/tests/oracle_adversarial.rs`
- Modify: `python/tests/test_oracle_parity.py`

**Interfaces:**
- Produces: `NORMALIZATION_SCHEMA: u16 = 5`
- Produces: `normalize_diagnostic(&str) -> String` with contextual UUID, timestamp, duration, and source-location rules.

- [ ] Add literal corpus cases for semantic/request UUID, semantic/log timestamp, timeout/elapsed duration, data-file/API/URL/source locations, and expected normalized anchors.
- [ ] Run `python -m pytest python/tests/test_oracle_parity.py -q` and record failures caused by schema-4 normalization.
- [ ] Add line-context classifiers in Rust and Python; remove data/manifest extensions as standalone source evidence; retain URLs and root-relative routes.
- [ ] Run Python parity and the Rust target `cargo test --locked -p reprocut-core --test oracle_adversarial` in CI; require identical fingerprints.
- [ ] Refactor shared context names without widening the volatility rules.
- [ ] Commit with `fix(oracle): add contextual normalization schema 5`.

### Task 2: Authoritative contract versions

**Files:**
- Create: `crates/reprocut-core/src/schema.rs`
- Modify: `crates/reprocut-core/src/lib.rs`
- Modify: `crates/reprocut-report/src/evidence.rs`
- Create: `scripts/release/schema_versions.py`
- Test: `python/tests/test_release_metadata.py`

**Interfaces:**
- Produces: `ContractVersions { normalization: 5, evidence: 4, session: 3, ci_evidence: 1, artifact_manifest: 1, server_database: 1 }`.
- Produces: machine-readable `scripts/release/schema_versions.py` constants consumed by audit/demo tooling.

- [ ] Add a failing release metadata test that rejects any README, changelog, demo, or report schema value differing from the authoritative table.
- [ ] Run the focused pytest and observe the current schema mismatch.
- [ ] Introduce the Rust version structure and Python mirror; make report constants re-export the core value instead of duplicating normalization.
- [ ] Run focused Python tests and Rust core/report contracts.
- [ ] Commit with `refactor(schema): centralize v0.1 contract versions`.

### Task 3: Evidence payload and retained manifest

**Files:**
- Create: `crates/reprocut-report/src/manifest.rs`
- Modify: `crates/reprocut-report/src/evidence.rs`
- Modify: `crates/reprocut-report/src/lib.rs`
- Modify: `crates/reprocut-report/tests/evidence_contract.rs`
- Modify: `crates/reprocut-cli/src/main.rs`

**Interfaces:**
- Produces: `RetainedEntry { path, kind, sha256, size_bytes, executable_mask, symlink_target }`.
- Produces: `EvidencePayload`, `EvidenceEnvelope`, `ArtifactMember`, and `ArtifactManifest` canonical digest methods.
- Produces: `ReductionEvidence::validate_schema_and_basic_invariants()`; the old misleading structural `validate()` name is removed.

- [ ] Write failing report tests for one-byte content identity, executable metadata, deterministic entry ordering, individual final observations, and non-circular envelope hashes.
- [ ] Run `cargo test --locked -p reprocut-report --test evidence_contract` in CI and observe missing fields/types.
- [ ] Implement canonical length-delimited hashing and evidence schema 4; construct retained entries from the verified final snapshot.
- [ ] Update JSON/report goldens and run report/CLI contract tests.
- [ ] Commit with `feat(evidence): bind retained artifact bytes`.

### Task 4: Structural verifier and verified type boundary

**Files:**
- Create: `crates/reprocut-report/src/verify.rs`
- Create: `crates/reprocut-report/tests/verify_contract.rs`
- Modify: `crates/reprocut-report/src/lib.rs`
- Modify: `crates/reprocut-cli/src/main.rs`
- Modify: `crates/reprocut-cli/tests/cli_contract.rs`

**Interfaces:**
- Produces: `verify_artifact(root: &Path) -> Result<VerifiedArtifact, VerificationError>`.
- Produces: `VerifiedArtifact` with private fields and read-only artifact ID/root accessors.
- Produces: CLI `reprocut verify OUTPUT [--rerun N]`.

- [ ] Add failing real-filesystem tests that mutate project bytes, mode, entry set, attempts, event order, report, and manifest envelope.
- [ ] Run the verifier contract and confirm every fixture fails because the command/API is absent.
- [ ] Implement bounded canonical parsing, path containment, exact member-set checking, ledger invariants, measurement checks, and report/member hashing.
- [ ] Add optional rerun orchestration using the recorded resolved contract and require exact fingerprint observations.
- [ ] Run verifier, CLI, and report tests; mutation tests must return specific integrity errors.
- [ ] Commit with `feat(verify): add independently checkable artifacts`.

### Task 5: Verification-gated derivative commands

**Files:**
- Modify: `crates/reprocut-cli/src/main.rs`
- Modify: `crates/reprocut-oci/src/lib.rs`
- Modify: `gallery/src/lib` or the existing gallery preparation entry point
- Modify: `crates/reprocut-cli/tests/cli_contract.rs`
- Modify: `scripts/release/audit.py`

**Interfaces:**
- Consumes: `VerifiedArtifact`.
- Produces: OCI/gallery derivative manifests with `parent_artifact_id`.

- [ ] Add failing CLI tests proving a tampered bundle cannot reach OCI or gallery preparation.
- [ ] Run focused tests and observe current acceptance.
- [ ] Change publication functions to consume `VerifiedArtifact`; create separate derivative identities without inserting them into the parent manifest.
- [ ] Run CLI, OCI, gallery, and static-audit tests.
- [ ] Commit with `fix(publish): require verified artifacts`.

### Task 6: Demo regeneration contract

**Files:**
- Modify: `scripts/build_demo.py`
- Modify: `scripts/release/audit.py`
- Modify: `python/tests/test_demo_builder.py`
- Modify: `python/tests/test_release_audit.py`
- Regenerate: `demo/result/**`, `assets/**`, `gallery/**`

**Interfaces:**
- Produces: demo artifacts that pass `reprocut verify demo/result` and whose source manifest matches `demo/source`.

- [ ] Add failing tests that change one demo source byte or retained byte and require audit failure.
- [ ] Run focused pytest and confirm the current stale demo escapes audit.
- [ ] Recompute source/retained manifests and measurements in audit; regenerate only through the exact release binary/protocol.
- [ ] Run demo tests, static audit, and `reprocut verify demo/result`.
- [ ] Commit with `docs(demo): regenerate verified v0.1 evidence`.

