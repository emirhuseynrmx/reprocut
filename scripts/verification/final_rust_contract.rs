#[cfg(test)]
mod final_rust_contract {
    use std::{ffi::OsString, fs, path::PathBuf, time::Duration};

    use super::reprocut_core::{
        reduce, CandidateVerdict, ExecutionObservation, FailureOracle, ReductionUnit,
    };
    use super::reprocut_runner::{CommandSpec, ProcessRunner};
    use super::reprocut_workspace::{CandidateWorkspace, ProjectInventory};

    #[test]
    fn exhaustive_small_universes_reduce_to_exactly_the_required_set() {
        let units = (0_u32..8)
            .map(|id| ReductionUnit::new(id, format!("unit-{id}.txt")))
            .collect::<Vec<_>>();

        for required_mask in 1_u16..(1_u16 << 8) {
            let result = reduce(&units, |candidate| {
                let preserves = (0_u32..8).all(|id| {
                    required_mask & (1_u16 << id) == 0
                        || candidate.iter().any(|unit| unit.id() == id)
                });
                if preserves {
                    CandidateVerdict::Preserved
                } else {
                    CandidateVerdict::Rejected
                }
            });
            let kept_mask = result
                .kept()
                .iter()
                .fold(0_u16, |mask, unit| mask | (1_u16 << unit.id()));
            assert_eq!(kept_mask, required_mask);
        }
    }

    #[test]
    fn oracle_rejects_wrong_and_incomplete_evidence() {
        let baseline = observation(1, "TypeError: request at /tmp/alpha.py", false, false);
        let second = observation(1, "TypeError: request at /var/tmp/beta.py", false, false);
        let oracle =
            FailureOracle::from_baselines(&[baseline, second]).expect("baseline is stable");

        assert_eq!(
            oracle.classify(&observation(
                1,
                "TypeError: request at /tmp/gamma.py",
                false,
                false,
            )),
            CandidateVerdict::Preserved
        );
        assert_eq!(
            oracle.classify(&observation(
                2,
                "TypeError: request at /tmp/x.py",
                false,
                false
            )),
            CandidateVerdict::Rejected
        );
        assert_eq!(
            oracle.classify(&observation(
                1,
                "TypeError: request at /tmp/x.py",
                true,
                false
            )),
            CandidateVerdict::Inconclusive
        );
    }

    #[test]
    fn process_capture_is_bounded_and_timeout_reaps_the_child() {
        let sandbox = tempfile::tempdir().expect("sandbox created");
        let payload = "A".repeat(256);
        let output_command = CommandSpec::new(
            PathBuf::from("/bin/sh"),
            vec![
                OsString::from("-c"),
                OsString::from(format!("printf '%s' '{payload}' >&2; exit 7")),
            ],
            sandbox.path().to_path_buf(),
            Duration::from_secs(2),
            32,
        );
        let bounded = ProcessRunner::run(&output_command).expect("child executes");
        assert_eq!(bounded.exit_code(), Some(7));
        assert_eq!(bounded.stderr().len(), 32);
        assert!(bounded.streams_truncated());

        let timeout_command = CommandSpec::new(
            PathBuf::from("/bin/sh"),
            vec![OsString::from("-c"), OsString::from("sleep 1")],
            sandbox.path().to_path_buf(),
            Duration::from_millis(20),
            32,
        );
        let timed_out = ProcessRunner::run(&timeout_command).expect("timed-out child is reaped");
        assert!(timed_out.timed_out());
    }

    #[test]
    fn disposable_workspace_never_removes_source_material() {
        let source = tempfile::tempdir().expect("source created");
        fs::create_dir(source.path().join("nested")).expect("nested source created");
        fs::write(source.path().join("bug.txt"), "required").expect("bug written");
        fs::write(source.path().join("nested/noise.txt"), "removable").expect("noise written");
        let inventory = ProjectInventory::scan(source.path()).expect("inventory succeeds");
        let bug = inventory
            .units()
            .iter()
            .find(|unit| unit.path() == "bug.txt")
            .expect("bug inventoried");
        let candidate =
            CandidateWorkspace::materialize(&inventory, &[bug]).expect("candidate materializes");

        assert!(candidate.root().join("bug.txt").is_file());
        assert!(!candidate.root().join("nested/noise.txt").exists());
        assert!(source.path().join("nested/noise.txt").is_file());
    }

    fn observation(
        exit_code: i32,
        diagnostic: &str,
        timed_out: bool,
        truncated: bool,
    ) -> ExecutionObservation {
        ExecutionObservation::new(
            Some(exit_code),
            None,
            Vec::new(),
            diagnostic.as_bytes().to_vec(),
            timed_out,
            truncated,
        )
    }
}
