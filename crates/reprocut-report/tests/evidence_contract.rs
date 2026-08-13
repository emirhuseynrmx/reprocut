//! Versioned reduction-evidence contracts.

use reprocut_report::{
    write_attempts_jsonl, AttemptSummary, ChannelAnchor, EvaluationPolicyEvidence, FailureEvidence,
    MaterialMeasurement, MeasurementSet, PreparationEvidence, ReductionEvidence, RetentionEvidence,
    SearchEvidence, EVIDENCE_SCHEMA_VERSION, NORMALIZATION_SCHEMA_VERSION,
};
use serde_json::json;

#[test]
fn one_model_serializes_consistent_measurements_and_attempts() {
    let evidence = fixture();
    let value = serde_json::to_value(&evidence).expect("evidence JSON");

    assert_eq!(value["schema_version"], EVIDENCE_SCHEMA_VERSION);
    assert_eq!(value["measurements"]["original"]["bytes"], 4_096);
    assert_eq!(value["measurements"]["retained"]["bytes"], 512);
    assert_eq!(value["search"]["attempts"], 41);
    assert_eq!(value["failure"]["same_failure"], true);
    evidence.validate().expect("current evidence is valid");

    let mut jsonl = Vec::new();
    write_attempts_jsonl(&evidence.attempts, &mut jsonl).expect("attempt JSONL");
    let lines = String::from_utf8(jsonl).expect("UTF-8 JSONL");
    assert_eq!(lines.lines().count(), evidence.attempts.len());
    assert!(lines.ends_with('\n'));
    for line in lines.lines() {
        let _: AttemptSummary = serde_json::from_str(line).expect("one complete JSON value");
    }
}

#[test]
fn display_command_is_derived_from_argv_without_shell_authority() {
    let evidence = fixture();
    assert_eq!(
        evidence.display_command(),
        "python bug.py --case \"two words\""
    );
}

fn fixture() -> ReductionEvidence {
    ReductionEvidence {
        schema_version: EVIDENCE_SCHEMA_VERSION,
        source_root: "fixture".to_owned(),
        source_snapshot_sha256: "1".repeat(64),
        output: "minimal".to_owned(),
        command: vec![
            "python".to_owned(),
            "bug.py".to_owned(),
            "--case".to_owned(),
            "two words".to_owned(),
        ],
        ecosystem: "python".to_owned(),
        preparation: PreparationEvidence {
            mode: "offline".to_owned(),
            contract_sha256: Some("2".repeat(64)),
            limitations: Vec::new(),
        },
        measurements: MeasurementSet {
            original: MaterialMeasurement {
                files: 18,
                bytes: 4_096,
                lines: 300,
                syntax_nodes: Some(900),
            },
            retained: MaterialMeasurement {
                files: 3,
                bytes: 512,
                lines: 40,
                syntax_nodes: Some(100),
            },
            elapsed_ms: 4_200,
        },
        search: SearchEvidence {
            attempts: 41,
            file_attempts: 31,
            structured_attempts: 10,
            inconclusive_attempts: 2,
            cache_hits: 7,
            baseline_runs: 3,
            final_verifications: 3,
            jobs: 4,
            state: Some("state.sqlite3".to_owned()),
            resumed: false,
            accepted_file_sizes: vec![18, 9, 5, 3],
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
        },
        kept_files: vec![RetentionEvidence {
            path: "bug.py".to_owned(),
            observation: "Present in the final verified snapshot.".to_owned(),
        }],
        accepted_structured_edits: vec!["syntax:delete:bug.py:0..10".to_owned()],
        attempts: vec![AttemptSummary {
            event_id: 1,
            candidate_sha256: "def456".to_owned(),
            verdict: "preserved".to_owned(),
            observed_runs: 3,
            inconclusive_runs: 0,
            completed_at_unix: 1_700_000_000,
            evidence: json!({"decision": "preserved"}),
        }],
        limitations: vec!["Timing is wall-clock, not a benchmark.".to_owned()],
    }
}
