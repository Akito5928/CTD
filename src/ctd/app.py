"""コマンド共通の実行コンテキスト。"""

from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path

from .config import Config, load_config
from .database.connection import connect
from .database.repository import Repository
from .database.schema import verify_schema


@dataclass
class AppContext:
    base_dir: Path

    def load_config(self) -> Config:
        return load_config(self.base_dir)

    def open_repository(self) -> Repository:
        """DBを開く。DBが無ければ ctd init を案内するエラーになる。"""
        config = self.load_config()
        conn = connect(config.database_path)
        try:
            verify_schema(conn)
        except BaseException:
            conn.close()
            raise
        return Repository(conn)
