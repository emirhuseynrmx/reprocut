# ReproCut Complete 0.1 Product and System Design

Date: 2026-08-11  
Status: Approved scope; awaiting written-spec review  
Release line: `0.1.x`  
Primary packages: crates.io `reprocut`, PyPI `reprocut`

## 1. Decision summary

ReproCut 0.1 will be a local-first, deterministic failing-project reducer rather than a file-deletion wrapper. It will stabilize failure evidence, search directory/file/manifest/syntax transformations, run candidates in contained process trees, persist every decision, resume after interruption, evaluate a deterministic frontier in parallel, and publish a minimal reproduction with machine-readable evidence.

The 0.1 release is not considered complete until all of the following ship together:

1. zero-configuration `cargo test`, `pytest`, and `npm test` discovery;
2. directory-aware and regular-file reduction;
3. manifest-aware reduction for Cargo.toml, pyproject.toml, and package.json;
4. syntax-aware reduction for Rust, Python, JavaScript, and TypeScript;
5. stdout/stderr-aware stable failure identity;
6. strict deterministic mode and statistically reported flaky mode;
7. whole-process-tree timeout cleanup on Unix and Windows;
8. crash-safe persistent cache and resume;
9. deterministic parallel frontier execution;
10. an evidence-rich HTML report and GitHub issue export;
11. before/after size and timing measurements;
12. reproducible OCI export;
13. a thin VS Code/Cursor extension;
14. an opt-in, PR-curated static public gallery;
15. reproducible performance evidence;
16. prebuilt release binaries with checksums and provenance;
17. crates.io and PyPI publication readiness.

The workspace version remains `0.1.0` during development. No package is published and no `v0.1.0` tag is created until every release gate in this document passes. After the first public release, fixes remain in the `0.1.x` line.

## 2. Research basis

The architecture follows established reducer research rather than inventing one undifferentiated search loop.

- Zeller and Hildebrandt's `ddmin` establishes automated 1-minimal failure-inducing inputs, while not promising a global minimum: <https://www.st.cs.uni-saarland.de/papers/tse2002/>.
- Hierarchical Delta Debugging shows why natural tree structure should be searched instead of flattened away: <https://web.cs.ucdavis.edu/~su/publications/icse06-hdd.pdf>.
- Perses shows that grammar-guided candidates avoid spending executions on syntactically invalid programs: <https://web.cs.ucdavis.edu/~su/publications/perses.pdf>.
- C-Reduce demonstrates that effective reduction needs domain transformations and fixpoint passes in addition to plain textual delta debugging: <https://fsl.cs.illinois.edu/publications/regehr-chen-cuoq-eide-ellison-yang-pldi-2012.html>.
- Failure-message classifiers vary sharply by project, so a loose log-similarity heuristic is not safe enough to authorize deletions: <https://arxiv.org/abs/2401.15788>.
- Repeated, statistically interpreted executions are necessary when the property is flaky: <https://arxiv.org/abs/2607.25695>.
- Windows Job Objects manage processes as a unit and can terminate associated descendants on handle close: <https://learn.microsoft.com/en-us/windows/win32/procthread/job-objects>.
- SQLite transactions provide atomic state changes; WAL permits same-host reader/writer concurrency but is not suitable for network filesystems: <https://www2.sqlite.org/atomiccommit.html> and <https://www2.sqlite.org/wal.html>.
- Tree-sitter supplies concrete syntax trees, byte ranges, error recovery, and grammars for the bundled languages: <https://tree-sitter.github.io/tree-sitter/index.html>.
- Criterion uses warm-up, measurement, bootstrap analysis, and comparison, while its documentation warns against trusting noisy cloud-VM wall-clock comparisons: <https://bheisler.github.io/criterion.rs/book/analysis.html> and <https://bheisler.github.io/criterion.rs/book/faq.html>.
- Cargo documents that crates.io releases are permanent and recommends clean `cargo publish --dry-run` packaging before upload: <https://doc.rust-lang.org/cargo/reference/publishing.html>.
- PyPI Trusted Publishing exchanges GitHub OIDC identity for short-lived credentials instead of storing a long-lived upload token: <https://docs.pypi.org/trusted-publishers/>.
- GitHub artifact attestations bind release binaries and SBOMs to their build provenance: <https://docs.github.com/en/actions/how-tos/secure-your-work/use-artifact-attestations/use-artifact-attestations>.

## 3. Non-negotiable invariants

### 3.1 Correctness

- The source checkout is read-only for the entire run.
- A transformation is committed only when the configured failure policy classifies it as preserved.
- Timeout, output truncation, runner failure, parser uncertainty, preparation failure, state mismatch, or cancellation can never authorize a reduction.
- Deterministic mode requires all three baseline runs and all three final runs to preserve exactly the same failure identity.
- Flaky mode records observations and confidence; it never relabels incomplete executions as successful evidence.
- `--jobs 1` and `--jobs N` produce the same accepted transformation chain for the same source, command, configuration, and observations.
- A resumed run validates source digest, command, environment contract, adapter versions, oracle policy, and schema before reusing state.

### 3.2 Safety

- Candidate commands run with user authority; ReproCut is not a hostile-code sandbox.
- Each candidate gets a fresh filesystem root.
- Symbolic links are never followed while inventorying or materializing candidates.
- Timeout cleanup addresses the contained process tree, not only the direct child.
- Captured streams remain bounded in memory while their pipes continue to drain.
- The final output path is no-clobber and is published by a sibling staging-directory rename.
- Gallery submission is explicit. ReproCut never uploads source automatically.
- Gallery validation workflows receive no repository secrets and never run unreviewed submitted code.

### 3.3 Evidence

- Every candidate has a stable ID, content digest, transformation description, verdict, timing, execution count, and diagnostic evidence.
- The report distinguishes observed facts from inference. “Why retained” means the recorded outcome when a final-context deletion was tested; it is not a causal claim.
- Performance marketing claims require committed fixtures, compiler/hardware metadata, raw samples, and confidence or instruction-count evidence.

## 4. Workspace architecture

The existing crates remain focused and new responsibilities receive separate crates.

```text
reprocut (CLI package and binary)
├── reprocut-engine      orchestration and phase state machine
├── reprocut-core        immutable models, ddmin, deterministic ranking
├── reprocut-runner      bounded I/O and process-tree containment
├── reprocut-workspace   source inventory and candidate materialization
├── reprocut-state       SQLite journal, cache, resume validation
├── reprocut-adapters    ecosystem discovery and manifest preparation
├── reprocut-syntax      Tree-sitter transforms and reparse validation
├── reprocut-report      HTML, Markdown issue body, JSON evidence
└── reprocut-oci         container recipe and OCI archive export

reprocut-python          PyO3 boundary for the public Python package
editors/vscode           VS Code/Cursor extension
gallery                  schema, static renderer, examples, Pages workflow
```

`reprocut-python` is a PyPI build crate and is not published to crates.io. All Rust crates required by the `reprocut` binary carry complete registry metadata and versioned-plus-path dependencies so `cargo package` can resolve them after publication.

The public Rust installation command is:

```console
cargo install reprocut
```

The public Python installation command is:

```console
python -m pip install reprocut
```

## 5. Unified project and transformation model

The engine no longer treats a candidate as only a list of retained file IDs.

```rust
pub struct ProjectSnapshot {
    source_digest: ContentDigest,
    files: Arc<[FileRecord]>,
}

pub struct Transformation {
    id: TransformationId,
    phase: ReductionPhase,
    rank: CandidateRank,
    operations: Arc<[Operation]>,
}

pub enum Operation {
    DeleteFile { path: ProjectPath },
    DeleteDirectoryGroup { files: Arc<[ProjectPath]> },
    RemoveManifestEntry { path: ProjectPath, key: ManifestKey },
    DeleteSyntaxNode { path: ProjectPath, range: ByteRange, kind: NodeKind },
    HoistSyntaxChild { path: ProjectPath, parent: ByteRange, child: ByteRange },
    ReplaceByteRange { path: ProjectPath, range: ByteRange, replacement: Arc<[u8]> },
}
```

Paths are validated project-relative paths. Byte ranges refer to the immutable source blob for the phase that produced them. Multiple edits to one file are canonicalized in descending byte order and rejected if they overlap. A candidate digest covers the source digest, ordered operation encoding, adapter identity, preparation contract, command, oracle policy, and tool schema.

Candidate materialization applies operations into a fresh temporary root. Unchanged regular files are copied by the portable baseline implementation. Platform-specific reflink or hardlink acceleration may be added only when candidate writes are guaranteed to copy-on-write; correctness cannot depend on it.

## 6. Reduction pipeline

The pipeline reaches a fixpoint at each layer before moving to the next:

1. ecosystem detection and command resolution;
2. deterministic/flaky baseline stabilization;
3. directory hierarchy reduction;
4. regular-file ddmin;
5. manifest-entry reduction and adapter preparation;
6. syntax-node deletion;
7. syntax-child hoisting;
8. final single-operation sweep across all surviving transform kinds;
9. repeated final verification;
10. artifact, report, issue body, and optional OCI publication.

Every phase consumes the accepted snapshot from the previous phase. If a later phase makes an earlier transformation newly possible, the engine returns to the earliest affected phase. The run stops when a complete cycle accepts no transformation.

### 6.1 Ordered ddmin

The core algorithm evaluates both subsets and complements in deterministic rank order. Granularity begins at two, doubles when no transition is found, and contracts after an accepted transition. A final singleton-removal sweep establishes 1-minimality with respect to the enabled operations.

Inconclusive candidates are retained in the ledger and behave as non-accepting results. They do not become cached rejections because a resumed run may explicitly retry them.

### 6.2 Directory hierarchy

The directory pass builds a trie from regular-file paths. A directory transformation deletes all descendant regular files as one operation. Root files become singleton groups. Empty directories need no cleanup because candidates are materialized from retained files; only parent directories required by retained files are created.

The pass descends only into retained groups. It then hands the surviving leaf files to regular-file ddmin.

### 6.3 Syntax reduction

Bundled Tree-sitter grammars cover:

- Rust (`.rs`);
- Python (`.py`, `.pyi`);
- JavaScript (`.js`, `.jsx`, `.mjs`, `.cjs`);
- TypeScript (`.ts`, `.tsx`, `.mts`, `.cts`).

Each grammar has an explicit allowlist of removable or hoistable named-node kinds. Initial transforms cover modules/items, declarations, functions, classes/impls, statements, match/switch arms, parameters/arguments, fields/properties, decorators/attributes, and expression children whose parent grammar permits replacement.

A candidate is rejected without executing user code when reparsing introduces an `ERROR` node, a `MISSING` node, a changed root grammar, an invalid UTF-8 boundary, or overlapping edits. Tree-sitter validity is syntactic evidence only; the user's command remains the semantic oracle.

## 7. Ecosystem adapters and one-command operation

The default command is:

```console
reprocut minimize
```

Detection rules are deterministic:

1. `Cargo.toml` selects Cargo;
2. Python configuration plus Python sources selects pytest;
3. `package.json` with a `test` script selects npm;
4. exactly one detected adapter runs automatically;
5. ambiguity is an error listing `--ecosystem cargo|python|npm` choices;
6. an explicit command after `--` always overrides command discovery while retaining adapter transforms unless `--ecosystem none` is supplied.

### 7.1 Cargo

- Default command: `cargo test --locked --offline` when `Cargo.lock` exists, otherwise `cargo test --offline`.
- Inventory excludes `.git`, `.reprocut`, output paths, and `target`.
- Manifest units include dependencies, dev-dependencies, build-dependencies, target-specific dependency tables, features, workspace members, examples, tests, and binary targets when their removal is structurally valid.
- A manifest candidate runs `cargo generate-lockfile --offline` followed by `cargo metadata --locked --offline --format-version 1` before the oracle command.
- Preparation failure rejects the candidate and is reported separately from the tested failure.
- Registry/network access is disabled unless the user explicitly enables it.

### 7.2 Python and pytest

- Default command: `python -m pytest`.
- Inventory excludes virtual environments, `.pytest_cache`, `__pycache__`, `.mypy_cache`, `.ruff_cache`, build, and dist.
- Manifest units include project dependencies, optional-dependency groups, pytest configuration entries, and console scripts.
- Dependency removal is enabled automatically only when a reproducible preparation path exists: an offline-capable lock tool such as `uv.lock`, or a user-supplied `--prepare` command.
- Without an isolated preparation path, dependency entries are displayed as unavailable rather than removed under the already-populated host environment.
- Adapter preparation uses a candidate-local environment and records the executable/tool versions.

### 7.3 npm

- Default command: `npm test -- --runInBand` only when the script/tool supports the argument; otherwise the exact declared `npm test` script is used.
- Inventory excludes `node_modules`, coverage, dist, build, and package-manager caches.
- Manifest units include dependencies, devDependencies, optionalDependencies, peerDependencies, scripts other than the selected oracle script, workspaces, exports, and engines where structural removal is valid.
- Lock preparation uses offline package-manager metadata and disables lifecycle scripts by default.
- `npm ci --ignore-scripts --offline` validates the candidate when a compatible lock and cache exist.
- A project that requires lifecycle scripts must opt in with `--allow-prepare-scripts`; the report marks that authority expansion.

## 8. Multi-stream failure oracle

The CLI surface is:

```text
--oracle-stream auto|stderr|stdout|combined
```

`auto` is the default. Baseline runs normalize stdout and stderr independently. A channel is eligible only when it is non-empty, complete, and identical after normalization across the required baseline observations.

- If one channel is eligible, its fingerprint is required.
- If both channels are eligible, both fingerprints are required.
- If neither is eligible, baseline stabilization fails.
- An unstable non-selected channel is recorded; it is never presented as evidence.
- `combined` means a conjunction of separately captured channel fingerprints. It does not fabricate temporal interleaving between concurrently drained pipes.
- Explicit `stdout` or `stderr` fails baseline construction if that channel is empty or unstable.

A fingerprint contains exit code, platform termination reason, selected channel anchors, normalization schema version, and a SHA-256 display hash. Candidate classification compares exact exit state and requires every selected anchor. Loose edit distance, embeddings, and generic log similarity do not authorize deletion.

## 9. Deterministic and flaky policies

### 9.1 Strict mode

Strict mode remains the default:

- three baseline executions, all identical;
- one execution for an ordinary candidate unless the result is incomplete;
- three final executions, all preserved.

### 9.2 Flaky mode

Flaky mode is explicit:

```console
reprocut minimize --flaky
```

Default policy:

- 11 baseline runs;
- one modal failure fingerprint must appear at least 9 times;
- a candidate is preserved only when the baseline fingerprint appears at least 9 times in at most 11 complete runs;
- evaluation stops early when success or failure is mathematically decided;
- timeout and truncation count as incomplete, not preserved;
- the report includes successes, failures, incomplete runs, observed rate, and a 95% Wilson interval.

Advanced users may set odd `--flaky-runs` values from 5 through 101 and a compatible `--flaky-required` supermajority. A simple bare majority is rejected by validation because it is too easy to accept a chance result. Final verification repeats the same aggregate policy.

## 10. Whole-process-tree containment

`reprocut-runner` uses a cross-platform process-group abstraction compatible with Rust 1.85:

- Unix: a new POSIX process group, group termination on timeout, direct-child wait/reap;
- Windows: a Job Object with kill-on-close semantics and direct-child wait/reap;
- drop/cancellation: best-effort group termination before handles are released;
- stdout and stderr: separate bounded reader threads that continue draining after the retained byte budget is exhausted.

The observation records the containment mechanism and a portable termination enum rather than pretending Unix signals exist on Windows:

```rust
pub enum TerminationReason {
    ExitCode(i32),
    UnixSignal(i32),
    TimedOut,
    RunnerFailure,
}
```

Tests spawn a descendant that outlives its parent unless group containment works, then prove the descendant cannot create its delayed marker after timeout. Unix and Windows have separate integration fixtures.

## 11. Persistent state, cache, and resume

State lives at `.reprocut/state.sqlite3` or at an explicit `--state` path. SQLite uses WAL on a local filesystem, `synchronous=FULL`, foreign keys, a busy timeout, and a versioned migration table.

Core tables represent:

- sessions and immutable configuration;
- source files and digests;
- baselines and fingerprints;
- transformations and canonical encodings;
- candidate attempts and per-run observations;
- cache entries;
- accepted transitions;
- publication records.

One writer owns the database connection. Workers send completed observations over a bounded channel; they never write SQLite directly. Each accepted transition and its causal attempt commit in one transaction.

`reprocut resume` validates all immutable session fields. Source changes, command changes, normalization-schema changes, adapter-version changes, or incompatible database schema cause a clear refusal. `--restart` creates a new session without deleting the prior database.

Complete preserved/rejected results are reusable. Inconclusive and cancelled results are retained as history but are retried unless the user chooses `--reuse-inconclusive` for diagnostics only; they still cannot authorize a cut.

## 12. Deterministic parallel frontier

The scheduler creates a bounded, totally ordered frontier. Candidate rank includes phase, granularity, subset/complement class, start index, and transformation ID.

Workers may finish in any order. The scheduler commits the earliest preserved rank only after every earlier rank has reached a terminal verdict. Results after the winner remain useful cache entries but cannot alter the transition. The next frontier derives solely from the committed state.

The worker count is `--jobs N`, with `0` meaning detected hardware parallelism. Queue capacity is at most twice the worker count. Candidate plans share immutable file metadata and blobs; only operation vectors and changed file buffers are candidate-local.

Loom models cover result publication, cancellation, winner ordering, and writer shutdown. Property tests compare complete accepted chains for worker counts 1, 2, 4, and 16 across generated verdict maps.

## 13. Attempt ledger and report

The artifact contains:

```text
reprocut-output/
├── project/
├── reduction.json
├── attempts.jsonl
├── report.html
├── issue.md
├── reproduce.sh
├── reproduce.ps1
└── container/          # present after OCI preparation/export
```

The report shows:

- original versus retained files, bytes, source lines, manifest entries, and syntax nodes;
- baseline, reduction, preparation, and final-verification durations;
- candidate executions, cache hits, retries, incomplete results, and worker utilization;
- selected oracle channels, fingerprint hash, and a green “Same failure” badge only after final verification;
- accepted stages grouped by directory/file/manifest/syntax phase;
- retained-item evidence from final-context deletion attempts;
- process containment mechanism;
- flaky observations and confidence interval when enabled;
- exact reproducibility and safety limitations.

The self-contained report performs no external requests. All user-controlled text is escaped. Large attempt histories are summarized in HTML while the complete stream remains in `attempts.jsonl`.

## 14. GitHub issue export

`issue.md` is generated from the same immutable report model. The HTML report provides “Copy GitHub issue” and “Download issue.md” actions.

The issue body includes:

- title suggestion;
- exact command;
- fingerprint hash and termination state;
- before/after measurements;
- retained tree;
- reproduction instructions;
- optional OCI invocation;
- ReproCut version and platform;
- a disclosure that the report proves observational equivalence, not root cause.

Clipboard failure falls back to selecting the Markdown and offering the download. The report never opens a prefilled GitHub URL containing source content because URL/query leakage is avoidable.

## 15. OCI export

The command is:

```console
reprocut export oci --from reprocut-output --output reprocut.oci.tar
```

The ecosystem adapter generates a minimal build context, an entrypoint that runs the exact reproduction command, and labels containing the fingerprint hash, ReproCut version, source digest, and creation timestamp. Base images are ecosystem-specific and their resolved digest is recorded.

ReproCut detects Docker Buildx, Podman, or BuildKit in that order and requests OCI archive output. Dependency installation runs with the least authority supported by the selected adapter. The output is not claimed reproducible unless two clean builds produce the same normalized OCI manifest and layer digests. If no builder exists, the command leaves a complete context and exits with a non-zero “builder unavailable” result; it does not call a Dockerfile an OCI image.

The default container has no embedded registry credentials, source checkout, ReproCut state database, or unrelated files.

## 16. VS Code and Cursor extension

The extension is intentionally thin. It contains no reducer implementation and communicates with the versioned CLI JSON protocol.

Capabilities:

- explorer context action: “Minimize this failure”;
- command-palette action with ecosystem auto-detection;
- active test command selection;
- progress and cancellation;
- resume existing session;
- open `report.html`, `issue.md`, or reduced project;
- display fingerprint and final verification status.

The extension verifies the CLI protocol version and explains how to install the binary. It never downloads or executes a binary without explicit user action. Cursor compatibility comes from the VS Code extension API; no separate fork is maintained unless incompatibility is demonstrated.

## 17. Public gallery

The gallery is static, opt-in, and PR-curated.

```console
reprocut gallery prepare --from reprocut-output
```

This produces a submission directory with a redacted metadata document, screenshots/report assets, license declaration, and optional minimal source selected by the user. Nothing is uploaded automatically.

Submission flow:

1. the user opens a generated GitHub issue/PR template;
2. PR CI validates schema, file sizes, paths, licenses, and secret-scan results without executing submitted code;
3. a maintainer reviews the diff;
4. optional reproduction happens only through a manually approved, no-secret, network-restricted workflow;
5. the static gallery is rebuilt and deployed to GitHub Pages;
6. “Repro of the Week” is a reviewed metadata flag committed through PR.

The gallery has no account system, tracking, private-repository upload, arbitrary backend storage, or automatic execution service in 0.1.

## 18. Performance and memory evidence

The benchmark suite separates pure scheduling cost from command execution cost.

Fixtures:

- flat universes of 1K, 10K, and 100K units;
- directory tries with shallow-wide and deep-narrow shapes;
- Rust, Python, and TypeScript syntax trees;
- manifest sets with 10, 100, and 1K entries;
- deterministic and flaky verdict maps;
- cold cache, warm memory cache, and resumed SQLite cache.

Measurements:

- Criterion wall-clock distributions on a documented dedicated machine;
- Iai/Cachegrind instructions, branches, L1 data misses, last-level misses, and allocations for pure reducers;
- peak RSS and bytes copied per candidate;
- candidate execution count and cache hit rate;
- sequential versus parallel throughput and deterministic-chain equality.

Buffers are reused in the ddmin hot path. Paths and blobs use shared immutable storage. Frontier queues are bounded. Attempt serialization streams to SQLite/JSONL instead of retaining full logs. Full captured output is never retained beyond configured byte limits.

README speed claims require raw results committed under `benchmarks/results/`, hardware/compiler metadata, at least 30 measured samples for wall-clock comparisons, confidence intervals, and a date. CI compiles benchmarks and runs deterministic instruction-count gates; noisy hosted-runner wall time is an artifact, not a regression gate.

## 19. Release binaries and provenance

Release creation is tag-driven and environment-protected. Required binary targets are:

- Linux x86_64 GNU;
- Linux x86_64 musl;
- Linux aarch64 GNU;
- Windows x86_64 MSVC;
- macOS x86_64;
- macOS aarch64.

Each archive contains the binary, README, licenses, shell completions, and version metadata. The release publishes SHA-256 checksums, CycloneDX or SPDX SBOMs, and GitHub build-provenance attestations. A clean-machine smoke test installs each archive and reduces its ecosystem fixture before the release job can publish assets.

This distribution path solves end-user dependence on a local Rust compiler. It does not bypass or weaken Windows Application Control on the development machine.

## 20. crates.io publication

As of 2026-08-11, registry API checks returned 404 for `reprocut` and the planned internal crate names. Availability is not ownership and remains a release-time blocker until the names are actually published.

The CLI package is renamed from `reprocut-cli` to package `reprocut` while retaining binary name `reprocut`. Internal packages publish in dependency order. Every registry dependency uses both a local path and exact compatible `0.1.0` version.

All publishable manifests include description, repository, homepage, readme, license, authors, keywords, categories, rust-version, include/exclude rules, and documentation URL. `reprocut-python` is marked `publish = false` for crates.io.

Release gates:

1. `cargo package --list` contains only intended files;
2. `cargo publish --dry-run` passes for every crate from a clean tree;
3. packaged crates compile and test without workspace path assumptions;
4. crates publish in dependency order with registry propagation checks;
5. `cargo install reprocut --version 0.1.0 --locked` passes in a clean container;
6. the installed CLI completes the real reduction smoke fixture.

crates.io publication is permanent. Upload requires a protected, least-scope `CARGO_REGISTRY_TOKEN`, manual environment approval, the exact `v0.1.0` tag, and a clean release commit.

## 21. PyPI publication

The PyPI project is `reprocut`. As of 2026-08-11 its JSON endpoint returned 404; final availability is checked immediately before Trusted Publisher creation.

The Python API exposes typed equivalents of the stable oracle, request/configuration models, progress events, outcome/attempt models, and a high-level `reprocut.reduce(...)`. `python -m reprocut` and the Python console entry point invoke the same native engine and JSON protocol as the Rust CLI; they do not maintain a second reducer.

PyO3 uses `abi3-py39`. Required wheel platforms are manylinux x86_64/aarch64, Windows x86_64, and macOS x86_64/aarch64; an sdist is included. Tests install the built wheel into clean Python 3.9, 3.10, 3.11, 3.12, and 3.13 environments and require the native backend.

Release gates:

1. metadata/readme/license validation;
2. wheel and sdist build through pinned Maturin;
3. wheel content inspection and `twine check` equivalent;
4. clean-environment import, typing, CLI, and real-reduction smoke tests;
5. TestPyPI installation test for the release candidate;
6. PyPI upload through a GitHub Actions Trusted Publisher using OIDC and a protected `pypi` environment;
7. post-publish `pip install reprocut==0.1.0` verification.

Long-lived PyPI tokens are not stored. The registered workflow path, repository, environment, and tag condition are treated as release credentials.

## 22. Testing strategy

Development follows red-green-refactor for every behavior change.

### 22.1 Core and property tests

- exhaustive small-universe ddmin subset/complement behavior;
- 1-minimal final results;
- parallel/sequential accepted-chain equality;
- canonical transformation encoding and digest stability;
- non-overlapping byte-edit properties;
- oracle channel and flaky aggregation properties;
- source-tree immutability.

### 22.2 Concurrency and persistence

- Loom models for winner ordering, cancellation, writer shutdown, and bounded work queues;
- crash/reopen tests at every SQLite transaction boundary;
- schema migration and incompatible-resume rejection;
- duplicate-result and late-worker-result idempotence;
- Miri for pure core/state abstractions where supported;
- AddressSanitizer for parser, runner, and process fixtures.

### 22.3 Ecosystems and syntax

- golden manifest edits for Cargo.toml, pyproject.toml, and package.json;
- offline preparation success and explicit-unavailable cases;
- Tree-sitter corpus tests for every allowlisted node kind;
- parse-error rejection and UTF-8 boundary tests;
- end-to-end Rust, Python, JavaScript, and TypeScript reductions.

### 22.4 Platform and product surfaces

- Unix and Windows descendant process-tree timeout fixtures;
- report HTML escaping, golden output, browser accessibility, reduced motion, and no-network contract;
- GitHub issue Markdown golden;
- OCI layout and clean-container reproduction;
- VS Code extension protocol and command tests;
- gallery schema, secret scanning, and static build tests;
- crates.io/PyPI package dry-runs and clean-install smoke tests.

## 23. Failure handling

Errors are typed and separated into source, adapter discovery, preparation, execution, oracle, persistence, parser, scheduler, publication, OCI, and release categories. User-facing messages identify the failed phase, affected candidate, whether the run is resumable, and the exact evidence that was unavailable.

Ctrl-C stops scheduling new work, asks workers to terminate their process groups, drains completed records into the writer, commits a resumable checkpoint, and exits non-zero. A second interrupt requests immediate best-effort teardown without claiming a clean checkpoint.

Corrupt or incompatible state is never silently reset. The user may export diagnostics, start a new session, or explicitly archive the old database.

## 24. Backward compatibility

The existing explicit form remains supported:

```console
reprocut reduce --root PROJECT --output OUTPUT -- COMMAND
```

`reprocut minimize` is the zero-configuration front door. Existing JSON fields remain present; the schema gains a version and additive structured sections. `--oracle-stream stderr` reproduces the original oracle selection. The default changes to `auto` because 0.1 has not been publicly released and silent omission of stdout failures is a correctness defect.

The Python fallback remains clearly labeled and covers pure oracle semantics only. Full reduction requires the native wheel; it does not silently fall back to a behaviorally different Python engine.

## 25. Explicit non-goals for 0.1

- Root-cause diagnosis or automated bug fixing.
- A hosted source-upload backend.
- Automatic execution of arbitrary gallery submissions.
- A hostile-code security sandbox.
- Perfect semantic validity for every language construct.
- Global-minimum guarantees.
- Distributed execution across multiple hosts.
- Unmeasured speed claims.

These exclusions do not weaken the requested local systems/compiler-tooling product. They prevent unrelated hosted-service and program-analysis claims from being smuggled into the release.

## 26. Release acceptance contract

`v0.1.0` may be created only when all statements below have fresh evidence:

1. The source tree remains byte-identical after interrupted, failed, completed, and resumed reductions.
2. Directory, file, manifest, and syntax phases each materially reduce at least one checked-in real fixture.
3. Rust, Python, JavaScript, and TypeScript final artifacts independently reproduce their stabilized failure.
4. `auto`, `stderr`, `stdout`, and `combined` oracle contracts pass.
5. Strict mode and the configurable flaky aggregate pass deterministic fixtures.
6. Timed-out descendant trees cannot create delayed marker files on Linux, Windows, or macOS.
7. SQLite crash/reopen testing never commits a transition without its causal preserved observation.
8. Worker counts 1, 2, 4, and 16 produce byte-identical accepted chains and final projects.
9. Report, JSON, JSONL, issue Markdown, reproducer scripts, and OCI metadata agree on command and fingerprint.
10. The report makes no external request and passes desktop/mobile keyboard and accessibility checks.
11. The VS Code/Cursor extension invokes the versioned protocol and never contains reducer logic.
12. Gallery preparation uploads nothing and gallery CI executes no unreviewed submission.
13. Benchmarks publish raw evidence and README claims do not exceed that evidence.
14. Loom, Miri, Clippy, rustfmt, Rust tests, Python tests, extension tests, sanitizer, dependency policy, and platform matrices pass.
15. Every prebuilt archive passes clean-machine reduction and has matching checksum, SBOM, and provenance.
16. `cargo publish --dry-run` and clean `cargo install` pass for packaged crates.
17. PyPI wheels/sdist pass clean native tests and TestPyPI installation.
18. Registry uploads are tag-bound, protected, manually approved, and use the documented credential model.

Until all 18 conditions pass, the repository may describe work as “ReproCut 0.1 development” but not as a completed 0.1 release.

## 27. Delivery decomposition

This document is the umbrella release contract. Implementation is intentionally divided into reviewable subprojects; every subproject receives its own TDD plan and commit sequence, but none is marketed as a separate product version.

1. **Failure evidence and containment:** multi-stream oracle, flaky aggregation, portable termination reasons, Unix process groups, Windows Job Objects, and descendant tests.
2. **Search kernel and durable execution:** transformation model, subset/complement ddmin, directory hierarchy, SQLite journal, resume, deterministic frontier, and Loom models.
3. **Structured reducers:** Cargo/Python/npm discovery, manifest preparation, Tree-sitter deletion/hoisting, and ecosystem end-to-end fixtures.
4. **Evidence and portability:** attempt ledger, metrics, report, issue export, reproducibility scripts, and OCI archive.
5. **Developer surfaces:** versioned JSON protocol, typed Python API, VS Code/Cursor extension, gallery preparation, and static gallery.
6. **Release proof:** performance/memory suite, prebuilt archives, checksums/SBOM/attestations, crates.io packaging, PyPI wheels/sdist, TestPyPI, and registry smoke tests.

The dependency order is strict. A later subproject may consume only public contracts accepted in earlier subprojects. Cross-cutting behavior changes require updating the umbrella contract and the affected subproject plan before implementation.
