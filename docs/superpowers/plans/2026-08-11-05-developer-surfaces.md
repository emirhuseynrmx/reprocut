# Python, Editor, and Gallery Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Expose one versioned engine protocol through typed Python, a thin VS Code/Cursor extension, and an explicit static-gallery submission flow.

**Architecture:** The Rust engine emits versioned JSON events and accepts one request schema. Python and the editor are adapters over that contract, not independent reducers. Gallery preparation is local and static; publication is review-driven through GitHub.

**Tech Stack:** Rust 1.85, PyO3 abi3-py39, Python typing/pytest, TypeScript, VS Code Extension API, static HTML/JSON, GitHub Pages.

## Global Constraints

- Keep package version `0.1.0` across Cargo, PyPI metadata, protocol, and extension prerelease metadata.
- Native and CLI paths share one engine; no behaviorally different Python/TypeScript reducer exists.
- The extension never silently downloads or executes a binary.
- Gallery preparation never uploads data and gallery CI never executes unreviewed submissions.
- Registry publication remains the user's final action.

---

### Task 1: Versioned JSON protocol

**Files:**
- Create: `crates/reprocut-core/src/protocol.rs`
- Create: `crates/reprocut-core/tests/protocol_contract.rs`
- Modify: `crates/reprocut-cli/src/main.rs`
- Create: `tests/golden/protocol-events.jsonl`

**Interfaces:**
- Produces: `PROTOCOL_VERSION`, `ReductionRequestV1`, `ProgressEventV1`, `OutcomeV1`, `ErrorV1`.
- CLI: `reprocut protocol run --request FILE --events jsonl`.

- [ ] **Step 1: Write failing schema/golden/unknown-version tests**
- [ ] **Step 2: Verify RED**
- [ ] **Step 3: Implement tagged, additive V1 request and event enums**
- [ ] **Step 4: Stream progress on stdout while reserving stderr for protocol-launch failures**
- [ ] **Step 5: Round-trip golden JSON and commit**

```bash
git add crates/reprocut-core crates/reprocut-cli tests/golden/protocol-events.jsonl
git commit -m "feat(protocol): expose versioned reduction events"
```

### Task 2: Full typed Python API and console entry point

**Files:**
- Modify: `crates/reprocut-python/Cargo.toml`
- Modify: `crates/reprocut-python/src/lib.rs`
- Modify: `python/reprocut/__init__.py`
- Create: `python/reprocut/__main__.py`
- Create: `python/reprocut/cli.py`
- Modify: `python/reprocut/_native.pyi`
- Create: `python/tests/test_reduce_api.py`
- Modify: `pyproject.toml`

**Interfaces:**
- Produces: `reprocut.reduce(request, progress=None)`, typed request/outcome/attempt classes, `python -m reprocut`, and console script `reprocut`.

- [ ] **Step 1: Write failing Python API, callback, typing, and console tests**
- [ ] **Step 2: Verify reference checkout fails clearly because full engine requires native code**
- [ ] **Step 3: Bind request/outcome/progress through PyO3 without holding the GIL during engine work**
- [ ] **Step 4: Implement Python wrappers and exact `.pyi` types**
- [ ] **Step 5: Build native wheels in CI, require native backend, and commit**

```bash
git add crates/reprocut-python python pyproject.toml
git commit -m "feat(python): expose the complete reduction engine"
```

### Task 3: VS Code and Cursor extension

**Files:**
- Create: `editors/vscode/package.json`
- Create: `editors/vscode/tsconfig.json`
- Create: `editors/vscode/src/extension.ts`
- Create: `editors/vscode/src/protocol.ts`
- Create: `editors/vscode/src/runner.ts`
- Create: `editors/vscode/test/extension.test.ts`
- Create: `editors/vscode/README.md`

**Interfaces:**
- Commands: `reprocut.minimize`, `reprocut.resume`, `reprocut.openReport`, `reprocut.openIssue`.
- Consumes JSON protocol V1 and a configured `reprocut.path`.

- [ ] **Step 1: Write failing protocol-version, cancellation, command, and no-auto-download tests**
- [ ] **Step 2: Run TypeScript tests and verify RED**
- [ ] **Step 3: Implement executable discovery and explicit install guidance**
- [ ] **Step 4: Stream progress, forward cancellation, and open generated artifacts**
- [ ] **Step 5: Run VS Code extension host tests and Cursor-compatible packaging; commit**

```bash
git add editors/vscode
git commit -m "feat(editor): minimize failures from VS Code and Cursor"
```

### Task 4: Local gallery submission preparation

**Files:**
- Create: `crates/reprocut-report/src/gallery.rs`
- Create: `crates/reprocut-report/tests/gallery_contract.rs`
- Modify: `crates/reprocut-cli/src/main.rs`
- Create: `gallery/schema/submission.schema.json`
- Create: `gallery/examples/checkout-typeerror/`

**Interfaces:**
- CLI: `reprocut gallery prepare --from OUTPUT --destination DIR`.
- Produces a redacted metadata file, selected assets, license declaration, and PR instructions.

- [ ] **Step 1: Write failing opt-in, redaction, size, and path contracts**
- [ ] **Step 2: Verify RED**
- [ ] **Step 3: Generate submission material without network access or implicit source inclusion**
- [ ] **Step 4: Validate against the committed JSON schema and secret patterns**
- [ ] **Step 5: Commit**

```bash
git add crates/reprocut-report crates/reprocut-cli gallery
git commit -m "feat(gallery): prepare explicit public repro submissions"
```

### Task 5: Static gallery and review workflows

**Files:**
- Create: `gallery/site/index.html`
- Create: `gallery/site/styles.css`
- Create: `gallery/scripts/build.mjs`
- Create: `gallery/scripts/validate.mjs`
- Create: `gallery/tests/gallery.test.mjs`
- Create: `.github/workflows/gallery-validate.yml`
- Create: `.github/workflows/gallery-pages.yml`
- Create: `.github/ISSUE_TEMPLATE/gallery.yml`

**Interfaces:**
- Produces a deterministic static site and reviewed “Repro of the Week” metadata.

- [ ] **Step 1: Write failing schema, secret, no-code-execution, and deterministic-build tests**
- [ ] **Step 2: Verify RED**
- [ ] **Step 3: Build accessible static cards from reviewed submissions**
- [ ] **Step 4: Configure PR validation with read-only/no-secret permissions and no submitted-code execution**
- [ ] **Step 5: Configure Pages build from the protected main branch and commit**

```bash
git add gallery .github
git commit -m "feat(gallery): publish reviewed reproductions statically"
```
