# ReproCut 0.1 release runbook

Publishing is permanent. Run this only from the reviewed `v0.1.0` commit after
all GitHub Actions jobs pass. Never paste registry tokens into logs, scripts, or
the repository.

## 1. Build and inspect artifacts

Download the `reprocut-release-packages` artifact produced by CI. It contains
all `.crate` packages, Python wheels, and the Python source distribution. The CI
job runs `cargo package --no-verify`, builds the ABI3 wheel, builds the sdist,
and runs `twine check` before uploading the artifact.

The `--no-verify` flag is limited to pre-publication packaging because dependent
ReproCut crates do not exist in the registry yet. Correctness is independently
gated by the workspace test, Clippy, docs, platform, sanitizer, Miri, and Loom
jobs. Actual `cargo publish` performs Cargo's normal verification.

## 2. Publish Rust crates in dependency order

Authenticate locally with a scoped crates.io token. Publish exactly in this
order, allowing the registry index to expose each layer before continuing:

```sh
cargo publish -p reprocut-core
cargo publish -p reprocut-report
cargo publish -p reprocut-oci

cargo publish -p reprocut-workspace
cargo publish -p reprocut-runner
cargo publish -p reprocut-state
cargo publish -p reprocut-syntax

cargo publish -p reprocut-adapters
cargo publish -p reprocut-engine
cargo publish -p reprocut
```

Confirm the clean install in a new temporary directory:

```sh
cargo install reprocut --version 0.1.0
reprocut --version
```

`reprocut-python` is intentionally `publish = false`; it is a Maturin build
crate, not a user-facing crates.io package.

## 3. Publish Python distributions

Inspect the wheel and sdist names, then upload both from the downloaded CI
artifact using a scoped PyPI token:

```sh
python -m pip install twine==7.0.0
python -m twine check dist/wheels/* dist/sdist/*
python -m twine upload dist/wheels/* dist/sdist/*
```

Validate in a clean Python 3.9+ virtual environment:

```sh
python -m pip install reprocut==0.1.0
python -c "import reprocut; print(reprocut.BACKEND)"
reprocut-py --help
```

The Python package contains the native failure-oracle binding and typed client.
Full project reduction additionally discovers the Rust `reprocut` binary via
`REPROCUT_BINARY` or `PATH`; this boundary is explicit and tested.

## 4. Tag only the published commit

After both registries pass clean-install verification:

```sh
git tag -s v0.1.0 -m "ReproCut 0.1.0"
git push origin v0.1.0
```

Create the GitHub release from the same tag and attach the package checksums,
benchmark evidence, terminal demo, and source archive.
