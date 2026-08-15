# ReproCut v0.1.0 Evidence and Panic-Safety Design

## Objective

Close three release-credibility gaps without inflating claims:

1. keep the tiny onboarding demo, but disclose its exact size;
2. add reproducible evidence at synthetic scale and on a real upstream compiler bug;
3. remove `unwrap`/`expect` from non-test library and binary targets and prevent regressions.

Independent adoption cannot be manufactured. The release will expose a verifiable contribution
path and report zero independent validations until a third party actually submits one.

## Evidence tiers

### Tier 1: onboarding fixture

The existing 18-to-3 Python demo remains the fast, checked-in walkthrough. README and launch copy
will call it a tiny onboarding fixture and report all four dimensions:

- 18 files to 3 files;
- 55 source lines;
- 1,669 source bytes;
- 24 candidate evaluations and 3/3 final verification runs.

It is evidence that the user flow works, not evidence of large-project performance.

### Tier 2: deterministic scale benchmark

The existing 312-file benchmark remains the repeatable performance fixture. Its generated nature
will be stated beside every result. The benchmark already records per-run wall time, engine time,
candidate attempts, oracle runs, process-tree peak RSS, and output digests. CI will retain the raw
JSON and Markdown artifacts and validate their schema and invariants.

This tier answers whether the release behaves consistently on hundreds of files. It does not claim
to represent the dependency or semantic complexity of a real repository.

### Tier 3: real upstream compiler-bug benchmark

Use Perses subject `clang-26760`, pinned by the existing `benchmarks/upstream-corpus.json` manifest.
The fetched subject contains five files, 33,171 source lines, and 1,944,800 bytes in the locally
validated corpus snapshot. It reproduces a Clang 3.6.0 `-O3` compilation failure.

The runner will:

1. require the existing explicit GPL acknowledgement before fetching the Perses subject;
2. verify every downloaded file against the allowlisted digest in the corpus manifest;
3. download the official LLVM 3.6.0 Ubuntu 14.04 binary archive from `releases.llvm.org` and verify
   a pinned SHA-256 digest;
4. execute only a repository-owned oracle wrapper, never the upstream `r.sh`;
5. establish the failure fingerprint three times before reduction;
6. run ReproCut against a fresh copy with explicit time and output limits;
7. verify the minimized result three more times;
8. emit machine-readable benchmark metadata, the ReproCut evidence bundle, and a Markdown summary;
9. avoid checking GPL corpus contents or the LLVM archive into the repository.

The workflow will be opt-in (`workflow_dispatch`) and scheduled, not a required pull-request job,
because it downloads a historical toolchain and executes a long reducer run. A lightweight CI test
will still validate the manifest, checksums, command construction, license acknowledgement, and
result parser without network access.

If the historical binary cannot run on the host, the benchmark must fail as an environment error.
It must not silently replace the compiler or downgrade the result to a successful proof.

## Production panic policy

The supported claim is narrowly defined: non-test library and binary targets contain no direct
`unwrap()` or `expect()` calls. The project will not claim that arbitrary dependency code cannot
panic or that all Rust panics are impossible.

Changes by subsystem:

- diagnostic regexes: compile a shared pattern bank through a fallible constructor and propagate a
  typed initialization error; capture access becomes conditional;
- oracle construction: remove assumptions that automatic and previously validated specs can be
  recompiled infallibly; carry typed configuration errors to callers;
- report rendering: replace `writeln!(String).expect(...)` call sites with an internal infallible
  string builder API based on `push_str` and explicit numeric formatting;
- state encoding: replace the `usize`-to-`u64` assertion with checked conversion and a state error;
- transformation hashing/escaping: replace formatting assertions with a hexadecimal lookup table;
- CLI parsing: turn the clap-guaranteed non-empty command invariant into a typed CLI error;
- unreachable workspace states remain outside this specific gate and keep their explicit invariant
  documentation unless a separate audit shows they are reachable from untrusted input.

CI adds a production-target gate:

```console
cargo clippy --locked --workspace --lib --bins -- \
  -D clippy::unwrap_used -D clippy::expect_used
```

The existing all-target Clippy job remains, so tests continue to receive normal lint coverage while
the production-only policy does not force noisy rewrites of test setup.

## Independent validation path

Add a GitHub issue form for real-world validation. A submission must include:

- ReproCut version and commit;
- platform and runner version;
- sanitized input/output size metrics;
- baseline and final fingerprints;
- the generated evidence manifest or its digest;
- consent to list the result publicly.

A gallery entry is generated only from a reviewed submission whose evidence verifier passes. The
README initially says `Independent validations: 0`; no seeded, self-authored, or friend-authored
entry is presented as external adoption. crates.io and PyPI publication remain a maintainer action
and are not performed by this implementation.

## Documentation and release behavior

README replaces the single headline number with a compact evidence table that labels each tier and
links to reproduction instructions. CV-safe wording will distinguish checked-in evidence from
external adoption. The release gate requires:

- all Rust tests and Python tests;
- formatting and all-target Clippy;
- the production `unwrap`/`expect` lint gate;
- the 312-file benchmark schema/invariant test;
- the offline upstream benchmark contract tests;
- package and documentation checks already present in CI.

The repository may be made public and the release branch merged only after required CI is green.
Registry publication is explicitly out of scope.

## Acceptance criteria

- README discloses the tiny demo's lines and bytes next to its file count.
- A reproducible, pinned, license-aware `clang-26760` benchmark exists and never executes upstream
  shell code.
- Benchmark outputs distinguish measured facts, fixture facts, and unavailable measurements.
- `cargo clippy --workspace --lib --bins` rejects any production `unwrap` or `expect` call.
- Existing behavior remains covered by tests, and new fallible paths have regression tests.
- External-validation count and gallery cannot be advanced without a verifier-passing submission.
- No claim of published packages, users, downloads, or independent validation is made without the
  corresponding external fact.
