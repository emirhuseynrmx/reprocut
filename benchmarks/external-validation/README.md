# External validation records

The numbers the project README quotes for third-party projects come from
`.github/workflows/external-validation.yml`. That workflow uploads a complete
evidence bundle, and GitHub deletes it after fourteen days. These records outlive
it, so a reader can check the claim rather than take it.

One directory per case in `scripts/external_validation/cases.json`:

- `summary.json` — what was pinned, the oracle contract and its digest, the
  isolation the container ran under, the measurements, the drift measurement, the
  artifact identity, and the final verification verdicts.
- `admission-logs/` — the failing command's output on the pinned base and head.
  Base passes three times and head fails three times; that is what makes the case
  a real regression rather than a broken checkout.
- `final-verification/` — the same command's output on the minimized project,
  three times. Read it against `admission-logs/head-*.log`: the minimized project
  is only a reproduction of the original defect if these say the same thing.

## What is deliberately not here

The minimized project itself. It is third-party source under its own license, and
republishing it here is a licensing decision this repository has not made. The
full bundle, including that snapshot, is attached to the CI run named in each
`summary.json`, and re-running the workflow reproduces it from the pinned commits.

## Reproducing a case

```console
python scripts/external_validation/run_case.py --case ipe --output ./out
```

It builds the pinned container, admits the case, reduces it, and re-verifies. The
run is offline: the container has no network, no added capabilities, and a
non-root user.

## Reading the drift measurement

`failure.diagnostic_drift` compares the minimized project's diagnostic against the
original's. `novel_lines` counts lines the original never printed. A case whose
minimized failure is genuinely the same defect prints nothing new, and the
validation harness rejects a case where novel lines are the majority — the engine
only reports drift, but this corpus exists to demonstrate the failure is kept, so
here it is a hard failure.
