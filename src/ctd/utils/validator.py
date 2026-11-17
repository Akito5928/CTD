"""入力値の検証と正規化。"""

from __future__ import annotations

import re
import unicodedata

from ..errors import CtdError, UsageError

ID_MIN_LENGTH = 8
_ID_PATTERN = re.compile(r"^[0-9a-f-]+$")
_TAG_SEPARATORS = re.compile(r"[,，、]")


def name_key(name: str) -> str:
    """カテゴリ名・タグ名の正規化キー(前後空白除去 + NFKC + casefold)。"""
    return unicodedata.normalize("NFKC", name).strip().casefold()


def display_name(name: str) -> str:
    """表示名。入力された表記のまま、前後の空白だけ除く。"""
    return name.strip()


def validate_name(name: str, kind: str) -> str:
    """名前が空でないことを確認し、表示名を返す。kind は「カテゴリ」「タグ」など。"""
    if not name_key(name):
        raise CtdError(f"{kind}名が空です。")
    return display_name(name)


def parse_tags(raw: str) -> list[str]:
    """カンマ区切りの入力をタグ表示名のリストにする。正規化キーが同じものは1つにまとめる。"""
    seen: set[str] = set()
    tags: list[str] = []
    for part in _TAG_SEPARATORS.split(raw):
        key = name_key(part)
        if not key or key in seen:
            continue
        seen.add(key)
        tags.append(display_name(part))
    return tags


def normalize_id_prefix(value: str) -> str:
    """ID入力(UUID前方一致)を検証して小文字にそろえる。"""
    text = value.strip().lower()
    if len(text) < ID_MIN_LENGTH:
        raise UsageError(f"IDは{ID_MIN_LENGTH}文字以上で指定してください。(入力: {value})")
    if not _ID_PATTERN.match(text):
        raise UsageError(f"IDの形式が正しくありません。英数字(0-9, a-f)とハイフンで指定してください。(入力: {value})")
    return text
