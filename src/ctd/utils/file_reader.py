"""TXTファイルの読み込みと検証。"""

from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path

from ..errors import CtdError
from .hash import sha256_text


@dataclass(frozen=True)
class TextFile:
    path: Path  # 絶対パス
    content: str  # BOM除去・改行LF正規化済み
    content_hash: str  # 正規化後の本文のSHA-256


def _strip_quotes(text: str) -> str:
    text = text.strip()
    if len(text) >= 2 and text[0] == text[-1] and text[0] in "\"'":
        return text[1:-1]
    return text


def read_text_file(path_text: str) -> TextFile:
    given = _strip_quotes(path_text)
    path = Path(given).expanduser()

    if not path.exists():
        raise CtdError(f"ファイルが見つかりません: {given}")
    if not path.is_file():
        raise CtdError(f"ファイルではありません(フォルダが指定されています): {given}")
    if path.suffix.lower() != ".txt":
        raise CtdError(f"TXTファイル(.txt)のみ登録できます: {given}")

    try:
        raw = path.read_bytes()
    except OSError as e:
        raise CtdError(f"ファイルを読み込めません: {given} ({e.strerror or e})")

    try:
        text = raw.decode("utf-8-sig")  # UTF-8 BOMは除去される
    except UnicodeDecodeError:
        raise CtdError(
            f"UTF-8として読み込めません: {given}",
            hint="UTF-8で保存し直してください。(Shift_JISなど他の文字コードには対応していません)",
        )
    if "\x00" in text:
        raise CtdError(
            f"テキストファイルとして読み込めません: {given}",
            hint="バイナリ、またはUTF-16形式の可能性があります。UTF-8で保存し直してください。",
        )

    text = text.replace("\r\n", "\n").replace("\r", "\n")
    if not text.strip():
        raise CtdError(f"ファイルが空です(空白・改行のみのファイルは登録できません): {given}")

    return TextFile(path=path.resolve(), content=text, content_hash=sha256_text(text))
