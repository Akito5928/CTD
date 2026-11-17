"""ctd delete — メモ削除。"""

from __future__ import annotations

from ..app import AppContext
from ..errors import Cancelled
from ..utils.console import confirm
from ._common import print_note_summary


def add_parser(subparsers):
    parser = subparsers.add_parser(
        "delete",
        help="メモを削除する",
        description="メモとタグの紐付けを削除します。カテゴリ・タグ自体と元のTXTファイルは削除しません。",
    )
    parser.add_argument("id", help="メモのID(UUIDの先頭8文字以上でも指定できます)")
    parser.set_defaults(handler=run)


def run(args, ctx: AppContext) -> int:
    with ctx.open_repository() as repo:
        note = repo.get_note(repo.resolve_id(args.id))
        print_note_summary(note)
        print()
        if not confirm("Are you sure?", default=False):
            raise Cancelled()
        repo.delete_note(note.id)
    print("削除しました。")
    return 0
