# External CI Validation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Run ReproCut against pinned OpenRuyi, Ipe-lang, and Bevy failures in isolated offline containers and publish integrity-checked 3/3 evidence before v0.1.0.

**Architecture:** A trusted Python driver validates an immutable case catalog, fetches pinned Git snapshots, builds a dependency-preloaded Docker image, and starts an unprivileged networkless container for admission and reduction. GitHub-hosted Ubuntu runners provide Docker because the local workstation has neither Docker nor Podman; jobs execute serially from small to large and upload sanitized evidence only after the untrusted container exits.

**Tech Stack:** Python 3.11 standard library, Docker on GitHub-hosted Ubuntu, Bash, GitHub Actions, Rust/Cargo, pre-commit, ReproCut CLI.

## Global Constraints

- The exact PR heads are OpenRuyi `1a0e915e4e0daa89cce0b97dc488801fe4225a0e`, Ipe-lang `072f647ca425694728de3aa6f508f1c3820681f1`, and Bevy `762326968f6fac9e69c81a831ab91ab29afb9933`.
- A case is admitted only after three passing base observations and three PR observations with the required failure signature.
- Third-party code never executes on the Windows host.
- Candidate code has no GitHub token, host credentials, SSH agent, cloud metadata, Docker socket, privileged mode, or network during admission and reduction.
- Each candidate container uses `--network none`, `--cap-drop ALL`, `--security-opt no-new-privileges`, `--pids-limit 1024`, `--cpus 2`, and a fixed memory/time limit.
- Exit code alone is never a failure oracle; every case has required and rejected regexes.
- Final artifacts require three fresh offline verification runs, a manifest, admission results, attempt ledger, reproduction script, captured logs, and SHA-256 integrity inventory.
- No upstream repository is modified and no upstream issue, comment, commit, or pull request is created.
- A valid ReproCut failure pauses the release; a candidate may be replaced only when it fails admission.

---

## File Map

- `scripts/external_validation/cases.json`: immutable candidate catalog, commands, signatures, limits, and CI provenance.
- `scripts/external_validation/validate_cases.py`: strict catalog parser and semantic validator.
- `scripts/external_validation/run_case.py`: trusted host orchestrator for fetch, image build, container execution, evidence sanitation, and hashing.
- `scripts/external_validation/container_entrypoint.sh`: untrusted-container admission, ReproCut invocation, three-run final verification, and raw evidence emission.
- `scripts/external_validation/Dockerfile`: dependency-preloaded execution image with a non-root runtime user.
- `scripts/external_validation/tests/test_validate_cases.py`: catalog contract tests.
- `scripts/external_validation/tests/test_run_case.py`: Docker-boundary, admission, sanitation, and integrity unit tests with a fake command runner.
- `.github/workflows/external-validation.yml`: manual serial OpenRuyi → Ipe-lang → Bevy execution and artifact retention.
- `docs/verification/2026-08-26-external-ci-validation.md`: final evidence summary populated only from downloaded, verified artifacts.

### Task 1: Immutable Case Catalog

**Files:**
- Create: `scripts/external_validation/cases.json`
- Create: `scripts/external_validation/validate_cases.py`
- Create: `scripts/external_validation/tests/test_validate_cases.py`

**Interfaces:**
- Produces: `CaseSpec` with `case_id`, `repository`, `base_ref`, `head_sha`, `ci_url`, `oracle_argv`, `required_regex`, `rejected_regex`, `memory`, and `timeout_minutes`.
- Produces: `load_cases(path: Path) -> tuple[CaseSpec, ...]` and `select_case(cases, case_id) -> CaseSpec`.

- [ ] **Step 1: Write catalog contract tests**

```python
def test_catalog_contains_exact_pinned_cases():
    cases = load_cases(CATALOG)
    assert [(case.case_id, case.head_sha) for case in cases] == [
        ("openruyi", "1a0e915e4e0daa89cce0b97dc488801fe4225a0e"),
        ("ipe", "072f647ca425694728de3aa6f508f1c3820681f1"),
        ("bevy", "762326968f6fac9e69c81a831ab91ab29afb9933"),
    ]

def test_rejects_shell_commands_and_unpinned_heads(tmp_path):
    path = write_catalog(tmp_path, head_sha="main", oracle_argv="cargo test && curl x")
    with pytest.raises(CatalogError):
        load_cases(path)
```

- [ ] **Step 2: Run tests and confirm the missing-module failure**

Run: `python -m unittest discover -s scripts/external_validation/tests -p 'test_validate_cases.py' -v`

Expected: FAIL because `validate_cases` and the catalog do not exist.

- [ ] **Step 3: Implement the strict dataclass parser**

The parser must require 40-character lowercase hexadecimal SHAs, HTTPS GitHub repository URLs, argv arrays rather than shell strings, at least one required regex, unique case IDs, positive limits, and catalog order `openruyi`, `ipe`, `bevy`. Unknown JSON keys are errors.

- [ ] **Step 4: Add the three exact catalog entries**

Use these oracle argv values:

```json
{
  "openruyi": ["pre-commit", "run", "--all-files", "--show-diff-on-failure"],
  "ipe": ["bash", "tools/scripts/regen-sky-examples.sh", "--check"],
  "bevy": ["cargo", "run", "-p", "ci", "--", "lints"]
}
```

OpenRuyi requires `files were modified by this hook`; Ipe requires `regen --check: committed ports are stale vs rename-map/ipe-edits`; Bevy requires `unsafe block missing a safety comment` and `undocumented_unsafe_blocks`. Ipe rejects `original/ missing`, `cannot read manifest`, and `transform/edits failed`; all cases reject dependency/network errors and missing-command errors.

- [ ] **Step 5: Run the catalog tests**

Run: `python -m unittest discover -s scripts/external_validation/tests -p 'test_validate_cases.py' -v`

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add scripts/external_validation/cases.json scripts/external_validation/validate_cases.py scripts/external_validation/tests/test_validate_cases.py
git commit -m "test: define external CI validation cases"
```

### Task 2: Trusted Docker Boundary and Evidence Sanitizer

**Files:**
- Create: `scripts/external_validation/run_case.py`
- Create: `scripts/external_validation/tests/test_run_case.py`

**Interfaces:**
- Consumes: `CaseSpec` and `select_case` from Task 1.
- Produces: `docker_create_argv(case: CaseSpec, image: str) -> list[str]`.
- Produces: `sanitize_evidence(source: Path, destination: Path) -> dict[str, str]`, rejecting symlinks, devices, FIFOs, sockets, absolute paths, traversal, and files above the declared size ceiling.
- Produces: CLI `python scripts/external_validation/run_case.py --case CASE --output PATH`.

- [ ] **Step 1: Write failing Docker-boundary tests**

```python
def test_container_has_hard_isolation_flags():
    argv = docker_create_argv(BEVY, "reprocut-validation:bevy")
    joined = " ".join(argv)
    for required in (
        "--network none", "--cap-drop ALL",
        "--security-opt no-new-privileges", "--pids-limit 1024",
        "--cpus 2", "--memory 7g",
    ):
        assert required in joined
    assert "/var/run/docker.sock" not in joined
    assert "GITHUB_TOKEN" not in joined

def test_sanitizer_rejects_symlinks(tmp_path):
    source = tmp_path / "raw"
    source.mkdir()
    (source / "escape").symlink_to(tmp_path / "outside")
    with pytest.raises(EvidenceError, match="symlink"):
        sanitize_evidence(source, tmp_path / "clean")
```

- [ ] **Step 2: Run tests and confirm failure**

Run: `python -m unittest discover -s scripts/external_validation/tests -p 'test_run_case.py' -v`

Expected: FAIL because the runner does not exist.

- [ ] **Step 3: Implement command execution without a shell**

Use `subprocess.run(argv, shell=False, check=False, timeout=...)`. Fetch into a temporary directory with `git init`, `git remote add origin`, and depth-1 fetches of the head SHA and resolved base SHA. Record the resolved base SHA before image construction. Never evaluate catalog values as shell text.

- [ ] **Step 4: Implement isolated container creation**

Create, start, and wait for the container separately; use `docker cp CONTAINER:/evidence/. RAW_DIR` only after it stops. Always remove the stopped container. Do not bind-mount host output into the candidate container.

- [ ] **Step 5: Implement evidence sanitation and SHA-256 inventory**

Walk with `followlinks=False`, reject every non-regular entry, copy bytes into a newly created clean directory, calculate lowercase SHA-256 values, and write `integrity.json` last. Reject duplicate normalized relative paths and more than 1 GiB total evidence.

- [ ] **Step 6: Run tests**

Run: `python -m unittest discover -s scripts/external_validation/tests -v`

Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add scripts/external_validation/run_case.py scripts/external_validation/tests/test_run_case.py
git commit -m "feat: add isolated external validation runner"
```

### Task 3: Preloaded Image and Container Experiment

**Files:**
- Create: `scripts/external_validation/Dockerfile`
- Create: `scripts/external_validation/container_entrypoint.sh`
- Modify: `scripts/external_validation/run_case.py`
- Modify: `scripts/external_validation/tests/test_run_case.py`

**Interfaces:**
- Consumes: `/inputs/base`, `/inputs/head`, `/opt/reprocut/reprocut`, and `/case.json` baked into the image.
- Produces: `/evidence/manifest.json`, `/evidence/admission.json`, `/evidence/reprocut/`, `/evidence/final-verification/*.log`, and `/evidence/result.json`.

- [ ] **Step 1: Add a fake-oracle integration test**

Construct a tiny base/head fixture where base exits zero, head prints `stable failure: 17` and exits 17, and deleting `noise.txt` preserves the failure. Build the image, run it networkless, and assert base observations `[0, 0, 0]`, head observations `[17, 17, 17]`, three final preserved observations, and absence of `noise.txt` in the artifact.

- [ ] **Step 2: Confirm the integration test fails before the image exists**

Run: `python -m unittest scripts.external_validation.tests.test_run_case.ContainerIntegrationTests -v`

Expected: FAIL because `Dockerfile` and `container_entrypoint.sh` do not exist. Skip only when Docker is unavailable, which is expected locally but not on the GitHub runner.

- [ ] **Step 3: Implement the dependency-preload image**

Start from `ubuntu:24.04`, install pinned apt package names needed by Python, Cargo/Clippy, Bevy Linux linking, and Ipe scripts, install Rust through the repository toolchain, create UID/GID 10001, and copy both immutable snapshots. During the network-enabled build phase:

- OpenRuyi installs pre-commit and warms every configured hook.
- Ipe installs script prerequisites and performs no upstream mirror fetch.
- Bevy runs `cargo fetch --locked` and a non-fatal warm-up of its CI lint command to populate Cargo artifacts.
- ReproCut is built from the current repository commit and copied to `/opt/reprocut/reprocut`.

The final runtime stage contains no Git, GitHub CLI, curl, wget, SSH client, token, or package-manager credentials.

- [ ] **Step 4: Implement admission and reduction entrypoint**

Copy each baked snapshot to a fresh `/work` directory for every observation. Capture stdout/stderr/exit code and require all regex contracts. Invoke ReproCut with `--oracle-mode regex`, repeated `--failure-regex`, repeated `--reject-regex`, `--jobs 1`, a case-specific `--timeout-ms`, and the argv following `--`. Wrap the complete reduction in GNU `timeout --signal=TERM --kill-after=30s` using the case budget.

- [ ] **Step 5: Implement three fresh final verifications**

For each final run, copy only the reduced `project/` into a new directory, invoke the exact oracle offline, and require the declared signature. Run `reprocut verify /evidence/reprocut` before emitting success.

- [ ] **Step 6: Run unit tests on the host and integration test on GitHub**

Run locally: `python -m unittest discover -s scripts/external_validation/tests -v`

Run on Ubuntu with Docker: `python -m unittest scripts.external_validation.tests.test_run_case.ContainerIntegrationTests -v`

Expected: all unit tests pass; the Docker integration test passes on GitHub and is explicitly skipped locally.

- [ ] **Step 7: Commit**

```bash
git add scripts/external_validation/Dockerfile scripts/external_validation/container_entrypoint.sh scripts/external_validation/run_case.py scripts/external_validation/tests/test_run_case.py
git commit -m "feat: run ReproCut validation in offline containers"
```

### Task 4: Serial GitHub Actions Execution

**Files:**
- Create: `.github/workflows/external-validation.yml`

**Interfaces:**
- Consumes: the runner CLI from Task 2 and container implementation from Task 3.
- Produces: Actions artifacts `external-validation-openruyi`, `external-validation-ipe`, and `external-validation-bevy`.

- [ ] **Step 1: Add workflow structure with least privilege**

Use `workflow_dispatch`, top-level `permissions: contents: read`, `persist-credentials: false`, and three Ubuntu jobs. `ipe` declares `needs: openruyi`; `bevy` declares `needs: ipe`. Each job has an explicit timeout: 45, 90, and 180 minutes respectively.

- [ ] **Step 2: Add trusted test and case execution steps**

Each job checks out only the ReproCut repository, runs the Python unit tests, runs `validate_cases.py`, executes exactly one `run_case.py --case ...`, and then uploads the sanitized directory. The workflow passes no secrets or write token to the scripts.

- [ ] **Step 3: Validate workflow syntax and static security invariants**

Run:

```bash
python -c "import pathlib; p=pathlib.Path('.github/workflows/external-validation.yml').read_text(); assert 'permissions:\n  contents: read' in p; assert 'persist-credentials: false' in p; assert 'pull_request_target' not in p; assert 'secrets.' not in p"
```

Expected: exit 0.

- [ ] **Step 4: Commit**

```bash
git add .github/workflows/external-validation.yml
git commit -m "ci: add serial external ReproCut validation"
```

### Task 5: Execute and Audit the Three Cases

**Files:**
- Modify only if an admission defect is discovered: `scripts/external_validation/cases.json`
- Create from verified results: `docs/verification/2026-08-26-external-ci-validation.md`

**Interfaces:**
- Consumes: workflow and artifacts from Task 4.
- Produces: a public Actions run URL and an evidence-backed release decision.

- [ ] **Step 1: Push the committed validation branch and dispatch the workflow**

Run:

```bash
git push origin HEAD
gh workflow run external-validation.yml --repo emirhuseynrmx/reprocut --ref main
```

Expected: one queued workflow run on the committed validation SHA.

- [ ] **Step 2: Wait for OpenRuyi and inspect its sanitized artifact**

Require base 3/3 pass, head 3/3 expected failure, strict reduction, final 3/3, successful `reprocut verify`, and matching `integrity.json`. Stop immediately on any violation.

- [ ] **Step 3: Wait for Ipe-lang and apply the admission rule**

If the pinned base does not pass or the head does not fail three times with the committed drift signature, record it as an admission failure and stop. Select a replacement only through a new spec/catalog commit; do not alter the oracle to accept a different failure.

- [ ] **Step 4: Wait for Bevy and enforce the large-case budget**

Require the five `undocumented_unsafe_blocks` diagnostics, no dependency/network reject signature, a strict reduction, final 3/3, and no timeout/resource-limit event.

- [ ] **Step 5: Download and independently hash all artifacts**

Run `gh run download RUN_ID --repo emirhuseynrmx/reprocut --dir external-validation-output`, then independently recompute every SHA-256 entry and reject symlinks or undeclared files.

- [ ] **Step 6: Write the final verification report**

Record candidate URLs, exact SHAs, CI job URLs, base/head observations, original/reduced metrics, attempt counts, durations, artifact digests, isolation limits, and the final pass/pause decision. Do not claim success for a stopped or inconclusive case.

- [ ] **Step 7: Commit the report only if its cited evidence verifies**

```bash
git add docs/verification/2026-08-26-external-ci-validation.md
git commit -m "docs: record external CI validation evidence"
```

### Task 6: Final Release-Gate Verification

**Files:**
- Verify: all files committed by Tasks 1-5.

- [ ] **Step 1: Run the complete external-validation test suite**

Run: `python -m unittest discover -s scripts/external_validation/tests -v`

Expected: PASS, with Docker integration explicitly passing in the GitHub run.

- [ ] **Step 2: Run ReproCut's existing release audit**

Run: `python scripts/release/audit.py`

Expected: all release gates pass, including the new evidence cited in the report.

- [ ] **Step 3: Confirm a clean worktree and inspect commits**

Run: `git status --short && git log --oneline --max-count=8`

Expected: empty status and focused validation commits.

- [ ] **Step 4: Make the publication decision**

Publish v0.1.0 only when all three evidence bundles meet the design gate. Otherwise report the exact failed gate and keep publication paused.
