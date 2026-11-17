"""ctd show — メモ詳細表示。"""

from __future__ import annotations

from ..app import AppContext
from ..utils.console import print_fields
from ..utils.timeutil import format_local
from ._common import NONE_TEXT, join_tags


def add_parser(subparsers):
    parser = subparsers.add_parser(
        "show",
        help="メモの詳細を表示する",
        description="指定したメモの情報と本文を表示します。",
    )
    parser.add_argument("id", help="メモのID(UUIDの先頭8文字以上でも指定できます)")
    parser.set_defaults(handler=run)


def run(args, ctx: AppContext) -> int:
    with ctx.open_repository() as repo:
        note = repo.get_note(repo.resolve_id(args.id))

    print_fields([
        ("ID", note.id),
        ("タイトル", note.title),
        ("カテゴリ", note.category or NONE_TEXT),
        ("タグ", join_tags(note.tags)),
        ("説明", note.description or NONE_TEXT),
        ("登録日時", format_local(note.created_at, with_seconds=True)),
        ("更新日時", format_local(note.updated_at, with_seconds=True)),
        ("元ファイル", note.source_path or NONE_TEXT),
    ])
    print("-" * 60)
    print(note.content, end="" if note.content.endswith("\n") else "\n")
    return 0
