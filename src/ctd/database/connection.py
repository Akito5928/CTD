"""SQLite接続。外部キー制約は常に有効にする。"""

from __future__ import annotations

import sqlite3
from pathlib import Path

from ..errors import CtdError


def connect(db_path: Path, *, must_exist: bool = True) -> sqlite3.Connection:
    if must_exist and not db_path.is_file():
        raise CtdError(
            f"データベースが見つかりません: {db_path}",
            hint="先に `ctd init` を実行して初期化してください。",
        )
    try:
        # isolation_level=None: トランザクションは Repository.transaction() で明示的に制御する
        conn = sqlite3.connect(str(db_path), isolation_level=None)
        conn.row_factory = sqlite3.Row
        conn.execute("PRAGMA foreign_keys = ON")
    except sqlite3.Error as e:
        raise CtdError(f"データベースに接続できません: {db_path} ({e})")
    return conn
