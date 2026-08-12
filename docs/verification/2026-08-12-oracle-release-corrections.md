# ReproCut 0.1.0 failure-oracle release corrections

Date (UTC): 2026-08-12T18:36:52Z  
Verified source commit: `ad4e0186fb265264ca46a4393432af12f6a4e282`  
Branch: `feat/reprocut-mvp`

## Outcome

The three release blockers in the automatic failure oracle are corrected without
changing the reducer architecture:

1. A colon-number token is normalized as a source location only when its token
   is path-like, has a recognized source/manifest extension, or is an internal
   normalized temporary path. Semantic values such as `status:404`,
   `expected:123`, and `shard:12` remain part of failure identity.
2. `combined` anchor selection reserves the highest-ranked stdout anchor and the
   highest-ranked stderr anchor before filling the remaining two positions from
   the global ranking.
3. The adversarial suite is a real Cargo integration target at
   `crates/reprocut-core/tests/oracle_adversarial.rs`. CI invokes it explicitly
   beside the contract and property targets. The property generator now wraps
   arbitrary content in a recognized `ValueError:` diagnostic.

Normalization schema 3 intentionally invalidates fingerprints and resume/cache
identity created by the earlier over-broad rule. Report validation, Python
parity fixtures, demo evidence, release documentation, and golden assets all use
the same schema.

## Test-first evidence

Before the implementation changed, the new Python regressions produced four
failures: the three semantic colon-number cases and the combined-stream case.
The equivalent Rust adversarial harness produced two failing tests: one
table-driven semantic-value contract and one combined-stream contract. The
positive `src/main.rs:12` to `src/main.rs:99` case already passed, establishing
that the correction had to narrow location recognition rather than remove it.

## Fresh verification evidence

| Gate | Result | Evidence |
|---|---:|---|
| Full Python suite | Pass with one explicit skip | `61 passed; 1 skipped`; the native-wheel smoke test remains isolated in its dedicated CI job. |
| Focused Python oracle/release suite | Pass | `26 passed`; includes semantic colon values, source locations, per-stream quotas, parity, and release-audit coverage. |
| Rust adversarial integration target | Pass | Official stable Rust Playground: `9 passed; 0 failed`. |
| Rust automatic-oracle contract | Pass | Official stable Rust Playground: `7 passed; 0 failed`. |
| Rust oracle modes/parity contract | Pass | Official stable Rust Playground: `7 passed; 0 failed`. |
| Full CLI source composition | Pass | Official stable Rust Playground: `7 passed; 0 failed`. |
| Evidence/report contract | Pass | Official stable Rust Playground: `3 passed; 0 failed`. |
| Report golden contract | Pass | Official stable Rust Playground: `1 passed; 0 failed`. |
| Rust formatting | Pass | Official rustfmt service accepted all 14 modified Rust files; the final adversarial assertion change was rechecked separately. |
| Static release audit | Pass | Nine gates passed, including the new `oracle-ci-coverage` gate. |
| CI workflow syntax | Pass | `.github/workflows/ci.yml` parsed successfully as YAML. |
| Python bytecode compilation | Pass | `python -m compileall -q python scripts`. |
| Repository whitespace audit | Pass | `git diff --check` produced no findings before the source commit. |

## Regenerated evidence

| Measurement | Value |
|---|---:|
| Evidence schema | 3 |
| Normalization schema | 3 |
| Original files | 18 |
| Retained files | 3 |
| Candidate attempts | 24 |
| Final verification | 3/3 |
| Failure fingerprint | `48ebabae825720ad781c82ce1da9dbf43f33aceac71b34d140c9a7fc40562b00` |
| Demo GIF | 24 frames, 1200x675, 814015 bytes |

The JSON evidence, JSONL attempt ledger, HTML report, issue body, golden report,
banner fingerprint, and GIF were regenerated together.

## Environment limitations — not passes

- Windows Application Control blocks the local Cargo and native Ruff binaries
  with OS error 4551. Native Cargo, Clippy, Miri, Loom, and Ruff remain CI gates;
  no local pass is claimed for them.
- The official Rust Playground manifest does not expose `proptest`, so the
  property target was not executed in the flattened remote harness. The test
  source was corrected, the workflow names the target explicitly, and the
  release audit fails if any of the three oracle targets disappears from that
  job. The GitHub CI result remains the publishing authority for this target.
- Flattened Playground compilation emits unused-item and unknown-`cfg(loom)`
  warnings because it does not reproduce the repository's workspace boundaries
  and lint table. No Clippy result is inferred from those compilations.

