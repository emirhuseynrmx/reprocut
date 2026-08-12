# ReproCut 0.1 release runbook

crates.io and PyPI publication are irreversible external writes. The workflows
are prepared, but the user chooses and manually approves each registry action.
Never paste registry credentials into logs, scripts, issues, or the repository.

## 1. Audit the immutable commit

Require the static contract locally and every native gate on the same
40-character commit in clean CI:

```console
python scripts/release/audit.py --static-only
python scripts/release/audit.py \
  --ci-evidence output/ci-evidence.json \
  --expected-commit "$(git rev-parse HEAD)"
```

Recheck that the `reprocut` names remain available before the first upload.
Registry availability is not ownership until publication succeeds.

`Cargo.lock` is part of the audited source commit. Static audit rejects a
missing lockfile, workflow-time regeneration, or a Cargo/Maturin graph command
without `--locked`. Pinned Cargo 1.85 runs `cargo metadata --locked` before any
release artifact is built.

## 2. Create one signed release tag

The binary workflow is tag-driven, so the signed tag is created only after the
audited commit is final and before either registry is published:

```console
git tag -s v0.1.0 -m "ReproCut 0.1.0"
git push origin v0.1.0
```

The tag triggers six native archives, real-failure smoke tests, deterministic
packaging, SPDX SBOMs, SHA-256 aggregation, and GitHub provenance. Publishing
the GitHub Release additionally requires manual approval of the protected
`release` environment. Never move or recreate the tag after any upload.

The audited commit must also have successful `oracle-adversarial`,
`python-isolation`, and `snapshot-integrity` gates. These prove all oracle modes
against hostile diagnostics, install committed wheels with indexes disabled
into fresh candidate venvs, and preserve immutable source bytes plus Unix
execute masks. A missing gate is not waivable release evidence.

## 3. Publish crates.io manually

Run **Publish registries (manual)** with:

- tag: `v0.1.0`
- registry: `crates-io`
- confirmation: `PUBLISH_REPROCUT_0_1_0`

The protected `crates-io` environment holds the least-scope
`CARGO_REGISTRY_TOKEN`. After approval, the workflow reruns format, Clippy, and
tests; publishes in this dependency order; waits for each registry entry; and
performs a clean `cargo install reprocut --version 0.1.0 --locked`:

```text
reprocut-core → reprocut-report → reprocut-oci
reprocut-workspace → reprocut-runner → reprocut-state → reprocut-syntax
reprocut-adapters → reprocut-engine → reprocut
```

The equivalent audited command order is:

```console
cargo publish --locked -p reprocut-core
cargo publish --locked -p reprocut-report
cargo publish --locked -p reprocut-oci
cargo publish --locked -p reprocut-workspace
cargo publish --locked -p reprocut-runner
cargo publish --locked -p reprocut-state
cargo publish --locked -p reprocut-syntax
cargo publish --locked -p reprocut-adapters
cargo publish --locked -p reprocut-engine
cargo publish --locked -p reprocut
```

`reprocut-python` is intentionally `publish = false`; it is a Maturin build crate, not a
user-facing crates.io package.

## 4. Publish PyPI manually through OIDC

Register `.github/workflows/publish-registries.yml` as the PyPI Trusted
Publisher for project `reprocut`, environment `pypi`, then run the same manual
workflow with registry `pypi` and the exact confirmation.

The workflow builds ABI3-Python-3.9 wheels for manylinux x86_64/aarch64,
Windows x86_64, and macOS x86_64/aarch64, builds the sdist, runs `twine check`,
and uses a short-lived OpenID Connect credential. No long-lived PyPI token is
stored.

After publication, validate a clean supported environment:

```console
python -m pip install reprocut==0.1.0
python -c "import reprocut; print(reprocut.BACKEND)"
reprocut-py --help
```

The Python package contains the native oracle binding and typed shared-engine
client. Full project reduction resolves the Rust `reprocut` CLI through
`REPROCUT_BINARY` or `PATH`; it never falls back to a second Python reducer.
