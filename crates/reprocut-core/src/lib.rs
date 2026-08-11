//! Correctness primitives for ReproCut.

mod model;
mod oracle;
mod policy;
mod reducer;
mod transformation;
mod winner;

pub use model::{
    CandidateVerdict, ContainmentMechanism, DiagnosticAnchor, DiagnosticChannel,
    ExecutionObservation, FailureFingerprint, TerminationReason,
};
pub use oracle::{normalize_diagnostic, FailureOracle, OracleError};
pub use policy::{
    wilson_interval, AggregateDecision, AggregateEvidence, ConfidenceInterval, EvaluationPolicy,
    PolicyError,
};
pub use reducer::{reduce, ReductionResult, ReductionUnit};
pub use transformation::{
    ByteRange, CandidateRank, ContentDigest, FrontierClass, Operation, ProjectPath, Transformation,
    TransformationError,
};
pub use winner::LowestWinner;
