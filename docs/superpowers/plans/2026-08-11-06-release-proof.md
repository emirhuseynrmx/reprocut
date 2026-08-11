# Performance and Release Proof Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Produce reproducible performance/memory evidence, installable platform binaries, and dry-run-proven crates.io/PyPI packages while leaving final registry publication to the user.

**Architecture:** Benchmarks emit raw versioned data and separate noisy wall time from deterministic instruction/cache measurements. Tag-gated workflows build, test, attest, and stage artifacts. Registry jobs stop before irreversible upload unless the user approves the protected environment.

**Tech Stack:** Criterion, Iai/Cachegrind, GitHub Actions, Maturin, PyO3 abi3, cargo-deny, SBOM/provenance tooling.

## Global Constraints

- Keep all deliverables at `0.1.0` and derive versions from one checked source.
- Never publish an unmeasured speed claim.
- Release jobs use pinned actions/tools, minimal permissions, protected environments, and clean tags.
- The user performs or approves actual crates.io and PyPI publication.
- The local blocked Rust toolchain is not bypassed; clean CI/build hosts are the release authority.

---

### Task 1: Benchmark fixtures and raw evidence schema

**Files:**
- Modify: `crates/reprocut-core/benches/reducer.rs`
- Create: `crates/reprocut-core/benches/frontier.rs`
- Create: `benchmarks/fixtures/generate.py`
- Create: `benchmarks/schema/result.schema.json`
- Modify: `benchmarks/README.md`
- Create: `.github/workflows/benchmarks.yml`

**Interfaces:**
- Produces flat/hierarchical/syntax/manifest fixtures at documented scales and versioned raw results.

- [ ] **Step 1: Write failing deterministic-fixture and result-schema tests**
- [ ] **Step 2: Verify RED**
- [ ] **Step 3: Add Criterion groups for 1K/10K/100K and sequential/parallel comparisons**
- [ ] **Step 4: Add Iai/Cachegrind instruction, branch, allocation, L1, and LL cache measurements**
- [ ] **Step 5: Store raw artifacts with compiler/hardware/sample metadata; commit**

```bash
git add crates/reprocut-core benchmarks .github/workflows/benchmarks.yml
git commit -m "perf: measure reducer work and cache behavior"
```

### Task 2: crates.io-ready workspace packaging

**Files:**
- Modify: `Cargo.toml`
- Modify: every publishable `crates/*/Cargo.toml`
- Modify: `crates/reprocut-python/Cargo.toml`
- Create: `scripts/release/check_crates.py`
- Create: `.github/workflows/package-crates.yml`

**Interfaces:**
- Public package `reprocut` installs binary `reprocut`.
- Internal dependencies use `{ version = "0.1.0", path = "..." }` and publish in topological order.

- [ ] **Step 1: Write a failing metadata/package-order checker**
- [ ] **Step 2: Verify current manifests fail because CLI package name/metadata/versioned dependencies are incomplete**
- [ ] **Step 3: Rename package `reprocut-cli` to `reprocut`, complete metadata, and mark Python build crate non-publishable**
- [ ] **Step 4: Run `cargo package --list` and `cargo publish --dry-run` for every publishable crate on CI**
- [ ] **Step 5: Install packaged `reprocut` into a clean container and run the reduction fixture; commit**

```bash
git add Cargo.toml crates scripts/release/check_crates.py .github/workflows/package-crates.yml
git commit -m "build: prepare ReproCut crates for crates.io"
```

### Task 3: PyPI-ready wheels and source distribution

**Files:**
- Modify: `pyproject.toml`
- Create: `scripts/release/check_wheel.py`
- Create: `.github/workflows/package-python.yml`
- Modify: `python/tests/test_native_backend.py`
- Modify: `python/tests/test_reduce_api.py`

**Interfaces:**
- Produces abi3-py39 wheels, sdist, console entry point, typed package, and TestPyPI-ready artifacts.

- [ ] **Step 1: Write a failing wheel-content/metadata/native-engine checker**
- [ ] **Step 2: Verify current wheel lacks the complete API/entry point**
- [ ] **Step 3: Complete PEP 621 metadata, project URLs, classifiers, console script, and included type/license files**
- [ ] **Step 4: Build manylinux x86_64/aarch64, Windows x86_64, macOS x86_64/aarch64 wheels and sdist**
- [ ] **Step 5: Install artifacts into Python 3.9-3.13 clean environments and run native reduction smoke tests**
- [ ] **Step 6: Add TestPyPI dry-run/install workflow and commit**

```bash
git add pyproject.toml python scripts/release/check_wheel.py .github/workflows/package-python.yml
git commit -m "build: prepare native ReproCut packages for PyPI"
```

### Task 4: Prebuilt binaries, SBOMs, checksums, and attestations

**Files:**
- Create: `.github/workflows/release.yml`
- Create: `scripts/release/package_binary.py`
- Create: `scripts/release/verify_archive.py`
- Create: `release/README.md`

**Interfaces:**
- Produces six target archives, SHA-256 manifest, SBOMs, and GitHub provenance attestations.

- [ ] **Step 1: Write failing archive-layout/version/checksum tests**
- [ ] **Step 2: Verify RED before release scripts exist**
- [ ] **Step 3: Build Linux GNU/musl x86_64, Linux GNU aarch64, Windows x86_64, and macOS x86_64/aarch64**
- [ ] **Step 4: Package binary, completions, README, and licenses reproducibly**
- [ ] **Step 5: Generate SBOM/checksums/attestations and run clean-machine reduction smoke tests**
- [ ] **Step 6: Commit**

```bash
git add .github/workflows/release.yml scripts/release release
git commit -m "build: attest portable ReproCut releases"
```

### Task 5: Protected registry handoff and final release audit

**Files:**
- Create: `docs/release/0.1.0.md`
- Create: `scripts/release/audit.py`
- Modify: `CHANGELOG.md`
- Modify: `README.md`
- Modify: `.github/workflows/release.yml`

**Interfaces:**
- Produces an audit report and user-run crates.io/PyPI handoff checklist; upload steps remain environment-protected.

- [ ] **Step 1: Encode all 18 umbrella acceptance conditions in the audit script**
- [ ] **Step 2: Run the audit and confirm it fails for every missing evidence artifact**
- [ ] **Step 3: Wire each completed gate to immutable logs/artifacts and remove no gate by exception**
- [ ] **Step 4: Configure crates.io token and PyPI OIDC jobs behind exact-tag, protected-environment, and manual-approval conditions**
- [ ] **Step 5: Run dry-run packaging, TestPyPI install, archive verification, and full matrix**
- [ ] **Step 6: Stop before irreversible registry upload and hand the documented approval steps to the user**
- [ ] **Step 7: Commit**

```bash
git add docs/release scripts/release CHANGELOG.md README.md .github/workflows/release.yml
git commit -m "docs: certify the ReproCut 0.1 release candidate"
```
