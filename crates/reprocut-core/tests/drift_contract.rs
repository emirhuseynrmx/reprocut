//! Diagnostic-drift measurement contracts.

use reprocut_core::{DiagnosticChannel, DiagnosticDrift, ExecutionObservation};

fn failing(stderr: &str) -> ExecutionObservation {
    ExecutionObservation::new(
        Some(1),
        None,
        Vec::new(),
        stderr.as_bytes().to_vec(),
        false,
        false,
    )
}

fn measure(baseline: &str, minimized: &str) -> DiagnosticDrift {
    let baselines = [failing(baseline), failing(baseline)];
    let minimized = failing(minimized);
    DiagnosticDrift::measure(
        DiagnosticChannel::Stderr,
        &baselines,
        &[&minimized, &minimized],
    )
}

#[test]
fn a_shrinking_diagnostic_is_not_drift() {
    let drift = measure(
        "Fixing docs/spec.md\nFixing tests/spec_test.py\nfiles were modified by this hook",
        "Fixing tests/spec_test.py\nfiles were modified by this hook",
    );

    assert_eq!(drift.novel_lines(), 0);
    assert_eq!(drift.retained_lines(), 2);
    assert!(!drift.is_reportable());
}

// The regression this exists for: an upstream check prints one summary line for two different
// causes. The original failed because a committed port was stale; deleting the corpus made the
// same summary line appear because the ports were gone. Every required expression still matched.
#[test]
fn a_failure_reached_by_a_different_cause_is_reported() {
    let drift = measure(
        "regen --check: examples/sky/ipe/08-notes-app differs from re-deriving it\n\
         regen --check: committed ports are stale vs rename-map/ipe-edits",
        "regen --check: examples/sky/original/00-standard-libs missing (run regen)\n\
         regen --check: examples/sky/original/01-hello-world missing (run regen)\n\
         regen --check: examples/sky/original/04-local-pkg missing (run regen)\n\
         regen --check: committed ports are stale vs rename-map/ipe-edits",
    );

    assert_eq!(drift.novel_lines(), 3);
    assert_eq!(drift.retained_lines(), 1);
    assert!(drift.is_reportable());
    assert!(drift
        .novel_sample()
        .iter()
        .all(|line| line.contains("missing (run regen)")));
}

#[test]
fn a_minority_of_novel_lines_stays_below_the_reporting_bar() {
    let drift = measure(
        "error: assertion failed\nleft: 1\nright: 2\ntest tests::check failed",
        "error: assertion failed\nleft: 1\nright: 2\nwarning: no tests found in examples",
    );

    assert_eq!(drift.novel_lines(), 1);
    assert!(!drift.is_reportable());
}

#[test]
fn an_unchanged_diagnostic_reports_no_novelty() {
    let drift = measure("ValueError: sentinel", "ValueError: sentinel");

    assert_eq!(drift.novel_lines(), 0);
    assert_eq!(drift.baseline_lines(), drift.final_lines());
    assert!(!drift.is_reportable());
}

#[test]
fn a_line_seen_in_only_one_baseline_run_is_not_novel() {
    let baselines = [
        failing("ValueError: sentinel\nflaky: retry scheduled"),
        failing("ValueError: sentinel"),
    ];
    let minimized = failing("ValueError: sentinel\nflaky: retry scheduled");

    let drift = DiagnosticDrift::measure(DiagnosticChannel::Stderr, &baselines, &[&minimized]);

    assert_eq!(drift.novel_lines(), 0);
}
