![ReproCut - same failure, less project](https://raw.githubusercontent.com/emirhuseynrmx/reprocut/main/assets/reprocut-banner.svg)

# ReproCut

Turn a failing repository into the smallest project that still fails the same
way — with evidence that it is the same failure.

ReproCut removes parts of a failing project while checking that the original
failure still occurs. It works on a copy, not your checkout. It establishes a
repeatable baseline, tests candidate reductions in fresh snapshots, and
publishes a result only after final verification satisfies the selected
evaluation policy.

The result is not a claim to be trusted. It is an artifact anyone can re-run:
`reprocut verify` re-checks the declared files, byte hashes, retained project,
attempt ledger, and reproduction scripts independently of the run that made
them.

![A ReproCut run reducing 18 files to 3](https://raw.githubusercontent.com/emirhuseynrmx/reprocut/main/assets/reprocut-demo.gif)

The GIF is a tiny onboarding fixture: **18 files to 3**, from **55 lines** and
**1,669 bytes**, in **24 candidate evaluations**, followed by **3/3 final
verification runs**. Its
[evidence](https://github.com/emirhuseynrmx/reprocut/blob/v0.1.0/demo/result/reduction.json),
[attempt log](https://github.com/emirhuseynrmx/reprocut/blob/v0.1.0/demo/result/attempts.jsonl),
and
[HTML report](https://github.com/emirhuseynrmx/reprocut/blob/v0.1.0/demo/result/report.html)
are checked in. It demonstrates the user flow; it is not a large-project
benchmark.

## Evidence, without mixing unlike claims

| Tier | Subject | Scale | What exists today |
|---|---|---:|---|
| Tiny onboarding fixture | Checked-in Python example | 18 → 3 files; 55 lines; 1,669 bytes | Complete evidence bundle |
| Synthetic 312-file fixture | Deterministically generated Python project | 312 files; five fresh measured runs | Raw CI benchmark artifact |
| Upstream real case, reduced | Pinned `openruyi-precommit-hooks` regression | 95 → 1 files; 15,749 → 69 lines; 1,501,477 → 551 bytes | Verified evidence bundle from CI |
| Upstream real case, reduced | Pinned `ipe-lang` stale-port regression | 4,632 → 769 files; 2,801,354 → 101,808 lines; 481 → 5.6 MB | Verified bundle; budget ended the search |
| Upstream real case | Pinned Perses `clang-26760` compiler bug | 2 C files; 33,171 lines; 1,933,944 source bytes | Download-only provenance; opt-in benchmark |
| Independent | Third-party projects | **Independent validations: 0** | No external adoption claim yet |

The 312-file fixture measures scale and repeatability, but it is generated and
does not stand in for a complex production repository.

Each reduced upstream row is a real third-party repository at pinned base and
head commits, run in a network-disabled container: the base passed three times,
the head failed three times, the reduction ran, and the minimized project failed
the same way three more times. `reprocut verify` re-checked the bundle
independently, and in both cases the minimized project's diagnostic contained no
line the original failure had not already printed. The runs are self-authored, so
they do not move the independent count.

Do not take those numbers on trust. The records are committed under
[`benchmarks/external-validation/`](benchmarks/external-validation/): what was
pinned, the oracle contract, and the failing command's own output on the original
and on the minimized project, so the two can be read side by side.

The `ipe-lang` search ended on its wall-time budget rather than converging, which
its record states. Every retained file still passed final verification; a longer
budget would reduce further.

The remaining upstream row records real, pinned source provenance; reduction
measurements remain unavailable until the opt-in historical-toolchain workflow
completes successfully. A reviewed third-party evidence submission—not a
self-authored example—is required before the independent count can increase.

ReproCut 0.1 is an alpha release focused on deterministic, evidence-backed
project reduction. Its checked-in result uses schema-4 evidence. Failure
identity uses schema-5 normalized diagnostics.

## In CI

The place a failing project is most often in front of someone is a red CI job.
Add the action after the step that failed:

```yaml
- name: Run tests
  id: tests
  run: cargo test

- uses: emirhuseynrmx/reprocut@v0.1.0
  if: failure() && steps.tests.outcome == 'failure'
  with:
    command: cargo test
    max-duration-seconds: 600
```

It downloads the checksummed release binary for the runner, reduces the project
inside the budget, writes the before/after mass into the job summary, and
comments on the pull request:

> **1,284 files · 37.4 MB** → **3 files · 11.8 KB**  (99.9% smaller)
> The minimized project fails the same way.

The budget matters in CI. Reduction converges asymptotically, so an unbounded run
is eventually killed by the job timeout and yields nothing. On a 703-file project
a two-minute budget reached 93.7% of the original mass removed; an unbounded run
reached 96.3% after 64 minutes. The budgeted result is fully verified, and its
evidence records that the budget, not the search, ended it.

## Quick start

Install the Rust CLI with Cargo 1.85 or newer:

```console
cargo install reprocut --version 0.1.0 --locked
```

The Python package provides native failure-oracle bindings, evaluation policy,
the typed client, and the `reprocut-py` console script:

```console
python -m pip install reprocut==0.1.0
```

The Python package does not bundle the Rust reducer CLI. Full project reduction
from Python resolves `reprocut` through `REPROCUT_BINARY` or `PATH`, so install
both packages when using `reduce()`.

To build the CLI from source:

```console
git clone https://github.com/emirhuseynrmx/reprocut.git
cd reprocut
cargo install --path crates/reprocut-cli
```

For Cargo, Python, and npm projects, ReproCut can discover the usual test
command:

```console
reprocut minimize --root ./failing-project --output ./minimal
```

To provide the failing command explicitly, put it after `--`:

```console
reprocut reduce \
  --root ./compiler-bug \
  --output ./minimal \
  --jobs 8 \
  --timeout-ms 5000 \
  -- cargo test parser::rejects_split_utf8
```

The command and its arguments are passed through without shell parsing.

## What it reduces

ReproCut searches several layers until no explored transformation can make the
current result smaller:

- directory and file subsets, including subset and complement `ddmin` passes;
- dependencies and targets in `Cargo.toml`, `pyproject.toml`, and `package.json`;
- syntax nodes in Rust, Python, JavaScript, TypeScript, C, C++, Go, and Java.

Syntax edits are reparsed before execution. Candidate timeouts, truncated output,
preparation failures, and runner failures are inconclusive and cannot authorize
a cut.

The result is locally 1-minimal for the transformations ReproCut explored. It is
not a proof of the globally smallest equivalent program.

## Defining the failure

Automatic mode derives a stable failure identity from repeated baseline runs.
It considers process termination and normalized diagnostic anchors from stdout,
stderr, or both. Source locations may vary; semantic values such as
`status:404`, `expected:123`, and `shard:12` do not.

For failures that need an explicit contract, use regular expressions:

```console
reprocut reduce \
  --oracle-mode regex \
  --failure-regex 'TypeError' \
  --failure-regex 'currency' \
  --reject-regex 'secondary failure' \
  -- python bug.py
```

For an interestingness script that returns success when a candidate should be
kept:

```console
reprocut reduce --oracle-mode exit-zero -- ./interesting.sh
```

For a genuinely nondeterministic property, use a repeated-run supermajority:

```console
reprocut reduce \
  --flaky --flaky-runs 11 --flaky-required 9 \
  -- python -m pytest tests/test_race.py
```

Only a preserved verdict can replace the current best snapshot.

### Your oracle is the contract

A reduction is only as precise as the failure definition it is given. A
too-permissive contract can be satisfied by a cause the original run never had,
and the reduction will be correct against that contract while no longer being
your bug.

The failure mode is concrete. An upstream check prints one summary line for two
different problems — a file is stale, or a file is missing. A required
expression matching only that summary line lets the search delete the input
entirely: the line still appears, so every candidate is preserved.

ReproCut measures this instead of leaving you to notice it. It compares the
minimized failure's diagnostic against the original's. A shrinking diagnostic is
expected. Lines the original never printed are not, and when they are the
majority, the artifact, the issue text, and the pull-request comment all say so:

```json
"diagnostic_drift": {
  "baseline_lines": 9, "final_lines": 56,
  "retained_lines": 3, "novel_lines": 53,
  "reportable": true,
  "novel_sample": ["examples/original/00-standard-libs missing (run regen)"]
}
```

This never rejects a reduction — a legitimate one can print incidental new text,
and a heuristic must not overrule repeated verification. It reports, so you can
tighten the contract and reduce again. An absent `diagnostic_drift` means drift
was not measured, which is not the same as measuring it and finding none.

## Output

ReproCut publishes a new directory. Existing output paths are not overwritten.

```text
minimal/
|-- project/                 verified retained snapshot
|-- artifact-manifest.json  byte identity of the artifact
|-- reduction.json          versioned reduction evidence
|-- attempts.jsonl          candidate observations
|-- report.html             self-contained report
|-- issue.md                issue-ready summary
|-- reproduce.sh
`-- reproduce.ps1
```

Verify the complete artifact independently before handing it to another team:

```console
reprocut verify ./minimal
```

Verification checks the declared files, byte hashes, retained project, attempt
ledger, report, issue text, and reproduction scripts.

## Resume

Search state is stored in SQLite. An interrupted run can continue only when the
source snapshot, command, oracle, preparation policy, and engine contracts still
match:

```console
reprocut resume \
  --root ./failing-project \
  --output ./minimal-resumed \
  --state ./reprocut-state.sqlite3 \
  -- cargo test parser::case
```

Use `--restart` to begin a new session without deleting prior journal history.

## Offline Python preparation

Dependency-sensitive Python projects can use a fresh virtual environment for
every candidate. Prepare the wheelhouse before starting:

```console
python -m pip download --only-binary=:all: --dest ./wheelhouse '.[test]'

reprocut minimize \
  --root . \
  --prepare isolated-python \
  --python-executable /usr/bin/python3 \
  --python-wheelhouse ./wheelhouse \
  --python-extra test \
  -- python -m pytest -q
```

Pip runs with `--isolated --no-index`. Interpreter and wheel identities are part
of the preparation contract. This is dependency isolation, not a sandbox for
hostile code.

## Python API

The typed Python client invokes the installed Rust CLI through the versioned
protocol:

```python
from pathlib import Path

from reprocut import ReductionRequest, reduce

result = reduce(
    ReductionRequest(
        root=Path("compiler-bug"),
        output=Path("minimal"),
        command=("cargo", "test", "parser::case"),
    )
)

print(result.fingerprint_sha256)
print(result.report_path)
```

The pure-Python fallback implements the oracle contract only. Full project
reduction always requires the separate Rust CLI.

## Integrations and export

Machine integrations use a versioned JSONL protocol:

```console
reprocut protocol run --request request.json
```

The [VS Code and Cursor extension](editors/vscode/README.md) is a thin protocol
client. It does not download a binary or contain a second reduction engine.

A verified artifact can be exported as an OCI image archive when Docker Buildx
or BuildKit is available:

```console
reprocut export oci --from ./minimal --output minimal.oci.tar
```

Gallery preparation is local and does not upload anything:

```console
reprocut gallery prepare \
  --from ./minimal \
  --output ./submission \
  --title "Parser split UTF-8 failure" \
  --license MIT
```

Source is excluded unless `--include-source` is provided explicitly.

## Development

The repository includes Rust unit and integration tests, property tests, Loom
models, Miri and AddressSanitizer jobs, Python tests, editor protocol tests,
gallery validation, cross-platform CLI smoke tests, package builds, SBOMs, and
release provenance checks.

Common local checks:

```console
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-targets
PYTHONPATH=python python -m pytest python/tests -q
node --test editors/vscode/test/*.test.js
node --test gallery/test/*.test.js
```

See [RELEASING.md](docs/RELEASING.md) for release gates and publication order.

## Prior art

Test-case reduction is a well-established field, and ReproCut is not an attempt
to beat the tools that defined it.

| | C-Reduce / C-Vise | Perses | treereduce | ReproCut |
|---|---|---|---|---|
| Input | One file | One file | One file | **A whole project** |
| Languages | C/C++ (deep) | Grammar-driven | Tree-sitter grammars | 8 languages |
| Build manifests | — | — | — | **Cargo, pyproject, package.json** |
| Verifiable evidence | — | — | — | **Signed bundle plus `verify`** |
| Resumable search | — | — | — | **Crash-safe journal** |

On a single C or C++ translation unit, C-Reduce is the better tool and will
usually produce a smaller result: it applies semantic, Clang-aware transforms
that a syntax-node reducer cannot. C-Vise parallelizes the same approach.
Perses and treereduce generalize syntax-guided reduction across grammars.

ReproCut answers a different question. Not *"how small can this one file get?"*
but *"which parts of this repository does the failure actually need?"* — files,
directories, manifest entries, and syntax nodes together, with an artifact
another person can verify without trusting the run that produced it.

If your input is one file, use C-Reduce. If your input is a failing repository,
that is the case ReproCut was built for.

## Limits

- Candidate commands run with the current user's authority. ReproCut is not a
  hostile-code sandbox.
- Network-disabled preparation is ecosystem-specific, not a universal guarantee.
- Grammar reducers are conservative and may leave a larger result.
- A retained file is part of the final observed snapshot; it is not a root-cause
  claim.
- Runtime is dominated by the failing command and the number of candidates.
  ReproCut currently claims no measured speedup.

## License

Licensed under the [Apache License 2.0](LICENSE).
