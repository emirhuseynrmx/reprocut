# ReproCut 0.1 release artifacts

Every platform archive is built from the tagged commit and contains only the
binary, four generated shell completions, README, the Apache-2.0 license, and a version record
binding the target, source revision, and `SOURCE_DATE_EPOCH`.

`SHA256SUMS` covers each archive and SPDX JSON SBOM. `release-manifest.json`
binds those files to all six required targets. GitHub's build-provenance
attestation is created from the same aggregate job.

The workflow structurally verifies every archive and runs its binary against a
real failing Python project before upload. The GitHub Release job is exact-tag
gated behind the manually approved `release` environment. It does not publish
crates.io or PyPI packages; those remain the documented user handoff.

Verify a downloaded archive without extraction:

```console
python scripts/release/verify_archive.py \
  reprocut-0.1.0-x86_64-unknown-linux-gnu.tar.gz \
  --target x86_64-unknown-linux-gnu \
  --version 0.1.0
```
