#[cfg(test)]
mod runner_environment_contract {
    use std::{env, ffi::OsString, path::PathBuf, time::Duration};

    use crate::reprocut_runner::{ChildEnvironment, CommandSpec, ProcessRunner};

    #[test]
    fn child_environment_sets_removes_and_prepends_without_a_shell() {
        let inherited_path = env::var_os("PATH").unwrap_or_default();
        let prepend = env::temp_dir().join("reprocut-contract-bin");
        env::set_var("REPROCUT_REMOVE_ME", "host-secret");
        let environment = ChildEnvironment::inherit()
            .remove("REPROCUT_REMOVE_ME")
            .set("REPROCUT_SET_ME", "candidate-value")
            .prepend_path(prepend.clone());
        let spec = CommandSpec::new(
            env::current_exe().expect("test executable"),
            vec![
                OsString::from("--exact"),
                OsString::from("runner_environment_contract::child_prints_environment"),
                OsString::from("--ignored"),
                OsString::from("--nocapture"),
            ],
            env::current_dir().expect("working directory"),
            Duration::from_secs(5),
            16 * 1_024,
        )
        .with_environment(environment);

        let observation = ProcessRunner::run(&spec).expect("child");
        env::remove_var("REPROCUT_REMOVE_ME");
        let output = String::from_utf8(observation.stdout().to_vec()).expect("UTF-8 output");

        assert!(output.contains("removed=<missing>"));
        assert!(output.contains("set=candidate-value"));
        let path_line = output
            .lines()
            .find_map(|line| line.strip_prefix("path="))
            .expect("PATH line");
        let paths = env::split_paths(&OsString::from(path_line)).collect::<Vec<PathBuf>>();
        assert_eq!(paths.first(), Some(&prepend));
        for inherited in env::split_paths(&inherited_path) {
            assert!(paths.contains(&inherited), "inherited PATH entry was lost");
        }
    }

    #[test]
    #[ignore = "spawned by child_environment_sets_removes_and_prepends_without_a_shell"]
    fn child_prints_environment() {
        println!(
            "removed={}",
            env::var("REPROCUT_REMOVE_ME").unwrap_or_else(|_| "<missing>".to_owned())
        );
        println!(
            "set={}",
            env::var("REPROCUT_SET_ME").unwrap_or_else(|_| "<missing>".to_owned())
        );
        println!("path={}", env::var("PATH").expect("PATH"));
    }
}
