#!/usr/bin/env python3
"""Fresh battles played by the engine, replayed on the real Showdown, and compared in every way check_recorded.py knows.

    python3 scripts/areneon/fuzz_replay.py --dex EXPORT.json --pool POOL.txt --mode mirror|cross --battles N [--base 0] [--chunk 500] [--keep-failures DIR]

The engine (the fast `Batch`, Showdown seeds, no channels) plays each battle with a random non-pending candidate for
each decision; the choices are then written to `scenario.cjs`, which replays them on the pinned Showdown and records the
channels and every draw, and `check_recorded.check` compares: request hashes, candidates, both channels and the
omniscient log, the engine's draws against Showdown's call for call, and a forced replay on the fast generator. `mirror`
gives both sides the same team (equal speeds everywhere: every tie is drawn), `cross` two teams of the pool. Battles are
done in chunks of `--chunk`, so memory stays bounded. Prints the totals per check and the first difference of each failing
battle; a failing battle's row is kept in `--keep-failures` when given. Exit status 1 when anything differs.
"""
import argparse
import collections
import gzip
import importlib.util
import json
import os
import random
import subprocess
import sys
import tempfile
from pathlib import Path

import ninjask

HERE = Path(__file__).resolve().parent
NAMES = ("p1", "p2")


def load_checker():
    spec = importlib.util.spec_from_file_location("check_recorded", HERE.parent / "check_recorded.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def play(dex, pool, mode, g):
    rng = random.Random(g * 7919 + 13)
    t1 = rng.choice(pool)
    t2 = t1 if mode == "mirror" else rng.choice(pool)
    seed = f"sodium,{rng.getrandbits(128):032x}"
    batch = ninjask.Batch(dex, 1, names=NAMES, channels=False)
    batch.turn_limit = 100
    batch.reset(0, t1, t2, seed)
    choices = []
    for _ in range(800):
        pending = batch.pending()
        if not pending:
            break
        for i, side in pending:
            candidates = batch.candidates(i, side)
            ok = [k for k, (_, p) in enumerate(candidates) if not p] or list(range(len(candidates)))
            k = rng.choice(ok)
            choices.append({"player": NAMES[side], "choice": candidates[k][0]})
            try:
                batch.choose_candidates([(i, side, k)])
            except ValueError:
                pass
    return {"id": f"{mode}-{g}", "seed": seed, "packed": {"p1": t1, "p2": t2}, "choices": choices, "turn_limit": 100}


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--dex", required=True)
    ap.add_argument("--pool", required=True)
    ap.add_argument("--mode", choices=["mirror", "cross"], required=True)
    ap.add_argument("--battles", type=int, help="how many battles, from --base")
    ap.add_argument("--games", help="comma-separated battle numbers instead of --battles/--base (a battle is a function of its number and the engine)")
    ap.add_argument("--export", help="write the games played, as scenario inputs (with their purpose empty), to this JSON file")
    ap.add_argument("--base", type=int, default=0)
    ap.add_argument("--chunk", type=int, default=500)
    ap.add_argument("--keep-failures")
    a = ap.parse_args()
    dex = ninjask.Dex.load(a.dex)
    pool = [l for l in Path(a.pool).read_text().split("\n") if l]
    checker = load_checker()
    totals, failures = collections.Counter(), collections.Counter()
    shown = 0
    numbers = [int(x) for x in a.games.split(",")] if a.games else list(range(a.base, a.base + (a.battles or 0)))
    if not numbers:
        ap.error("give --battles or --games")
    exported = []
    for start in range(0, len(numbers), a.chunk):
        games = [play(dex, pool, a.mode, g) for g in numbers[start : start + a.chunk]]
        exported += games
        with tempfile.TemporaryDirectory() as tmp:
            src, rows, draws = (os.path.join(tmp, n) for n in ("in.json", "rows.jsonl", "draws.jsonl"))
            Path(src).write_text(json.dumps(games))
            subprocess.run(["node", str(HERE / "scenario.cjs"), src, rows, draws], check=True)
            with open(rows) as r, open(draws) as d:
                for rline, dline in zip(r, d):
                    row, trace = json.loads(rline), json.loads(dline)
                    result = checker.check(row, dex, trace["writes"])
                    bad = False
                    for name, problem in result.items():
                        totals[name] += 1
                        if problem:
                            failures[name] += 1
                            bad = True
                            if shown < 40:
                                shown += 1
                                print(f"{row['id']}: {name}: {problem}", flush=True)
                    if bad and a.keep_failures:
                        out = Path(a.keep_failures)
                        out.mkdir(parents=True, exist_ok=True)
                        (out / f"{row['id']}.json").write_text(json.dumps([{k: row[k] for k in ("id", "seed", "packed", "choices", "turn_limit") if k in row}]))
        print(f"... {start + len(games)} battles", file=sys.stderr, flush=True)
    if a.export:
        Path(a.export).write_text(json.dumps(exported))
    print(f"{a.mode}: {len(numbers)} battles" + ("" if a.games else f" from {a.base}"))
    for name in totals:
        print(f"  {name:11} {totals[name] - failures[name]:6} of {totals[name]} equal")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
