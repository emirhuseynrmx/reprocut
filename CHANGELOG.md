# Changelog

All notable changes to ReproCut will be documented in this file.

## [Unreleased]

### Added

- Stable failure oracle with volatile path, address, ID, whitespace, and newline normalization.
- Deterministic hierarchical file reducer with conservative three-valued candidate results.
- Disposable project inventories and candidate workspaces that do not follow symbolic links.
- Bounded concurrent child-output capture, deadlines, direct-child termination, and reaping.
- End-to-end engine with three baseline runs, run-local candidate caching, and three final verification runs.
- No-clobber CLI artifact containing the retained project, JSON state, self-contained HTML report, and reproduction scripts.
- Typed PyO3 oracle binding and a clearly identified source-checkout reference backend.
- Atomic lowest-candidate primitive, Loom model, property tests, Criterion fixtures, and CI definitions for Miri, AddressSanitizer, cargo-deny, native wheels, and three host operating systems.
- Measured 18-to-3 Python checkout demonstration and a verified 24-frame animated GIF.

### Verification evidence

- Official Rust Playground compilation covers exhaustive reducer subsets, failure identity, bounded streams, timeout/reap behavior, disposable workspaces, real CLI publication, and the byte-exact report golden.
- Local Python acceptance: 10 passed and 1 native-wheel-only test skipped.
- Browser acceptance at 1440×1000 and 390×844: no horizontal overflow, no console errors, visible keyboard focus, reduced-motion final state, and no external requests.
- GIF contract: 24 frames, 1200×675, infinite loop, 446,389 bytes.

### Known limitations

- Reduction is regular-file-level only; manifest and syntax reducers are not implemented.
- Candidate caching is memory-only and runs cannot resume.
- The command must fail deterministically enough to form a stable baseline.
- A disposable directory is an isolation boundary for project files, not a hostile-code security sandbox.
- Timeout handling kills and reaps the direct child but does not yet guarantee descendant-process cleanup.
- Native PyO3, Loom, Miri, Clippy, rustfmt 1.85, cargo-deny, sanitizer, and platform jobs are configured in CI but could not be executed locally because Windows Application Control blocks the installed Rust toolchain with OS error 4551.
