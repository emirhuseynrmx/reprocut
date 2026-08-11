# ReproCut Product and System Design

Date: 2026-08-11
Status: File-reduction MVP implemented; broader research design remains staged
Repository name: `reprocut`

## 1. Product thesis

ReproCut turns a failing software project into the smallest practical, standalone project that still produces the same failure.

The user supplies a command that currently fails:

```text
reprocut reduce -- npm test
reprocut reduce -- pytest tests/test_checkout.py
reprocut reduce -- cargo test parser_rejects_invalid_utf8
```

ReproCut repeatedly executes controlled variants of the project, removes irrelevant material, and emits a shareable reproduction:

```text
reprocut-output/
├── project/
├── README.md
├── reprocut.json
├── reproduce.sh
├── reproduce.ps1
└── report.html
```

The core promise is not “package my machine.” It is:

> Preserve this exact failure while removing everything that is not needed to reproduce it.

## 2. Target user and primary jobs

The target is any developer who can provide a repeatable failing command, independent of language or framework.

Primary jobs:

1. Prepare a minimal reproduction before opening an upstream issue.
2. Reduce a large internal failure until the relevant files and dependencies become visible.
3. Turn a CI-only or teammate-only failure into a portable artifact.
4. Remove proprietary, unrelated code before sharing a bug externally.
5. Produce small regression fixtures for maintainers.

The first release is a local CLI with no account, hosted service, API key, or editor requirement.

## 3. Product boundaries

### 3.1 Implemented first public slice

- Portable Rust orchestration intended for Windows, macOS, and Linux, with platform smoke tests defined in CI.
- Language-agnostic regular-file reduction for an arbitrary repeatable command.
- Stable failure identity from repeated exit state and normalized stderr diagnostics.
- Three-valued candidate evaluation: preserved, rejected, or inconclusive.
- A fresh disposable workspace for every baseline, candidate, and final verification.
- Deterministic hierarchical delta debugging with an in-memory candidate cache.
- Bounded concurrent stdout/stderr drains and a per-run deadline.
- JSON result state, a self-contained HTML report, and shell/PowerShell launchers.
- A typed PyO3 surface for the stabilized failure oracle plus an explicit source-checkout reference backend.
- Loom, Miri, sanitizer, Clippy, rustfmt, dependency-policy, native-wheel, and platform jobs defined in CI.

### 3.2 Explicit post-MVP slices

- Manifest-aware dependency reduction for Rust, Python, and JavaScript/TypeScript.
- Syntax-aware statement, function, and module reduction.
- Persistent candidate caching, interruption recovery, and resume.
- Parallel candidate execution; the atomic lowest-winner primitive exists, but the engine remains sequential.
- User-configurable stdout/stderr patterns and plugin-defined predicates.
- Prebuilt signed binaries and release automation.

### 3.3 Explicit non-goals for MVP

- Automatically fixing or explaining the bug.
- Supporting every build system with equal semantic depth.
- Treating arbitrary downloaded code as safely sandboxed.
- Capturing an entire operating system or emulating a different CPU.
- Minimizing databases, cloud resources, or distributed systems.
- Guaranteeing the mathematically smallest possible program.
- Using an LLM as the correctness oracle.

The output is “practically minimal” and locally irreducible under the configured reducers, not a global minimum.

## 4. User experience

### 4.1 First run

```text
$ reprocut reduce -- npm test -- checkout

ReproCut 0.1
Baseline run 1/3 ........ SAME FAILURE
Baseline run 2/3 ........ SAME FAILURE
Baseline run 3/3 ........ SAME FAILURE

Failure fingerprint
  exit: 1
  anchor: TypeError: Cannot read properties of undefined
  frame: src/checkout/price.ts:84

Project inventory
  18,421 files • 143 direct dependencies • 1.4 GiB

Reducing directories  [██████████████████░░] 91%
Reducing files        [██████████░░░░░░░░░░] 50%
Reducing dependencies [████░░░░░░░░░░░░░░░░] 22%
```

At completion:

```text
Failure preserved in 6 files and 4 dependencies.
Output: ./reprocut-output
Report: ./reprocut-output/report.html
Re-run: ./reprocut-output/reproduce.sh
```

### 4.2 Failure oracle configuration

The default matcher derives a conservative fingerprint from three baseline runs. Users can override it:

```text
reprocut reduce \
  --expect-exit 1 \
  --expect-stderr "TypeError:.*currency" \
  -- npm test -- checkout
```

Other supported predicates:

- process terminated by a particular signal;
- command exceeded a configured duration;
- stdout or stderr matched or did not match a regular expression;
- a file was produced with a particular digest;
- a plugin returned preserved, rejected, or inconclusive.

The UI must always show the active oracle. A generic compilation failure must never silently replace the original runtime failure.

## 5. Reduction model

### 5.1 Candidate state

A candidate is an immutable description, not a copied directory:

- baseline content identifier;
- retained unit bitset;
- manifest edits;
- source-tree edits;
- command and sanitized environment;
- failure-oracle version;
- reducer and adapter versions.

The candidate key is a BLAKE3 digest of this normalized description. Results are cached by candidate key.

### 5.2 Three-valued result

Every execution returns exactly one classification:

- `Preserved`: the intended failure is still present;
- `Rejected`: the intended failure disappeared;
- `Inconclusive`: infrastructure error, unrelated build failure, timeout ambiguity, or unstable output.

`Inconclusive` is never treated as `Preserved`. This prevents aggressive reduction from converging on a broken but unrelated project.

### 5.3 Reduction stages

1. **Baseline stabilization**
   - Run the original command three times by default.
   - Normalize volatile text such as temporary paths, timestamps, PIDs, ports, and addresses.
   - Refuse automatic reduction if the failure is not stable enough, unless the user enables flaky mode.

2. **Inventory and protection**
   - Detect repository root, VCS state, manifests, generated directories, ignored files, symlinks, and large assets.
   - Record the baseline tree digest and warn about uncommitted files.

3. **Coarse hierarchical reduction**
   - Partition removable directories using hierarchical delta debugging.
   - Descend only into retained partitions.

4. **File-level reduction**
   - Reduce files in dependency-informed groups, followed by a one-minimal sweep.

5. **Manifest reduction**
   - Remove direct dependencies and optional features through ecosystem adapters.
   - Preserve lockfiles unless the adapter can update them deterministically.

6. **Syntax-aware reduction**
   - Parse selected source files with bundled tree-sitter grammars.
   - Attempt deletion or simplification of syntactically valid subtrees, largest first.

7. **Normalization and verification**
   - Remove empty directories and unused generated configuration.
   - Execute the final candidate multiple times.
   - Emit provenance, limitations, and exact reproduction commands.

### 5.4 Search strategy

ReproCut uses hierarchical delta debugging rather than naive single-item deletion. Independent candidate complements may run concurrently, but accepted transitions remain deterministic:

- candidates receive monotonically increasing IDs;
- a reduction round evaluates an immutable frontier;
- if several candidates preserve the failure, the deterministic score order chooses the winner;
- score order is retained size, semantic damage penalty, execution cost, then candidate ID;
- all workers are cancelled after the winning transition is committed.

This provides parallel speed without making output depend on scheduler timing.

## 6. Architecture

```text
CLI
 │
 ├── Project inventory
 ├── Failure oracle
 ├── Reduction coordinator
 │    ├── Reducer pipeline
 │    ├── Candidate cache
 │    └── Deterministic scheduler
 │
 ├── Workspace backend
 │    ├── portable copy/reflink backend
 │    ├── git worktree backend
 │    └── optional container backend
 │
 ├── Process runner
 │    ├── resource limits
 │    ├── stdout/stderr capture
 │    └── cancellation and timeout
 │
 ├── Ecosystem adapters
 │    ├── Cargo
 │    ├── Python
 │    └── npm/pnpm/yarn
 │
 └── Report generator
```

Proposed Rust workspace:

```text
crates/
├── reprocut-cli
├── reprocut-core
├── reprocut-oracle
├── reprocut-reducer
├── reprocut-workspace
├── reprocut-runner
├── reprocut-adapters
├── reprocut-report
└── reprocut-python
```

Crate boundaries follow independently testable protocols. Ecosystem-specific logic may not leak into the core search engine.

## 7. Workspace isolation and safety

Each candidate runs in a disposable workspace. The original project is never intentionally edited.

Backend preference:

1. filesystem reflink when supported;
2. Git worktree plus copied untracked inputs when safe;
3. content-addressed materialization into a temporary directory;
4. optional user-selected container backend.

Important security boundary:

> Workspace isolation protects project state; it is not a security sandbox for hostile commands.

The default runner executes a command supplied by the developer. Network blocking, host filesystem containment, and syscall filtering require an explicit container or operating-system sandbox backend. ReproCut must not claim otherwise.

Additional safeguards:

- refuse to use the project root itself as a candidate workspace;
- resolve and validate every deletion target below the candidate root;
- never follow a symlink for mutation outside the candidate root;
- maintain a baseline digest and report unexpected original-tree changes;
- redact secrets and absolute user paths from shareable reports by default;
- require explicit opt-in before packaging files outside the project root.

## 8. Performance and memory design

The implementation should demonstrate systems-quality engineering without distorting the product around micro-optimizations.

- Candidate retained sets use dense bitsets for ordinary inventories and a sparse representation only when measured to be beneficial.
- Immutable interned paths are stored once and referenced by compact IDs.
- Candidate descriptions use structural sharing.
- Worker communication uses bounded channels to enforce backpressure.
- Execution concurrency is controlled by CPU, memory, and process permits rather than an unbounded task queue.
- Stdout and stderr use bounded capture with streaming fingerprints; full logs are optional.
- Candidate results and artifacts use a content-addressed on-disk cache.
- Hot scheduling paths avoid per-candidate string allocation.
- Large filesystem inventories are traversed incrementally and sorted deterministically.
- Cancellation must terminate the complete child-process tree.

Optimization claims require Criterion benchmarks, allocation counts, and end-to-end measurements. “Zero allocation” will not be used as marketing unless verified for a precisely named hot path.

## 9. Ecosystem adapters

### 9.1 Portable core

The core can remove directories and files for any project as long as the user supplies a reliable failing command.

### 9.2 Rust

- understand Cargo workspaces and package membership;
- reduce direct dependencies and features in `Cargo.toml`;
- preserve `Cargo.lock` deterministically;
- recognize generated `target/` content as non-input;
- parse Rust through a pinned tree-sitter grammar.

### 9.3 Python

- recognize `pyproject.toml`, requirements files, and common test layouts;
- reduce declared dependencies without silently consulting a different global environment;
- capture interpreter identity and relevant environment metadata;
- parse Python through a pinned tree-sitter grammar.

### 9.4 JavaScript and TypeScript

- recognize npm, pnpm, and yarn lockfile ownership;
- reduce manifest dependencies while maintaining the selected package-manager contract;
- avoid repeatedly copying `node_modules` by using an immutable dependency layer when safe;
- parse JavaScript and TypeScript through pinned grammars.

### 9.5 Plugin protocol

The stable protocol is process-based JSON first so any language can implement an adapter. A PyO3 binding then provides an ergonomic Python SDK without making Python part of the core runtime.

Plugin hooks:

- inventory enrichment;
- candidate transformation;
- failure predicate;
- normalization rule;
- output packager.

## 10. Persistence and resumability

Every run creates `.reprocut/run.json` in its state directory, containing:

- tool and schema versions;
- baseline digest;
- command and redacted environment allowlist;
- oracle configuration;
- accepted reduction chain;
- candidate result cache index;
- random seeds, if any;
- adapter versions;
- final verification result.

Writes use temporary-file plus atomic rename. The run can resume only if the baseline and configuration still match.

## 11. Testing strategy

### 11.1 Unit and property tests

- oracle normalization and fingerprint equivalence;
- three-valued classification;
- ddmin invariants and one-minimal termination;
- deterministic winner selection under permuted completion order;
- path containment and symlink escape rejection;
- manifest round-tripping;
- cache-key stability;
- state-file crash recovery.

### 11.2 Integration tests

Hermetic fixture projects for:

- stable runtime exception;
- compiler error that must not replace the intended failure;
- process crash by signal;
- timeout predicate;
- flaky failure;
- multi-package workspace;
- symlink escape attempt;
- child process that survives parent cancellation;
- interrupted and resumed reduction.

### 11.3 Differential and fuzz testing

- compare the sequential reference reducer and parallel reducer on generated universes;
- fuzz manifest adapters and oracle normalizers;
- fuzz state loading and report parsing;
- run reducers against generated projects with a planted minimal failure set.

### 11.4 Real-world benchmark corpus

Before launch, curate at least five historical, redistributable bugs across Rust, Python, and JavaScript/TypeScript. For each case publish:

- original and final sizes;
- command executions;
- wall time and peak memory;
- final stability rate;
- whether a maintainer could reproduce from the emitted bundle;
- known limitations.

No performance or reduction ratio will be advertised before this corpus exists.

## 12. Error handling

User-facing failures must identify the next action:

- **Unstable baseline:** show fingerprint differences and suggest a stricter oracle or flaky mode.
- **Unrelated candidate failure:** mark inconclusive and retain the last valid candidate.
- **Missing toolchain:** report the exact executable and detected adapter.
- **Workspace exhaustion:** pause safely, retain state, and show cache/storage controls.
- **Child process leak:** terminate the process group/job object and record the event.
- **Plugin crash:** quarantine the plugin result; never classify it as preserved.

## 13. HTML report and launch demo

The report is local and self-contained. It should show:

- original versus final file, dependency, and byte counts;
- a reduction waterfall by stage;
- the accepted candidate chain;
- removed directory and dependency groups;
- failure-fingerprint stability;
- exact reproduce command;
- redaction summary and limitations.

The launch GIF must demonstrate one real bug:

1. run a failing command in a recognizable project;
2. start ReproCut with the same command;
3. show the live reduction counters;
4. open the final six-or-similar-file project;
5. execute the generated reproducer and show the same failure.

Proposed Hacker News title:

> Show HN: ReproCut – turn a failing project into a minimal reproducible example

Launch readiness gates:

- prebuilt binaries for Windows, macOS, and Linux;
- no signup or network service;
- a five-minute quickstart and copy-paste demo;
- at least five real benchmark cases;
- explicit limitations;
- complete architecture and contributor documentation;
- CI, release signing/checksums, fuzzing, and reproducible fixtures.

This follows Show HN’s preference for non-trivial work that people can try directly without account barriers.

## 14. Success criteria

Engineering status for the implemented file-reduction slice:

1. **Implemented, CI verification pending:** source installation and one-command fixture reduction.
2. **Verified:** the emitted three-file Python project independently reproduces the stabilized failure.
3. **Verified:** source-tree digests are unchanged in the acceptance flow.
4. **Deferred:** a parallel scheduler is not integrated; the current engine is deterministic and sequential.
5. **Deferred:** the cache is run-local and interrupted runs do not resume.
6. **Partially demonstrated:** one Python checkout fixture is materially reduced; first-party semantic ecosystem adapters are deferred.

Launch targets are directional, not guaranteed:

- 200 genuine GitHub stars;
- 40 new profile followers;
- substantive HN discussion;
- external bug reports or contributed adapters;
- at least one upstream issue using a ReproCut-generated reproduction.

## 15. Principal risks and mitigations

### Wrong failure preservation

Risk: the reducer removes necessary code and converges on a different error.

Mitigation: conservative multi-signal fingerprints, three-valued results, baseline repetitions, and visible oracle configuration.

### Combinatorial execution cost

Risk: large projects require thousands of expensive command executions.

Mitigation: hierarchical units, dependency-guided ordering, deterministic parallel frontiers, persistent cache, and early coarse reduction.

### Flaky failures

Risk: a random pass or failure corrupts the search.

Mitigation: deterministic mode refuses unstable baselines. Flaky mode uses repeated trials and confidence thresholds and is not required for the first usable release.

### Cross-platform isolation

Risk: filesystem and process semantics differ materially.

Mitigation: portable correctness backend first, platform-specialized accelerators behind identical protocols, and an honest non-hostile-code boundary.

### Too many ecosystems

Risk: shallow adapters make the initial release unreliable.

Mitigation: language-agnostic file reduction is universal; Rust, Python, and JS/TS receive first-party semantic adapters; other ecosystems use the plugin protocol.

## 16. Decisions locked by this design

- Product name and repository: ReproCut / `reprocut`.
- User-facing category: automatic minimal reproduction generator.
- Rust owns orchestration, search, execution, persistence, and reporting.
- The first public surface is a local CLI.
- Correct failure identity is more important than maximal reduction.
- The core is language-agnostic, with deep initial adapters for Rust, Python, and JS/TS.
- Python is an extension surface, not a runtime dependency of the core.
- The original project is not used as an experimental workspace.
- No AI model is required for correctness.
- No public performance claim ships without a reproducible benchmark.

## 17. Prior art informing the design

- GCC, “How to Minimize Test Cases for Bugs”: https://www.gnu.org/software/gcc/bugs/minimize.html
- ReproZip: https://github.com/VIDA-NYU/reprozip
- Perses-style syntax-guided reduction / nappe: https://pypi.org/project/nappe/
- Coq Bug Minimizer: https://arxiv.org/abs/2202.13823
- Delta debugging overview: https://en.wikipedia.org/wiki/Delta_debugging
- Show HN guidelines: https://news.ycombinator.com/showhn.html

## 18. Implementation audit

The 0.1 implementation is intentionally the smallest honest product slice of this design: stabilize one failure, remove regular files in isolated copies, repeatedly prove the final result, and publish a portable artifact. It does not represent completion of the manifest-aware, syntax-aware, resumable, or parallel research system described elsewhere in this document.

Measured checked-in demo result:

- 18 original regular files;
- 3 retained files;
- 19 candidate evaluations;
- 5 in-memory cache reuses;
- 0 inconclusive evaluations;
- 3 baseline and 3 final verification runs;
- exact retained set: `bug.py`, `checkout.py`, `fixtures/order.json`.

The detailed evidence and local host constraints are recorded in `docs/verification/2026-08-11-mvp.md`.
