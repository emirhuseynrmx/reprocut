#[cfg(test)]
mod python_isolation_contract {
    use std::{ffi::OsString, fs, path::PathBuf, time::Duration};

    use crate::reprocut_adapters::Ecosystem;
    use crate::reprocut_engine::{
        EngineError, PreparationMode, PythonIsolationRequest, PythonPreparationError,
        ReductionEngine, ReductionRequest,
    };

    #[test]
    fn isolation_request_normalizes_extras_and_is_bound_to_the_engine_request() {
        let isolation =
            PythonIsolationRequest::new(PathBuf::from("python"), PathBuf::from("wheelhouse"))
                .with_extras(["Fast_JSON.parser".to_owned(), "fast-json-parser".to_owned()])
                .expect("extras");
        let source = tempfile::tempdir().expect("source");
        fs::write(source.path().join("bug.py"), b"raise RuntimeError('x')").expect("fixture");
        let request = ReductionRequest::new(
            source.path().to_path_buf(),
            PathBuf::from("python"),
            vec![OsString::from("bug.py")],
            Duration::from_secs(5),
            8_192,
        )
        .with_python_isolation(isolation);

        assert_eq!(request.ecosystem(), Ecosystem::Python);
        assert_eq!(request.preparation_mode(), PreparationMode::IsolatedPython);
        assert_eq!(
            request.python_isolation().expect("isolation").extras(),
            ["fast-json-parser"]
        );
        assert!(matches!(
            PythonIsolationRequest::new(PathBuf::from("python"), PathBuf::from("wheels"))
                .with_extras(["../escape".to_owned()]),
            Err(PythonPreparationError::InvalidExtra { .. })
        ));
    }

    #[test]
    fn selecting_isolation_without_inputs_fails_before_a_child_starts() {
        let source = tempfile::tempdir().expect("source");
        fs::write(source.path().join("bug.py"), b"raise RuntimeError('x')").expect("fixture");
        let request = ReductionRequest::new(
            source.path().to_path_buf(),
            PathBuf::from("python"),
            vec![OsString::from("bug.py")],
            Duration::from_secs(5),
            8_192,
        )
        .with_ecosystem(Ecosystem::Python, PreparationMode::IsolatedPython);

        assert!(matches!(
            ReductionEngine::run(&request),
            Err(EngineError::MissingPythonIsolation)
        ));
    }
}
