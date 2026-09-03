//! Real-filesystem structural verification and tamper contracts.

use std::{fs, path::Path};

use reprocut_report::{
    build_artifact_manifest, render_issue, render_report, render_reproduction_scripts,
    verify_artifact, write_attempts_jsonl, AttemptSummary, ChannelAnchor, DriftEvidence,
    EvaluationPolicyEvidence, FailureEvidence, FinalObservationEvidence, MaterialMeasurement,
    MeasurementSet, PreparationEvidence, ReductionEvidence, ReportModel, RetainedEntry,
    RetainedManifest, RetentionEvidence, SearchEvidence, VerificationError,
    EVIDENCE_SCHEMA_VERSION, NORMALIZATION_SCHEMA_VERSION,
};

fn no_drift() -> DriftEvidence {
    DriftEvidence {
        baseline_lines: 4,
        final_lines: 3,
        retained_lines: 3,
        novel_lines: 0,
        reportable: false,
        novel_sample: Vec::new(),
    }
}
use serde_json::json;

#[test]
fn complete_artifact_is_structurally_verified() {
    let fixture = artifact_fixture();

    let verified = verify_artifact(fixture.path()).expect("valid artifact");

    assert_eq!(verified.root(), fixture.path());
    assert_eq!(verified.artifact_id().len(), 64);
}

#[test]
fn verifier_rejects_byte_member_set_ledger_and_derived_output_tampering() {
    let fixture = artifact_fixture();
    fs::write(
        fixture.path().join("project/bug.py"),
        b"raise RuntimeError\n",
    )
    .expect("tamper project");
    assert!(matches!(
        verify_artifact(fixture.path()),
        Err(VerificationError::MemberMismatch(path)) if path == "project/bug.py"
    ));

    let fixture = artifact_fixture();
    fs::write(fixture.path().join("extra.txt"), b"undeclared").expect("write extra member");
    assert!(matches!(
        verify_artifact(fixture.path()),
        Err(VerificationError::MemberSetMismatch)
    ));

    let fixture = artifact_fixture();
    fs::write(fixture.path().join("attempts.jsonl"), b"{}\n").expect("tamper ledger");
    refresh_manifest(fixture.path());
    assert!(matches!(
        verify_artifact(fixture.path()),
        Err(VerificationError::AttemptLedgerMismatch)
    ));

    let fixture = artifact_fixture();
    fs::write(
        fixture.path().join("report.html"),
        b"self-consistent forgery",
    )
    .expect("tamper report");
    refresh_manifest(fixture.path());
    assert!(matches!(
        verify_artifact(fixture.path()),
        Err(VerificationError::ReportMismatch)
    ));

    let fixture = artifact_fixture();
    fs::write(fixture.path().join("empty-marker"), b"").expect("empty member");
    refresh_manifest(fixture.path());
    assert!(matches!(
        verify_artifact(fixture.path()),
        Err(VerificationError::MemberSetMismatch)
    ));
}

#[test]
fn verifier_rejects_reordered_attempts_and_noncanonical_manifest_envelope() {
    let fixture = artifact_fixture();
    let evidence_path = fixture.path().join("reduction.json");
    let mut evidence: ReductionEvidence =
        serde_json::from_slice(&fs::read(&evidence_path).expect("evidence bytes"))
            .expect("evidence");
    evidence.attempts = vec![attempt(2, "preserved"), attempt(1, "rejected")];
    fs::write(
        &evidence_path,
        serde_json::to_vec_pretty(&evidence).expect("evidence JSON"),
    )
    .expect("rewrite evidence");
    let mut ledger = Vec::new();
    write_attempts_jsonl(&evidence.attempts, &mut ledger).expect("ledger");
    fs::write(fixture.path().join("attempts.jsonl"), ledger).expect("rewrite ledger");
    fs::write(
        fixture.path().join("report.html"),
        render_report(&ReportModel::from(&evidence)),
    )
    .expect("rewrite report");
    fs::write(fixture.path().join("issue.md"), render_issue(&evidence)).expect("rewrite issue");
    refresh_manifest(fixture.path());
    assert!(matches!(
        verify_artifact(fixture.path()),
        Err(VerificationError::AttemptLedgerMismatch)
    ));

    let fixture = artifact_fixture();
    let manifest_path = fixture.path().join("artifact-manifest.json");
    let value: serde_json::Value =
        serde_json::from_slice(&fs::read(&manifest_path).expect("manifest bytes"))
            .expect("manifest");
    fs::write(
        &manifest_path,
        serde_json::to_vec(&value).expect("compact manifest"),
    )
    .expect("rewrite manifest");
    assert!(matches!(
        verify_artifact(fixture.path()),
        Err(VerificationError::InvalidManifest(_))
    ));
}

#[cfg(unix)]
#[test]
fn verifier_rejects_execute_mask_tampering() {
    use std::os::unix::fs::PermissionsExt as _;

    let fixture = artifact_fixture();
    let project = fixture.path().join("project/bug.py");
    let mut permissions = fs::metadata(&project).expect("metadata").permissions();
    permissions.set_mode(permissions.mode() | 0o100);
    fs::set_permissions(&project, permissions).expect("change execute mask");

    assert!(matches!(
        verify_artifact(fixture.path()),
        Err(VerificationError::MemberMismatch(path)) if path == "project/bug.py"
    ));
}

fn artifact_fixture() -> tempfile::TempDir {
    let root = tempfile::tempdir().expect("artifact root");
    fs::create_dir(root.path().join("project")).expect("project directory");
    let project_bytes = b"raise ValueError('sentinel')\n";
    fs::write(root.path().join("project/bug.py"), project_bytes).expect("project file");
    let evidence = evidence(project_bytes);
    fs::write(
        root.path().join("reduction.json"),
        serde_json::to_vec_pretty(&evidence).expect("evidence JSON"),
    )
    .expect("evidence file");
    let mut ledger = Vec::new();
    write_attempts_jsonl(&evidence.attempts, &mut ledger).expect("ledger");
    fs::write(root.path().join("attempts.jsonl"), ledger).expect("ledger file");
    fs::write(
        root.path().join("report.html"),
        render_report(&ReportModel::from(&evidence)),
    )
    .expect("report");
    fs::write(root.path().join("issue.md"), render_issue(&evidence)).expect("issue");
    let scripts = render_reproduction_scripts(&evidence.command);
    fs::write(root.path().join("reproduce.sh"), scripts.shell).expect("shell launcher");
    fs::write(root.path().join("reproduce.ps1"), scripts.powershell).expect("PowerShell launcher");
    refresh_manifest(root.path());
    root
}

fn refresh_manifest(root: &Path) {
    let manifest_path = root.join("artifact-manifest.json");
    if manifest_path.exists() {
        fs::remove_file(&manifest_path).expect("remove prior manifest");
    }
    let manifest = build_artifact_manifest(root).expect("artifact manifest");
    fs::write(
        manifest_path,
        serde_json::to_vec_pretty(&manifest).expect("manifest JSON"),
    )
    .expect("manifest file");
}

fn evidence(project_bytes: &[u8]) -> ReductionEvidence {
    let retained_manifest = RetainedManifest::new(vec![RetainedEntry::regular_file(
        "bug.py",
        project_bytes,
        0,
    )
    .expect("retained entry")])
    .expect("retained manifest");
    ReductionEvidence {
        schema_version: EVIDENCE_SCHEMA_VERSION,
        source_root: "fixture".to_owned(),
        source_snapshot_sha256: "1".repeat(64),
        output: "artifact".to_owned(),
        command: vec!["python".to_owned(), "bug.py".to_owned()],
        ecosystem: "python".to_owned(),
        preparation: PreparationEvidence {
            mode: "offline".to_owned(),
            contract_sha256: Some("2".repeat(64)),
            limitations: Vec::new(),
        },
        measurements: MeasurementSet {
            original: MaterialMeasurement {
                files: 2,
                bytes: 200,
                lines: 8,
                syntax_nodes: None,
            },
            retained: MaterialMeasurement {
                files: 1,
                bytes: u64::try_from(project_bytes.len()).expect("fixture length"),
                lines: 1,
                syntax_nodes: None,
            },
            elapsed_ms: 12,
        },
        search: SearchEvidence {
            attempts: 2,
            file_attempts: 2,
            structured_attempts: 0,
            inconclusive_attempts: 0,
            cache_hits: 0,
            baseline_runs: 3,
            final_verifications: 3,
            jobs: 1,
            state: None,
            resumed: false,
            completion: "converged".to_owned(),
            accepted_file_sizes: vec![2, 1],
            evaluation_policy: EvaluationPolicyEvidence {
                mode: "strict".to_owned(),
                runs: 3,
                required: 3,
            },
        },
        failure: FailureEvidence {
            same_failure: true,
            fingerprint_sha256: "a".repeat(64),
            exit_code: Some(1),
            signal: None,
            termination: "exit 1".to_owned(),
            oracle_stream: "stderr".to_owned(),
            oracle_mode: "automatic".to_owned(),
            anchor: "ValueError: sentinel".to_owned(),
            anchors: vec![ChannelAnchor {
                channel: "stderr".to_owned(),
                text: "ValueError: sentinel".to_owned(),
            }],
            normalization_schema: NORMALIZATION_SCHEMA_VERSION,
            failure_patterns: Vec::new(),
            reject_patterns: Vec::new(),
            oracle_spec_sha256: "b".repeat(64),
            diagnostic_drift: Some(no_drift()),
        },
        kept_files: vec![RetentionEvidence {
            path: "bug.py".to_owned(),
            observation: "retained".to_owned(),
        }],
        retained_manifest,
        final_observations: (1..=3)
            .map(|ordinal| FinalObservationEvidence {
                ordinal,
                verdict: "preserved".to_owned(),
                termination: "exit 1".to_owned(),
                exit_code: Some(1),
                signal: None,
                timed_out: false,
                streams_truncated: false,
                containment: "direct_child".to_owned(),
                stdout_sha256: "c".repeat(64),
                stdout_bytes: 0,
                stderr_sha256: "d".repeat(64),
                stderr_bytes: 21,
            })
            .collect(),
        accepted_structured_edits: Vec::new(),
        attempts: vec![attempt(1, "preserved"), attempt(2, "rejected")],
        limitations: vec!["fixture".to_owned()],
    }
}

fn attempt(event_id: u64, verdict: &str) -> AttemptSummary {
    AttemptSummary {
        event_id,
        candidate_sha256: format!("{event_id:064x}"),
        verdict: verdict.to_owned(),
        observed_runs: 3,
        inconclusive_runs: 0,
        completed_at_unix: 1_700_000_000,
        evidence: json!({"decision": verdict}),
    }
}
