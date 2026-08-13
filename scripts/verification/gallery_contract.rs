#[cfg(test)]
mod gallery_contract {
    use super::*;

    #[test]
    fn redacted_gallery_submission_is_local_atomic_and_schema_bounded() {
        let sandbox = tempfile::tempdir().expect("sandbox");
        let artifact = sandbox.path().join("artifact");
        let output = sandbox.path().join("submission");
        fs::create_dir(&artifact).expect("artifact");
        let evidence = ReductionEvidence {
            schema_version: EVIDENCE_SCHEMA_VERSION,
            source_root: "/private/source".to_owned(),
            source_snapshot_sha256: "c".repeat(64),
            output: "/private/output".to_owned(),
            command: vec!["python".to_owned(), "--token=private".to_owned()],
            ecosystem: "python".to_owned(),
            preparation: PreparationEvidence {
                mode: "offline".to_owned(),
                contract_sha256: Some("d".repeat(64)),
                limitations: Vec::new(),
            },
            measurements: MeasurementSet {
                original: MaterialMeasurement {
                    files: 18,
                    bytes: 4_096,
                    lines: 300,
                    syntax_nodes: None,
                },
                retained: MaterialMeasurement {
                    files: 3,
                    bytes: 512,
                    lines: 40,
                    syntax_nodes: None,
                },
                elapsed_ms: 4_200,
            },
            search: SearchEvidence {
                attempts: 19,
                file_attempts: 19,
                structured_attempts: 0,
                inconclusive_attempts: 0,
                cache_hits: 5,
                baseline_runs: 3,
                final_verifications: 3,
                jobs: 1,
                state: Some("/private/state.sqlite3".to_owned()),
                resumed: false,
                accepted_file_sizes: vec![18, 9, 3],
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
                anchor: "private diagnostic".to_owned(),
                anchors: vec![ChannelAnchor {
                    channel: "stderr".to_owned(),
                    text: "private diagnostic".to_owned(),
                }],
                normalization_schema: 4,
                failure_patterns: Vec::new(),
                reject_patterns: Vec::new(),
                oracle_spec_sha256: "b".repeat(64),
            },
            kept_files: Vec::new(),
            accepted_structured_edits: Vec::new(),
            attempts: Vec::new(),
            limitations: Vec::new(),
        };
        fs::write(
            artifact.join("reduction.json"),
            serde_json::to_vec(&evidence).expect("JSON"),
        )
        .expect("evidence");

        prepare_gallery(GalleryPrepareArgs {
            from: artifact,
            output: output.clone(),
            title: "Decimal mismatch".to_owned(),
            license: "MIT".to_owned(),
            include_source: false,
        })
        .expect("prepare");

        let entry = fs::read_to_string(output.join("entry.json")).expect("entry");
        assert!(!entry.contains("private"));
        assert!(!entry.contains("command"));
        assert!(output.join("index.html").is_file());
        assert!(!output.join("source").exists());
    }
}
