#[cfg(test)]
mod cli_remote_contract {
    use std::{fs, path::Path, time::{SystemTime, UNIX_EPOCH}};

    use clap::CommandFactory as _;

    use super::{Action, Cli, CliError, ReduceArgs, execute};

    fn sandbox(label: &str) -> std::path::PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock follows the Unix epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("reprocut-{label}-{nonce}"));
        fs::create_dir(&path).expect("sandbox created");
        path
    }

    fn reduction_args(root: &Path, output: &Path) -> ReduceArgs {
        ReduceArgs {
            root: root.to_path_buf(),
            output: output.to_path_buf(),
            timeout_ms: 3_000,
            max_output_bytes: 64 * 1024,
            json: false,
            command: vec!["/bin/sh".to_owned(), "bug.sh".to_owned()],
        }
    }

    #[test]
    fn help_names_the_job_and_real_invocation() {
        let help = Cli::command().render_long_help().to_string();
        assert!(help.contains("Shrink a failing project"));
        assert!(help.contains("reprocut reduce"));
        assert!(help.contains("reduce"));
    }

    #[test]
    fn real_failure_is_reduced_and_published_without_touching_source() {
        let sandbox = sandbox("publish");
        let source = sandbox.join("source");
        let output = sandbox.join("minimal");
        fs::create_dir(&source).expect("source created");
        fs::write(source.join("bug.sh"), "#!/bin/sh\necho 'ValueError: stable bug' >&2\nexit 1\n")
            .expect("bug fixture written");
        fs::write(source.join("noise.txt"), "removable").expect("noise written");
        fs::create_dir(source.join("nested")).expect("nested directory created");
        fs::write(source.join("nested/unused.txt"), "also removable").expect("nested noise written");

        execute(Cli { action: Action::Reduce(reduction_args(&source, &output)) })
            .expect("reduction succeeds");

        assert!(source.join("noise.txt").is_file());
        assert!(output.join("project/bug.sh").is_file());
        assert!(!output.join("project/noise.txt").exists());
        assert!(output.join("report.html").is_file());
        assert!(output.join("reduction.json").is_file());
        assert!(output.join("reproduce.sh").is_file());
        assert!(output.join("reproduce.ps1").is_file());
        let state = fs::read_to_string(output.join("reduction.json")).expect("state readable");
        assert!(state.contains("\"schema_version\": 1"));
        assert!(state.contains("\"retained_files\": 1"));

        fs::remove_dir_all(sandbox).expect("sandbox removed");
    }

    #[test]
    fn existing_output_is_rejected_without_mutation() {
        let sandbox = sandbox("noclobber");
        let source = sandbox.join("source");
        let output = sandbox.join("owned");
        fs::create_dir(&source).expect("source created");
        fs::write(source.join("bug.sh"), "exit 1\n").expect("fixture written");
        fs::create_dir(&output).expect("output created");
        fs::write(output.join("marker.txt"), "user data").expect("marker written");

        let error = execute(Cli { action: Action::Reduce(reduction_args(&source, &output)) })
            .expect_err("existing output must fail");
        assert!(matches!(error, CliError::OutputExists(_)));
        assert_eq!(
            fs::read_to_string(output.join("marker.txt")).expect("marker remains readable"),
            "user data"
        );

        fs::remove_dir_all(sandbox).expect("sandbox removed");
    }
}
