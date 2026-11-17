"""ctd search — メモ検索。"""

from __future__ import annotations

from ..app import AppContext
from ..errors import UsageError
from ..utils.console import paginate, render_table, terminal_width
from ._common import short_id

PAGE_SIZE = 20


def add_parser(subparsers):
    parser = subparsers.add_parser(
        "search",
        help="メモを検索する",
        description="タイトル・本文・説明からキーワードを検索します。日本語に対応しています。",
    )
    parser.add_argument("keyword", help="検索キーワード(空白を含む場合は引用符で囲む)")
    parser.set_defaults(handler=run)


def _render(rows) -> None:
    table = [[short_id(r["id"]), r["title"], r["category"] or "-"] for r in rows]
    for line in render_table(["ID", "タイトル", "カテゴリ"], table, flex_col=1, total_width=terminal_width()):
        print(line)


def run(args, ctx: AppContext) -> int:
    keyword = args.keyword.strip()
    if not keyword:
        raise UsageError("検索キーワードを指定してください。")

    with ctx.open_repository() as repo:
        total = repo.count_search(keyword)
        if total == 0:
            print(f"「{keyword}」に一致するメモは見つかりませんでした。")
            return 0
        print(f"「{keyword}」: {total}件見つかりました。")
        paginate(total, lambda offset, limit: repo.search_notes(keyword, limit, offset), _render, PAGE_SIZE)
    return 0
