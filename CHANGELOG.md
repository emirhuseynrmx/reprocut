# Changelog

All notable changes to ReproCut are documented here.

## [0.1.0-alpha.1] - Unreleased

### Added

- Strict and flaky multi-stream same-failure oracles with bounded evidence.
- Hierarchical subset/complement reduction, deterministic parallel frontiers,
  SQLite attempt journaling, cache reuse, and compatibility-checked resume.
- Descendant-process containment on Unix process groups and Windows Job Objects.
- Cargo, Python, and npm discovery plus conservative manifest preparation.
- Tree-sitter deletion and hoisting for Rust, Python, JavaScript, TypeScript, C,
  C++, Go, and Java.
- Evidence schema 4 for JSON, JSONL, standalone HTML, GitHub issue Markdown,
  reproducer scripts, protocol events, and the typed Python client.
- Validated OCI archive export, redacted gallery preparation, and a protocol-only
  VS Code/Cursor extension.
- A pinned, opt-in 24-case GCC/Clang upstream reduction corpus.
- A deterministic 312-file release benchmark recording raw wall time, engine
  time, oracle runs, candidate/cache counters, project mass, and sampled RSS.
- crates.io/PyPI package metadata, generated shell completions, six-target
  deterministic archive tooling, SPDX/SHA-256 aggregation, provenance workflow,
  and real failure smoke tests.
- A normalization-schema-5, 18-to-3 measured demo and evidence-bound 24-frame
  animation.
- A committed Cargo dependency graph consumed with `--locked` by CI, release,
  crates.io, and PyPI build paths.
- `reprocut doctor` runs the checks a reduction runs before its first cut and
  stops there. It takes the same arguments as `reduce` and shares the engine's
  code path rather than repeating its logic, so its answer is the answer the
  reduction would give. It executes the command, writes no output directory, and
  opens no session state. `--json` emits one versioned document on stdout; exit
  codes are 0 ready, 1 not ready, 2 unusable request. Reports carry a bounded
  output tail (ten lines per stream per run, 240 bytes per line) declared in
  `output_disclosure`.
- `reduce --command-line` splits one command string into argv on documented
  quoting rules, with no expansion of variables, globs, or substitutions.
- The action uploads the verified artifact, and takes `artifact-name` and
  `upload` to name or suppress it.

### Fixed

- The action ran the command through the runner's shell, so quotes reached the
  program as argument bytes and globs and variables resolved against the runner.
  It now hands the command to `--command-line` unexpanded.
- The action's summary said the artifact was attached to the run when nothing
  uploaded it. It is uploaded now, and the summary says so only when it is.
- Reports embedded whatever line endings the builder's checkout held, so an
  artifact produced on one platform could fail verification against a binary
  built on another. Embedded templates are normalized before rendering, and
  `.gitattributes` pins the asset line endings.
- The summary and comment steps ran only when the reduction removed something,
  hiding a verified artifact published when the budget ended the search.
- Diagnostic drift compared the union of the final verification runs against the
  original, so any line that differed between runs of the same snapshot counted
  as novel: an elapsed time, a progress bar, a Python object address. A correct
  reduction of a Python project reported drift against itself. The minimized side
  is now the intersection of its runs, so a line counts only when every run
  printed it. The original stays a union. Failure identity is unchanged.

### Changed

- Package `reprocut-cli` is published as crates.io package `reprocut` while the
  binary name remains `reprocut`.
- Python reduction now invokes the versioned Rust JSONL protocol instead of
  maintaining a second reducer.
- Failure detection defaults to stream-aware `auto` selection.
- Evidence schema 4, normalization schema 5, and session contract schema 3 are
  independent versioned contracts.

### Release status

- Prepared as `0.1.0-alpha.1`: an alpha, measured on real projects and looking for its first users.
- crates.io/PyPI upload and the `v0.1.0-alpha.1` tag are intentionally left to the user.
- Native Rust, Miri, sanitizer, wheel, OCI, cross-platform archive, SBOM, and
  provenance gates are configured for clean CI because Windows Application
  Control blocks the installed local Rust executables with OS error 4551.

### Known blocked path

- `scripts/build_demo.py --refresh` cannot run. The checked-in demo is generated
  by the Rust Playground on purpose, so the artifact does not depend on the
  author's machine; the composed workspace now exceeds what that service will
  compile, and the failure reproduces on an unmodified checkout. Until it is
  fixed, no change that alters failure fingerprints can ship, because the demo
  cannot be regenerated to match.

### Deliberate limits

- ReproCut does not promise a global minimum or root-cause diagnosis.
- Candidate commands run with user authority; the tool is not a hostile-code sandbox.
- Retained files are observed final-snapshot facts, not semantic causality claims.
- No benchmark speedup, public usage, testimonial, star, or download claim is made.
- The wall-time budget bounds the search only. Proving the baseline and verifying
  the final snapshot fall outside it, so a run finishes after its budget elapses.
- The Python client has no wall-time budget; `--max-duration-secs` is a Rust CLI
  option, and a client timeout kills the process instead of publishing a result.
- Diagnostic drift is a warning that the minimized project prints lines the
  original never did. It is not proof that the cause is unchanged.
- A passing `doctor` means the failure was proven stable and recognizable. It is
  not a prediction that the search will find a smaller project.
- `doctor` reports the program the engine handed the runner. It does not perform
  its own `PATH` resolution and does not diagnose why a program is unusable; it
  shows what ran and what came back.
- Normalization does not rewrite Python's `<Thing object at 0x...>` addresses.
  Doing so changes every fingerprint and needs a normalization-schema bump, which
  needs a regenerated demo artifact; that path is blocked (see below).
