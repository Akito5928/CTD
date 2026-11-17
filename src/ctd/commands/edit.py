"""ctd edit — メモ編集。"""

from __future__ import annotations

from ..app import AppContext
from ..errors import Cancelled, CtdError
from ..utils.console import confirm, print_fields, prompt
from ..utils.file_reader import TextFile, read_text_file
from ..utils.validator import name_key
from ._common import KEEP, NONE_TEXT, ask_category, ask_tags, clean_description, join_tags, print_note_summary, warn_duplicates


def add_parser(subparsers):
    parser = subparsers.add_parser(
        "edit",
        help="メモを編集する",
        description=(
            "タイトル・カテゴリ・タグ・説明・本文を編集します。"
            "各項目はEnterで変更なし。本文を変えるときは新しいTXTファイルを指定します。"
        ),
    )
    parser.add_argument("id", help="メモのID(UUIDの先頭8文字以上でも指定できます)")
    parser.set_defaults(handler=run)


def _ask_new_content(repo, note) -> TextFile | None:
    """本文の差し替え。変更しない場合はNone。"""
    while True:
        raw = prompt("本文を差し替える場合は新しいTXTファイルのパス(Enterで変更なし): ").strip()
        if not raw:
            return None
        try:
            data = read_text_file(raw)
        except CtdError as e:
            print(f"エラー: {e}")
            if e.hint:
                print(f"  {e.hint}")
            continue
        if data.content_hash == note.content_hash:
            print("現在の本文と同じ内容のため、本文は変更しません。")
            return None
        duplicates = repo.find_by_hash(data.content_hash, exclude_id=note.id)
        if duplicates:
            warn_duplicates(duplicates)
            if not confirm("このファイルで本文を差し替えますか?", default=False):
                continue
        return data


def run(args, ctx: AppContext) -> int:
    with ctx.open_repository() as repo:
        note = repo.get_note(repo.resolve_id(args.id))
        print_note_summary(note, with_dates=False)
        print(f"説明     : {note.description or NONE_TEXT}")
        print()
        print("各項目の現在値を [ ] 内に表示します。Enterで変更なしです。")

        title = prompt(f"タイトル [{note.title}]: ").strip() or note.title

        category_choice = ask_category(repo, current=note.category, editing=True)
        category = note.category if category_choice is KEEP else category_choice

        tags_choice = ask_tags(current=note.tags, editing=True)
        tags = note.tags if tags_choice is KEEP else tags_choice

        desc_raw = prompt(f"説明 [{note.description or NONE_TEXT}] (Enterで変更なし / - で削除): ").strip()
        if desc_raw == "":
            description = note.description
        elif desc_raw == "-":
            description = None
        else:
            description = clean_description(desc_raw)

        new_file = _ask_new_content(repo, note)

        # --- 変更内容の確認 ---
        changes: list[tuple[str, str]] = []
        if title != note.title:
            changes.append(("タイトル", f"{note.title} → {title}"))
        if (category or None) != (note.category or None) and name_key(category or "") != name_key(note.category or ""):
            changes.append(("カテゴリ", f"{note.category or NONE_TEXT} → {category or NONE_TEXT}"))
        if sorted(name_key(t) for t in tags) != sorted(name_key(t) for t in note.tags):
            changes.append(("タグ", f"{join_tags(note.tags)} → {join_tags(tags)}"))
        if description != note.description:
            changes.append(("説明", f"{note.description or NONE_TEXT} → {description or NONE_TEXT}"))
        if new_file is not None:
            changes.append((
                "本文",
                f"{len(note.content)}文字 → {len(new_file.content)}文字 (ファイル: {new_file.path})",
            ))

        if not changes:
            print("変更はありません。")
            return 0

        print()
        print("--- 変更内容の確認 ---")
        print_fields(changes)
        print()
        if not confirm("この内容で保存しますか?", default=True):
            raise Cancelled()

        repo.update_note(
            note.id,
            title=title,
            content=new_file.content if new_file else note.content,
            description=description,
            category_name=category,
            tags=tags,
            source_path=str(new_file.path) if new_file else note.source_path,
            content_hash=new_file.content_hash if new_file else note.content_hash,
        )

    print("更新しました。")
    return 0
