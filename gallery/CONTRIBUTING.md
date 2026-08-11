# Submit a minimal reproduction

Gallery publication is explicit and pull-request curated. ReproCut never uploads
an artifact. Prepare a redacted directory locally:

```console
reprocut gallery prepare \
  --from ./reprocut-output \
  --output ./my-submission \
  --title "Short public title" \
  --license "MIT"
```

Copy the reviewed directory to `gallery/entries/<entry-slug>/`. Source is absent
unless you explicitly pass `--include-source`; review it and its license before
submission. Pull-request CI validates the fixed schema, size/path limits,
license declaration, and common credential patterns. It does not execute any
submitted program. A maintainer may mark `featured: true` only during review.

Run the same local gate with:

```console
node --test gallery/test/*.test.js
node gallery/scripts/build.js
```
