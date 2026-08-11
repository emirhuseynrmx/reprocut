#[cfg(all(test, unix))]
mod process_group_contract {
    use std::{
        ffi::OsString,
        fs,
        path::PathBuf,
        thread,
        time::{Duration, SystemTime},
    };

    use super::reprocut_core::ContainmentMechanism;
    use super::reprocut_runner::{containment_mechanism, CommandSpec, ProcessRunner};

    #[test]
    fn timeout_owns_and_terminates_the_descendant_group() {
        let marker = unique_marker();
        let script = r#"(sleep 0.20; printf survived > "$1") & sleep 5"#;
        let spec = CommandSpec::new(
            PathBuf::from("/bin/sh"),
            vec![
                OsString::from("-c"),
                OsString::from(script),
                OsString::from("reprocut-fixture"),
                marker.clone().into_os_string(),
            ],
            std::env::current_dir().expect("working directory"),
            Duration::from_millis(40),
            1_024,
        );

        let observation = ProcessRunner::run(&spec).expect("contained command");
        assert!(observation.timed_out());
        assert_eq!(
            containment_mechanism(),
            ContainmentMechanism::PosixProcessGroup
        );
        thread::sleep(Duration::from_millis(350));
        assert!(!marker.exists(), "descendant escaped its process group");
        let _ = fs::remove_file(marker);
    }

    fn unique_marker() -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        std::env::temp_dir().join(format!("reprocut-descendant-{nonce}"))
    }
}
