"""CTD共通の例外。"""

from __future__ import annotations


class CtdError(Exception):
    """利用者に表示すべきエラー(終了コード1)。"""

    exit_code = 1

    def __init__(self, message: str, hint: str | None = None):
        super().__init__(message)
        self.message = message
        self.hint = hint


class UsageError(CtdError):
    """引数・使用方法のエラー(終了コード2)。"""

    exit_code = 2

    def __init__(self, message: str, hint: str | None = None, usage: str | None = None):
        super().__init__(message, hint)
        self.usage = usage


class Cancelled(Exception):
    """利用者による中断(Ctrl+C / EOF / 確認でNo)。終了コードは0。"""
