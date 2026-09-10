use std::collections::BTreeSet;

use crate::{diagnostic::normalize_bytes, DiagnosticChannel, ExecutionObservation};

const MAX_SAMPLE: usize = 8;

/// How much of the minimized failure's diagnostic was never observed in the original.
///
/// A shrinking diagnostic is expected: removing files removes the messages they produced. Text
/// the original failure never printed is the opposite signal. It means the oracle is being
/// satisfied by something the original run did not do, which is how a regex contract that is
/// looser than its author intended quietly reduces to a different failure.
///
/// This is an observation, never a verdict. It cannot reject a candidate or fail a reduction,
/// because a legitimate reduction may print incidental new text.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DiagnosticDrift {
    baseline_lines: usize,
    final_lines: usize,
    retained_lines: usize,
    novel_lines: usize,
    novel_sample: Vec<String>,
}

impl DiagnosticDrift {
    /// Measures drift between the original failure and the verified minimized failure.
    ///
    /// The two sides use opposite operators, and both choices point the same way: toward
    /// reporting drift only when the evidence for it is unanimous.
    ///
    /// The original is the union of its runs. A line the original printed even once is
    /// something the original can print, so it cannot be novel.
    ///
    /// The minimized failure is the intersection of its runs. A line only counts as part of
    /// what the minimized project prints if every verification run printed it. This is what
    /// separates drift from noise without a list of patterns to maintain: an elapsed time, a
    /// progress bar, an object address, a temporary path the normalizer has never heard of —
    /// all of them differ between runs of the same snapshot, so none of them survives the
    /// intersection. A genuinely different failure prints its message every time, and does.
    ///
    /// The engine already runs both sides repeatedly to prove the failure is stable. This
    /// reads the noise floor out of those runs rather than assuming what it looks like.
    pub fn measure(
        channel: DiagnosticChannel,
        baselines: &[ExecutionObservation],
        finals: &[&ExecutionObservation],
    ) -> Self {
        let baseline = line_union(channel, baselines.iter());
        let observed = line_intersection(channel, finals.iter().copied());
        let novel = observed.difference(&baseline).cloned().collect::<Vec<_>>();
        Self {
            baseline_lines: baseline.len(),
            final_lines: observed.len(),
            retained_lines: observed.intersection(&baseline).count(),
            novel_lines: novel.len(),
            novel_sample: novel.into_iter().take(MAX_SAMPLE).collect(),
        }
    }

    /// Returns distinct normalized lines the original failure printed.
    pub const fn baseline_lines(&self) -> usize {
        self.baseline_lines
    }

    /// Returns distinct normalized lines the minimized failure prints on every run.
    pub const fn final_lines(&self) -> usize {
        self.final_lines
    }

    /// Returns minimized lines the original failure also printed.
    pub const fn retained_lines(&self) -> usize {
        self.retained_lines
    }

    /// Returns minimized lines the original failure never printed.
    pub const fn novel_lines(&self) -> usize {
        self.novel_lines
    }

    /// Returns up to eight novel lines, in normalized lexical order.
    pub fn novel_sample(&self) -> &[String] {
        &self.novel_sample
    }

    /// Returns true when most of the minimized diagnostic is text the original never printed.
    ///
    /// Callers surface this; they must not let it change a verdict.
    pub const fn is_reportable(&self) -> bool {
        self.novel_lines > self.final_lines.saturating_sub(self.novel_lines)
    }
}

fn line_union<'a>(
    channel: DiagnosticChannel,
    observations: impl Iterator<Item = &'a ExecutionObservation>,
) -> BTreeSet<String> {
    let mut lines = BTreeSet::new();
    for observation in observations {
        lines.extend(observation_lines(channel, observation));
    }
    lines
}

/// Lines every observation printed.
///
/// An empty iterator yields an empty set, which reports no drift. That is the right answer
/// for the only way it can happen: no final observation to compare against.
fn line_intersection<'a>(
    channel: DiagnosticChannel,
    observations: impl Iterator<Item = &'a ExecutionObservation>,
) -> BTreeSet<String> {
    let mut shared: Option<BTreeSet<String>> = None;
    for observation in observations {
        let lines = observation_lines(channel, observation);
        shared = Some(match shared {
            None => lines,
            Some(previous) => previous.intersection(&lines).cloned().collect(),
        });
    }
    shared.unwrap_or_default()
}

fn observation_lines(
    channel: DiagnosticChannel,
    observation: &ExecutionObservation,
) -> BTreeSet<String> {
    let mut lines = BTreeSet::new();
    for stream in streams(channel) {
        let bytes = match stream {
            DiagnosticChannel::Stdout => observation.stdout(),
            DiagnosticChannel::Stderr => observation.stderr(),
            DiagnosticChannel::Auto | DiagnosticChannel::Combined => continue,
        };
        lines.extend(normalize_bytes(bytes).lines().map(str::to_owned));
    }
    lines
}

const fn streams(channel: DiagnosticChannel) -> &'static [DiagnosticChannel] {
    match channel {
        DiagnosticChannel::Stdout => &[DiagnosticChannel::Stdout],
        DiagnosticChannel::Stderr => &[DiagnosticChannel::Stderr],
        DiagnosticChannel::Auto | DiagnosticChannel::Combined => {
            &[DiagnosticChannel::Stdout, DiagnosticChannel::Stderr]
        }
    }
}
