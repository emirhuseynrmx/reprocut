# ADR-001: ReproCut Server is a single-node modular monolith

- **Status:** Accepted
- **Date:** 2026-08-13
- **Release:** ReproCut v0.1.0

## Context

ReproCut needs to turn authenticated GitHub and GitLab failure requests into
verified minimal reproductions without running untrusted work inside webhook
handlers or allowing stale workers to publish. The first release must be
self-hostable by one operator and must not require a distributed database or
cluster.

## Decision

ReproCut Server uses:

- Rust and Axum for the control plane;
- local SQLite WAL for durable single-node state;
- separate delivery, reduction-job, and publication-outbox state machines;
- lease ownership plus monotonic fencing tokens for every worker write;
- rootless isolated production workers;
- content-addressed, atomically promoted artifacts;
- mandatory `reprocut verify` before publication;
- transactional outbox plus provider reconciliation for PR/MR results;
- provider-neutral domain types with GitHub and GitLab adapters.

Repository content and webhook fields are untrusted. Execution policy is
server-controlled. `UntrustedJobIntent`, `TrustedExecutionPolicy`,
`ResolvedJobContract`, and `VerifiedArtifact` are distinct domain types so the
trust boundary is enforced by construction.

The server is explicitly single-host. SQLite and its WAL remain on local
storage. Production startup requires a bundled SQLite version containing the
WAL-reset fix and a rootless worker backend. The process worker is available
only through an explicit unsafe development flag.

## Consequences

This design gives v0.1.0 a durable queue, crash recovery, idempotent webhook
handling, stale-worker exclusion, and independently verified publication with
far less operational surface than Postgres and Kubernetes.

Only one SQLite writer can commit at a time and the server cannot scale across
hosts. Domain interfaces (`JobStore`, `ArtifactStore`, `SourceProvider`,
`ChangeRequestPublisher`, and `WorkerBackend`) preserve a later migration path,
but v0.1.0 does not implement unused distributed backends.

The detailed invariants, state machines, security model, and release gates are
normative in the
[v0.1.0 release-lock design](../superpowers/specs/2026-08-13-reprocut-v0.1.0-release-lock-design.md).
