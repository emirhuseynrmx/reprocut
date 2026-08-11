# ReproCut

**Turn a failing project into the smallest project that still fails.**

Bug reports often arrive as entire repositories. Removing “irrelevant” files by hand is slow, and one careless deletion can replace the original failure with a different one. ReproCut stabilizes the failure first, tests isolated project copies, and accepts a cut only when the same exit state and normalized diagnostic remain.

```console
$ reprocut reduce --root ./checkout --output ./minimal -- python bug.py
reprocut: proving a stable baseline and searching safe cuts...
reprocut: stable baseline preserved; 18 → 3 files
Reduced 18 files to 3. Open ./minimal/report.html
```

The original checkout is never edited. `minimal/` is published only after the reduced command passes three final verification runs.

## What the output contains

```text
minimal/
├── project/          # retained files only
├── reduction.json   # machine-readable measurements and failure identity
├── report.html      # portable, self-contained visual report
├── reproduce.sh
└── reproduce.ps1
```

The report has no web dependencies, tracking, fonts, or network requests. Send one file to a maintainer or attach the whole directory to an issue.

## Install from source

ReproCut currently requires Rust 1.85 or newer.

```sh
git clone https://github.com/emirhuseynrmx/reprocut
cd reprocut
cargo install --path crates/reprocut-cli
```

Then place the failing command after `--` so ReproCut never interprets its flags:

```sh
reprocut reduce \
  --root ./my-project \
  --output ./reprocut-output \
  --timeout-ms 5000 \
  --max-output-bytes 1048576 \
  -- cargo test parser::rejects_split_utf8
```

Use `--json` when another tool consumes standard output. Progress and diagnostics stay on standard error.

## What “same failure” means

Before minimizing, ReproCut runs the untouched project three times. A baseline is stable only when every run has:

1. the same exit code or signal;
2. complete output within the configured time and byte bounds; and
3. the same diagnostic after volatile paths, addresses, numeric IDs, whitespace, and line endings are normalized.

Candidate evaluation is deliberately three-valued:

- **preserved** — exact stabilized failure observed;
- **rejected** — command passed or failed differently;
- **inconclusive** — timeout, truncated evidence, or execution error.

Only `preserved` permits a deletion. Inconclusive evidence never becomes a shortcut to a smaller result.

## Safety boundary

- Every baseline and candidate runs in a fresh disposable directory.
- Inventory order is deterministic; symbolic links are not followed.
- Captured stdout and stderr are bounded while pipes are still fully drained.
- The output path is no-clobber: an existing file, directory, or symlink is rejected.
- Final artifacts are assembled in a sibling staging directory and renamed into place.
- Retained paths are validated as project-relative before any copy or removal.

## Current scope

The first release minimizes **regular files for arbitrary commands**. That is useful now, but intentionally narrower than the long-term system.

Not implemented yet:

- syntax-aware statement, function, or module reduction;
- ecosystem-specific manifest and lockfile rewriting;
- distributed candidate execution;
- guaranteed descendant-process cleanup after a timeout (the direct child is killed and reaped);
- semantic comparison of binary or structured diagnostics.

These limits are failure-safe: they can make a result larger or stop a run, but they must not approve the wrong failure.

## Architecture

```text
CLI
 ├── engine ── stable oracle + deterministic hierarchical reducer
 ├── runner ── bounded concurrent pipe drains + execution deadline
 ├── workspace ── sorted inventory + isolated candidates + safe publication
 └── report ── escaped, self-contained HTML
```

The complete product and implementation rationale lives in [the design specification](docs/superpowers/specs/2026-08-11-reprocut-design.md).

## Verification

The repository separates example tests from contract tests. Core behavior includes exhaustive small-universe reducer checks, normalization properties, real subprocess fixtures, source-tree immutability checks, a byte-for-byte HTML golden, responsive browser captures, and CI gates for Clippy, rustfmt, tests, Loom, Miri, and Python bindings.

Local quality commands:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```

Benchmark claims will be published only with the fixture generator, hardware, compiler version, warm-up policy, sample count, and raw Criterion output. ReproCut does not currently claim a measured speedup.

## License

Licensed under either Apache-2.0 or MIT, at your option.
