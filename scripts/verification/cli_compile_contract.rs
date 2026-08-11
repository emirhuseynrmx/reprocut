#[cfg(test)]
mod cli_compile_contract {
    use std::path::PathBuf;

    use clap::CommandFactory as _;

    use super::{Action, Cli, EcosystemArg, OracleStreamArg, PrepareArg, ReduceArgs};

    #[test]
    fn help_and_resume_surface_compile_together() {
        let mut command = Cli::command();
        let help = command.render_long_help().to_string();
        assert!(help.contains("resume"));
        assert!(help.contains("export"));
        let reduce_help = command
            .find_subcommand_mut("reduce")
            .expect("reduce subcommand")
            .render_long_help()
            .to_string();
        assert!(reduce_help.contains("--jobs"));
        assert!(reduce_help.contains("--state"));

        let arguments = ReduceArgs {
            root: PathBuf::from("project"),
            output: PathBuf::from("output"),
            ecosystem: EcosystemArg::Python,
            prepare: PrepareArg::Offline,
            timeout_ms: 1_000,
            max_output_bytes: 4_096,
            oracle_stream: OracleStreamArg::Auto,
            flaky: false,
            flaky_runs: None,
            flaky_required: None,
            json: true,
            jobs: 4,
            state: Some(PathBuf::from("state.sqlite3")),
            restart: false,
            command: vec!["python".to_owned(), "bug.py".to_owned()],
        };
        assert!(matches!(Action::Resume(arguments), Action::Resume(_)));
    }
}
