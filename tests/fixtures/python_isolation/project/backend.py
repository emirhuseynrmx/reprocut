from __future__ import annotations

import csv
import io
import re
import zipfile
from ast import literal_eval
from pathlib import Path


NAME = "reprocut_isolation_fixture"
VERSION = "1.0.0"


def project_dependencies() -> list[str]:
    """Read the candidate's current PEP 621 dependency array without third-party packages."""
    source = Path("pyproject.toml").read_text(encoding="utf-8")
    project = re.search(r"(?ms)^\[project\]\s*(.*?)(?=^\[|\Z)", source)
    if project is None:
        return []
    dependencies = re.search(
        r"(?ms)^dependencies\s*=\s*(\[[^\]]*\])",
        project.group(1),
    )
    if dependencies is None:
        return []
    values = literal_eval(dependencies.group(1))
    if not isinstance(values, list) or not all(isinstance(value, str) for value in values):
        raise ValueError("fixture dependencies must be a string array")
    return values


def build_wheel(wheel_directory: str, config_settings=None, metadata_directory=None) -> str:
    filename = f"{NAME}-{VERSION}-py3-none-any.whl"
    target = Path(wheel_directory) / filename
    info = f"{NAME}-{VERSION}.dist-info"
    requires_dist = "".join(
        f"Requires-Dist: {dependency}\n" for dependency in project_dependencies()
    )
    files = {
        f"{NAME}/__init__.py": b"FIXTURE = True\n",
        f"{info}/METADATA": (
            "Metadata-Version: 2.1\n"
            "Name: reprocut-isolation-fixture\n"
            "Version: 1.0.0\n"
            f"{requires_dist}\n"
        ).encode(),
        f"{info}/WHEEL": b"Wheel-Version: 1.0\nRoot-Is-Purelib: true\nTag: py3-none-any\n",
    }
    record = f"{info}/RECORD"
    rows = [[path, "", ""] for path in sorted(files)] + [[record, "", ""]]
    output = io.StringIO(newline="")
    csv.writer(output, lineterminator="\n").writerows(rows)
    files[record] = output.getvalue().encode()
    with zipfile.ZipFile(target, "w", zipfile.ZIP_DEFLATED) as archive:
        for path, data in sorted(files.items()):
            archive.writestr(path, data)
    return filename
