# Hacker News launch draft

## Title

Show HN: ReproCut – shrink a failing repository without changing the failure

## Post

I built ReproCut, a local-first reducer for bug reproductions.

You give it a failing project and command. It runs the untouched project three
times to stabilize an exit state plus normalized stdout/stderr anchors, then
searches isolated copies using directory-aware ddmin, manifest edits, and
Tree-sitter transforms. A deletion is accepted only when the same fingerprint
survives; timeout/truncation/runner errors are inconclusive. The final project
must pass the oracle three more times before it is published.

The checked-in demo goes from 18 files to 3 in 24 candidate evaluations. That is
a fixture measurement, not a speed claim. Its JSON evidence, JSONL attempt
ledger, standalone HTML report, issue body, and reproducer scripts are all in
the repository.

The systems parts I cared most about were bounded concurrent pipe draining,
Unix/Windows descendant-process containment, deterministic parallel frontier
ordering, crash-safe SQLite resume, and ensuring that incomplete evidence can
never make a candidate look smaller.

It currently handles Cargo/Python/npm manifests and syntax passes for Rust,
Python, JS/TS, C/C++, Go, and Java. There is also a typed Python client and a
thin VS Code/Cursor client over a versioned JSONL protocol.

This is still 0.1 release-candidate work. I am not claiming a global minimum,
root-cause diagnosis, or a sandbox. I would especially value adversarial cases
where the failure identity is too strict/loose, or where a reducer transform
should be added.

Repository: https://github.com/emirhuseynrmx/reprocut

## First comment

Two implementation details that may save readers a click:

1. ReproCut has a three-valued oracle (`preserved`, `rejected`, `inconclusive`).
   A timeout cannot authorize deletion.
2. Parallel workers may finish out of order, but only the earliest valid result
   in canonical rank order can commit. The accepted chain is intended to be
   identical for `--jobs 1` and `--jobs N`.

I also pinned a download-only 24-case GCC/Clang corpus from Perses for regression
work. GPL material is not bundled and is never fetched or executed implicitly.
