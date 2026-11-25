#!/usr/bin/env python3
"""Which of the engine's sorted-event lists a corpus of recorded battles exercises, and which of them it has seen draw
between ties.

Showdown sorts the handlers of an event (`speedSort`) and shuffles runs of equal ones; the engine builds each such list by
hand (`speed_sort_by`, `each_event_order` in src/), and a list that leaves out a handler which never acts changes the
framing of the shuffle (`shuffle(len, start, end)`: the length) although the numbers drawn are equal. The recorded
battles' draws are compared with Showdown's call for call (scripts/check_recorded.py): a site whose shuffles occur in the
corpus and equal Showdown's is checked there; a site that sorted but never drew, or that no battle reached, is not.

    python3 scripts/areneon/sort_sites.py --dex EXPORT.json --src . [--limit N] RECORDING.jsonl.gz [...]

Replays every battle's choices with the engine's own sort trace (`Battle.set_rng_trace`) and prints, per call site found
in the source, the number of sorts (lists of two or more), the sorts that drew (a tie), the battles with a draw there,
and the lengths of the lists that drew.
"""
import argparse
import collections
import gzip
import json
import re
import sys
from pathlib import Path

import ninjask

NAMES = ("p1", "p2")
CALL = re.compile(r"\.(speed_sort_by|speed_sort|each_event_order|each_event_order_draws)\(")
SORT = re.compile(r"^# sort (\d+) @ (.+)$")
DRAW = re.compile(r"^(?:random|randomChance|sample|shuffle)[^@]*@ (.+)$")


def opener(path):
    return gzip.open(path, "rt") if str(path).endswith(".gz") else open(path)


def static_sites(src: Path):
    sites = {}
    for path in sorted((src / "src").rglob("*.rs")):
        for n, line in enumerate(path.read_text().splitlines(), 1):
            if CALL.search(line) and "fn " not in line and not line.lstrip().startswith("//"):
                sites[f"{path.relative_to(src)}:{n}"] = line.strip()[:70]
    return sites


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--dex", required=True)
    ap.add_argument("--src", default=".")
    ap.add_argument("--limit", type=int, default=0)
    ap.add_argument("recordings", nargs="+")
    a = ap.parse_args()
    dex = ninjask.Dex.load(a.dex)
    sorts = collections.Counter()
    drew = collections.Counter()
    battles = collections.defaultdict(set)
    lengths = collections.defaultdict(collections.Counter)
    n = 0
    for path in a.recordings:
        for k, line in enumerate(opener(path)):
            if a.limit and k >= a.limit:
                break
            row = json.loads(line)
            if "failure" in row:
                continue
            n += 1
            b = ninjask.Battle.reference(dex, row["packed"]["p1"], row["packed"]["p2"], row["seed"], names=NAMES)
            b.set_rng_trace(True)
            for c in row["choices"]:
                try:
                    b.choose(c["player"], c["choice"])
                except ValueError:
                    pass
            current = None
            for entry in b.rng_trace():
                m = SORT.match(entry)
                if m:
                    current = None
                    if int(m.group(1)) >= 2:
                        sorts[m.group(2)] += 1
                        current = (m.group(2), int(m.group(1)), False)
                    continue
                d = DRAW.match(entry)
                if d and current and d.group(1) == current[0]:
                    if not current[2]:
                        drew[current[0]] += 1
                        battles[current[0]].add((path, k))
                        lengths[current[0]][current[1]] += 1
                        current = (current[0], current[1], True)
    static = static_sites(Path(a.src))
    print(f"{n} battles; {len(static)} call sites of the sorting helpers in src/, {len(sorts)} reached with a list of two or more, "
          f"{len(drew)} drew between ties")
    print(f"{'site':36} {'sorts':>9} {'drew':>8} {'battles':>8}  lengths that drew")
    for site in sorted(set(static) | set(sorts)):
        lens = ",".join(f"{l}x{c}" for l, c in sorted(lengths[site].items()))
        print(f"{site:36} {sorts[site]:9} {drew[site]:8} {len(battles[site]):8}  {lens}" + ("" if site in static else "  (not a direct call: a wrapper's)"))
    never = sorted(s for s in static if not drew[s])
    print(f"\nnever drew between ties in the corpus ({len(never)} of {len(static)} call sites):")
    for s in never:
        print(f"  {s:36} {static[s]}  [{'sorted, no tie' if sorts[s] else 'not reached'}]")


if __name__ == "__main__":
    sys.exit(main())
