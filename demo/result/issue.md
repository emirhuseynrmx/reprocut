# Minimal reproduction: TypeError: unsupported operand type(s) for +: 'decimal.Decimal' and 'str'

> **Same failure verified.** Fingerprint `e9fcf47255b5def95058ae8dc0dd1d0a7e176d2c7d8361b51f32e5c221a37d4e` matched across 3 final execution(s).

## Reduction

| Measure | Before | After | Removed |
|---|---:|---:|---:|
| Files | 18 | 3 | 15 |
| Bytes | 1669 | 675 | 994 |
| Lines | 55 | 28 | 27 |

## Failure identity

- Termination: `exit 1`
- Oracle stream: `auto`
- Normalization schema: `1`

```text
TypeError: unsupported operand type(s) for +: 'decimal.Decimal' and 'str'
```

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
- Wall time: 569 ms

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
