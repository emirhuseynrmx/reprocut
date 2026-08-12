# ReproCut 0.1.0 Final Oracle and Release-Lock Design

**Status:** Approved by the attached release review and the instruction to
implement every item.  
**Version:** ReproCut remains `0.1.0`.

## Goal

Close the remaining automatic-oracle false positives and make every published
artifact resolve the committed Rust dependency graph. The correction is
surgical: no reducer architecture, public CLI, or evidence-schema redesign.

## Oracle selection

`auto` continues to ignore generic message-only lines. It ranks eligible lines
within stdout and stderr, reserves the best line from every error-bearing
stream, then fills the four-anchor budget by category diversity followed by
the global rank. Unlike `combined`, it does not require both streams to contain
an eligible line. `combined` retains its stricter both-stream precondition and
its existing stdout/stderr reservation.

Rust and Python use the explicit channel order `stdout = 0`, `stderr = 1` in
the rank key after kind, descending score, and source position. Text remains
the final tie-breaker. Anchor ordering therefore does not depend on an unstable
sort or a language runtime's stable-sort implementation.

## Source-location recognition

A `token:number` candidate is volatile only when one of these rules holds:

- the token ends in a recognized source or manifest extension;
- the token is the normalized `<temp>` path marker;
- the token follows an explicit `at ` or `--> ` source-location context;
- its basename is a conventional extensionless build file: `Makefile`,
  `Dockerfile`, `BUILD`, or `WORKSPACE`.

A slash alone is not evidence of a source location. API routes and URLs such as
`/api/v1:404` and `https://example.com/v1:404` retain their numeric value.
False negatives are preferred over erasing a semantic failure discriminator.
Explicit `line N` and `column N` normalization remains unchanged.

## Rust/Python parity

The Rust duration alternation is ordered longest-to-shortest exactly like the
Python fallback, preventing `10 seconds` from becoming `<duration>econds`.
The shared parity corpus gains a seconds case with the literal expected anchor
`RuntimeError: failed after <duration>`. Existing normalization schema 3 is
corrected in place because 0.1.0 has not been tagged or published; all affected
fixtures and generated evidence are regenerated before the final source ZIP.

## Release dependency identity

`Cargo.lock` is generated with the pinned Rust 1.85 toolchain and committed.
No workflow may run `cargo generate-lockfile`. Every workflow command that can
resolve or build a Cargo graph uses `--locked`, including test, clippy, bench,
doc, build, run, package, publish, Miri, sanitizer, and maturin build/sdist
paths. Formatting and `miri setup` do not resolve the workspace graph and are
outside this rule.

The release workflow validates the checked-in lock with
`cargo metadata --locked`; it no longer creates or transfers a runtime
lockfile artifact. The static release audit fails closed when the lock is
missing, a workflow regenerates it, or a graph-consuming command omits
`--locked`.

## Release communication

`CHANGELOG.md` moves the finished work to `## [0.1.0] - 2026-08-12` and names
the three independent versions explicitly:

- evidence schema 3;
- normalization schema 3;
- session contract schema 2.

Registry publication and the immutable tag remain user-controlled.

## Verification

Tests must demonstrate RED before production edits for auto stream coverage,
route/URL identity, seconds parity, channel ordering, and the release-lock
audit. GREEN verification covers the full Python suite, Rust oracle/adversarial
and parity contracts, report/golden contracts, all workflow YAML files, release
audit, rustfmt, generated demo/gallery assets, archive structure, and a fresh
tracked-HEAD source ZIP.

