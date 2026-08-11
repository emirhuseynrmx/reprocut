# Evidence, Issue Export, and OCI Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Turn the reduction journal into a trustworthy visual record, paste-ready GitHub issue, measured before/after artifact, and real OCI archive.

**Architecture:** One immutable evidence model feeds JSON, JSONL, HTML, Markdown, scripts, and OCI metadata so surfaces cannot disagree. Large attempts stream from state storage rather than accumulating in the renderer.

**Tech Stack:** Rust 1.85, serde/serde_json, self-contained HTML/CSS/JS, OCI image spec, Playwright, golden tests.

## Global Constraints

- Version stays `0.1.0`; renderer input has no filesystem authority.
- Escape every user-controlled string and make no external browser requests.
- “Why retained” reports observed final-context experiments, never invented causality.
- Do not call a Dockerfile or build context an OCI image.
- Local Rust toolchain limitations remain explicit; browser/Python/Node verification runs locally.
- Registry publication remains the user's final action.

---

### Task 1: Unified evidence and measurement model

**Files:**
- Create: `crates/reprocut-report/src/evidence.rs`
- Modify: `crates/reprocut-report/src/lib.rs`
- Modify: `crates/reprocut-engine/src/lib.rs`
- Create: `crates/reprocut-report/tests/evidence_contract.rs`

**Interfaces:**
- Produces: `ReductionEvidence`, `AttemptSummary`, `MeasurementSet`, `RetentionEvidence`, `FingerprintBadge`.
- Consumes state-store attempt iterator and engine outcome.

- [ ] **Step 1: Write failing consistency tests across counts, bytes, lines, nodes, durations, and fingerprints**
- [ ] **Step 2: Verify RED because the report model lacks phase/attempt evidence**
- [ ] **Step 3: Implement checked measurement aggregation and final-context retention records**
- [ ] **Step 4: Stream `attempts.jsonl` and version `reduction.json`**
- [ ] **Step 5: Commit**

```bash
git add crates/reprocut-report crates/reprocut-engine
git commit -m "feat(report): model complete reduction evidence"
```

### Task 2: Evidence-rich self-contained report

**Files:**
- Modify: `crates/reprocut-report/assets/report.html`
- Modify: `crates/reprocut-report/assets/report.css`
- Modify: `crates/reprocut-report/assets/report.js`
- Modify: `crates/reprocut-report/tests/report_golden.rs`
- Modify: `tests/golden/reduction-report.html`
- Modify: `scripts/verification/report_browser.cjs`

**Interfaces:**
- Produces phase timeline, metrics, same-failure badge, channel evidence, retained reasons, and copy/download controls.

- [ ] **Step 1: Extend the golden contract with every required evidence section**
- [ ] **Step 2: Verify byte-golden RED**
- [ ] **Step 3: Render accessible semantic sections with bounded summaries**
- [ ] **Step 4: Implement clipboard failure fallback and issue download without network calls**
- [ ] **Step 5: Run desktop/mobile/reduced-motion/keyboard/no-network browser contracts and commit**

```bash
git add crates/reprocut-report tests/golden scripts/verification/report_browser.cjs
git commit -m "feat(report): explain every accepted and retained transformation"
```

### Task 3: GitHub issue Markdown and repro scripts

**Files:**
- Create: `crates/reprocut-report/src/issue.rs`
- Create: `crates/reprocut-report/tests/issue_golden.rs`
- Create: `tests/golden/issue.md`
- Modify: `crates/reprocut-cli/src/main.rs`

**Interfaces:**
- Produces: `render_issue(&ReductionEvidence) -> String` and aligned `reproduce.sh`/`reproduce.ps1`.

- [ ] **Step 1: Write a failing Markdown golden with title, hash, tree, metrics, command, and limits**
- [ ] **Step 2: Verify RED**
- [ ] **Step 3: Render shell-safe reproduction scripts and escaped Markdown from the shared model**
- [ ] **Step 4: Assert report/JSON/issue/scripts use the same command and fingerprint**
- [ ] **Step 5: Commit**

```bash
git add crates/reprocut-report crates/reprocut-cli tests/golden
git commit -m "feat(report): export paste-ready GitHub reproductions"
```

### Task 4: OCI build context and archive export

**Files:**
- Create: `crates/reprocut-oci/Cargo.toml`
- Create: `crates/reprocut-oci/src/lib.rs`
- Create: `crates/reprocut-oci/src/builder.rs`
- Create: `crates/reprocut-oci/tests/oci_contract.rs`
- Modify: `Cargo.toml`
- Modify: `crates/reprocut-cli/src/main.rs`

**Interfaces:**
- Produces: `OciRequest`, `Builder::{DockerBuildx, Podman, BuildKit}`, `prepare_context`, `export_archive`.
- Produces CLI: `reprocut export oci --from ... --output ...`.

- [ ] **Step 1: Write failing builder-detection, context-minimality, label, and unavailable-builder tests**
- [ ] **Step 2: Verify RED before the OCI crate exists**
- [ ] **Step 3: Generate ecosystem entrypoint/context with no source/state/credentials**
- [ ] **Step 4: Invoke a detected builder without a shell and require actual OCI archive output**
- [ ] **Step 5: Inspect index/manifest/layers, run the image twice, and compare normalized digests**
- [ ] **Step 6: Commit**

```bash
git add Cargo.toml Cargo.lock crates/reprocut-oci crates/reprocut-cli
git commit -m "feat(oci): export runnable minimal reproductions"
```

### Task 5: End-to-end publication artifact

**Files:**
- Modify: `crates/reprocut-cli/tests/cli_contract.rs`
- Modify: `python/tests/test_demo_assets.py`
- Modify: `scripts/build_demo.py`
- Modify: `scripts/capture_demo.py`
- Modify: `README.md`

**Interfaces:**
- Verifies the complete artifact tree and measured demo.

- [ ] **Step 1: Make the artifact contract fail for missing attempts/issue/metrics/badge**
- [ ] **Step 2: Publish every file through one no-clobber staging transaction**
- [ ] **Step 3: Regenerate the real demo and GIF from measured output**
- [ ] **Step 4: Run Python, browser, GIF, report golden, and CLI contracts**
- [ ] **Step 5: Commit**

```bash
git add crates/reprocut-cli python scripts demo assets README.md
git commit -m "docs: demonstrate complete ReproCut evidence"
```
