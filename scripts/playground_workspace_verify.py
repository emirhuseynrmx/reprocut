#!/usr/bin/env python3
"""Compile the complete ReproCut Rust workspace as one Playground crate."""

from __future__ import annotations

import argparse
import json
import re
import sys
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
INCLUDE_STR = re.compile(r'include_str!\(\s*"([^"]+)"\s*\)')


def read(path: str) -> str:
    source_path = ROOT / path
    source = source_path.read_text(encoding="utf-8")

    def replace(match: re.Match[str]) -> str:
        asset = (source_path.parent / match.group(1)).resolve().read_text(encoding="utf-8")
        fence = "####"
        while f'"{fence}' in asset:
            fence += "#"
        return f'r{fence}"{asset}"{fence}'

    return INCLUDE_STR.sub(replace, source)


def without_inner_attributes(source: str) -> str:
    return "\n".join(
        line
        for line in source.splitlines()
        if not line.startswith("#![") and not line.startswith("//!")
    )


def wrap(name: str, source: str) -> str:
    return f"mod {name} {{\n{without_inner_attributes(source)}\n}}\n"


def workspace_source() -> str:
    hierarchy = read("crates/reprocut-workspace/src/hierarchy.rs")
    snapshot = read("crates/reprocut-workspace/src/snapshot.rs")
    return (
        read("crates/reprocut-workspace/src/lib.rs")
        .replace("mod hierarchy;", f"mod hierarchy {{ {hierarchy} }}")
        .replace("mod snapshot;", f"mod snapshot {{ {snapshot} }}")
    )


def report_source() -> str:
    evidence = read("crates/reprocut-report/src/evidence.rs").replace(
        "reprocut_core::", "crate::reprocut_core::"
    ).replace(
        "use crate::{RetainedEntryKind, RetainedManifest};",
        "use super::{RetainedEntryKind, RetainedManifest};",
    )
    manifest = read("crates/reprocut-report/src/manifest.rs").replace(
        "use reprocut_core::", "use crate::reprocut_core::"
    )
    issue = read("crates/reprocut-report/src/issue.rs").replace(
        "use crate::ReductionEvidence;", "use super::ReductionEvidence;"
    )
    verify = (
        read("crates/reprocut-report/src/verify.rs")
        .replace("use reprocut_core::", "use crate::reprocut_core::")
        .replace("use crate::{", "use super::{", 1)
    )
    return (
        read("crates/reprocut-report/src/lib.rs")
        .replace("mod evidence;", f"mod evidence {{ {evidence} }}")
        .replace("mod issue;", f"mod issue {{ {issue} }}")
        .replace("mod manifest;", f"mod manifest {{ {manifest} }}")
        .replace("mod verify;", f"mod verify {{ {verify} }}")
    )


def compose_cli(
    *,
    runner_override: str | None = None,
    python_isolation_override: str | None = None,
) -> str:
    engine = compose_engine(
        runner_override=runner_override,
        python_isolation_override=python_isolation_override,
    ).removesuffix("fn main() {}")
    report = report_source()
    oci = read("crates/reprocut-oci/src/lib.rs")
    completion_stub = r"""
use std::io::Write;
#[derive(Clone, Copy)]
pub enum Shell { Bash, Elvish, Fish, PowerShell, Zsh }
pub fn generate(_: Shell, _: &mut clap::Command, _: &str, output: &mut dyn Write) {
    let _ = output.write_all(b"playground completion stub\n");
}
"""
    cli = (
        without_inner_attributes(read("crates/reprocut-cli/src/main.rs"))
        .replace("use reprocut_adapters::", "use crate::reprocut_adapters::")
        .replace("use reprocut_core::", "use crate::reprocut_core::")
        .replace("use reprocut_engine::", "use crate::reprocut_engine::")
        .replace("use reprocut_oci::", "use crate::reprocut_oci::")
        .replace("use reprocut_report::", "use crate::reprocut_report::")
        .replace("use reprocut_workspace::", "use crate::reprocut_workspace::")
    )
    return "\n".join(
        [
            engine,
            wrap("reprocut_oci", oci),
            wrap("reprocut_report", report),
            wrap("clap_complete", completion_stub),
            cli,
        ]
    )


def compose_report() -> str:
    core = compose_core().replace("fn main() {}\n", "")
    return "\n".join([core, wrap("reprocut_report", report_source()), "fn main() {}"])


def compose_oci() -> str:
    return "\n".join([wrap("reprocut_oci", read("crates/reprocut-oci/src/lib.rs")), "fn main() {}"])


def compose_core() -> str:
    schema = read("crates/reprocut-core/src/schema.rs")
    diagnostic = read("crates/reprocut-core/src/diagnostic.rs").replace(
        "use crate::{", "use super::{", 1
    )
    model = (
        read("crates/reprocut-core/src/model.rs")
        .replace("crate::NORMALIZATION_SCHEMA", "super::NORMALIZATION_SCHEMA")
        .replace("use crate::transformation::", "use super::transformation::")
    )
    oracle = (
        read("crates/reprocut-core/src/oracle.rs")
        .replace("use crate::{", "use super::{", 1)
        .replace("crate::NORMALIZATION_SCHEMA", "super::NORMALIZATION_SCHEMA")
    )
    policy = read("crates/reprocut-core/src/policy.rs").replace(
        "use crate::CandidateVerdict;", "use super::CandidateVerdict;"
    )
    protocol = read("crates/reprocut-core/src/protocol.rs")
    reducer = read("crates/reprocut-core/src/reducer.rs").replace(
        "use crate::{CandidateVerdict, FrontierClass};",
        "use super::{CandidateVerdict, FrontierClass};",
    )
    winner = read("crates/reprocut-core/src/winner.rs")
    transformation_path = ROOT / "crates/reprocut-core/src/transformation.rs"
    transformation = (
        f"mod transformation {{ {read('crates/reprocut-core/src/transformation.rs')} }}\n"
        "pub use transformation::*;"
        if transformation_path.exists()
        else ""
    )
    return f"""#![forbid(unsafe_code)]
mod reprocut_core {{
mod diagnostic {{ {diagnostic} }}
mod model {{ {model} }}
mod oracle {{ {oracle} }}
mod policy {{ {policy} }}
mod protocol {{ {protocol} }}
mod reducer {{ {reducer} }}
mod schema {{ {schema} }}
mod winner {{ {winner} }}
{transformation}
pub use model::*;
pub use diagnostic::*;
pub use oracle::*;
pub use policy::*;
pub use protocol::*;
pub use reducer::*;
pub use schema::*;
pub use winner::*;
}}
fn main() {{}}
"""


def compose_workspace() -> str:
    core = compose_core().replace("fn main() {}\n", "")
    workspace = workspace_source().replace("use reprocut_core::", "use crate::reprocut_core::")
    return "\n".join([core, wrap("reprocut_workspace", workspace), "fn main() {}"])


def compose_state() -> str:
    core = compose_core().replace("fn main() {}\n", "")
    schema = read("crates/reprocut-state/src/schema.rs").replace(
        "reprocut_core::", "crate::reprocut_core::"
    )
    state = (
        read("crates/reprocut-state/src/lib.rs")
        .replace("mod schema;", f"mod schema {{ {schema} }}")
        .replace("use reprocut_core::", "use crate::reprocut_core::")
    )
    return "\n".join([core, wrap("reprocut_state", state), "fn main() {}"])


def compose_scheduler() -> str:
    core = compose_core().replace("fn main() {}\n", "")
    scheduler = read("crates/reprocut-engine/src/scheduler.rs").replace(
        "use reprocut_core::", "use crate::reprocut_core::"
    )
    return "\n".join([core, wrap("reprocut_engine", scheduler), "fn main() {}"])


def compose_runner() -> str:
    core = compose_core().replace("fn main() {}\n", "")
    command_group = r"""
use std::{io, process::{Child, Command, ExitStatus}};
pub trait CommandGroup { fn group_spawn(&mut self) -> io::Result<GroupChild>; }
impl CommandGroup for Command {
    fn group_spawn(&mut self) -> io::Result<GroupChild> { self.spawn().map(GroupChild) }
}
pub struct GroupChild(Child);
impl GroupChild {
    pub fn inner(&mut self) -> &mut Child { &mut self.0 }
    pub fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> { self.0.try_wait() }
    pub fn kill(&mut self) -> io::Result<()> { self.0.kill() }
    pub fn wait(&mut self) -> io::Result<ExitStatus> { self.0.wait() }
}
"""
    runner = (
        read("crates/reprocut-runner/src/lib.rs")
        .replace("use reprocut_core::", "use crate::reprocut_core::")
        .replace("use command_group::", "use crate::command_group::")
    )
    return "\n".join(
        [
            core,
            wrap("command_group", command_group),
            wrap("reprocut_runner", runner),
            "fn main() {}",
        ]
    )


def compose_engine(
    *,
    runner_override: str | None = None,
    python_isolation_override: str | None = None,
) -> str:
    core = compose_core().replace("fn main() {}\n", "")
    workspace = workspace_source().replace("use reprocut_core::", "use crate::reprocut_core::")
    schema = read("crates/reprocut-state/src/schema.rs").replace(
        "reprocut_core::", "crate::reprocut_core::"
    )
    state = (
        read("crates/reprocut-state/src/lib.rs")
        .replace("mod schema;", f"mod schema {{ {schema} }}")
        .replace("use reprocut_core::", "use crate::reprocut_core::")
    )
    scheduler = read("crates/reprocut-engine/src/scheduler.rs").replace(
        "use reprocut_core::", "use crate::reprocut_core::"
    )
    python_isolation = python_isolation_override or (
        read("crates/reprocut-engine/src/python_isolation.rs")
        .replace("use reprocut_core::", "use crate::reprocut_core::")
        .replace("use reprocut_runner::", "use crate::reprocut_runner::")
        .replace(
            "reprocut_core::ExecutionObservation", "crate::reprocut_core::ExecutionObservation"
        )
    )
    pipeline = r"""
use crate::reprocut_adapters::{Ecosystem, PreparationPlan};
use crate::reprocut_workspace::ProjectSnapshot;
use super::PreparationMode;
use thiserror::Error;
#[derive(Clone, Debug)]
pub(crate) struct StructuredCandidate;
impl StructuredCandidate {
    pub(crate) fn key(&self) -> &str { "stub" }
    pub(crate) fn snapshot(&self) -> &ProjectSnapshot { panic!("compile-only stub") }
    pub(crate) fn preparation(&self) -> Option<&PreparationPlan> { None }
    pub(crate) fn capture_paths(&self) -> &'static [&'static str] { &[] }
}
#[derive(Clone, Copy)]
pub(crate) enum SyntaxPhase { Delete, Hoist }
#[derive(Debug, Error)]
#[error("compile-only pipeline stub")]
pub(crate) struct PipelineError;
pub(crate) fn manifest_candidates(
    _: &ProjectSnapshot, _: Ecosystem, _: PreparationMode,
) -> Result<Vec<StructuredCandidate>, PipelineError> { Ok(Vec::new()) }
pub(crate) fn syntax_candidates(
    _: &ProjectSnapshot, _: SyntaxPhase,
) -> Result<Vec<StructuredCandidate>, PipelineError> { Ok(Vec::new()) }
"""
    discovery = read("crates/reprocut-adapters/src/discovery.rs").replace(
        "use reprocut_workspace::", "use crate::reprocut_workspace::"
    )
    manifests = read("crates/reprocut-adapters/src/manifests.rs").replace(
        "use crate::AdapterCommand;", "use super::AdapterCommand;"
    )
    adapters = (
        read("crates/reprocut-adapters/src/lib.rs")
        .replace("mod discovery;", f"mod discovery {{ {discovery} }}")
        .replace("mod manifests;", f"mod manifests {{ {manifests} }}")
        .replace("pub use reprocut_workspace::", "pub use crate::reprocut_workspace::")
    )
    runner = (
        runner_override
        or r"""
use std::{ffi::{OsStr, OsString}, path::{Path, PathBuf}, time::Duration};
use crate::reprocut_core::ExecutionObservation;
use thiserror::Error;
#[derive(Debug, Error)]
#[error("Playground engine compile stub")]
pub struct RunnerError;
#[derive(Clone, Debug, Default)]
pub struct ChildEnvironment;
impl ChildEnvironment {
    pub fn inherit() -> Self { Self }
    pub fn remove(self, _: impl AsRef<OsStr>) -> Self { self }
    pub fn set(self, _: impl AsRef<OsStr>, _: impl AsRef<OsStr>) -> Self { self }
    pub fn prepend_path(self, _: impl AsRef<Path>) -> Self { self }
}
pub struct CommandSpec;
impl CommandSpec {
    pub fn new(_: PathBuf, _: Vec<OsString>, _: PathBuf, _: Duration, _: usize) -> Self { Self }
    pub fn with_environment(self, _: ChildEnvironment) -> Self { self }
}
pub struct ProcessRunner;
impl ProcessRunner {
    pub fn run(_: &CommandSpec) -> Result<ExecutionObservation, RunnerError> { Err(RunnerError) }
}
"""
    )
    engine = (
        read("crates/reprocut-engine/src/lib.rs")
        .replace("mod scheduler;", f"mod scheduler {{ {scheduler} }}")
        .replace("mod pipeline;", f"mod pipeline {{ {pipeline} }}")
        .replace("mod python_isolation;", f"mod python_isolation {{ {python_isolation} }}")
        .replace("use reprocut_adapters::", "use crate::reprocut_adapters::")
        .replace("use reprocut_core::", "use crate::reprocut_core::")
        .replace("use reprocut_runner::", "use crate::reprocut_runner::")
        .replace("use reprocut_state::", "use crate::reprocut_state::")
        .replace("use reprocut_workspace::", "use crate::reprocut_workspace::")
        .replace(
            "Result<reprocut_core::ExecutionObservation, EngineError>",
            "Result<crate::reprocut_core::ExecutionObservation, EngineError>",
        )
    )
    return "\n".join(
        [
            core,
            wrap("reprocut_workspace", workspace),
            wrap("reprocut_adapters", adapters),
            wrap("reprocut_state", state),
            wrap("reprocut_runner", runner),
            wrap("reprocut_engine", engine),
            "fn main() {}",
        ]
    )


def compose_adapters() -> str:
    core = compose_core().replace("fn main() {}\n", "")
    workspace = workspace_source().replace("use reprocut_core::", "use crate::reprocut_core::")
    discovery = read("crates/reprocut-adapters/src/discovery.rs").replace(
        "use reprocut_workspace::", "use crate::reprocut_workspace::"
    )
    manifests = read("crates/reprocut-adapters/src/manifests.rs").replace(
        "use crate::AdapterCommand;", "use super::AdapterCommand;"
    )
    adapters = (
        read("crates/reprocut-adapters/src/lib.rs")
        .replace("mod discovery;", f"mod discovery {{ {discovery} }}")
        .replace("mod manifests;", f"mod manifests {{ {manifests} }}")
        .replace("pub use reprocut_workspace::", "pub use crate::reprocut_workspace::")
    )
    return "\n".join(
        [
            core,
            wrap("reprocut_workspace", workspace),
            wrap("reprocut_adapters", adapters),
            "fn main() {}",
        ]
    )


def compose_pipeline() -> str:
    adapters = compose_adapters().removesuffix("fn main() {}")
    syntax = r"""
use std::path::Path;
use crate::reprocut_core::{ByteRange, Operation, ProjectPath};
use thiserror::Error;
#[derive(Clone, Copy)]
pub enum SyntaxLanguage { Rust }
impl SyntaxLanguage { pub fn from_path(_: &Path) -> Option<Self> { None } }
#[derive(Clone, Copy)]
pub enum SyntaxStrategy { DeleteNode, HoistChild }
pub struct SyntaxTransform;
impl SyntaxTransform {
    pub fn operation(&self, path: ProjectPath) -> Operation {
        Operation::replace(path, ByteRange::new(0, 1).unwrap(), Vec::new())
    }
    pub fn strategy(&self) -> SyntaxStrategy { SyntaxStrategy::DeleteNode }
    pub fn range(&self) -> ByteRange { ByteRange::new(0, 1).unwrap() }
}
#[derive(Debug, Error)]
pub enum SyntaxError {
    #[error("invalid syntax")] InvalidSyntax,
    #[error("invalid UTF-8")] InvalidUtf8,
    #[error("grammar error")] Grammar,
}
pub fn deletion_transforms(
    _: SyntaxLanguage,
    _: &[u8],
) -> Result<Vec<SyntaxTransform>, SyntaxError> {
    Ok(Vec::new())
}
pub fn hoist_transforms(_: SyntaxLanguage, _: &[u8]) -> Result<Vec<SyntaxTransform>, SyntaxError> {
    Ok(Vec::new())
}
"""
    source = (
        read("crates/reprocut-engine/src/pipeline.rs")
        .replace("use reprocut_adapters::", "use crate::reprocut_adapters::")
        .replace("reprocut_core::", "crate::reprocut_core::")
        .replace("use reprocut_syntax::", "use crate::reprocut_syntax::")
        .replace("use reprocut_workspace::", "use crate::reprocut_workspace::")
    )
    engine = f"""
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PreparationMode {{ None, Offline, LifecycleScripts, IsolatedPython }}
pub(crate) mod pipeline {{ {source} }}
"""
    return "\n".join(
        [adapters, wrap("reprocut_syntax", syntax), wrap("reprocut_engine", engine), "fn main() {}"]
    )


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--append", type=Path, required=True)
    parser.add_argument(
        "--scope",
        choices=(
            "full",
            "core",
            "workspace",
            "state",
            "scheduler",
            "runner",
            "engine",
            "adapters",
            "pipeline",
            "report",
            "oci",
        ),
        default="full",
    )
    args = parser.parse_args()

    workspace = {
        "full": compose_cli,
        "core": compose_core,
        "workspace": compose_workspace,
        "state": compose_state,
        "scheduler": compose_scheduler,
        "runner": compose_runner,
        "engine": compose_engine,
        "adapters": compose_adapters,
        "pipeline": compose_pipeline,
        "report": compose_report,
        "oci": compose_oci,
    }[args.scope]()
    append_path = args.append.resolve()
    try:
        append_relative = append_path.relative_to(ROOT).as_posix()
    except ValueError:
        append_source = append_path.read_text(encoding="utf-8")
    else:
        append_source = read(append_relative)
    code = workspace + "\n" + append_source
    payload = json.dumps(
        {
            "channel": "stable",
            "mode": "debug",
            "edition": "2021",
            "crateType": "bin",
            "tests": True,
            "code": code,
        }
    ).encode("utf-8")
    request = urllib.request.Request(
        "https://play.rust-lang.org/execute",
        data=payload,
        headers={"Content-Type": "application/json"},
        method="POST",
    )
    with urllib.request.urlopen(request, timeout=180) as response:
        result = json.load(response)

    sys.stderr.write(result.get("stderr", ""))
    sys.stdout.write(result.get("stdout", ""))
    return 0 if result.get("success") else 1


if __name__ == "__main__":
    raise SystemExit(main())
