# ReproCut 0.1.0 integrity-hardening verification

Date (UTC): 2026-08-12T16:09:12Z  
Verified source commit: `42b875f7ec29c23ef7fd362f897c01e5a3d85d23`  
Branch: `feat/reprocut-mvp`

## Outcome

The 0.1.0 source now binds reductions to immutable source bytes, executable metadata, explicit child environments, mode-aware oracle identity, preparation identity, and a complete resume/cache contract. The public CLI and Python request surfaces expose the same validated modes. Evidence schema 3 records those contracts, and the release audit rejects stale or incomplete proof.

No performance claim is made. This record covers correctness, integrity, packaging metadata, and the checked-in demo/report contracts.

## Fresh verification evidence

| Gate | Result | Evidence |
|---|---:|---|
| Full CLI source composition | Pass | Official stable Rust Playground: `7 passed; 0 failed` using `--scope full` with `cli_compile_contract.rs`. |
| Adversarial automatic oracle | Pass | Official stable Rust Playground: `6 passed; 0 failed`. Covers changed exception identity, semantic assertion numbers, pytest node IDs, punctuation-only baselines, and shortened stacks. |
| Immutable snapshot integrity | Pass | Official stable Rust Playground: `4 passed; 0 failed`. Covers byte replacement/deletion from captured snapshots and Unix execute-mask identity. |
| Isolated Python preparation | Pass | Official stable Rust Playground: `7 passed; 0 failed`. Covers normalized extras, request binding, and fail-closed missing isolation inputs. |
| Session identity and resume boundary | Pass | Official stable Rust Playground: `3 passed; 0 failed`. Every integrity dimension changes identity; legacy schema-1 sessions fail closed. |
| Explicit child environment | Pass with one platform ignore | Official stable Rust Playground: `2 passed; 0 failed; 1 ignored`. The ignored test is platform-specific in the flattened remote harness. |
| Evidence/report schema | Pass | Official stable Rust Playground: `3 passed; 0 failed`. |
| Reduction pipeline | Pass | Official stable Rust Playground: `3 passed; 0 failed`. |
| Python suite | Pass with one explicit skip | `56 passed; 1 skipped`. The skip is the native-wheel import smoke test, which is enabled in its dedicated CI job. |
| Playground include inliner regression | Pass | `2 passed`; both single-file and workspace verifiers inline multiline `include_str!` assets. |
| Static release audit | Pass | Eight gates passed: version, demo evidence, attempt ledger, upstream corpus, release surfaces, targets, integrity, and honest README. |
| Browser/report contract | Pass | Edge/Playwright at 1440x1000 and 390x844: one local request, no horizontal overflow, reveal state `1`, visible 3 px focus outline, working copy status, and `issue.md` download filename. |
| Python bytecode compilation | Pass | `python -m compileall -q python scripts`. |
| Repository whitespace audit | Pass | `git diff --check` produced no findings before this record was committed. |
| Modified Rust formatting | Pass | Official rustfmt service reported both modified Rust contract files compatible. |

## Demo evidence

| Measurement | Value |
|---|---:|
| Evidence schema | 3 |
| Original files | 18 |
| Retained files | 3 |
| Candidate attempts | 24 |
| Baseline agreement | 3/3 |
| Final verification | 3/3 |
| Failure fingerprint | `3a19748e9e2191ac0c0080a6648cdebe1d74e683132d9be85b4ccb17554d9e1c` |

The checked-in JSON, JSONL ledger, HTML report, issue body, banner fingerprint, and 24-frame 1200x675 GIF were regenerated from the schema-3 fixture. The local Python source and reduced project were each executed three times.

## Environment limitations — not passes

- Native local Rust commands and native Ruff are blocked by Windows Application Control with OS error 4551. Cargo, Clippy, Miri, Loom, native PyO3-wheel import, and native Ruff results are therefore delegated to the committed CI jobs; this record does not claim local passes for them.
- The all-90-file rustfmt API sweep exceeded both 120-second and 300-second host limits. The two Rust files changed in the release-proof slice passed the same official rustfmt endpoint individually.
- The larger full behavioral CLI contract was compiled by the Playground but the remote linker was killed with `SIGKILL` under its memory limit. The same full source composition passed the lighter seven-test CLI compile/API contract. Crate-bounded behavioral contracts above all passed.
- Flattened Playground crates emit unused-item and unknown-`cfg(loom)` warnings because workspace boundaries and the repository lint table are not reproduced. No Clippy pass is inferred from Playground output.
- The first final pytest attempt was unable to enumerate pytest's global Windows temp directory. Re-running with a fresh workspace-local `--basetemp` produced the recorded `56 passed; 1 skipped`; no failed test body was hidden.

## Distribution handoff

The source ZIP is created after this verification record from tracked `HEAD` with `git archive`. Its SHA-256 is intentionally reported in the external handoff rather than embedded here, because embedding an archive's own digest would change the archive recursively. Publishing to crates.io or PyPI, pushing, and tagging remain explicit user actions.
