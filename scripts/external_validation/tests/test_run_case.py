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
        expected_tmpfs = (
            "/work:rw,exec,nosuid,nodev,size=12g,uid=10001,gid=10001,mode=1770",
            "/tmp:rw,exec,nosuid,nodev,size=2g,uid=10001,gid=10001,mode=1770",
        )
        for value in expected_tmpfs:
            self.assertIn(value, argv)
            self.assertEqual(argv[argv.index(value) - 1], "--tmpfs")
        self.assertNotIn("--privileged", argv)
        evidence_mount = "type=volume,destination=/evidence"
        self.assertIn(evidence_mount, argv)
        self.assertEqual(argv[argv.index(evidence_mount) - 1], "--mount")
        self.assertFalse(any("type=bind" in value for value in argv))
        self.assertNotIn("-v", argv)
        self.assertFalse(any("GITHUB_TOKEN" in value for value in argv))
        self.assertFalse(any("docker.sock" in value for value in argv))

    def test_container_cleanup_removes_anonymous_evidence_volume(self):
        self.assertEqual(
            self.runner.docker_remove_argv("reprocut-validation-ipe"),
            ["docker", "rm", "--force", "--volumes", "reprocut-validation-ipe"],
        )

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

    def test_runtime_seed_caches_are_root_owned_but_validator_readable(self):
        dockerfile = (SCRIPT_DIR / "Dockerfile").read_text(encoding="utf-8")

        self.assertIn("chown -R root:root /inputs /opt/reprocut", dockerfile)
        self.assertIn("chmod -R a+rX,a-w /inputs /opt/reprocut /opt/precommit", dockerfile)
        self.assertNotIn("/opt/pre-commit-cache", dockerfile)

    def test_runtime_copies_immutable_seeds_without_preserving_owner_or_mode(self):
        entrypoint = (SCRIPT_DIR / "container_entrypoint.sh").read_text(encoding="utf-8")

        self.assertIn('cp -R --no-preserve=ownership,mode "$source"/. "$destination"/', entrypoint)
        self.assertIn('chmod -R u+rwX "$destination"', entrypoint)
        self.assertNotIn("cp -a /opt/", entrypoint)
        self.assertNotIn('cp -a "$source"', entrypoint)

    def test_openruyi_bootstrap_is_targeted_and_failures_are_captured_early(self):
        dockerfile = (SCRIPT_DIR / "Dockerfile").read_text(encoding="utf-8")
        entrypoint = (SCRIPT_DIR / "container_entrypoint.sh").read_text(encoding="utf-8")
        oracle = (SCRIPT_DIR / "openruyi_eof_oracle.sh").read_text(encoding="utf-8")

        self.assertIn("pre-commit-hooks==6.0.0", dockerfile)
        self.assertNotIn("PRE_COMMIT_HOME=/opt/pre-commit-cache", dockerfile)
        self.assertIn("end-of-file-fixer", oracle)
        self.assertIn("files were modified by this hook", oracle)
        self.assertNotIn("reduction_argv=(/opt/precommit/bin/pre-commit run", entrypoint)
        self.assertIn("reprocut_args+=(--ecosystem none --prepare none)", entrypoint)
        self.assertLess(
            entrypoint.index("trap 'container_rc=$?"),
            entrypoint.index("copy_seed_tree /opt/cargo"),
        )
        self.assertIn("install -d -o 10001 -g 10001 -m 0700 /evidence", dockerfile)

    def test_ipe_bootstrap_builds_snapshot_specific_clis(self):
        dockerfile = (SCRIPT_DIR / "Dockerfile").read_text(encoding="utf-8")
        entrypoint = (SCRIPT_DIR / "container_entrypoint.sh").read_text(encoding="utf-8")
        oracle = (SCRIPT_DIR / "ipe_regen_oracle.sh").read_text(encoding="utf-8")

        self.assertIn("cp -R /inputs/base /tmp/ipe-base-build", dockerfile)
        self.assertIn("cd /tmp/ipe-base-build; cargo build --release -p ipe", dockerfile)
        self.assertNotIn("cd /inputs/base; cargo build", dockerfile)
        self.assertIn("install -m 0555 target/release/ipe /usr/local/bin/ipe-base", dockerfile)
        self.assertIn("cd /inputs/head; cargo build --locked --release -p ipe", dockerfile)
        self.assertIn("install -m 0555 target/release/ipe /usr/local/bin/ipe-head", dockerfile)
        self.assertIn('IPE_BIN="/usr/local/bin/ipe-${label}"', entrypoint)
        self.assertIn("reduction_argv=(env IPE_BIN=/usr/local/bin/ipe-head", entrypoint)
        self.assertIn("final_oracle_argv=(env IPE_BIN=/usr/local/bin/ipe-head", entrypoint)
        self.assertIn("COPY reprocut/scripts/external_validation/ipe_regen_oracle.sh", dockerfile)
        self.assertIn("git grep jq ripgrep", dockerfile)
        self.assertIn("tools/scripts/regen-sky-examples.sh", oracle)
        self.assertIn("scripts/regen-sky-examples.sh", oracle)

    def test_bevy_bootstrap_supports_snapshots_without_a_lockfile(self):
        dockerfile = (SCRIPT_DIR / "Dockerfile").read_text(encoding="utf-8")

        self.assertIn("--component clippy --component rustfmt", dockerfile)
        fetch = "if [ -f Cargo.lock ]; then cargo fetch --locked; else cargo fetch; fi"
        self.assertEqual(dockerfile.count(fetch), 2)
        self.assertNotIn("cd /inputs/head; cargo fetch --locked", dockerfile)
        self.assertNotIn("cd /inputs/base; cargo fetch --locked", dockerfile)


if __name__ == "__main__":
    unittest.main()
