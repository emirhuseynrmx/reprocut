# Minimal reproduction: TypeError: unsupported operand type(s) for +: 'decimal.Decimal' and 'str'

> **Same failure verified.** Fingerprint `b6b897ed734c446373d7b59f988edc50f8f13369f8438cc8b6d4756221f86415` matched across 3 final execution(s).

## Reduction

| Measure | Before | After | Removed |
|---|---:|---:|---:|
| Files | 18 | 3 | 15 |
| Bytes | 1669 | 675 | 994 |
| Lines | 55 | 28 | 27 |

## Failure identity

- Termination: `exit 1`
- Oracle stream: `auto`
- Oracle mode: `automatic`
- Oracle spec SHA-256: `3015cdd3dcd09acb2c9e17736d828908ee6ccab62db68928f1de9e2c1468d142`
- Normalization schema: `4`

```text
TypeError: unsupported operand type(s) for +: 'decimal.Decimal' and 'str'
```

## Integrity contracts

- Source snapshot SHA-256: `e6271f52f712d909b34912ae88b1120939d27d57fe1201e15bb85e0161121d2f`
- Preparation mode: `none`
- Preparation contract: `43186a7b3c01e42e26c8da61a442cde7b8c25c26d1780f8290698dbf0cb3c728`

## Reproduce

```sh
python bug.py
```

## Retained project

- `bug.py`
- `checkout.py`
- `fixtures/order.json`

## Search evidence

- Candidate attempts: 24
- Cache reuses: 7
- Inconclusive candidates: 0
- Wall time: 557 ms

## Included evidence

- `project/` — exact final verified snapshot
- `reduction.json` — versioned shared evidence
- `attempts.jsonl` — append-only candidate events
- `report.html` — self-contained visual record
- `reproduce.sh` / `reproduce.ps1` — quoted argv launchers

## Limits

- Elapsed time is one wall-clock observation, not a benchmark.
- Retained paths are observations from the verified final snapshot, not claims of semantic necessity.
- Syntax-node counts are omitted until a grammar-valid cross-language counter is available.
- The official Playground host has no Python executable, so search used a content-equivalent shell oracle; the source and final project are independently executed three times by this builder's local Python runtime.
