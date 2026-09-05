"""The bevy oracle must separate the defect under test from one a cut introduced.

The three fixtures are real captures: the original failing run, the passing base, and the
reduction that CI rejected for drifting - which kept the five original errors and added
thirty unrelated ones, and which the previous lint-name match accepted.
"""

import json
import re
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

SCRIPT_DIR = Path(__file__).resolve().parents[1]
ORACLE = SCRIPT_DIR / "bevy_clippy_oracle.sh"
FIXTURES = Path(__file__).resolve().parent / "fixtures" / "bevy"
CATALOG = SCRIPT_DIR / "cases.json"


def bevy_case():
    catalog = json.loads(CATALOG.read_text(encoding="utf-8"))
    cases = catalog["cases"] if isinstance(catalog, dict) else catalog
    return next(case for case in cases if case["case_id"] == "bevy")


def run_against(log: str, exit_code: int) -> str:
    """Runs the oracle with cargo replaced by a stub that replays a captured log."""
    with tempfile.TemporaryDirectory() as raw:
        root = Path(raw)
        (root / "Cargo.toml").write_text("[package]\nname = 'stub'\n", encoding="utf-8")
        stub = root / "bin"
        stub.mkdir()
        shutil.copyfile(FIXTURES / log, root / "captured.log")
        (stub / "cargo").write_text(
            "#!/usr/bin/env bash\n"
            f"cat {root.as_posix()}/captured.log\n"
            f"exit {exit_code}\n",
            encoding="utf-8",
        )
        (stub / "cargo").chmod(0o755)
        completed = subprocess.run(
            ["bash", str(ORACLE)],
            cwd=root,
            capture_output=True,
            text=True,
            env={"PATH": f"{stub.as_posix()}:/usr/bin:/bin", "HOME": raw},
        )
        return completed.stderr


class BevyOracleTest(unittest.TestCase):
    def verdict(self, stderr: str) -> str:
        lines = [line for line in stderr.splitlines() if line.startswith("BEVY-ORACLE:")]
        self.assertEqual(len(lines), 1, f"expected one verdict, got {lines}")
        return lines[0]

    def test_the_original_failure_is_the_defect(self):
        verdict = self.verdict(run_against("original-failure.log", 101))
        self.assertIn("exactly 5 undocumented unsafe block(s)", verdict)
        self.assertIn("world/despawn_all.rs", verdict)

    def test_the_drifted_reduction_is_rejected(self):
        verdict = self.verdict(run_against("drifted-reduction.log", 101))
        self.assertIn("not the defect under test", verdict)
        self.assertIn("different lint", verdict)

    def test_a_clean_run_is_not_the_defect(self):
        verdict = self.verdict(run_against("clean-base.log", 0))
        self.assertIn("no error", verdict)

    def test_the_catalog_regex_accepts_only_the_defect(self):
        # No MULTILINE: the engine matches one combined stream, so an anchored pattern
        # would pass here and fail there - which is exactly what it did.
        pattern = re.compile(bevy_case()["required_regex"][0])
        accepted = run_against("original-failure.log", 101)
        rejected = run_against("drifted-reduction.log", 101)
        self.assertTrue(pattern.search(accepted), accepted)
        self.assertFalse(pattern.search(rejected), rejected)


if __name__ == "__main__":
    sys.exit(unittest.main())
