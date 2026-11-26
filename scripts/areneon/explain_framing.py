#!/usr/bin/env python3
"""For battles whose draws differ from Showdown's, say where: the first differing draw, the handlers of the list Showdown
sorted there, and the engine's call site that sorted its own list.

    python3 scripts/areneon/explain_framing.py --dex EXPORT.json INPUT.json [...]

INPUT is what `fuzz_replay.py --keep-failures` writes (or any scenario input of `scenario.cjs`): seeds, packed teams and
choices. Each is replayed on the pinned Showdown with its sort instrumented (`SCENARIO_HANDLERS`: every recorded `shuffle`
names the effects of the list, with holder, order, priority, speed and subOrder) and through the engine with its own trace
(`Battle.set_rng_trace`: every random call with the line of the engine that sorted). Prints, per battle, the first draw
that differs: Showdown's call and handlers against the engine's call and site.
"""
import argparse
import json
import os
import re
import subprocess
import sys
import tempfile
from pathlib import Path

import ninjask

HERE = Path(__file__).resolve().parent
NAMES = ("p1", "p2")
TRACE_DRAW = re.compile(r"^(random|randomChance|sample)\S* = .* @ (.+)$|^random\((\d+)\) = (\d+) @ (.+)$")


def draw_lines(trace):
    """The engine's trace as one (text, site) per random call; a call made inside a sort carries the site of that sort
    (the `# sort N @ site` entry before it), which is the engine line that built the list."""
    out, sort_site = [], None
    for line in trace:
        m = re.match(r"^# sort \d+ @ (\S+)(?: of .*)?$", line)
        if m:
            sort_site = m.group(1)
            continue
        if line.startswith("#") or line.startswith("pokemon"):
            continue
        m = re.match(r"^(.*) = (.*) @ (.+)$", line)
        out.append((m.group(1), sort_site or m.group(3)) if m else (line, "?"))
    return out


def sites_of(draws, lines):
    """The call site of each engine draw: a shuffle of `[start, end)` made `end - start - 1` random calls, every other kind one.
    The draws the start made (genders) come before the trace was on, so the lines are aligned from the end."""
    sizes = [max(int(a[2]) - int(a[1]) - 1, 0) if m == "shuffle" else 1 for m, a, _ in draws]
    at = len(lines) - sum(sizes)
    sites = []
    for n in sizes:
        sites.append(lines[at][1] if 0 <= at < len(lines) else "start")
        at += n
    return sites


def explain(dex, row, trace_writes):
    battle = ninjask.Battle.reference(dex, row["packed"]["p1"], row["packed"]["p2"], row["seed"], names=NAMES, record_draws=True)
    battle.set_rng_trace(True)
    mine = [battle.take_draws()]
    for c in row["choices"]:
        try:
            battle.choose(c["player"], c["choice"])
        except ValueError:
            pass
        mine.append(battle.take_draws())
    flat_engine = [d for w in mine for d in w]
    flat_showdown = [d for w in trace_writes for d in w["draws"]]
    sites = sites_of(flat_engine, draw_lines(battle.rng_trace()))
    for i, (s, e) in enumerate(zip(flat_showdown, flat_engine)):
        if (s["method"], s["args"], s["result"]) != (e[0], list(e[1]), list(e[2])):
            return i, s, e, sites[i]
    if len(flat_showdown) != len(flat_engine):
        return min(len(flat_showdown), len(flat_engine)), None, None, "length"
    return None


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--dex", required=True)
    ap.add_argument("inputs", nargs="+")
    a = ap.parse_args()
    dex = ninjask.Dex.load(a.dex)
    for path in a.inputs:
        games = json.load(open(path))
        with tempfile.TemporaryDirectory() as tmp:
            rows, draws = os.path.join(tmp, "rows.jsonl"), os.path.join(tmp, "draws.jsonl")
            subprocess.run(["node", str(HERE / "scenario.cjs"), path, rows, draws], check=True, env=dict(os.environ, SCENARIO_HANDLERS="1"))
            for rline, dline in zip(open(rows), open(draws)):
                row, trace = json.loads(rline), json.loads(dline)
                found = explain(dex, row, trace["writes"])
                if found is None:
                    print(f"{row['id']}: draws equal")
                    continue
                i, s, e, site = found
                print(f"{row['id']}: draw {i}")
                if s is not None:
                    print(f"  showdown {s['method']}{s['args']} -> {s['result']}")
                    for h in s.get("handlers", []):
                        print(f"     {h['id']} ({h.get('type')}, holder {h.get('holder')}) order {h.get('order')} priority {h.get('priority')} speed {h.get('speed')} sub {h.get('sub')}")
                    print(f"  engine   {e[0]}{list(e[1])} -> {list(e[2])} at {site}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
