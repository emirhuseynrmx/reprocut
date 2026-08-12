use pyo3::{
    exceptions::PyValueError,
    prelude::*,
    types::{PyAny, PyDict, PyList, PyModule},
};
use reprocut_core::{
    CandidateVerdict, DiagnosticChannel, EvaluationPolicy, ExecutionObservation, FailureOracle,
    OracleMode, OracleSpec, TerminationReason,
};

/// Immutable Python owner for a validated repeated-execution policy.
#[pyclass(name = "EvaluationPolicy", module = "reprocut._native", frozen)]
struct NativeEvaluationPolicy {
    inner: EvaluationPolicy,
}

#[pymethods]
impl NativeEvaluationPolicy {
    /// Return the fail-closed 3-of-3 policy.
    #[staticmethod]
    fn strict() -> Self {
        Self {
            inner: EvaluationPolicy::strict(),
        }
    }

    /// Validate and return a bounded repeated-execution supermajority.
    #[staticmethod]
    #[pyo3(signature = (runs=11, required=9))]
    fn flaky(runs: u16, required: u16) -> PyResult<Self> {
        let inner = EvaluationPolicy::flaky(runs, required)
            .map_err(|error| PyValueError::new_err(error.to_string()))?;
        Ok(Self { inner })
    }

    /// Return `strict` or `flaky`.
    #[getter]
    fn mode(&self) -> &'static str {
        match self.inner {
            EvaluationPolicy::Strict => "strict",
            EvaluationPolicy::Flaky { .. } => "flaky",
        }
    }

    /// Return the maximum run budget.
    #[getter]
    fn runs(&self) -> u16 {
        self.inner.runs()
    }

    /// Return the required preserved count.
    #[getter]
    fn required(&self) -> u16 {
        self.inner.required()
    }
}

/// An immutable Python owner for the Rust failure oracle.
#[pyclass(name = "FailureOracle", module = "reprocut._native", frozen)]
struct NativeFailureOracle {
    inner: FailureOracle,
}

#[pymethods]
impl NativeFailureOracle {
    /// Stabilize one failure from legacy pairs or `(exit_code, stdout, stderr)` triples.
    #[staticmethod]
    #[pyo3(signature = (
        baselines,
        *,
        mode="automatic",
        channel="auto",
        failure_patterns=None,
        reject_patterns=None
    ))]
    fn from_baselines(
        baselines: &Bound<'_, PyAny>,
        mode: &str,
        channel: &str,
        failure_patterns: Option<Vec<String>>,
        reject_patterns: Option<Vec<String>>,
    ) -> PyResult<Self> {
        let observations = extract_observations(baselines)?;
        let spec = OracleSpec::new(
            parse_mode(mode)?,
            parse_channel(channel)?,
            failure_patterns.unwrap_or_default(),
            reject_patterns.unwrap_or_default(),
        )
        .map_err(|error| PyValueError::new_err(error.to_string()))?;
        let inner = FailureOracle::from_spec_and_baselines(spec, &observations)
            .map_err(|error| PyValueError::new_err(error.to_string()))?;
        Ok(Self { inner })
    }

    /// Classify a fresh diagnostic without accepting incomplete evidence.
    #[pyo3(signature = (exit_code, diagnostic, *, stdout="", timed_out=false, truncated=false))]
    fn classify(
        &self,
        exit_code: i32,
        diagnostic: String,
        stdout: String,
        timed_out: bool,
        truncated: bool,
    ) -> &'static str {
        let candidate = observation(exit_code, stdout, diagnostic, timed_out, truncated);
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
        result.set_item("mode", mode_name(fingerprint.mode()))?;
        result.set_item("exit_code", fingerprint.exit_code())?;
        result.set_item("signal", fingerprint.signal())?;
        result.set_item(
            "termination",
            termination_value(py, fingerprint.termination())?,
        )?;
        result.set_item("anchor", fingerprint.anchor())?;
        let anchors = PyList::empty(py);
        for anchor in fingerprint.anchors() {
            let value = PyDict::new(py);
            value.set_item("channel", channel_name(anchor.channel()))?;
            value.set_item("text", anchor.text())?;
            anchors.append(value)?;
        }
        result.set_item("anchors", anchors)?;
        result.set_item("failure_patterns", fingerprint.failure_patterns().to_vec())?;
        result.set_item("reject_patterns", fingerprint.reject_patterns().to_vec())?;
        result.set_item("normalization_schema", fingerprint.normalization_schema())?;
        result.set_item(
            "oracle_spec_sha256",
            fingerprint.oracle_spec_digest().to_hex(),
        )?;
        result.set_item("fingerprint_sha256", fingerprint.digest().to_hex())?;
        Ok(result)
    }
}

fn parse_mode(value: &str) -> PyResult<OracleMode> {
    match value {
        "automatic" => Ok(OracleMode::Automatic),
        "regex" => Ok(OracleMode::Regex),
        "exit_zero" => Ok(OracleMode::ExitZero),
        _ => Err(PyValueError::new_err(format!(
            "unsupported oracle mode: {value}"
        ))),
    }
}

const fn mode_name(mode: OracleMode) -> &'static str {
    match mode {
        OracleMode::Automatic => "automatic",
        OracleMode::Regex => "regex",
        OracleMode::ExitZero => "exit_zero",
    }
}

fn extract_observations(baselines: &Bound<'_, PyAny>) -> PyResult<Vec<ExecutionObservation>> {
    if let Ok(values) = baselines.extract::<Vec<(i32, String, String)>>() {
        return Ok(values
            .into_iter()
            .map(|(exit_code, stdout, stderr)| observation(exit_code, stdout, stderr, false, false))
            .collect());
    }
    let values = baselines.extract::<Vec<(i32, String)>>().map_err(|_| {
        PyValueError::new_err(
            "baselines must contain (exit_code, diagnostic) pairs or (exit_code, stdout, stderr) triples",
        )
    })?;
    Ok(values
        .into_iter()
        .map(|(exit_code, diagnostic)| {
            observation(exit_code, String::new(), diagnostic, false, false)
        })
        .collect())
}

fn parse_channel(value: &str) -> PyResult<DiagnosticChannel> {
    match value {
        "auto" => Ok(DiagnosticChannel::Auto),
        "stderr" => Ok(DiagnosticChannel::Stderr),
        "stdout" => Ok(DiagnosticChannel::Stdout),
        "combined" => Ok(DiagnosticChannel::Combined),
        _ => Err(PyValueError::new_err(format!(
            "unsupported diagnostic channel: {value}"
        ))),
    }
}

const fn channel_name(channel: DiagnosticChannel) -> &'static str {
    match channel {
        DiagnosticChannel::Auto => "auto",
        DiagnosticChannel::Stderr => "stderr",
        DiagnosticChannel::Stdout => "stdout",
        DiagnosticChannel::Combined => "combined",
    }
}

fn termination_value<'py>(
    py: Python<'py>,
    termination: TerminationReason,
) -> PyResult<Bound<'py, PyDict>> {
    let value = PyDict::new(py);
    match termination {
        TerminationReason::ExitCode(code) => {
            value.set_item("kind", "exit_code")?;
            value.set_item("value", code)?;
        }
        TerminationReason::UnixSignal(signal) => {
            value.set_item("kind", "unix_signal")?;
            value.set_item("value", signal)?;
        }
        TerminationReason::TimedOut => value.set_item("kind", "timed_out")?,
        TerminationReason::RunnerFailure => value.set_item("kind", "runner_failure")?,
    }
    Ok(value)
}

fn observation(
    exit_code: i32,
    stdout: String,
    stderr: String,
    timed_out: bool,
    truncated: bool,
) -> ExecutionObservation {
    ExecutionObservation::new(
        Some(exit_code),
        None,
        stdout.into_bytes(),
        stderr.into_bytes(),
        timed_out,
        truncated,
    )
}

#[pymodule]
#[pyo3(name = "_native")]
fn native(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<NativeEvaluationPolicy>()?;
    module.add_class::<NativeFailureOracle>()?;
    module.add("__version__", env!("CARGO_PKG_VERSION"))?;
    Ok(())
}
