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
