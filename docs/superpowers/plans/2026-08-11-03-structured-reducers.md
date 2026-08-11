# Ecosystem and Syntax Reducers Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Deliver one-command Cargo/pytest/npm discovery, honest manifest reduction, and grammar-valid Rust/Python/JavaScript/TypeScript source reduction.

**Architecture:** Adapters discover commands, exclusions, manifests, and candidate preparation. Syntax grammars emit canonical byte-range transformations. The engine runs structured passes through the same oracle/state/scheduler contracts as file reduction.

**Tech Stack:** Rust 1.85, toml_edit, serde_json, Tree-sitter and bundled language grammars, golden tests.

## Global Constraints

- Keep version `0.1.0` and forbid handwritten unsafe.
- Never infer dependency removal from an already-populated host environment.
- Network and lifecycle scripts are disabled unless explicitly authorized.
- Parse validity avoids wasted executions but never replaces the failure oracle.
- Local Rust execution remains blocked; remote compilation and CI are recorded separately.
- Registry publication remains the user's final action.

---

### Task 1: Adapter discovery and inventory policies

**Files:**
- Create: `crates/reprocut-adapters/Cargo.toml`
- Create: `crates/reprocut-adapters/src/lib.rs`
- Create: `crates/reprocut-adapters/src/discovery.rs`
- Create: `crates/reprocut-adapters/tests/discovery_contract.rs`
- Modify: `Cargo.toml`
- Modify: `crates/reprocut-workspace/src/lib.rs`

**Interfaces:**
- Produces: `Ecosystem::{Cargo, Python, Npm, None}`, `Adapter::detect`, `Adapter::command`, `InventoryPolicy`.

- [ ] **Step 1: Write failing unique/ambiguous detection tests**
- [ ] **Step 2: Verify RED before the adapter crate exists**
- [ ] **Step 3: Implement deterministic marker scoring and ambiguity refusal**
- [ ] **Step 4: Add adapter-specific generated/cache directory exclusions**
- [ ] **Step 5: Run Windows/Unix path fixtures and commit**

```bash
git add Cargo.toml Cargo.lock crates/reprocut-adapters crates/reprocut-workspace
git commit -m "feat(adapters): detect project ecosystems safely"
```

### Task 2: Cargo manifest reducer

**Files:**
- Create: `crates/reprocut-adapters/src/cargo.rs`
- Create: `crates/reprocut-adapters/tests/cargo_manifest_contract.rs`
- Create: `tests/fixtures/cargo-manifest/`

**Interfaces:**
- Produces `ManifestEntry` transformations for dependencies, features, members, and explicit targets.
- Produces preparation commands `cargo generate-lockfile --offline` and `cargo metadata --locked --offline --format-version 1`.

- [ ] **Step 1: Add golden Cargo.toml/Cargo.lock fixtures and failing entry enumeration tests**
- [ ] **Step 2: Verify RED because manifest transforms are unavailable**
- [ ] **Step 3: Parse with `toml_edit`, preserve formatting, and emit stable manifest keys**
- [ ] **Step 4: Apply removal in a candidate and validate its regenerated offline lock**
- [ ] **Step 5: Prove preparation failure is `Rejected`, never `Preserved`; commit**

```bash
git add crates/reprocut-adapters tests/fixtures/cargo-manifest
git commit -m "feat(adapters): reduce Cargo manifests offline"
```

### Task 3: Python and npm manifest reducers

**Files:**
- Create: `crates/reprocut-adapters/src/python.rs`
- Create: `crates/reprocut-adapters/src/npm.rs`
- Create: `crates/reprocut-adapters/tests/python_manifest_contract.rs`
- Create: `crates/reprocut-adapters/tests/npm_manifest_contract.rs`
- Create: `tests/fixtures/python-manifest/`
- Create: `tests/fixtures/npm-manifest/`

**Interfaces:**
- Produces pyproject dependency/group/script transformations with an explicit preparation capability.
- Produces package.json dependency/script/workspace transformations and npm offline preparation.

- [ ] **Step 1: Write failing golden tests for pyproject and package.json entry enumeration/removal**
- [ ] **Step 2: Verify RED**
- [ ] **Step 3: Implement pyproject transforms and refuse dependency pruning without isolated preparation**
- [ ] **Step 4: Implement package.json transforms, exact Jest `--runInBand` detection, and lifecycle-script opt-in**
- [ ] **Step 5: Exercise offline available/unavailable paths and commit**

```bash
git add crates/reprocut-adapters tests/fixtures/python-manifest tests/fixtures/npm-manifest
git commit -m "feat(adapters): reduce Python and npm manifests honestly"
```

### Task 4: Tree-sitter transformation engine

**Files:**
- Create: `crates/reprocut-syntax/Cargo.toml`
- Create: `crates/reprocut-syntax/src/lib.rs`
- Create: `crates/reprocut-syntax/src/languages.rs`
- Create: `crates/reprocut-syntax/src/transforms.rs`
- Create: `crates/reprocut-syntax/tests/syntax_contract.rs`
- Create: `crates/reprocut-syntax/tests/corpus/`
- Modify: `Cargo.toml`

**Interfaces:**
- Produces: `SyntaxLanguage`, `parse_valid`, `deletion_transforms`, `hoist_transforms`.
- Consumes canonical `Operation::{DeleteSyntaxNode, HoistSyntaxChild}`.

- [ ] **Step 1: Add one failing corpus test per bundled language and node family**

```rust
#[test]
fn python_function_deletion_reparses_without_error_or_missing_nodes() {
    let transforms = deletion_transforms(PYTHON, b"def keep(): pass\ndef drop(): pass\n").unwrap();
    assert!(transforms.iter().any(|item| item.kind() == "function_definition"));
}
```

- [ ] **Step 2: Verify RED before grammar dependencies exist**
- [ ] **Step 3: Add exact compatible Tree-sitter grammar versions and language mapping**
- [ ] **Step 4: Emit allowlisted named-node deletions and grammar-safe hoists**
- [ ] **Step 5: Reparse candidates and reject ERROR/MISSING/UTF-8/overlap cases before command execution**
- [ ] **Step 6: Run corpus, golden, sanitizer CI, and commit**

```bash
git add Cargo.toml Cargo.lock crates/reprocut-syntax
git commit -m "feat(syntax): reduce four languages through concrete syntax trees"
```

### Task 5: Structured fixpoint pipeline and real fixtures

**Files:**
- Modify: `crates/reprocut-engine/src/lib.rs`
- Create: `crates/reprocut-engine/src/pipeline.rs`
- Create: `crates/reprocut-engine/tests/structured_pipeline_contract.rs`
- Create: `tests/fixtures/rust-project/`
- Create: `tests/fixtures/python-project/`
- Create: `tests/fixtures/typescript-project/`
- Modify: `crates/reprocut-cli/src/main.rs`

**Interfaces:**
- Produces phase order and return-to-earliest-affected fixpoint behavior.
- Produces CLI `reprocut minimize --ecosystem ... --prepare ...`.

- [ ] **Step 1: Write end-to-end failing fixtures requiring file, manifest, and syntax cuts**
- [ ] **Step 2: Verify current file-only engine cannot meet expected retained bytes/nodes**
- [ ] **Step 3: Integrate directory/file/manifest/delete/hoist phases through state and scheduler**
- [ ] **Step 4: Re-run earlier phases when later transforms expose new cuts**
- [ ] **Step 5: Verify every final project independently reproduces its failure and source digests stay unchanged**
- [ ] **Step 6: Commit**

```bash
git add crates/reprocut-engine crates/reprocut-cli tests/fixtures
git commit -m "feat(engine): run structured reducers to a fixpoint"
```
