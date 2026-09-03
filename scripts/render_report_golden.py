#!/usr/bin/env python3
"""Render the reviewed report fixture with official stable Rust Playground."""

from __future__ import annotations

import json
import urllib.request

from playground_workspace_verify import ROOT, compose_core, report_source, wrap


def main() -> int:
    fixture = (ROOT / "scripts/verification/render_report_fixture.rs").read_text(encoding="utf-8")
    # The report crate reaches into reprocut_core for its schema constants and hashing, so the
    # remote build needs both, exactly as compose_report() assembles them.
    core = compose_core().replace("fn main() {}\n", "")
    code = "\n".join([core, wrap("reprocut_report", report_source()), fixture])
    payload = json.dumps(
        {
            "channel": "stable",
            "mode": "debug",
            "edition": "2021",
            "crateType": "bin",
            "tests": False,
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
    if not result.get("success"):
        raise RuntimeError(result.get("stderr", "remote report rendering failed"))
    report = str(result.get("stdout", "")).replace("\r\n", "\n").rstrip("\n") + "\n"
    target = ROOT / "tests/golden/reduction-report.html"
    target.write_text(report, encoding="utf-8", newline="\n")
    print(f"rendered {target} ({len(report.encode('utf-8'))} bytes)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
