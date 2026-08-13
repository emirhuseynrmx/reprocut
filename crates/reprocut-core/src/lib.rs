//! Correctness primitives for `ReproCut`.

mod diagnostic;
mod model;
mod oracle;
mod policy;
mod protocol;
mod reducer;
mod schema;
mod transformation;
mod winner;

pub use diagnostic::normalize_diagnostic;
pub use model::{
    CandidateVerdict, ContainmentMechanism, DiagnosticAnchor, DiagnosticChannel,
    ExecutionObservation, FailureFingerprint, OracleMode, TerminationReason,
};
pub use oracle::{FailureOracle, OracleError, OracleSpec};
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
pub use schema::{
    ContractVersions, ARTIFACT_MANIFEST_SCHEMA, CI_EVIDENCE_SCHEMA, CONTRACT_VERSIONS,
    EVIDENCE_SCHEMA, NORMALIZATION_SCHEMA, SERVER_DATABASE_SCHEMA, SESSION_SCHEMA,
};
pub use transformation::{
    ByteRange, CandidateRank, ContentDigest, ContentHasher, FrontierClass, Operation, ProjectPath,
    Transformation, TransformationError,
};
pub use winner::LowestWinner;
