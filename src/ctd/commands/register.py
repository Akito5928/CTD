"""ctd register — メモ登録。"""

from __future__ import annotations

from ..app import AppContext
from ..errors import Cancelled
from ..utils.console import confirm, print_fields, prompt, truncate
from ..utils.file_reader import read_text_file
from ._common import NONE_TEXT, ask_category, ask_tags, clean_description, join_tags, warn_duplicates

PREVIEW_LINES = 5
PREVIEW_WIDTH = 100


def add_parser(subparsers):
    parser = subparsers.add_parser(
        "register",
        help="TXTファイルをメモとして登録する",
        description="TXTファイル(UTF-8)を読み込み、対話形式でタイトル・カテゴリ・タグ・説明を入力して登録します。",
    )
    parser.add_argument("file", help="登録するTXTファイル(空白を含むパスは引用符で囲む)")
    parser.set_defaults(handler=run)


def run(args, ctx: AppContext) -> int:
    data = read_text_file(args.file)  # 存在・形式・UTF-8・空ファイルの検証

    with ctx.open_repository() as repo:
        duplicates = repo.find_by_hash(data.content_hash)
        if duplicates:
            warn_duplicates(duplicates)
            if not confirm("このまま登録を続けますか?", default=False):
                raise Cancelled()

        default_title = data.path.stem
        title = prompt(f"タイトル [{default_title}]: ").strip() or default_title
        category_name = ask_category(repo)
        tags = ask_tags()
        description = clean_description(prompt("説明(任意): "))

        # --- プレビュー ---
        is_new_category = bool(category_name) and repo.find_category(category_name) is None
        category_text = NONE_TEXT if not category_name else (
            f"{category_name} (新規作成)" if is_new_category else category_name
        )
        print()
        print("--- 登録内容の確認 ---")
        print_fields([
            ("ファイル", str(data.path)),
            ("タイトル", title),
            ("カテゴリ", category_text),
            ("タグ", join_tags(tags)),
            ("説明", description or NONE_TEXT),
            ("本文", f"{len(data.content)}文字 / {len(data.content.splitlines())}行"),
        ])
        print("--- 本文(先頭) ---")
        lines = data.content.splitlines()
        for line in lines[:PREVIEW_LINES]:
            print(truncate(line, PREVIEW_WIDTH))
        if len(lines) > PREVIEW_LINES:
            print("...")
        print()

        if not confirm("この内容で登録しますか?", default=True):
            raise Cancelled()

        note_id = repo.create_note(
            title=title,
            content=data.content,
            description=description,
            category_name=category_name,
            tags=tags,
            source_path=str(data.path),
            content_hash=data.content_hash,
        )

    print("登録しました。")
    print(f"  ID: {note_id}")
    return 0
