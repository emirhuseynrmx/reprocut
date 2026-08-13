from __future__ import annotations

import os
import sys

import required_dep


assert required_dep.VALUE == "required-dep"
assert os.environ.get("PYTHONNOUSERSITE") == "1"
assert os.environ.get("PIP_NO_INDEX") == "1"
raise RuntimeError(
    f"REPROCUT_ISOLATED_FAILURE dependency={required_dep.VALUE} prefix={sys.prefix}"
)
