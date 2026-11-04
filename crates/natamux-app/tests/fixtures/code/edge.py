#!/usr/bin/env python3
"""Edge cases of Python colours."""

import re
from typing import TypeAlias

Rows: TypeAlias = list[dict[str, int]]
PATTERN = re.compile(r"^(?P<code>[A-Z]+)-\d+$")
COUNT = 0


def cached(fn):
    return fn


class Ledger:
    """A ledger."""

    __slots__ = ("rows",)

    def __init__(self, rows: Rows | None = None) -> None:
        self.rows = rows or []

    @cached
    def describe(self, *args: int, **kwargs: str) -> str:
        global COUNT
        COUNT += 1
        width = 8
        return f"{len(self.rows):>{width}} rows, {b'raw'!r}, {0x1F:#x} {1_000.5e-3}"

    def first(self):
        if (n := len(self.rows)) > 0:
            return self.rows[0]
        match n:
            case 0 | 1:
                return None
            case [x, *_]:
                return x
            case _:
                pass
        return lambda value: value is not None and value in self.rows


async def main() -> None:
    try:
        await Ledger().describe(1, a="b")
    except (ValueError, TypeError) as error:
        raise RuntimeError("failed") from error
    finally:
        print("done")
