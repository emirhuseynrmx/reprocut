use thiserror::Error;

use crate::CandidateVerdict;

const STRICT_RUNS: u16 = 3;
const MIN_FLAKY_RUNS: u16 = 5;
const MAX_FLAKY_RUNS: u16 = 101;

/// A validated rule for aggregating repeated candidate executions.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EvaluationPolicy {
    /// Require three preserved executions with no contrary or incomplete run.
    Strict,
    /// Require a configured supermajority within a bounded number of runs.
    Flaky {
        /// Maximum number of observations considered.
        runs: u16,
        /// Preserved observations required for acceptance.
        required: u16,
    },
}

impl EvaluationPolicy {
    /// Returns the fail-closed deterministic policy.
    pub const fn strict() -> Self {
        Self::Strict
    }

    /// Validates and returns a bounded supermajority policy.
    ///
    /// # Errors
    ///
    /// Returns an error for an even or out-of-range run count, or for a threshold that is not a
    /// valid two-thirds supermajority.
    pub const fn flaky(runs: u16, required: u16) -> Result<Self, PolicyError> {
        if runs < MIN_FLAKY_RUNS || runs > MAX_FLAKY_RUNS {
            return Err(PolicyError::RunsOutOfRange);
        }
        if runs % 2 == 0 {
            return Err(PolicyError::RunsMustBeOdd);
        }
        if required == 0 || required > runs {
            return Err(PolicyError::RequiredOutOfRange);
        }
        // At least two thirds is intentionally stronger than a bare majority.
        if (required as u32) * 3 < (runs as u32) * 2 {
            return Err(PolicyError::RequiredNotSupermajority);
        }
        Ok(Self::Flaky { runs, required })
    }

    /// Returns the documented 9-of-11 flaky policy.
    pub const fn default_flaky() -> Self {
        Self::Flaky {
            runs: 11,
            required: 9,
        }
    }

    /// Returns the maximum number of observations this policy consumes.
    pub const fn runs(self) -> u16 {
        match self {
            Self::Strict => STRICT_RUNS,
            Self::Flaky { runs, .. } => runs,
        }
    }

    /// Returns the preserved-observation threshold.
    pub const fn required(self) -> u16 {
        match self {
            Self::Strict => STRICT_RUNS,
            Self::Flaky { required, .. } => required,
        }
    }

    /// Consumes observations lazily and stops once the outcome is inevitable.
    pub fn aggregate<I>(self, verdicts: I) -> AggregateEvidence
    where
        I: IntoIterator<Item = CandidateVerdict>,
    {
        let mut evidence = AggregateEvidence::new(self.runs(), self.required());
        for verdict in verdicts.into_iter().take(usize::from(self.runs())) {
            evidence.record(verdict);
            if evidence.terminal_decision().is_some() {
                break;
            }
        }
        evidence.finish();
        evidence
    }
}

impl Default for EvaluationPolicy {
    fn default() -> Self {
        Self::Strict
    }
}

/// Configuration error for a repeated-execution policy.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum PolicyError {
    /// Flaky policies accept between 5 and 101 observations.
    #[error("flaky runs must be between 5 and 101")]
    RunsOutOfRange,
    /// Odd run counts avoid ambiguous symmetrical policies.
    #[error("flaky runs must be odd")]
    RunsMustBeOdd,
    /// The preservation threshold must fit inside the run budget.
    #[error("flaky required must be between 1 and runs")]
    RequiredOutOfRange,
    /// `ReproCut` deliberately rejects chance-sensitive bare majorities.
    #[error("flaky required must be at least a two-thirds supermajority")]
    RequiredNotSupermajority,
}

/// Final interpretation of a bounded group of executions.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AggregateDecision {
    /// The configured failure reached the required count.
    Preserved,
    /// Complete contrary evidence made the threshold unreachable.
    Rejected,
    /// Missing, incomplete, or insufficient evidence cannot authorize a cut.
    Inconclusive,
}

/// A bounded 95% Wilson score interval.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ConfidenceInterval {
    lower: f64,
    upper: f64,
}

impl ConfidenceInterval {
    /// Returns the inclusive lower bound.
    pub const fn lower(self) -> f64 {
        self.lower
    }

    /// Returns the inclusive upper bound.
    pub const fn upper(self) -> f64 {
        self.upper
    }
}

/// Counts and statistical display evidence for one aggregate evaluation.
#[derive(Clone, Debug, PartialEq)]
pub struct AggregateEvidence {
    max_runs: u16,
    required: u16,
    observed: u16,
    preserved: u16,
    rejected: u16,
    inconclusive: u16,
    decision: Option<AggregateDecision>,
}

impl AggregateEvidence {
    const fn new(max_runs: u16, required: u16) -> Self {
        Self {
            max_runs,
            required,
            observed: 0,
            preserved: 0,
            rejected: 0,
            inconclusive: 0,
            decision: None,
        }
    }

    fn record(&mut self, verdict: CandidateVerdict) {
        self.observed += 1;
        match verdict {
            CandidateVerdict::Preserved => self.preserved += 1,
            CandidateVerdict::Rejected => self.rejected += 1,
            CandidateVerdict::Inconclusive => self.inconclusive += 1,
        }
        self.decision = self.terminal_decision();
    }

    fn terminal_decision(&self) -> Option<AggregateDecision> {
        if self.preserved >= self.required {
            return Some(AggregateDecision::Preserved);
        }
        let remaining = self.max_runs.saturating_sub(self.observed);
        if self.preserved + remaining < self.required {
            return Some(if self.inconclusive == 0 {
                AggregateDecision::Rejected
            } else {
                AggregateDecision::Inconclusive
            });
        }
        None
    }

    fn finish(&mut self) {
        if self.decision.is_none() {
            self.decision = Some(AggregateDecision::Inconclusive);
        }
    }

    /// Returns the fail-closed final decision.
    pub fn decision(&self) -> AggregateDecision {
        self.decision.unwrap_or(AggregateDecision::Inconclusive)
    }

    /// Returns the number of consumed observations, including incomplete ones.
    pub const fn observed_runs(&self) -> u16 {
        self.observed
    }

    /// Returns observations that preserved the stabilized failure.
    pub const fn preserved_runs(&self) -> u16 {
        self.preserved
    }

    /// Returns complete observations that contradicted the fingerprint.
    pub const fn rejected_runs(&self) -> u16 {
        self.rejected
    }

    /// Returns timed-out, truncated, or otherwise incomplete observations.
    pub const fn inconclusive_runs(&self) -> u16 {
        self.inconclusive
    }

    /// Returns the preserved rate over complete observations.
    pub fn observed_rate(&self) -> Option<f64> {
        let complete = self.preserved + self.rejected;
        (complete != 0).then(|| f64::from(self.preserved) / f64::from(complete))
    }

    /// Returns a display-only 95% Wilson score interval over complete observations.
    pub fn wilson_95(&self) -> Option<ConfidenceInterval> {
        wilson_interval(self.preserved, self.preserved + self.rejected)
    }
}

/// Computes the display-only 95% Wilson score interval for binomial observations.
pub fn wilson_interval(successes: u16, observations: u16) -> Option<ConfidenceInterval> {
    const Z: f64 = 1.959_963_984_540_054;

    if observations == 0 || successes > observations {
        return None;
    }
    let n = f64::from(observations);
    let probability = f64::from(successes) / n;
    let z_squared = Z * Z;
    let denominator = 1.0 + z_squared / n;
    let center = (probability + z_squared / (2.0 * n)) / denominator;
    let margin = Z * ((probability * (1.0 - probability) / n + z_squared / (4.0 * n * n)).sqrt())
        / denominator;
    Some(ConfidenceInterval {
        lower: (center - margin).max(0.0),
        upper: (center + margin).min(1.0),
    })
}
