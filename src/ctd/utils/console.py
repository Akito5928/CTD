"""コンソール入出力の補助(日本語の表示幅、対話入力、ページネーション)。"""

from __future__ import annotations

import shutil
import sys
import unicodedata
from typing import Callable, Sequence

from ..errors import Cancelled

try:  # 実行時の外部ライブラリはwcwidthのみ許可。無ければ標準ライブラリで代替する。
    from wcwidth import wcswidth as _wcswidth
except ImportError:  # pragma: no cover - 環境依存
    _wcswidth = None

ELLIPSIS = "..."


# ---- 表示幅 ---------------------------------------------------------------

def _char_width(ch: str) -> int:
    if unicodedata.combining(ch):
        return 0
    return 2 if unicodedata.east_asian_width(ch) in ("W", "F") else 1


def display_width(text: str) -> int:
    if _wcswidth is not None:
        width = _wcswidth(text)
        if width >= 0:
            return width
    return sum(_char_width(ch) for ch in text)


def pad(text: str, width: int) -> str:
    return text + " " * max(0, width - display_width(text))


def truncate(text: str, max_width: int) -> str:
    if display_width(text) <= max_width:
        return text
    limit = max(0, max_width - len(ELLIPSIS))
    result = ""
    used = 0
    for ch in text:
        w = display_width(ch)
        if used + w > limit:
            break
        result += ch
        used += w
    return result + ELLIPSIS


def single_line(text: str) -> str:
    return text.replace("\r", " ").replace("\n", " ").replace("\t", " ")


# ---- 端末の状態 -----------------------------------------------------------

def configure_stdio() -> None:
    """文字化け・例外終了を避けるため標準入出力の設定を行う。

    パイプ・リダイレクト時は入出力ともUTF-8にそろえる。端末(TTY)ではそのまま使い、
    出力で変換できない文字は ? にする。
    """
    for stream in (sys.stdin, sys.stdout, sys.stderr):
        reconfigure = getattr(stream, "reconfigure", None)
        if reconfigure is None:
            continue
        try:
            if stream.isatty():
                if stream is not sys.stdin:
                    reconfigure(errors="replace")
            else:
                reconfigure(encoding="utf-8", errors="replace")
        except Exception:  # pragma: no cover - 設定に失敗しても動作は続ける
            pass


def is_tty_output() -> bool:
    try:
        return bool(sys.stdout.isatty())
    except Exception:  # pragma: no cover
        return False


def terminal_width() -> int | None:
    """端末幅。TTYでなければNone(折り返し・省略なしで出力する)。"""
    if not is_tty_output():
        return None
    return max(40, shutil.get_terminal_size((100, 24)).columns - 1)


# ---- 対話入力 -------------------------------------------------------------

def prompt(text: str) -> str:
    """1行入力。Ctrl+C / EOF はキャンセルとして扱う。"""
    try:
        return input(text)
    except (EOFError, KeyboardInterrupt):
        print()
        raise Cancelled()


def confirm(question: str, default: bool = False) -> bool:
    suffix = "[Y/n]" if default else "[y/N]"
    while True:
        answer = prompt(f"{question} {suffix} ").strip().lower()
        if answer == "":
            return default
        if answer in ("y", "yes"):
            return True
        if answer in ("n", "no"):
            return False
        print("y か n で答えてください。")


# ---- 表示 -----------------------------------------------------------------

def print_fields(pairs: Sequence[tuple[str, str]]) -> None:
    width = max(display_width(label) for label, _ in pairs)
    for label, value in pairs:
        print(f"{pad(label, width)} : {value}")


def render_table(
    headers: Sequence[str],
    rows: Sequence[Sequence[str]],
    flex_col: int | None = None,
    total_width: int | None = None,
) -> list[str]:
    """表を文字列の行リストにする。total_width指定時はflex_col列を縮めて収める。"""
    gap = 2
    cells = [[single_line(str(c)) for c in row] for row in rows]
    widths = [display_width(h) for h in headers]
    for row in cells:
        for i, cell in enumerate(row):
            widths[i] = max(widths[i], display_width(cell))

    if total_width is not None and flex_col is not None:
        others = sum(w for i, w in enumerate(widths) if i != flex_col) + gap * (len(widths) - 1)
        allowed = max(10, total_width - others)
        if widths[flex_col] > allowed:
            widths[flex_col] = allowed
            for row in cells:
                row[flex_col] = truncate(row[flex_col], allowed)

    sep = " " * gap
    lines = [sep.join(pad(h, widths[i]) for i, h in enumerate(headers)).rstrip()]
    lines.append(sep.join("-" * w for w in widths))
    for row in cells:
        lines.append(sep.join(pad(c, widths[i]) for i, c in enumerate(row)).rstrip())
    return lines


def paginate(
    total: int,
    fetch: Callable[[int, int | None], list],
    render: Callable[[list], None],
    page_size: int = 20,
) -> None:
    """ページ単位で表示する。標準出力がTTYでなければ全件を一度に出力する。"""
    if not is_tty_output():
        render(fetch(0, None))
        return

    offset = 0
    while offset < total:
        rows = fetch(offset, page_size)
        if not rows:
            break
        render(rows)
        offset += len(rows)
        if offset >= total:
            break
        try:
            answer = prompt(f"-- {offset}/{total}件を表示 -- [Enter: 次のページ / q: 終了] ")
        except Cancelled:
            break
        if answer.strip().lower() == "q":
            break
