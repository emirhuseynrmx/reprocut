import importlib
import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


SCRIPT_DIR = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(SCRIPT_DIR))

from validate_cases import load_cases, select_case


def load_runner_module():
    try:
        return importlib.import_module("run_case")
    except ModuleNotFoundError:
        return None


class DockerBoundaryTests(unittest.TestCase):
    def setUp(self):
        self.runner = load_runner_module()
        self.assertIsNotNone(self.runner, "run_case module must implement the Docker boundary")
        self.assertTrue(hasattr(self.runner, "run_argv"), "run_case must expose shell-free run_argv")
        self.assertTrue(hasattr(self.runner, "CommandError"), "run_case must expose CommandError")
        cases = load_cases(SCRIPT_DIR / "cases.json")
        self.bevy = select_case(cases, "bevy")

    def test_container_has_hard_isolation_flags(self):
        argv = self.runner.docker_create_argv(self.bevy, "reprocut-validation:bevy")
        required_pairs = (
            ("--network", "none"),
            ("--cap-drop", "ALL"),
            ("--security-opt", "no-new-privileges"),
            ("--pids-limit", "1024"),
            ("--cpus", "2"),
            ("--memory", "7g"),
            ("--memory-swap", "7g"),
            ("--user", "10001:10001"),
        )
        self.assertEqual(argv[0:2], ["docker", "create"])
        for flag, value in required_pairs:
            index = argv.index(flag)
            self.assertEqual(argv[index + 1], value)
        self.assertIn("--read-only", argv)
        self.assertIn("/evidence:rw,nosuid,nodev,size=1g", argv)
        evidence_tmpfs = argv.index("/evidence:rw,nosuid,nodev,size=1g")
        self.assertEqual(argv[evidence_tmpfs - 1], "--tmpfs")
        self.assertNotIn("--privileged", argv)
        self.assertNotIn("--mount", argv)
        self.assertNotIn("-v", argv)
        self.assertFalse(any("GITHUB_TOKEN" in value for value in argv))
        self.assertFalse(any("docker.sock" in value for value in argv))

    def test_command_runner_treats_shell_metacharacters_as_literal_argv(self):
        completed = self.runner.run_argv(
            [
                sys.executable,
                "-c",
                "import sys; print(sys.argv[1])",
                "safe; echo INJECTED",
            ]
        )
        self.assertEqual(completed.returncode, 0)
        self.assertEqual(completed.stdout.strip(), "safe; echo INJECTED")
        self.assertEqual(completed.stderr, "")

    def test_command_runner_raises_with_captured_stderr(self):
        with self.assertRaisesRegex(self.runner.CommandError, "stable failure"):
            self.runner.run_argv(
                [sys.executable, "-c", "import sys; print('stable failure', file=sys.stderr); raise SystemExit(7)"],
                check=True,
            )

    def test_container_name_and_image_are_case_scoped(self):
        argv = self.runner.docker_create_argv(self.bevy, "reprocut-validation:bevy")
        name_index = argv.index("--name")
        self.assertEqual(argv[name_index + 1], "reprocut-validation-bevy")
        self.assertEqual(argv[-1], "reprocut-validation:bevy")


class EvidenceSanitizerTests(unittest.TestCase):
    def setUp(self):
        self.runner = load_runner_module()
        self.assertIsNotNone(self.runner, "run_case module must implement evidence sanitation")
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)

    def test_copies_regular_files_and_writes_literal_digest(self):
        source = self.root / "raw"
        destination = self.root / "clean"
        (source / "nested").mkdir(parents=True)
        (source / "nested" / "result.txt").write_bytes(b"safe")

        inventory = self.runner.sanitize_evidence(source, destination)

        self.assertEqual(
            inventory,
            {"nested/result.txt": "8b3369944dd2a3fab39e32d1aeb1f763946a458ae3e6368a46432adc8f3a0860"},
        )
        self.assertEqual((destination / "nested" / "result.txt").read_bytes(), b"safe")
        envelope = json.loads((destination / "integrity.json").read_text(encoding="utf-8"))
        self.assertEqual(envelope, {"algorithm": "sha256", "files": inventory, "schema_version": 1})

    @unittest.skipIf(os.name == "nt", "creating symlinks is not reliably permitted on Windows")
    def test_rejects_symlinks(self):
        source = self.root / "raw"
        source.mkdir()
        (source / "escape").symlink_to(self.root / "outside")
        with self.assertRaisesRegex(self.runner.EvidenceError, "symlink"):
            self.runner.sanitize_evidence(source, self.root / "clean")

    def test_rejects_destination_that_already_exists(self):
        source = self.root / "raw"
        destination = self.root / "clean"
        source.mkdir()
        destination.mkdir()
        with self.assertRaisesRegex(self.runner.EvidenceError, "already exists"):
            self.runner.sanitize_evidence(source, destination)


class BuildContextTests(unittest.TestCase):
    def setUp(self):
        self.runner = load_runner_module()
        self.assertIsNotNone(self.runner, "run_case module must implement build-context creation")
        self.assertTrue(
            hasattr(self.runner, "prepare_build_context"),
            "run_case must expose prepare_build_context",
        )
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.case = select_case(load_cases(SCRIPT_DIR / "cases.json"), "ipe")

    def test_context_contains_pinned_inputs_without_repository_metadata(self):
        reprocut = self.root / "reprocut"
        base = self.root / "base"
        head = self.root / "head"
        context = self.root / "context"
        (reprocut / ".git").mkdir(parents=True)
        (reprocut / "target").mkdir()
        (reprocut / "scripts").mkdir()
        (reprocut / "Cargo.toml").write_text("[workspace]\n", encoding="utf-8")
        (reprocut / ".git" / "config").write_text("secret", encoding="utf-8")
        (reprocut / "target" / "large").write_text("cache", encoding="utf-8")
        base.mkdir()
        head.mkdir()
        (base / "version.txt").write_text("base", encoding="utf-8")
        (head / "version.txt").write_text("head", encoding="utf-8")

        self.runner.prepare_build_context(
            case=self.case,
            repo_root=reprocut,
            base_snapshot=base,
            head_snapshot=head,
            destination=context,
            base_sha="a" * 40,
            reprocut_sha="b" * 40,
        )

        self.assertEqual((context / "base" / "version.txt").read_text(encoding="utf-8"), "base")
        self.assertEqual((context / "head" / "version.txt").read_text(encoding="utf-8"), "head")
        self.assertTrue((context / "reprocut" / "Cargo.toml").is_file())
        self.assertFalse((context / "reprocut" / ".git").exists())
        self.assertFalse((context / "reprocut" / "target").exists())
        case_document = json.loads((context / "case.json").read_text(encoding="utf-8"))
        self.assertEqual(case_document["base_sha"], "a" * 40)
        self.assertEqual(case_document["reprocut_sha"], "b" * 40)
        self.assertEqual(case_document["case_id"], "ipe")


if __name__ == "__main__":
    unittest.main()
