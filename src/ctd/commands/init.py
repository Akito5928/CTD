"""ctd init — データベース初期化。"""

from __future__ import annotations

from ..app import AppContext
from ..config import TXT_DIR_NAME, load_config, write_default_config
from ..database.connection import connect
from ..database.schema import create_schema
from ..errors import CtdError


def add_parser(subparsers):
    parser = subparsers.add_parser(
        "init",
        help="データベースを初期化する",
        description="設定ファイル・保存用フォルダ・データベースを作成します。既に初期化済みの場合は何も変更しません。",
    )
    parser.set_defaults(handler=run)


def run(args, ctx: AppContext) -> int:
    base = ctx.base_dir
    write_default_config(base)  # 既存の設定ファイルは上書きしない
    config = load_config(base)

    try:
        for directory in (config.database_path.parent, config.backup_dir, base / TXT_DIR_NAME):
            directory.mkdir(parents=True, exist_ok=True)
    except OSError as e:
        raise CtdError(f"フォルダを作成できません: {e.filename} ({e.strerror or e})")

    if config.database_path.exists():
        print("既に初期化済みです。(既存のデータベースは変更しません)")
        print(f"  データベース: {config.database_path}")
        return 0

    created_db = False
    conn = connect(config.database_path, must_exist=False)
    try:
        created_db = True
        fts_enabled = create_schema(conn)
    except BaseException:
        conn.close()
        if created_db:  # 作りかけのDBファイルを残さない
            try:
                config.database_path.unlink()
            except OSError:
                pass
        raise
    conn.close()

    print("初期化が完了しました。")
    print(f"  設定ファイル: {base / 'config' / 'config.json'}")
    print(f"  データベース: {config.database_path}")
    print(f"  バックアップ: {config.backup_dir}")
    if not fts_enabled:
        print("注意: この環境ではFTS5(trigram)を利用できないため、検索は通常検索で動作します。")
    return 0
