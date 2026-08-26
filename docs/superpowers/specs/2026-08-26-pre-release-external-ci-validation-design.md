# ReproCut v0.1.0 External CI Validation Design

**Date:** 2026-08-26
**Status:** Proposed for final review

## Objective

Before publishing ReproCut v0.1.0, validate it against three independently authored, publicly visible, currently failing GitHub pull requests. The validation must show that ReproCut can preserve a real CI failure while reducing its reproducer, without executing untrusted project code directly on the host or changing any upstream repository.

This is a release gate, not a benchmark marketing claim. A candidate counts only when its base revision passes the selected oracle and its pinned pull-request revision fails that same oracle.

## Candidate Matrix

| Tier | Repository and pull request | Pinned head | Initial oracle | Purpose |
| --- | --- | --- | --- | --- |
| Small Python | `redrose2100/openruyi-precommit-hooks#28` | `1a0e915e4e0daa89cce0b97dc488801fe4225a0e` | `pre-commit run end-of-file-fixer --all-files --show-diff-on-failure` | Deterministic, mutation-producing CI hook failure and fresh-snapshot containment without materializing unrelated hook environments |
| Medium Rust | `arthurmaciel/ipe-lang#1370` | `072f647ca425694728de3aa6f508f1c3820681f1` | Exact failing Ubuntu CI command, extracted from the completed run | Medium workspace and platform-sensitive CI behavior |
| Large Rust | `bevyengine/bevy#25553` | `762326968f6fac9e69c81a831ab91ab29afb9933` | `cargo run -p ci -- lints` with the five `undocumented_unsafe_blocks` diagnostics | Large, well-known workspace with a narrow PR-introduced failure |

The base refs are respectively `master`, `main`, and `main`. Their exact base commit SHAs will be recorded at experiment start so the evidence remains reproducible after branches move.

## Candidate Admission Gate

Each candidate must satisfy all of the following before ReproCut is run:

1. Record repository URL, base SHA, PR head SHA, CI run URL, job URL, command, toolchain, and failure signature.
2. Run the oracle three times on the pinned base snapshot. All three runs must pass.
3. Run the same oracle three times on the pinned PR snapshot. All three runs must fail with the selected signature.
4. Reject the candidate if the failure depends on unavailable secrets, external services, mutable network content, privileged hardware, or an unsupported operating system.
5. Reject the candidate if the observed failure is merely a deployment authorization, CLA, AI review, title-policy, or other non-code check.

If Ipe-lang fails admission, replace it with another medium Rust repository rather than weakening the gate. If Bevy fails admission, select another large repository with at least 100 MB GitHub disk usage and a locally reproducible code/test/lint failure.

## Isolation Model

Third-party code must never execute directly on the Windows host. Every build, oracle invocation, and ReproCut attempt runs in a disposable Linux container or equivalent ephemeral VM with:

- a read-only pinned source input and a disposable writable worktree;
- no host credentials, SSH agent, cloud metadata, Docker socket, or GitHub token;
- network access only during an explicit dependency-preload phase;
- network disabled during the admission oracle and every reduction attempt;
- CPU, memory, process-count, disk, and wall-clock limits;
- no privileged mode and no host filesystem mounts beyond explicit input/output directories;
- captured stdout, stderr, exit status, tool versions, and resource-limit events.

Dependencies downloaded during preload are treated as experiment inputs and their lockfiles or resolved identities are captured in the evidence manifest.

## Experiment Procedure

For each admitted candidate:

1. Materialize immutable base and PR snapshots.
2. Preload dependencies in a disposable network-enabled builder.
3. Confirm the 3-pass base / 3-fail PR admission gate in the offline runner.
4. Define an oracle that requires both the expected non-zero outcome and a stable failure signature. Exit code alone is insufficient.
5. Run ReproCut from a fresh PR snapshot for every candidate evaluation.
6. Enforce per-attempt and whole-experiment time budgets.
7. Re-run the final minimized reproducer three times in fresh offline snapshots.
8. Check the minimized reproducer once against the base snapshot when meaningful, to guard against reducing to an unrelated baseline failure.
9. Produce a self-contained reproduction script and machine-readable evidence bundle.

The experiments run sequentially from small to large. A smaller candidate must pass the release gate before resources are spent on the next tier.

## Required Evidence

Each candidate's output directory must contain:

- `manifest.json`: repository, SHAs, CI URLs, command, environment, dependency identities, and limits;
- `admission.json`: three base results and three PR results;
- `attempts.jsonl`: bounded ReproCut attempt ledger;
- `result.json`: original and reduced measurements plus oracle signature;
- `reproduce.sh`: offline reproduction command;
- final reduced files or patch;
- captured final-verification logs for all three runs;
- an integrity digest for every evidence file.

No upstream comment, pull request, issue, commit, or artifact upload is part of this validation. Public case-study use requires a separate review of licenses, attribution, and wording.

## Release Gate

The external validation gate passes only if:

- all three candidates pass admission;
- all three produce a strictly smaller reproducer according to a declared metric;
- all three minimized results preserve the intended failure signature in three out of three fresh offline runs;
- evidence bundles pass integrity verification;
- no isolation violation or unbounded resource event occurs.

If a candidate cannot be reduced despite being admitted, the result is recorded honestly and v0.1.0 publication pauses for review. Candidates may be replaced only for admission failures, not to hide a valid ReproCut failure.

## Distribution Decision

The v0.1.0 release targets GitHub Releases, crates.io, and PyPI. A separate `reprocut-action` repository and GitHub Actions Marketplace listing are the preferred next distribution layer because they match the current CLI execution model. A hosted GitHub App is deferred until there is a hardened multi-tenant execution service, privacy/support surfaces, webhook handling, and enough real usage to justify operational and Marketplace requirements.

## Non-Goals

- Fixing or contributing to the selected upstream pull requests.
- Claiming that ReproCut supports every CI provider or failure class.
- Publishing a paid GitHub App as part of v0.1.0.
- Running arbitrary third-party code on the developer workstation.
