import importlib
import json
import sys
import tempfile
import unittest
from pathlib import Path


SCRIPT_DIR = Path(__file__).resolve().parents[1]
CATALOG = SCRIPT_DIR / "cases.json"
sys.path.insert(0, str(SCRIPT_DIR))


def load_validation_module():
    try:
        return importlib.import_module("validate_cases")
    except ModuleNotFoundError:
        return None


def valid_case(case_id="openruyi", head_sha="1" * 40):
    return {
        "case_id": case_id,
        "repository": "https://github.com/example/project.git",
        "base_ref": "2" * 40,
        "head_sha": head_sha,
        "ci_url": "https://github.com/example/project/actions/runs/1/job/2",
        "oracle_argv": ["python", "-m", "pytest"],
        "required_regex": ["stable failure"],
        "rejected_regex": ["network error"],
        "memory": "1g",
        "timeout_minutes": 10,
        "attempt_timeout_ms": 30000,
    }


class CatalogTests(unittest.TestCase):
    def setUp(self):
        self.validation = load_validation_module()
        self.assertIsNotNone(
            self.validation,
            "validate_cases module must implement the catalog contract",
        )

    def write_catalog(self, cases):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        path = Path(directory.name) / "cases.json"
        path.write_text(json.dumps({"schema_version": 1, "cases": cases}), encoding="utf-8")
        return path

    def test_catalog_contains_exact_pinned_cases(self):
        cases = self.validation.load_cases(CATALOG)
        self.assertEqual(
            [(case.case_id, case.base_ref, case.head_sha) for case in cases],
            [
                ("openruyi", "19d328ca44ee6066afb3909d1533c919681c311b", "1a0e915e4e0daa89cce0b97dc488801fe4225a0e"),
                ("ipe", "eba6ed15155e42b73c0df1b69ec19b82a35f852e", "072f647ca425694728de3aa6f508f1c3820681f1"),
                ("bevy", "0de26631b0603acdc945aeae5e05b07ce58bc4dc", "762326968f6fac9e69c81a831ab91ab29afb9933"),
            ],
        )

    def test_openruyi_oracle_targets_the_reproduced_mutating_hook(self):
        cases = self.validation.load_cases(CATALOG)
        openruyi = self.validation.select_case(cases, "openruyi")

        self.assertEqual(
            list(openruyi.oracle_argv),
            ["/usr/local/bin/openruyi-eof-oracle"],
        )

    def test_ipe_oracle_is_snapshot_path_compatible(self):
        cases = self.validation.load_cases(CATALOG)
        ipe = self.validation.select_case(cases, "ipe")

        self.assertEqual(list(ipe.oracle_argv), ["/usr/local/bin/ipe-regen-oracle"])

    def test_rejects_unpinned_head(self):
        path = self.write_catalog([valid_case(head_sha="main")])
        with self.assertRaisesRegex(self.validation.CatalogError, "head_sha"):
            self.validation.load_cases(path, expected_order=("openruyi",))

    def test_rejects_unpinned_base(self):
        case = valid_case()
        case["base_ref"] = "main"
        path = self.write_catalog([case])
        with self.assertRaisesRegex(self.validation.CatalogError, "base_ref"):
            self.validation.load_cases(path, expected_order=("openruyi",))

    def test_rejects_shell_command_string(self):
        case = valid_case()
        case["oracle_argv"] = "python -m pytest && curl example.invalid"
        path = self.write_catalog([case])
        with self.assertRaisesRegex(self.validation.CatalogError, "oracle_argv"):
            self.validation.load_cases(path, expected_order=("openruyi",))

    def test_rejects_unknown_case_key(self):
        case = valid_case()
        case["surprise"] = True
        path = self.write_catalog([case])
        with self.assertRaisesRegex(self.validation.CatalogError, "unknown"):
            self.validation.load_cases(path, expected_order=("openruyi",))

    def test_rejects_non_github_repository(self):
        case = valid_case()
        case["repository"] = "https://example.com/project.git"
        path = self.write_catalog([case])
        with self.assertRaisesRegex(self.validation.CatalogError, "repository"):
            self.validation.load_cases(path, expected_order=("openruyi",))

    def test_select_case_rejects_unknown_identifier(self):
        case = self.validation.CaseSpec(**valid_case())
        with self.assertRaisesRegex(self.validation.CatalogError, "unknown case"):
            self.validation.select_case((case,), "missing")


if __name__ == "__main__":
    unittest.main()
