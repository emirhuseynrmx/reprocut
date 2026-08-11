use pyo3::{
    exceptions::PyValueError,
    prelude::*,
    types::{PyDict, PyModule},
};
use reprocut_core::{CandidateVerdict, ExecutionObservation, FailureOracle};

/// An immutable Python owner for the Rust failure oracle.
#[pyclass(name = "FailureOracle", module = "reprocut._native", frozen)]
struct NativeFailureOracle {
    inner: FailureOracle,
}

#[pymethods]
impl NativeFailureOracle {
    /// Stabilize one failure from repeated `(exit_code, diagnostic)` samples.
    #[staticmethod]
    fn from_baselines(baselines: Vec<(i32, String)>) -> PyResult<Self> {
        let observations = baselines
            .into_iter()
            .map(|(exit_code, diagnostic)| observation(exit_code, diagnostic, false, false))
            .collect::<Vec<_>>();
        let inner = FailureOracle::from_baselines(&observations)
            .map_err(|error| PyValueError::new_err(error.to_string()))?;
        Ok(Self { inner })
    }

    /// Classify a fresh diagnostic without accepting incomplete evidence.
    #[pyo3(signature = (exit_code, diagnostic, *, timed_out=false, truncated=false))]
    fn classify(
        &self,
        exit_code: i32,
        diagnostic: String,
        timed_out: bool,
        truncated: bool,
    ) -> &'static str {
        let candidate = observation(exit_code, diagnostic, timed_out, truncated);
        match self.inner.classify(&candidate) {
            CandidateVerdict::Preserved => "preserved",
            CandidateVerdict::Rejected => "rejected",
            CandidateVerdict::Inconclusive => "inconclusive",
        }
    }

    /// Return a detached plain-Python copy of the stabilized fingerprint.
    #[getter]
    fn fingerprint<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        let fingerprint = self.inner.fingerprint();
        let result = PyDict::new(py);
        result.set_item("exit_code", fingerprint.exit_code())?;
        result.set_item("signal", fingerprint.signal())?;
        result.set_item("anchor", fingerprint.anchor())?;
        Ok(result)
    }
}

fn observation(
    exit_code: i32,
    diagnostic: String,
    timed_out: bool,
    truncated: bool,
) -> ExecutionObservation {
    ExecutionObservation::new(
        Some(exit_code),
        None,
        Vec::new(),
        diagnostic.into_bytes(),
        timed_out,
        truncated,
    )
}

#[pymodule]
#[pyo3(name = "_native")]
fn native(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<NativeFailureOracle>()?;
    module.add("__version__", env!("CARGO_PKG_VERSION"))?;
    Ok(())
}
