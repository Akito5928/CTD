"""ctd backup — データベースのバックアップ。"""

from __future__ import annotations

from ..app import AppContext
from ..database.backup import create_backup


def add_parser(subparsers):
    parser = subparsers.add_parser(
        "backup",
        help="データベースのバックアップを作成する",
        description="SQLiteのバックアップ機能でデータベースを複製します。既存のバックアップは上書きしません。",
    )
    parser.set_defaults(handler=run)


def run(args, ctx: AppContext) -> int:
    config = ctx.load_config()
    with ctx.open_repository() as repo:
        path = create_backup(repo.conn, config.backup_dir)
    print(f"バックアップを作成しました: {path}")
    return 0
