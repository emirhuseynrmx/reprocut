"""Rebuild deterministic dependency wheels used by the offline isolation contract."""

from __future__ import annotations

import base64
import csv
import hashlib
import io
import zipfile
from pathlib import Path


ROOT = Path(__file__).resolve().parent
STAMP = (1980, 1, 1, 0, 0, 0)


def digest(data: bytes) -> str:
    value = base64.urlsafe_b64encode(hashlib.sha256(data).digest()).rstrip(b"=")
    return f"sha256={value.decode('ascii')}"


def wheel(name: str) -> None:
    distribution = name.replace("-", "_")
    info = f"{distribution}-1.0.0.dist-info"
    files = {
        f"{distribution}/__init__.py": f"VALUE = {name!r}\n".encode(),
        f"{info}/METADATA": (
            f"Metadata-Version: 2.1\nName: {name}\nVersion: 1.0.0\n\n"
        ).encode(),
        f"{info}/WHEEL": b"Wheel-Version: 1.0\nGenerator: reprocut-fixture\nRoot-Is-Purelib: true\nTag: py3-none-any\n",
        f"{info}/top_level.txt": f"{distribution}\n".encode(),
    }
    rows = [[path, digest(data), str(len(data))] for path, data in sorted(files.items())]
    record = f"{info}/RECORD"
    rows.append([record, "", ""])
    output = io.StringIO(newline="")
    csv.writer(output, lineterminator="\n").writerows(rows)
    files[record] = output.getvalue().encode()
    target = ROOT / f"{distribution}-1.0.0-py3-none-any.whl"
    with zipfile.ZipFile(target, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=9) as archive:
        for path, data in sorted(files.items()):
            member = zipfile.ZipInfo(path, STAMP)
            member.compress_type = zipfile.ZIP_DEFLATED
            member.external_attr = 0o100644 << 16
            archive.writestr(member, data)


if __name__ == "__main__":
    wheel("required-dep")
    wheel("unused-dep")
