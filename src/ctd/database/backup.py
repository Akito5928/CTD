"""SQLiteのバックアップ機能によるデータベースの安全な複製。"""

from __future__ import annotations

import os
import sqlite3
from datetime import datetime
from pathlib import Path

from ..errors import CtdError
from ..utils.timeutil import backup_timestamp


def _reserve_unique_path(backup_dir: Path, stamp: str) -> Path:
    """既存のバックアップを上書きしないよう、新規作成(x)でファイル名を確保する。"""
    for n in range(0, 1000):
        suffix = "" if n == 0 else f"_{n}"
        candidate = backup_dir / f"ctd_backup_{stamp}{suffix}.db"
        try:
            with open(candidate, "xb"):
                pass
        except FileExistsError:
            continue
        return candidate
    raise CtdError("バックアップ先のファイル名を確保できませんでした。")


def create_backup(conn: sqlite3.Connection, backup_dir: Path, now: datetime | None = None) -> Path:
    try:
        backup_dir.mkdir(parents=True, exist_ok=True)
        final_path = _reserve_unique_path(backup_dir, backup_timestamp(now))
    except OSError as e:
        raise CtdError(f"バックアップ先を準備できません: {backup_dir} ({e.strerror or e})")

    temp_path = final_path.with_name(final_path.name + ".part")
    dest: sqlite3.Connection | None = None
    try:
        dest = sqlite3.connect(str(temp_path))
        conn.backup(dest)  # SQLite標準のオンラインバックアップ
        dest.close()
        dest = None
        os.replace(temp_path, final_path)
    except BaseException as e:
        if dest is not None:
            dest.close()
        for leftover in (temp_path, final_path):
            try:
                leftover.unlink()
            except OSError:
                pass
        if isinstance(e, (sqlite3.Error, OSError)):
            raise CtdError(f"バックアップに失敗しました: {e}")
        raise
    return final_path
