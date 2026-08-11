//! Correctness primitives for ReproCut.

mod model;
mod oracle;
mod policy;
mod protocol;
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
pub use protocol::{
    ProgressEventV1, ProtocolAction, ProtocolError, ReductionRequestV1, PROTOCOL_VERSION,
};
pub use reducer::{
    ordered_frontier, reduce, reduce_hierarchical, reduce_hierarchical_frontiers,
    FrontierPartition, ReductionResult, ReductionUnit,
};
pub use transformation::{
    ByteRange, CandidateRank, ContentDigest, ContentHasher, FrontierClass, Operation, ProjectPath,
    Transformation, TransformationError,
};
pub use winner::LowestWinner;
