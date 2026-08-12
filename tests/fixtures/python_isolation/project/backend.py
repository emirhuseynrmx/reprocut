from __future__ import annotations

import csv
import io
import zipfile
from pathlib import Path


NAME = "reprocut_isolation_fixture"
VERSION = "1.0.0"


def build_wheel(wheel_directory: str, config_settings=None, metadata_directory=None) -> str:
    filename = f"{NAME}-{VERSION}-py3-none-any.whl"
    target = Path(wheel_directory) / filename
    info = f"{NAME}-{VERSION}.dist-info"
    files = {
        f"{NAME}/__init__.py": b"FIXTURE = True\n",
        f"{info}/METADATA": (
            "Metadata-Version: 2.1\n"
            "Name: reprocut-isolation-fixture\n"
            "Version: 1.0.0\n"
            "Requires-Dist: required-dep==1.0.0\n"
            "Requires-Dist: unused-dep==1.0.0\n\n"
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
