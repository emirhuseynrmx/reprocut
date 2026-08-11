# Public launch checklist

## Evidence gate

- [ ] CI is green on the exact public commit.
- [ ] The release benchmark artifact exists; README makes no stronger claim.
- [ ] All six binary archives pass smoke tests and match `SHA256SUMS`.
- [ ] SPDX SBOMs and GitHub provenance attestations are downloadable.
- [ ] crates.io and PyPI names are rechecked immediately before publication.
- [ ] The checked-in demo, GIF fingerprint comment, and gallery example agree.

## Repository gate

- [ ] README commands work from a clean clone.
- [ ] One-click demo evidence is visible above the fold.
- [ ] Issues and Discussions are enabled and templates are concise.
- [ ] No generated benchmark number, testimonial, user count, or compatibility
      claim is invented.
- [ ] `v0.1.0` points to the audited commit and release publication is manually approved.

## Hacker News gate

- [ ] Submit the canonical repository URL once with the `Show HN:` title.
- [ ] Post the prepared first comment immediately with technical context.
- [ ] Be available for several hours to answer concrete engineering criticism.
- [ ] Convert valid criticism into linked issues; do not argue with taste.
- [ ] Do not ask friends for coordinated votes or stars.
- [ ] Report actual limitations plainly and update the README when a finding is reproducible.

## After 48 hours

- [ ] Record traffic, clone/install failures, issues, and stars as outcomes—not guarantees.
- [ ] Prioritize failures that block first use over new transforms.
- [ ] Publish a small evidence-backed follow-up only if something materially improved.
