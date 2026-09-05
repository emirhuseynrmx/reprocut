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
    /// Both sides are unions across their repeated observations: a line counts as seen if any
    /// run printed it, so a flaky line is never mistaken for drift.
    pub fn measure(
        channel: DiagnosticChannel,
        baselines: &[ExecutionObservation],
        finals: &[&ExecutionObservation],
    ) -> Self {
        let baseline = line_union(channel, baselines.iter());
        let observed = line_union(channel, finals.iter().copied());
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

    /// Returns distinct normalized lines the minimized failure prints.
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
        for stream in streams(channel) {
            let bytes = match stream {
                DiagnosticChannel::Stdout => observation.stdout(),
                DiagnosticChannel::Stderr => observation.stderr(),
                DiagnosticChannel::Auto | DiagnosticChannel::Combined => continue,
            };
            lines.extend(normalize_bytes(bytes).lines().map(str::to_owned));
        }
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
