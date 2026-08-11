//! Correctness primitives for ReproCut.

mod model;
mod oracle;
mod policy;
mod reducer;
mod winner;

pub use model::{
    CandidateVerdict, DiagnosticAnchor, DiagnosticChannel, ExecutionObservation, FailureFingerprint,
};
pub use oracle::{normalize_diagnostic, FailureOracle, OracleError};
pub use policy::{
    wilson_interval, AggregateDecision, AggregateEvidence, ConfidenceInterval, EvaluationPolicy,
    PolicyError,
};
pub use reducer::{reduce, ReductionResult, ReductionUnit};
pub use winner::LowestWinner;
