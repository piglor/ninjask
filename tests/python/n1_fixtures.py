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


@lru_cache(maxsize=1)
def scenarios() -> tuple[tuple[dict, list[dict]], ...]:
    """The hand-built scenarios (scripts/areneon/make_scenarios.py): each row with the writes of its draw trace."""
    with gzip.open(FIXTURES / "scenarios.jsonl.gz", "rt") as f:
        rows = [json.loads(line) for line in f]
    with gzip.open(FIXTURES / "scenarios-draws.jsonl.gz", "rt") as f:
        traces = {t["id"]: t["writes"] for t in map(json.loads, f)}
    return tuple((row, traces[row["id"]]) for row in rows)


@lru_cache(maxsize=1)
def recorded_checker():
    """scripts/check_recorded.py, the checker the tests share with the 3,200-battle run, loaded by path."""
    import importlib.util

    path = Path(__file__).resolve().parents[2] / "scripts" / "check_recorded.py"
    spec = importlib.util.spec_from_file_location("check_recorded", path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def teams() -> list[str]:
    return [t for t in (FIXTURES / "teams.txt").read_text().split("\n") if t]


def is_refusal(error: str) -> bool:
    return error.startswith("[Unavailable choice]")
