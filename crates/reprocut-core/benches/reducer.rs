//! Reduction hot-path benchmarks.

use criterion::{black_box, criterion_group, criterion_main, BatchSize, Criterion, Throughput};
use reprocut_core::{reduce, CandidateVerdict, ExecutionObservation, FailureOracle, ReductionUnit};

fn benchmark_reducer(criterion: &mut Criterion) {
    const UNIT_COUNT: usize = 4_096;
    const REQUIRED: [u32; 4] = [7, 1_023, 2_047, 4_095];
    let units = (0..UNIT_COUNT)
        .map(|index| {
            ReductionUnit::new(
                u32::try_from(index).expect("fixture fits stable identifiers"),
                format!("src/generated/module_{index:04}.rs"),
            )
        })
        .collect::<Vec<_>>();
    let mut group = criterion.benchmark_group("hierarchical_reducer");
    group.throughput(Throughput::Elements(
        u64::try_from(UNIT_COUNT).expect("fixture size fits throughput counter"),
    ));

    group.bench_function("4096_units_four_required", |bencher| {
        bencher.iter_batched(
            || units.clone(),
            |candidate_units| {
                let result = reduce(&candidate_units, |candidate| {
                    if REQUIRED
                        .iter()
                        .all(|required| candidate.iter().any(|unit| unit.id() == *required))
                    {
                        CandidateVerdict::Preserved
                    } else {
                        CandidateVerdict::Rejected
                    }
                });
                black_box(result);
            },
            BatchSize::SmallInput,
        );
    });
    group.finish();
}

fn benchmark_oracle(criterion: &mut Criterion) {
    let diagnostic = (0..512)
        .map(|index| format!("worker {index} at /tmp/build/module_{index}.rs address 0xDEADBEEF"))
        .collect::<Vec<_>>()
        .join("\n");
    let baseline = ExecutionObservation::new(
        Some(1),
        None,
        Vec::new(),
        diagnostic.clone().into_bytes(),
        false,
        false,
    );
    let oracle = FailureOracle::from_baselines(&[baseline.clone(), baseline.clone()])
        .expect("fixture is stable");
    let mut group = criterion.benchmark_group("failure_oracle");
    group.throughput(Throughput::Bytes(
        u64::try_from(diagnostic.len()).expect("diagnostic size fits throughput counter"),
    ));

    group.bench_function("classify_32k_diagnostic", |bencher| {
        bencher.iter(|| black_box(oracle.classify(black_box(&baseline))));
    });
    group.finish();
}

// Criterion 0.5 emits a public `BENCHES` static that cannot carry user-authored docs.
#[allow(missing_docs)]
criterion_group!(benches, benchmark_reducer, benchmark_oracle);
criterion_main!(benches);
