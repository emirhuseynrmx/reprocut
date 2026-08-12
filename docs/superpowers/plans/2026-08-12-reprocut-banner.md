# ReproCut Forensic Banner Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the current static banner with a self-contained forensic reduction record that matches the demo GIF and binds every numeric claim to checked-in evidence.

**Architecture:** Treat the SVG as a versioned evidence surface rather than a decorative image. A Python contract test parses both the SVG and `demo/result/reduction.json`, verifies accessibility/security constraints, and counts semantic file-state groups before the new asset is accepted. The final asset uses only SVG primitives and portable font stacks, then receives full-size and reduced-size visual inspection.

**Tech Stack:** SVG primitives, Python 3.9+, `xml.etree.ElementTree`, pytest, Codex image renderer

## Global Constraints

- Keep `viewBox="0 0 1600 600"` and a single static SVG file.
- Make no network requests and use no scripts, raster images, filters, animation, external fonts, gradients, or glows.
- Use paper `#FBF9F3`, grid `#D3D8DC`, ink `#171A1D`, cobalt `#3157D5`, signal red `#BF4E3A`, and muted text `#66717D`.
- Encode exactly 18 file cards: 15 rejected and three retained.
- Copy must agree with checked-in evidence: 18 source files, three retained files, 24 candidates, strict 3/3 verification, the retained paths, and the current fingerprint prefix.
- Preserve an accessible `<title>` and `<desc>`.
- Do not change the GIF, README copy, evidence, CLI, or release workflows.

---

### Task 1: Evidence-bound SVG contract

**Files:**
- Modify: `python/tests/test_demo_assets.py`
- Read: `demo/result/reduction.json`
- Test: `python/tests/test_demo_assets.py`

**Interfaces:**
- Consumes: `assets/reprocut-banner.svg` and schema-2 `demo/result/reduction.json`.
- Produces: `test_banner_is_static_accessible_and_evidence_bound()`, the acceptance contract for Task 2.

- [ ] **Step 1: Write the failing test**

Add XML parsing imports and a test that counts semantic file states, rejects active/external content, and compares banner text with evidence:

```python
import xml.etree.ElementTree as ET


def test_banner_is_static_accessible_and_evidence_bound() -> None:
    banner = ROOT / "assets" / "reprocut-banner.svg"
    evidence = json.loads((ROOT / "demo" / "result" / "reduction.json").read_text())
    source = banner.read_text(encoding="utf-8")
    root = ET.fromstring(source)
    namespace = {"svg": "http://www.w3.org/2000/svg"}
    text = " ".join("".join(root.itertext()).split())
    states = [node.attrib["data-file-state"] for node in root.iter() if "data-file-state" in node.attrib]

    assert root.attrib["viewBox"] == "0 0 1600 600"
    assert root.find("svg:title", namespace) is not None
    assert root.find("svg:desc", namespace) is not None
    assert len(states) == evidence["measurements"]["original"]["files"] == 18
    assert states.count("retained") == evidence["measurements"]["retained"]["files"] == 3
    assert states.count("rejected") == 15
    assert len([node for node in root.iter() if node.attrib.get("data-role") == "cut-trace"]) == 1
    assert str(len(evidence["attempts"])) in text
    assert "STRICT 3 / 3" in text
    assert evidence["failure"]["fingerprint_sha256"][:16] in text
    assert all(path in text for path in ("bug.py", "checkout.py", "fixtures/order.json"))
    assert "<script" not in source.lower()
    assert "<filter" not in source.lower()
    assert "<animate" not in source.lower()
    assert "http://" not in source.replace("http://www.w3.org/2000/svg", "")
    assert "https://" not in source
    assert "href=" not in source.lower()
```

- [ ] **Step 2: Run the focused test and verify RED**

Run `$python -m pytest python/tests/test_demo_assets.py::test_banner_is_static_accessible_and_evidence_bound -q` with `PYTHONPATH=.test-deps;python`.

Expected: FAIL because the current banner has no 18 semantic `data-file-state` groups and no `data-role="cut-trace"`.

- [ ] **Step 3: Commit the verified failing contract**

```powershell
git add python/tests/test_demo_assets.py
git commit -m "test(brand): bind banner to reduction evidence"
```

### Task 2: Forensic reduction SVG

**Files:**
- Modify: `assets/reprocut-banner.svg`
- Test: `python/tests/test_demo_assets.py`

**Interfaces:**
- Consumes: the semantic contract from Task 1 and visual tokens from the approved spec.
- Produces: one self-contained `1600 × 600` SVG with 18 `data-file-state` groups and one `data-role="cut-trace"` path.

- [ ] **Step 1: Replace the SVG with the minimal design that satisfies the contract**

Build the asset with these named layers in paint order:

```xml
<svg viewBox="0 0 1600 600" role="img" aria-labelledby="title desc">
  <title id="title">ReproCut — same failure, less project</title>
  <desc id="desc">A verified reduction record: 18 files reduced to 3 while the same failure remains.</desc>
  <defs><!-- 32 px technical grid pattern only --></defs>
  <rect data-layer="paper"/>
  <g data-layer="identity"><!-- wordmark, command, 18 → 03 --></g>
  <g data-layer="file-matrix">
    <g data-file-state="retained"><!-- file card --></g>
    <!-- exactly two more retained and fifteen rejected groups -->
  </g>
  <path data-role="cut-trace"/>
  <g data-layer="evidence"><!-- strict 3/3, paths, fingerprint --></g>
</svg>
```

Use the six approved colors exactly. Keep the wordmark dominant, the file matrix secondary, and the bottom evidence rail quiet. The red trace must pass only through rejected cards and must not touch the three cobalt retained cards.

- [ ] **Step 2: Run the focused test and verify GREEN**

Run the focused pytest command from Task 1. Expected: `1 passed`.

- [ ] **Step 3: Run the complete asset test module**

Run `$python -m pytest python/tests/test_demo_assets.py -q` with `PYTHONPATH=.test-deps;python`.

Expected: every demo/GIF/banner test passes; the native backend test is not part of this module.

- [ ] **Step 4: Commit the evidence-bound SVG**

```powershell
git add assets/reprocut-banner.svg
git commit -m "feat(brand): redesign forensic reduction banner"
```

### Task 3: Visual QA and repository verification

**Files:**
- Inspect: `assets/reprocut-banner.svg`
- Inspect: `assets/reprocut-demo.gif`
- Test: `python/tests/test_demo_assets.py`

**Interfaces:**
- Consumes: the contract-green SVG from Task 2.
- Produces: visual inspection evidence and a clean verified worktree; no preview raster is committed.

- [ ] **Step 1: Render and inspect at full size**

Open `assets/reprocut-banner.svg` through the Codex image renderer with original detail. Verify no clipped text, trace/card collision, uneven baseline, accidental color, or weak contrast. Compare it directly with the first frame of `assets/reprocut-demo.gif`.

- [ ] **Step 2: Inspect at README-like width**

Render through the resized/high-detail path. Verify that `REPRO/CUT`, `18 → 03`, the three cobalt cards, and `SAME FAILURE` remain identifiable when fine evidence text becomes secondary.

- [ ] **Step 3: Fix only visual defects and rerun the focused contract after every edit**

Allowed fixes are SVG coordinate, size, stroke, spacing, and text hierarchy changes that preserve the semantic contract. After each edit, rerun the focused pytest command from Task 1.

- [ ] **Step 4: Run final verification**

Run the full Python suite with a fresh `--basetemp`, then `git diff --check` and `git status --short`.

Expected: the Python suite passes with only the dedicated native-wheel smoke skipped, `git diff --check` is silent, and the worktree has no uncommitted banner changes.

