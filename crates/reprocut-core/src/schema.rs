/// Version numbers for every independently persisted v0.1 contract.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ContractVersions {
    /// Diagnostic normalization and fingerprint semantics.
    pub normalization: u16,
    /// Published reduction evidence.
    pub evidence: u16,
    /// Durable reduction sessions and cache compatibility.
    pub session: u16,
    /// Exact-commit CI release evidence.
    pub ci_evidence: u16,
    /// Verified artifact member manifests.
    pub artifact_manifest: u16,
    /// Self-hosted server database migrations.
    pub server_database: u16,
}

/// Authoritative v0.1 contract versions.
pub const CONTRACT_VERSIONS: ContractVersions = ContractVersions {
    normalization: 5,
    evidence: 4,
    session: 3,
    ci_evidence: 1,
    artifact_manifest: 1,
    server_database: 1,
};

/// Diagnostic normalization contract included in fingerprints.
pub const NORMALIZATION_SCHEMA: u16 = CONTRACT_VERSIONS.normalization;

/// Machine-readable reduction evidence contract.
pub const EVIDENCE_SCHEMA: u16 = CONTRACT_VERSIONS.evidence;

/// Durable reduction session and cache contract.
pub const SESSION_SCHEMA: u16 = CONTRACT_VERSIONS.session;

/// Exact-commit CI release-evidence contract.
pub const CI_EVIDENCE_SCHEMA: u16 = CONTRACT_VERSIONS.ci_evidence;

/// Verified artifact manifest contract.
pub const ARTIFACT_MANIFEST_SCHEMA: u16 = CONTRACT_VERSIONS.artifact_manifest;

/// Self-hosted server database contract.
pub const SERVER_DATABASE_SCHEMA: u16 = CONTRACT_VERSIONS.server_database;
