"""ctd list — メモ一覧。"""

from __future__ import annotations

from ..app import AppContext
from ..utils.console import paginate, render_table, terminal_width
from ..utils.timeutil import format_local
from ._common import short_id

PAGE_SIZE = 20


def add_parser(subparsers):
    parser = subparsers.add_parser(
        "list",
        help="メモの一覧を表示する(登録日時の新しい順)",
        description="登録済みのメモを登録日時の新しい順に表示します。",
    )
    parser.set_defaults(handler=run)


def _render(rows) -> None:
    table = [
        [short_id(r["id"]), r["title"], r["category"] or "-", format_local(r["created_at"])]
        for r in rows
    ]
    headers = ["ID", "タイトル", "カテゴリ", "登録日時"]
    for line in render_table(headers, table, flex_col=1, total_width=terminal_width()):
        print(line)


def run(args, ctx: AppContext) -> int:
    with ctx.open_repository() as repo:
        total = repo.count_notes()
        if total == 0:
            print("登録されているメモはありません。")
            return 0
        print(f"全{total}件")
        paginate(total, lambda offset, limit: repo.list_notes(limit, offset), _render, PAGE_SIZE)
    return 0
