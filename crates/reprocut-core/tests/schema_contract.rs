//! Independent persisted-contract version checks.

use reprocut_core::{
    ARTIFACT_MANIFEST_SCHEMA, CI_EVIDENCE_SCHEMA, CONTRACT_VERSIONS, EVIDENCE_SCHEMA,
    NORMALIZATION_SCHEMA, SERVER_DATABASE_SCHEMA, SESSION_SCHEMA,
};

#[test]
fn v0_1_contract_versions_are_explicit_and_independent() {
    assert_eq!(CONTRACT_VERSIONS.normalization, 5);
    assert_eq!(CONTRACT_VERSIONS.evidence, 4);
    assert_eq!(CONTRACT_VERSIONS.session, 3);
    assert_eq!(CONTRACT_VERSIONS.ci_evidence, 1);
    assert_eq!(CONTRACT_VERSIONS.artifact_manifest, 1);
    assert_eq!(CONTRACT_VERSIONS.server_database, 1);

    assert_eq!(NORMALIZATION_SCHEMA, CONTRACT_VERSIONS.normalization);
    assert_eq!(EVIDENCE_SCHEMA, CONTRACT_VERSIONS.evidence);
    assert_eq!(SESSION_SCHEMA, CONTRACT_VERSIONS.session);
    assert_eq!(CI_EVIDENCE_SCHEMA, CONTRACT_VERSIONS.ci_evidence);
    assert_eq!(
        ARTIFACT_MANIFEST_SCHEMA,
        CONTRACT_VERSIONS.artifact_manifest
    );
    assert_eq!(SERVER_DATABASE_SCHEMA, CONTRACT_VERSIONS.server_database);
}
