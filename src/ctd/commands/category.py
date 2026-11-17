"""ctd category — カテゴリ管理(list / add / rename)。"""

from __future__ import annotations

from ..app import AppContext
from ..errors import UsageError
from ..utils.console import render_table
from ..utils.timeutil import format_local


def add_parser(subparsers):
    parser = subparsers.add_parser(
        "category",
        help="カテゴリを管理する(list / add / rename)",
        description="カテゴリの一覧表示・追加・名前変更を行います。",
    )
    sub = parser.add_subparsers(dest="category_command", metavar="<subcommand>")

    p_list = sub.add_parser("list", help="カテゴリの一覧を表示する")
    p_list.set_defaults(handler=run_list)

    p_add = sub.add_parser("add", help="カテゴリを追加する")
    p_add.add_argument("name", help="カテゴリ名")
    p_add.set_defaults(handler=run_add)

    p_rename = sub.add_parser("rename", help="カテゴリ名を変更する(メモとの紐付けは維持)")
    p_rename.add_argument("old", help="現在のカテゴリ名")
    p_rename.add_argument("new", help="新しいカテゴリ名")
    p_rename.set_defaults(handler=run_rename)

    parser.set_defaults(handler=_missing_subcommand, category_parser=parser)


def _missing_subcommand(args, ctx: AppContext) -> int:
    raise UsageError(
        "サブコマンドを指定してください: list / add / rename",
        usage=args.category_parser.format_usage(),
    )


def run_list(args, ctx: AppContext) -> int:
    with ctx.open_repository() as repo:
        categories = repo.list_categories()
    if not categories:
        print("カテゴリは登録されていません。")
        return 0
    rows = [[c.name, str(c.note_count), format_local(c.created_at)] for c in categories]
    for line in render_table(["カテゴリ", "メモ数", "作成日時"], rows):
        print(line)
    print(f"全{len(categories)}件")
    return 0


def run_add(args, ctx: AppContext) -> int:
    with ctx.open_repository() as repo:
        name = repo.add_category(args.name)
    print(f"カテゴリ「{name}」を追加しました。")
    return 0


def run_rename(args, ctx: AppContext) -> int:
    with ctx.open_repository() as repo:
        new_name = repo.rename_category(args.old, args.new)
    print(f"カテゴリ名を「{args.old.strip()}」から「{new_name}」に変更しました。")
    return 0
