"""ctd.exe のエントリースクリプト(PyInstaller用)。開発時は `python ctd_entry.py <command>` でも実行できる。"""

import pathlib
import sys

if not getattr(sys, "frozen", False):
    sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent / "src"))

from ctd.main import cli  # noqa: E402

if __name__ == "__main__":
    sys.exit(cli())
