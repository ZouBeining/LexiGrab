"""Run an automation script with the uv-managed venv's site-packages.

The uv venv's python.exe trampoline cannot inject input on this machine
(SetCursorPos/SendInput fail with "no token"), so the harness is executed with
the uv-managed base CPython while importing packages from the uv venv.
"""

from __future__ import annotations

import os
import runpy
import sys
from pathlib import Path

MANUAL = Path(__file__).resolve().parent
SITE = MANUAL / ".venv" / "Lib" / "site-packages"


def main() -> None:
    for dll_dir in (SITE / "pywin32_system32", SITE / "win32", SITE / "win32" / "lib"):
        if dll_dir.is_dir():
            try:
                os.add_dll_directory(str(dll_dir))
            except (AttributeError, OSError):
                pass
    for extra in (SITE, SITE / "win32", SITE / "win32" / "lib", SITE / "Pythonwin"):
        if extra.is_dir() and str(extra) not in sys.path:
            sys.path.insert(0, str(extra))
    script = Path(sys.argv[1]).resolve()
    sys.argv = sys.argv[1:]
    runpy.run_path(str(script), run_name="__main__")


if __name__ == "__main__":
    main()
