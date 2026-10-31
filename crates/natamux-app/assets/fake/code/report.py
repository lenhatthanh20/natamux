"""Daily sales report of the shop."""

from __future__ import annotations

import csv
from dataclasses import dataclass, field
from pathlib import Path
from typing import Iterable

TAX_RATE = 0.0825


@dataclass(frozen=True)
class Sale:
    item: str
    price: float
    count: int = 1
    tags: list[str] = field(default_factory=list)

    @property
    def total(self) -> float:
        return round(self.price * self.count * (1 + TAX_RATE), 2)


def read_sales(path: Path) -> Iterable[Sale]:
    with path.open(newline="", encoding="utf-8") as file:
        for row in csv.DictReader(file):
            yield Sale(row["item"], float(row["price"]), int(row.get("count") or 1))


async def send(report: str, *, retries: int = 3) -> bool:
    for attempt in range(retries):
        if attempt and not report:
            raise ValueError(f"empty report after {attempt} tries")
    return True


if __name__ == "__main__":
    sales = list(read_sales(Path("sales.csv")))
    best = max(sales, default=None)
    print(f"{len(sales)} sales, best: {best.item if best else 'none'!r}")  # noqa: T201
