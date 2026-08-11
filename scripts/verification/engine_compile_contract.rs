#[cfg(test)]
mod engine_compile_contract {
    use std::{path::PathBuf, time::Duration};

    use crate::reprocut_engine::{ReductionRequest, SessionMode};

    #[test]
    fn runtime_policy_is_explicit_and_zero_jobs_is_preserved() {
        let request = ReductionRequest::new(
            PathBuf::from("project"),
            PathBuf::from("python"),
            Vec::new(),
            Duration::from_secs(1),
            1024,
        )
        .with_runtime(0, SessionMode::Resume(PathBuf::from("state.sqlite3")));

        assert_eq!(request.jobs(), 0);
        assert_eq!(
            request.session_mode(),
            &SessionMode::Resume(PathBuf::from("state.sqlite3"))
        );
    }
}
