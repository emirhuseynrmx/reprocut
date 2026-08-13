# ReproCut v0.1.0 Self-Hosted Server Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Deliver a single-node self-hosted Axum server with authenticated provider ingress, durable fenced jobs, isolated workers, verified artifacts, and reconciled PR/MR publication.

**Architecture:** A modular monolith separates delivery, reduction, and outbox state machines. SQLite is the durable authority, workers have lease-scoped write authority, and provider adapters cannot construct trusted execution policy.

**Tech Stack:** Rust 1.85, Axum, Tokio, rusqlite, HMAC-SHA256, rootless OCI CLI backend.

## Global Constraints

- Production support is Linux single-host with SQLite/WAL on local storage.
- Repository/webhook data is untrusted; policy is server-controlled and type-separated.
- Every worker write requires `job_id + lease_owner + fencing_token`.
- Nothing publishable exists before `reprocut verify` returns `VerifiedArtifact`.
- Process workers require `--allow-unsafe-process-worker` and are forbidden in production profile.
- Every production change follows RED, GREEN, REFACTOR and is committed separately.

---

### Task 1: Server domain crate and configuration

**Files:**
- Create: `crates/reprocut-server/Cargo.toml`
- Create: `crates/reprocut-server/src/lib.rs`
- Create: `crates/reprocut-server/src/domain.rs`
- Create: `crates/reprocut-server/src/config.rs`
- Modify: `Cargo.toml`
- Test: `crates/reprocut-server/tests/domain_contract.rs`

**Interfaces:**
- Produces: `UntrustedJobIntent`, `TrustedExecutionPolicy`, `ResolvedJobContract`, `JobIntentId`, and `ResolvedContractId`.
- Produces: `ServerConfig::validate()` with production/process-worker refusal.

- [ ] Add compile/runtime failing tests proving untrusted fields cannot construct policy or resolved jobs and invalid production config is rejected.
- [ ] Run the server test target and observe the missing crate/API.
- [ ] Implement private-field domain types and validated constructors.
- [ ] Run domain tests; commit `feat(server): define trusted job domain`.

### Task 2: Durable state machines and migrations

**Files:**
- Create: `crates/reprocut-server/src/store.rs`
- Create: `crates/reprocut-server/src/migrations.rs`
- Test: `crates/reprocut-server/tests/store_contract.rs`

**Interfaces:**
- Produces: `JobStore` trait and `SqliteJobStore`.
- Produces: separate `DeliveryState`, `JobState`, and `OutboxState` transitions.

- [ ] Add failing migration/idempotency/illegal-transition/duplicate-delivery tests using real SQLite.
- [ ] Run store tests and observe missing schema.
- [ ] Implement schema v1, foreign keys, WAL/FULL, uniqueness, monotonic events, and compare-and-swap transitions.
- [ ] Kill/reopen the database across every transition in tests.
- [ ] Commit `feat(server): add durable state machines`.

### Task 3: Lease fencing

**Files:**
- Modify: `crates/reprocut-server/src/store.rs`
- Create: `crates/reprocut-server/src/lease.rs`
- Test: `crates/reprocut-server/tests/lease_contract.rs`
- Test: `crates/reprocut-server/tests/loom_fencing.rs`

**Interfaces:**
- Produces: `LeaseGuard { job_id, owner, fencing_token, expires_at }`.
- Produces: fenced state/artifact/verification/outbox/completion mutations.

- [ ] Add failing two-worker, expiry, stale-wake, heartbeat, and every-write-authority tests.
- [ ] Run them and observe duplicate/stale authority without implementation.
- [ ] Claim with an immediate transaction, increment fence, and predicate every worker write on the live tuple.
- [ ] Run real concurrency and Loom model tests; commit `feat(server): fence every worker mutation`.

### Task 4: Content-addressed artifact store

**Files:**
- Create: `crates/reprocut-server/src/artifact_store.rs`
- Test: `crates/reprocut-server/tests/artifact_store_contract.rs`

**Interfaces:**
- Produces: `ArtifactStore` trait and `FsArtifactStore` staging under `tmp/<job>/<fence>` and promotion under `sha256/<prefix>/<id>`.

- [ ] Add failing tests for stale promotion, crash before rename, disk-full injection, collision, tampering, and idempotent promotion.
- [ ] Run focused tests.
- [ ] Implement verified staging, fence callback, sync, atomic promotion, full verify-on-read, and cleanup.
- [ ] Run artifact/store tests; commit `feat(server): add fenced CAS artifacts`.

### Task 5: Provider-authenticated Axum ingress

**Files:**
- Create: `crates/reprocut-server/src/http.rs`
- Create: `crates/reprocut-server/src/providers/mod.rs`
- Create: `crates/reprocut-server/src/providers/github.rs`
- Create: `crates/reprocut-server/src/providers/gitlab.rs`
- Test: `crates/reprocut-server/tests/webhook_contract.rs`

**Interfaces:**
- Produces: Axum routes `/webhooks/github` and `/webhooks/gitlab`.
- Produces: raw-body signature verification and durable delivery acceptance.

- [ ] Add failing real-request tests for valid/invalid signatures, body mutation, replay/timestamp, duplicates, malformed payload, and fast enqueue response.
- [ ] Run webhook tests and observe missing routes.
- [ ] Implement bounded raw-body parsing, constant-time verification, delivery journaling, dedupe, and 2xx only after durable acceptance.
- [ ] Run webhook/store tests; commit `feat(server): authenticate provider webhooks`.

### Task 6: Exact checkout and rootless worker

**Files:**
- Create: `crates/reprocut-server/src/worker.rs`
- Create: `crates/reprocut-server/src/checkout.rs`
- Create: `crates/reprocut-server/src/oci_worker.rs`
- Test: `crates/reprocut-server/tests/worker_contract.rs`

**Interfaces:**
- Produces: `WorkerBackend`, `RootlessOciWorker`, and explicitly unsafe `ProcessWorker`.

- [ ] Add failing tests for hooks/config/submodule controls, immutable SHA, credential removal, network-off execution, limits, process-tree cleanup, and malicious workloads.
- [ ] Run worker tests in the real Linux OCI CI gate.
- [ ] Implement sanitized fetch followed by rootless read-only OCI execution and lease heartbeat/cancellation.
- [ ] Require verified artifact promotion before job verification transition.
- [ ] Run worker/isolation/crash tests; commit `feat(server): run fenced rootless reductions`.

### Task 7: Transactional outbox and provider publishers

**Files:**
- Create: `crates/reprocut-server/src/outbox.rs`
- Modify: `crates/reprocut-server/src/providers/github.rs`
- Modify: `crates/reprocut-server/src/providers/gitlab.rs`
- Test: `crates/reprocut-server/tests/outbox_contract.rs`

**Interfaces:**
- Produces: `ChangeRequestPublisher::find_existing_result` and `upsert_result`.
- Produces: deterministic hidden marker and reconciliation loop.

- [ ] Add failing crash-after-remote-success, retry, duplicate, dead-letter, and stale-fence tests against complete fake provider responses.
- [ ] Run outbox tests.
- [ ] Enqueue publication in the verified transition transaction, reconcile marker, and update instead of duplicate.
- [ ] Generate GitHub installation tokens on demand in memory; support GitLab signed/legacy modes and host allowlists.
- [ ] Run outbox/provider integration tests; commit `feat(server): reconcile PR and MR results`.

### Task 8: Server binary, health, shutdown, and packaging

**Files:**
- Create: `crates/reprocut-server/src/main.rs`
- Modify: `crates/reprocut-server/Cargo.toml`
- Modify: `.github/workflows/ci.yml`
- Modify: `.github/workflows/release.yml`
- Create: `release/server/Containerfile`
- Test: `crates/reprocut-server/tests/server_lifecycle.rs`

**Interfaces:**
- Produces: `reprocut-server` binary, readiness/liveness endpoints, graceful drain, and digest-pinned rootless image.

- [ ] Add failing lifecycle tests for startup migration/version checks, readiness, graceful lease handoff, and production unsafe-worker refusal.
- [ ] Implement binary wiring, structured redacted logs, shutdown drain, and package metadata.
- [ ] Run server unit/integration/OCI/package gates; commit `feat(server): package self-hosted ReproCut`.

