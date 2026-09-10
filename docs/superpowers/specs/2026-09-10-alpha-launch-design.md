# ReproCut 0.1.0-alpha.1 launch design

## Goal

Prepare the `feat/external-ci-validation` branch for an alpha launch without
publishing a tag or uploading to crates.io or PyPI. The release must include a
working `reprocut doctor` preflight, English-only launch assets, and current
package and CI evidence.

## Product story

ReproCut turns a failing repository into a smaller project that preserves the
same stabilized failure. The launch story is a three-step sequence:

1. `doctor` proves that the original failure is stable and recognizable.
2. `reduce` removes project material while preserving that failure.
3. `verify` independently checks the resulting artifact.

The primary line is **“Same failure. Less project.”** The process line is
**“Diagnose → Reduce → Verify.”** All public copy and embedded image text is
English.

## Code scope

Finish the existing uncommitted `doctor` implementation without creating a
second oracle or preparation path. `doctor` and `reduce` must build the same
request and use the same baseline observation and stabilization logic.

The command reports a versioned machine-readable document with three outcomes:

- exit 0: the failure is stable and recognizable;
- exit 1: the project or command is not ready for reduction;
- exit 2: the request itself is invalid.

It must not create an output directory, open persistent session state, or
modify the source project. Human output must explain the observed runs and the
failure contract. JSON output must remain bounded and declare its disclosure
limits.

## Visual system

Use the supplied reference as the visual direction: deep navy background,
violet and warm orange light, a central package/cut motif, restrained glow, and
crisp technical typography. Do not reproduce its Turkish text.

Deliverables:

- repository banner at 1600×600;
- launch visual at 1920×1080;
- animated demo GIF at 800×450;
- reusable square mark when the composition requires it.

Generated imagery may provide the atmospheric background and central object.
All product text, numbers, terminal lines, and logos are overlaid from code so
they remain exact and readable. The GIF shows a factual run: `doctor`, a
reduction from 18 files to 3, three final verification runs, and the verified
artifact. It does not present the onboarding fixture as a large-project
benchmark.

## Documentation and packaging

Update README and CHANGELOG references to use the new English assets and the
`0.1.0-alpha.1` version. Keep current limitations visible: ReproCut is alpha,
commands run with the caller's authority, a passing preflight does not promise a
smaller result, and independent validations remain zero until a third party
submits one.

## Verification

Run the repository's complete local gates with an explicit real Python
interpreter on Windows:

- Rust formatting, Clippy, workspace tests, and package checks;
- Python tests and package builds;
- VS Code and gallery Node tests;
- release/audit scripts and artifact checks that can run locally;
- `doctor` against a stable failure, a passing command, unstable output, and an
  invalid oracle;
- an end-to-end failing fixture through `doctor`, `reduce`, and `verify`;
- rendered inspection of the banner, launch image, and animated GIF.

Any platform-only, signing, provenance, or registry check that cannot be
performed locally must be stated explicitly. Commit and push the verified
changes to `feat/external-ci-validation`; do not create a tag or publish a
registry release.
