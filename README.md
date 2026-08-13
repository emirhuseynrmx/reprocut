![ReproCut - same failure, less project](assets/reprocut-banner.svg)

# ReproCut

ReproCut removes parts of a failing project while checking that the original
failure still occurs.

It works on a copy, not your checkout. It establishes a repeatable baseline,
tests candidate reductions in fresh snapshots, and publishes a result only
after final verification satisfies the selected evaluation policy.

![A ReproCut run reducing 18 files to 3](assets/reprocut-demo.gif)

The checked-in demo reduces a Python project from **18 files to 3** in **24
candidate evaluations**, followed by **3/3 final verification runs**. The
[evidence](demo/result/reduction.json), [attempt log](demo/result/attempts.jsonl),
and [HTML report](demo/result/report.html) are included in the repository.

ReproCut 0.1.0 is a release candidate. The crates.io and PyPI packages have not
been published.

## Quick start

Building from source requires Rust 1.85 or newer.

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

The typed Python client invokes the same Rust protocol engine:

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
reduction requires the Rust CLI or native package.

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

Licensed under either [Apache-2.0](LICENSE-APACHE) or [MIT](LICENSE-MIT), at your
option.
