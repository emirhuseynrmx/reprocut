# Benchmark protocol

ReproCut does not publish performance numbers without their evidence bundle.

Run:

```sh
rustc --version --verbose
cargo bench -p reprocut-core --bench reducer -- --save-baseline local
```

Record the CPU model, logical core count, memory configuration, operating system, power profile, compiler output above, and the complete `target/criterion` directory. Do not compare two commits unless both runs use the same machine and the machine is otherwise idle.

The reducer fixture contains 4,096 sorted units with four required files. It measures the whole deterministic search, including retained `Arc<str>` clones in the final result. The oracle fixture repeatedly normalizes and classifies an approximately 32 KiB diagnostic. Criterion owns warm-up, sampling, outlier reporting, and confidence intervals; no single timing is used as a claim.

Performance changes must retain all correctness, Loom, Miri, and sanitizer gates. A faster candidate that weakens failure identity is a regression.

## Pinned upstream compiler-bug corpus

`upstream-corpus.json` identifies 24 Clang/GCC bug subjects curated by the Perses
project. Every subject is linked to its public issue, pinned to one immutable
Perses commit, and classified by its upstream layout.

The Perses subjects are GPL-3.0-only, so they are deliberately not copied into
ReproCut's Apache-2.0 source tree. Fetching requires an explicit license
acknowledgement:

```sh
python scripts/fetch_upstream_corpus.py \
  --destination ./output/upstream-corpus \
  --accept-gpl-3.0
```

The fetcher downloads only 95 exact allowlisted files from the pinned commit.
It limits every response to 8 MiB, limits the selected corpus to 128 MiB,
refuses overwrite, publishes atomically, writes provenance beside every case,
and never executes the upstream `r.sh` files. Running historical compiler
oracles is a separate, opt-in benchmark operation because those scripts invoke
untrusted code and depend on specific compiler builds.
