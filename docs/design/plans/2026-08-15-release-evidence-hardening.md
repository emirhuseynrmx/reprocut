# ReproCut v0.1.0 Evidence Hardening Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `executing-plans` to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace ambiguous portfolio claims with three reproducible evidence tiers, eliminate direct production `unwrap`/`expect` calls, and create an honest independent-validation path.

**Architecture:** Keep the reducer unchanged. Add a network-free benchmark contract layer around the existing pinned Perses manifest, plus an opt-in network runner for `clang-26760`; make diagnostic initialization explicitly fallible and propagate errors through the oracle; use allocation-conscious append helpers for infallible `String` rendering. CI separates fast required gates from the historical-toolchain benchmark.

**Tech Stack:** Rust 1.85, Python 3.12 standard library, GitHub Actions, Node.js gallery validator, LLVM/Clang 3.6.0 official Linux archive.

## Global Constraints

- Do not publish crates.io or PyPI packages.
- Do not check GPL corpus source or downloaded LLVM binaries into Git.
- Never execute a fetched upstream `r.sh` file.
- Keep `.tmp/` and `dist/` untouched because they are pre-existing untracked user artifacts.
- Claim only that non-test library and binary targets contain no direct `unwrap()` or `expect()` calls; do not claim that all panics are impossible.
- The repository may be made public or merged only after required CI is green.

---

### Task 1: Honest evidence copy and release-audit contract

**Files:**
- Modify: `README.md`
- Modify: `docs/launch/HACKER_NEWS.md`
- Modify: `scripts/release/audit.py`
- Test: `scripts/release/audit.py`

**Interfaces:**
- Consumes: checked-in `demo/result/reduction.json` and `scripts/benchmark_release.py` constants.
- Produces: an evidence table with `onboarding`, `synthetic-scale`, `upstream-real`, and `independent` rows.

- [ ] **Step 1: Add a failing release-audit assertion**

Require README to contain the literal disclosures `55 lines`, `1,669 bytes`, `Synthetic 312-file fixture`, and `Independent validations: 0`, and reject wording that calls the onboarding fixture large or real-world.

- [ ] **Step 2: Run the audit and observe failure**

Run: `python scripts/release/audit.py --root .`

Expected: FAIL because README does not yet include the exact evidence-tier disclosures.

- [ ] **Step 3: Rewrite README and HN copy**

Keep the GIF, but label it `Tiny onboarding fixture`. Add a compact table whose measured facts are:

```text
Tiny onboarding | Python | 18 -> 3 files | 55 lines / 1,669 bytes | checked in
Synthetic scale | generated Python | 312 files | 5 measured runs in CI | CI artifact
Upstream real | Perses clang-26760 | 33,171 lines / 1,944,800 bytes | opt-in benchmark
Independent | third-party submissions | 0 validated runs | none yet
```

State next to the table that the 312-file case is generated and the upstream result is unavailable until the opt-in workflow completes successfully.

- [ ] **Step 4: Run the audit and README link checks**

Run: `python scripts/release/audit.py --root .`

Expected: PASS, including the new evidence-copy checks.

- [ ] **Step 5: Commit**

```console
git add README.md docs/launch/HACKER_NEWS.md scripts/release/audit.py
git commit -m "docs: make release evidence claims explicit"
```

### Task 2: Offline upstream benchmark contract

**Files:**
- Create: `benchmarks/clang-26760.json`
- Create: `scripts/upstream_benchmark.py`
- Create: `scripts/test_upstream_benchmark.py`
- Modify: `benchmarks/README.md`

**Interfaces:**
- Consumes: `benchmarks/upstream-corpus.json` and the existing `scripts/fetch_upstream_corpus.py` output.
- Produces: `load_contract(path: Path) -> BenchmarkContract`, `build_oracle_command(contract, case_root, clang) -> list[str]`, `validate_result(contract, result_root) -> dict[str, object]`.

- [ ] **Step 1: Write failing Python contract tests**

Tests create temporary manifests and assert:

```python
self.assertEqual(contract.case_id, "clang-26760")
self.assertEqual(contract.source_lines, 33171)
self.assertNotIn("r.sh", build_oracle_command(contract, case_root, clang))
with self.assertRaisesRegex(BenchmarkError, "SHA-256"):
    verify_sha256(archive, "0" * 64)
with self.assertRaisesRegex(BenchmarkError, "GPL"):
    require_gpl_acknowledgement(False)
```

Also assert that absolute paths, `..` components, unknown case IDs, unpinned URLs, and result documents without three preserved final observations are rejected.

- [ ] **Step 2: Run the tests and observe import failure**

Run: `python -m unittest scripts.test_upstream_benchmark -v`

Expected: FAIL because `scripts/upstream_benchmark.py` does not exist.

- [ ] **Step 3: Implement the bounded contract parser**

Use frozen dataclasses and `json.loads`; accept only the declared keys. Pin:

```json
{
  "schema_version": 1,
  "case_id": "clang-26760",
  "source_files": 2,
  "source_lines": 33171,
  "source_bytes": 1933944,
  "compiler_version": "3.6.0",
  "compiler_url": "https://releases.llvm.org/3.6.0/clang+llvm-3.6.0-x86_64-linux-gnu-ubuntu-14.04.tar.xz",
  "compiler_sha256": "e8396103fbf794e6af671593659458dfe841c32234d3cd4f37be0b48cd6a9f8b",
  "compiler_arguments": ["-O3", "-w", "-c", "small.c"]
}
```

The implementation must reject redirects outside `releases.llvm.org`, files over 128 MiB, non-regular paths, and checksum mismatches. The owned oracle command invokes only the resolved `clang` binary with the allowlisted arguments.

- [ ] **Step 4: Implement result validation**

Read `reduction.json` and require schema compatibility, `same_failure == true`, three baseline runs, three final verifications, no truncated final observation, and retained measurements no larger than original measurements. Emit a summary dictionary with source provenance and measured reduction fields kept separate.

- [ ] **Step 5: Run tests**

Run: `python -m unittest scripts.test_upstream_benchmark -v`

Expected: PASS.

- [ ] **Step 6: Commit**

```console
git add benchmarks/clang-26760.json benchmarks/README.md scripts/upstream_benchmark.py scripts/test_upstream_benchmark.py
git commit -m "feat: add pinned upstream benchmark contract"
```

### Task 3: Historical compiler benchmark workflow

**Files:**
- Create: `.github/workflows/upstream-benchmark.yml`
- Modify: `scripts/upstream_benchmark.py`
- Modify: `scripts/test_upstream_benchmark.py`

**Interfaces:**
- Consumes: Task 2 contract functions and `target/release/reprocut`.
- Produces: `output/upstream-clang-26760/summary.json`, `summary.md`, and the complete ReproCut artifact directory.

- [ ] **Step 1: Add failing orchestration tests**

Patch subprocess and URL open calls. Assert the runner performs exactly three baseline probes before ReproCut, passes `--oracle-stream stderr`, uses a fixed timeout, verifies exactly three final observations, and never invokes `bash`, `sh`, or `r.sh`.

- [ ] **Step 2: Run tests and observe missing orchestration**

Run: `python -m unittest scripts.test_upstream_benchmark -v`

Expected: FAIL on the new orchestration assertions.

- [ ] **Step 3: Implement `run` subcommand**

Create all work under a caller-provided output directory, use exclusive temporary files followed by `os.replace`, download with a byte cap while hashing, extract only regular archive members under one expected top-level directory, fetch only `clang-26760`, and pass the owned compiler command after `--` to ReproCut. Preserve stdout/stderr logs with byte bounds and classify toolchain startup failures separately from oracle mismatches.

- [ ] **Step 4: Add opt-in workflow**

The workflow has `workflow_dispatch` and a weekly schedule, Ubuntu runner, 120-minute timeout, pinned Rust 1.85.0 and Python 3.12, `cargo build --locked --release -p reprocut`, then:

```console
python scripts/upstream_benchmark.py run \
  --accept-gpl-3.0 \
  --reprocut target/release/reprocut \
  --output output/upstream-clang-26760
```

Upload the entire output with `if-no-files-found: error`. Do not make this a pull-request status check.

- [ ] **Step 5: Run offline tests and action syntax audit**

Run: `python -m unittest scripts.test_upstream_benchmark -v`

Run: `python scripts/release/audit.py --root .`

Expected: PASS.

- [ ] **Step 6: Commit**

```console
git add .github/workflows/upstream-benchmark.yml scripts/upstream_benchmark.py scripts/test_upstream_benchmark.py scripts/release/audit.py
git commit -m "ci: add real compiler bug benchmark"
```

### Task 4: Fallible diagnostics and oracle construction

**Files:**
- Modify: `crates/reprocut-core/src/diagnostic.rs`
- Modify: `crates/reprocut-core/src/oracle.rs`
- Modify: `crates/reprocut-core/src/lib.rs`
- Modify: `crates/reprocut-engine/src/lib.rs`
- Test: `crates/reprocut-core/tests/oracle_contract.rs`
- Test: `crates/reprocut-core/tests/oracle_properties.rs`

**Interfaces:**
- Produces: `pub enum DiagnosticError { PatternInitialization }` and `pub fn normalize_diagnostic(input: &str) -> Result<String, DiagnosticError>`.
- Changes: `stable_discriminators(...) -> Result<Vec<DiagnosticAnchor>, DiagnosticError>` and internal `compile_patterns(...) -> Result<Vec<Regex>, OracleError>`.
- Preserves: `OracleSpec::automatic(channel) -> OracleSpec` as an infallible public convenience constructor built directly from known-valid fields.

- [ ] **Step 1: Add compile-time and behavior tests**

Update normalization tests to call `.expect` only in test code. Add an oracle test proving diagnostic initialization errors map to `OracleError::DiagnosticInitialization`; keep existing adversarial identity tests unchanged.

- [ ] **Step 2: Run core tests before implementation**

Run: `cargo test --locked -p reprocut-core`

Expected: FAIL because the new result-returning API and error variant do not exist.

- [ ] **Step 3: Introduce one shared fallible pattern bank**

Replace per-regex `OnceLock<Regex>` values with `OnceLock<Result<DiagnosticPatterns, regex::Error>>`. Match the stored result without `unwrap` or `expect`; map any literal compilation failure to `DiagnosticError::PatternInitialization`. Replace capture indexing and `captures.get(0).expect(...)` with checked `get` branches.

- [ ] **Step 4: Propagate errors through oracle creation**

Add `OracleError::DiagnosticInitialization`, use `compile_patterns(...)?`, and use `stable_discriminators(...).map_err(...) ?`. Build `OracleSpec::automatic` directly with empty canonical pattern vectors and `spec_digest`, avoiding a known-valid `Result` round trip.

- [ ] **Step 5: Run focused and property tests**

Run: `cargo test --locked -p reprocut-core`

Expected: PASS.

- [ ] **Step 6: Commit**

```console
git add crates/reprocut-core crates/reprocut-engine/src/lib.rs
git commit -m "refactor: make diagnostic initialization fallible"
```

### Task 5: Remove remaining production `unwrap` and `expect`

**Files:**
- Modify: `crates/reprocut-core/src/transformation.rs`
- Modify: `crates/reprocut-report/src/issue.rs`
- Modify: `crates/reprocut-report/src/lib.rs`
- Modify: `crates/reprocut-state/src/lib.rs`
- Modify: `crates/reprocut-cli/src/main.rs`
- Test: existing crate contract suites

**Interfaces:**
- Produces: private `append_fmt(output: &mut String, args: fmt::Arguments<'_>)` helpers in report modules.
- Changes: `split_command(command: &[String]) -> Result<(&str, &[String]), CliError>`.
- Changes: state contract encoding uses a checked `u64` length returned through `Result` where the contract digest is constructed.

- [ ] **Step 1: Add the production lint command and observe failures**

Run:

```console
cargo clippy --locked --workspace --lib --bins -- \
  -D clippy::unwrap_used -D clippy::expect_used
```

Expected: FAIL at every remaining production call site.

- [ ] **Step 2: Replace transformation formatting assertion**

Use `const HEX: &[u8; 16] = b"0123456789abcdef"` and push two `char::from(...)` values per byte. Keep digest output byte-for-byte identical under existing transformation tests.

- [ ] **Step 3: Replace report formatting assertions without per-field allocations**

Implement:

```rust
fn append_fmt(output: &mut String, arguments: std::fmt::Arguments<'_>) {
    let result = output.write_fmt(arguments);
    debug_assert!(result.is_ok(), "String formatting is infallible");
}
```

Call it with `format_args!` at each former `write!`/`writeln!` assertion. Existing HTML and issue snapshot/contract tests must remain byte-identical.

- [ ] **Step 4: Make CLI and state invariants typed**

Return `CliError::InvalidArguments("the failing command is empty")` from `split_command`; propagate with `?`. Change state contract construction to return `Result<ContentDigest, StateError>` and map an impossible oversized string to `StateError::InvalidRecord("state contract text exceeds u64 length")`, then propagate through session creation.

- [ ] **Step 5: Run focused suites**

Run: `cargo test --locked -p reprocut-core -p reprocut-report -p reprocut-state -p reprocut`

Expected: PASS.

- [ ] **Step 6: Run the production lint**

Run: `cargo clippy --locked --workspace --lib --bins -- -D clippy::unwrap_used -D clippy::expect_used`

Expected: PASS with no allow attributes added to production modules.

- [ ] **Step 7: Commit**

```console
git add crates/reprocut-core/src/transformation.rs crates/reprocut-report/src crates/reprocut-state/src/lib.rs crates/reprocut-cli/src/main.rs
git commit -m "refactor: remove production unwrap and expect calls"
```

### Task 6: Independent-validation contract and CI gate

**Files:**
- Create: `.github/ISSUE_TEMPLATE/real-world-validation.yml`
- Modify: `.github/PULL_REQUEST_TEMPLATE/gallery.md`
- Modify: `gallery/schema/entry.schema.json`
- Modify: `gallery/scripts/build.js`
- Modify: `gallery/test/gallery.test.js`
- Modify: `.github/workflows/ci.yml`
- Modify: `scripts/release/audit.py`

**Interfaces:**
- Produces: gallery field `validation_source` with enum `self`, `independent`, and `upstream-benchmark`, plus `parent_artifact_id` as a lowercase 64-character SHA-256.
- Consumes: the parent artifact ID already emitted by `reprocut gallery prepare`.

- [ ] **Step 1: Add failing gallery validation tests**

Require `parent_artifact_id`, reject malformed digests, require `validation_source`, and assert the built page counts only `validation_source == "independent"` as an independent validation. Update the checked-in self-authored example to `validation_source: "self"` and assert the count is zero.

- [ ] **Step 2: Run Node tests and observe schema failure**

Run: `node --test gallery/test/*.test.js`

Expected: FAIL because the schema and builder do not recognize the new fields.

- [ ] **Step 3: Implement the gallery schema and renderer**

Add both fields to JSON schema, JS allowlist, and validator. Render `Independent validations: N` from the validated records. Do not infer independence from repository owner, title, or license.

- [ ] **Step 4: Add the issue form**

The form requires ReproCut commit/version, OS/architecture, runner version, original/retained measurements, fingerprint, artifact manifest SHA-256, public issue/project URL or a statement that it is private, and an explicit consent checkbox. It instructs users to run `reprocut verify <artifact>` before submission and warns them not to attach confidential source.

- [ ] **Step 5: Add CI and release-audit gates**

Add the production Clippy command from Task 5, Python upstream contract tests, and checks that the issue form exists and the checked-in gallery's independent count is zero. Keep all-target Clippy unchanged.

- [ ] **Step 6: Run focused gates**

Run: `node --test gallery/test/*.test.js`

Run: `python -m unittest scripts.test_upstream_benchmark -v`

Run: `python scripts/release/audit.py --root .`

Expected: PASS.

- [ ] **Step 7: Commit**

```console
git add .github gallery scripts/release/audit.py
git commit -m "ci: gate independent validation and panic policy"
```

### Task 7: Full release verification

**Files:**
- Modify only files required by failures proven in this task.

**Interfaces:**
- Consumes: all previous task outputs.
- Produces: a release-evidence log and a green local release gate.

- [ ] **Step 1: Format and check diffs**

Run: `cargo fmt --all -- --check`

Run: `git diff --check`

Expected: PASS.

- [ ] **Step 2: Run all Rust tests**

Run: `cargo test --locked --workspace --all-targets`

Expected: PASS.

- [ ] **Step 3: Run Python and Node tests**

Run: `python -m pytest python/tests -q`

Run: `python -m unittest scripts.test_upstream_benchmark -v`

Run: `node --test gallery/test/*.test.js`

Expected: PASS.

- [ ] **Step 4: Run both Clippy gates**

Run: `cargo clippy --locked --workspace --all-targets -- -D warnings`

Run: `cargo clippy --locked --workspace --lib --bins -- -D clippy::unwrap_used -D clippy::expect_used`

Expected: PASS.

- [ ] **Step 5: Run release audit and deterministic benchmark smoke**

Run: `python scripts/release/audit.py --root .`

Run: `cargo build --locked --release -p reprocut`

Run: `python scripts/benchmark_release.py --reprocut target/release/reprocut --python python --output output/release-benchmark-final --runs 1 --warmup 0`

Expected: PASS; the benchmark emits a valid summary and does not reuse a previous state database.

- [ ] **Step 6: Review repository state and commit verification-only fixes**

Run: `git status --short`

Expected: only `.tmp/`, `dist/`, and ignored benchmark output remain outside committed work. If verification required code fixes, commit only those proven fixes with `fix: close release verification failures`.
