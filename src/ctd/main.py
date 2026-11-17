"""CTDのエントリーポイント。コマンドの振り分けと共通のエラー処理。"""

from __future__ import annotations

import argparse
import os
import sqlite3
import sys
import traceback

from . import __version__
from . import config as config_module
from .app import AppContext
from .commands import backup, category, delete, edit, init, list_notes, register, search, show
from .errors import Cancelled, CtdError, UsageError
from .utils.console import configure_stdio

PROG = "ctd"

_EPILOG = """\
使用例:
  ctd init                       データベースを初期化する
  ctd register memo.txt          TXTファイルをメモとして登録する
  ctd search 検索語              メモを検索する
  ctd list                       メモの一覧を表示する
  ctd show <ID>                  メモの詳細を表示する(IDは先頭8文字以上でOK)
  ctd edit <ID>                  メモを編集する
  ctd delete <ID>                メモを削除する
  ctd category list              カテゴリの一覧を表示する
  ctd backup                     データベースのバックアップを作成する
"""


class CtdArgumentParser(argparse.ArgumentParser):
    """不正な引数のとき、日本語のエラーと使用方法を表示して終了コード2で終わる。"""

    def error(self, message: str):
        self.print_usage(sys.stderr)
        self.exit(2, f"エラー: {message}\n")


def build_parser() -> argparse.ArgumentParser:
    parser = CtdArgumentParser(
        prog=PROG,
        description="CTD (CLI Text Database) — テキストファイルを登録・管理・検索するツール",
        epilog=_EPILOG,
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    subparsers = parser.add_subparsers(
        dest="command", metavar="<command>", parser_class=CtdArgumentParser
    )

    init.add_parser(subparsers)
    register.add_parser(subparsers)
    search.add_parser(subparsers)
    list_notes.add_parser(subparsers)
    show.add_parser(subparsers)
    edit.add_parser(subparsers)
    delete.add_parser(subparsers)
    category.add_parser(subparsers)
    backup.add_parser(subparsers)

    def run_help(args, ctx) -> int:
        parser.print_help()
        return 0

    def run_version(args, ctx) -> int:
        print(f"{PROG} {__version__}")
        return 0

    subparsers.add_parser("help", help="ヘルプを表示する").set_defaults(handler=run_help)
    subparsers.add_parser("version", help="バージョンを表示する").set_defaults(handler=run_version)
    parser.set_defaults(handler=run_help)  # コマンド省略時はヘルプを表示
    return parser


def _report(message: str, hint: str | None = None) -> None:
    print(f"エラー: {message}", file=sys.stderr)
    if hint:
        print(f"  {hint}", file=sys.stderr)


def main(argv: list[str] | None = None, base_dir=None) -> int:
    """終了コードを返す。0=正常/キャンセル、1=実行時エラー、2=引数・使用方法エラー。"""
    parser = build_parser()
    try:
        args = parser.parse_args(argv)
    except SystemExit as e:
        return e.code if isinstance(e.code, int) else (0 if e.code is None else 2)

    ctx = AppContext(base_dir or config_module.get_default_base_dir())
    try:
        return args.handler(args, ctx) or 0
    except (Cancelled, KeyboardInterrupt):
        print("キャンセルしました。変更は保存されていません。")
        return 0
    except UsageError as e:
        _report(e.message, e.hint)
        if e.usage:
            print(e.usage, file=sys.stderr, end="")
        return e.exit_code
    except CtdError as e:
        _report(e.message, e.hint)
        return e.exit_code
    except sqlite3.Error as e:
        _report(f"データベースの操作に失敗しました: {e}", "データは変更されていません。")
        return 1
    except OSError as e:
        _report(f"ファイルの操作に失敗しました: {e}")
        return 1
    except Exception as e:  # 予期しない例外でもクラッシュ表示にしない
        _report(f"予期しないエラーが発生しました: {type(e).__name__}: {e}")
        if os.environ.get("CTD_DEBUG"):
            traceback.print_exc()
        return 1


def cli() -> int:
    """コンソールから実行するときの入口。"""
    configure_stdio()
    return main()
