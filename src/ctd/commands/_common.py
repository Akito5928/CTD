"""複数コマンドで共有する対話入力・表示の部品。"""

from __future__ import annotations

from ..database.repository import Note, Repository
from ..errors import CtdError
from ..utils.console import print_fields, prompt
from ..utils.timeutil import format_local
from ..utils.validator import display_name, parse_tags, validate_name

KEEP = object()  # 編集時の「変更なし」を表す目印
NONE_TEXT = "(なし)"


def short_id(note_id: str) -> str:
    return note_id[:8]


def join_tags(tags: list[str]) -> str:
    return ", ".join(tags) if tags else NONE_TEXT


def ask_category(repo: Repository, current: str | None = None, editing: bool = False):
    """カテゴリを選ばせる。戻り値: カテゴリ名 / None(未設定) / KEEP(編集時の変更なし)。

    新規カテゴリはここではDBに作らない(最終確認後のトランザクション内で作成する)。
    """
    categories = repo.list_categories()
    print("カテゴリ:")
    if categories:
        for i, c in enumerate(categories, 1):
            print(f"  {i}. {c.name}")
    else:
        print("  (登録済みのカテゴリはありません)")

    hint = "番号で選択 / n で新規作成 / 0 で未設定"
    hint += f" / Enterで変更なし(現在: {current or NONE_TEXT})" if editing else " / Enterで未設定"

    while True:
        answer = prompt(f"カテゴリ [{hint}]: ").strip()
        if answer == "":
            return KEEP if editing else None
        if answer == "0":
            return None
        if answer.lower() == "n":
            raw = prompt("新しいカテゴリ名: ")
            try:
                name = validate_name(raw, "カテゴリ")
            except CtdError as e:
                print(f"エラー: {e}")
                continue
            existing = repo.find_category(name)
            if existing is not None:
                print(f"既存のカテゴリ「{existing['name']}」を使用します。")
                return existing["name"]
            return name
        if answer.isdigit() and 1 <= int(answer) <= len(categories):
            return categories[int(answer) - 1].name
        print("入力が正しくありません。")


def ask_tags(current: list[str] | None = None, editing: bool = False):
    """タグを入力させる。戻り値: タグ名のリスト / KEEP(編集時の変更なし)。"""
    if editing:
        raw = prompt(
            f"タグ [{join_tags(current or [])}] (カンマ区切りで全置換 / Enterで変更なし / - で全解除): "
        ).strip()
        if raw == "":
            return KEEP
        if raw == "-":
            return []
        return parse_tags(raw)
    return parse_tags(prompt("タグ(カンマ区切り・任意): "))


def print_note_summary(note: Note, with_dates: bool = True) -> None:
    pairs = [
        ("ID", note.id),
        ("タイトル", note.title),
        ("カテゴリ", note.category or NONE_TEXT),
        ("タグ", join_tags(note.tags)),
    ]
    if with_dates:
        pairs.append(("登録日時", format_local(note.created_at, with_seconds=True)))
    print_fields(pairs)


def warn_duplicates(rows) -> None:
    print("警告: 同じ内容のメモが既に登録されています。")
    for r in rows[:5]:
        print(f"  {short_id(r['id'])}  {r['title']}")
    if len(rows) > 5:
        print(f"  ...ほか{len(rows) - 5}件")


def clean_description(raw: str) -> str | None:
    text = display_name(raw)
    return text or None
