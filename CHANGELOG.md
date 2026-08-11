# Changelog

All notable changes to ReproCut are documented here.

## [Unreleased]

### Added

- Strict and flaky multi-stream same-failure oracles with bounded evidence.
- Hierarchical subset/complement reduction, deterministic parallel frontiers,
  SQLite attempt journaling, cache reuse, and compatibility-checked resume.
- Descendant-process containment on Unix process groups and Windows Job Objects.
- Cargo, Python, and npm discovery plus conservative manifest preparation.
- Tree-sitter deletion and hoisting for Rust, Python, JavaScript, TypeScript, C,
  C++, Go, and Java.
- One schema-2 evidence model for JSON, JSONL, standalone HTML, GitHub issue
  Markdown, reproducer scripts, protocol events, and the typed Python client.
- Validated OCI archive export, redacted gallery preparation, and a protocol-only
  VS Code/Cursor extension.
- A pinned, opt-in 24-case GCC/Clang upstream reduction corpus.
- A deterministic 312-file release benchmark recording raw wall time, engine
  time, oracle runs, candidate/cache counters, project mass, and sampled RSS.
- crates.io/PyPI package metadata, generated shell completions, six-target
  deterministic archive tooling, SPDX/SHA-256 aggregation, provenance workflow,
  and real failure smoke tests.
- A schema-2 18-to-3 measured demo and evidence-bound 24-frame animation.

### Changed

- Package `reprocut-cli` is published as crates.io package `reprocut` while the
  binary name remains `reprocut`.
- Python reduction now invokes the versioned Rust JSONL protocol instead of
  maintaining a second reducer.
- Failure detection defaults to stream-aware `auto` selection.

### Release status

- Version remains `0.1.0` during release-candidate development.
- crates.io/PyPI upload and the `v0.1.0` tag are intentionally left to the user.
- Native Rust, Miri, sanitizer, wheel, OCI, cross-platform archive, SBOM, and
  provenance gates are configured for clean CI because Windows Application
  Control blocks the installed local Rust executables with OS error 4551.

### Deliberate limits

- ReproCut does not promise a global minimum or root-cause diagnosis.
- Candidate commands run with user authority; the tool is not a hostile-code sandbox.
- Retained files are observed final-snapshot facts, not semantic causality claims.
- No benchmark speedup, public usage, testimonial, star, or download claim is made.
