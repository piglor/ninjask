"""Shared by the tests of the seeds, channels, candidates and draw log (test_n1_*.py): the fixtures recorded from
Areneon's pinned Showdown (tests/fixtures/areneon/README.md)."""

from __future__ import annotations

import gzip
import json
from functools import lru_cache
from pathlib import Path

import ninjask

FIXTURES = Path(__file__).resolve().parents[1] / "fixtures" / "areneon"
NAMES = ("p1", "p2")


@lru_cache(maxsize=1)
def dex():
    return ninjask.Dex.load(str(FIXTURES / "dex-gen9championsvgc2026regmc-7244fe8e.json"))


@lru_cache(maxsize=1)
def battles() -> tuple[dict, ...]:
    with gzip.open(FIXTURES / "battles.jsonl.gz", "rt") as f:
        return tuple(json.loads(line) for line in f)


@lru_cache(maxsize=1)
def golden_cases() -> tuple[tuple[dict, dict], ...]:
    packed = json.loads((FIXTURES / "choices-packed.json").read_text())
    out = []
    for path in sorted((FIXTURES / "choices").glob("*.json")):
        case = json.loads(path.read_text())
        out.append((case, packed[case["id"]]))
    return tuple(out)


def teams() -> list[str]:
    return [t for t in (FIXTURES / "teams.txt").read_text().split("\n") if t]


def is_refusal(error: str) -> bool:
    return error.startswith("[Unavailable choice]")
