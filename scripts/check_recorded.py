#!/usr/bin/env python3
"""Replay recorded Showdown battles into the engine and compare what a player is sent.

Input: JSON Lines (optionally gzipped) of battles recorded from Pokémon Showdown's own streams, one object per
battle (the format of tests/fixtures/areneon/README.md, and of the recorder of Areneon's training harness):
`seed`, `packed` (both teams), `choices` (every submitted choice in order, `refused` set when Showdown refused it),
`channels` (each player's stream, `|request|` lines included), `omniscient` or `omniscient_log`, `requests`
(`request_hash`, and `legal`: the candidates of the reference enumerator), and optionally the Showdown draw traces
(`--draws FILE`, one object per battle with `writes[].draws`).

For every battle the checks are, each reported as a count of battles:

  channels    both players' channels and the omniscient log equal Showdown's line for line
  hashes      every request in the stream has the request_hash the recording has
  candidates  at every request the candidates equal the reference lists, in order, `pending` included
  draws       the engine's draws equal Showdown's, call for call (with --draws)
  forced      the same game is played by an engine on the fast generator with Showdown's draws forced (with --draws)

    python scripts/check_recorded.py --dex EXPORT.json [--draws DRAWS.jsonl.gz] BATTLES.jsonl.gz [...] [--limit N]

Exit status 1 when any check fails; the first difference of each failing battle is printed.
"""

from __future__ import annotations

import argparse
import gzip
import hashlib
import json
import sys
from collections import Counter

import ninjask

NAMES = ("p1", "p2")


def opener(path):
    return gzip.open(path, "rt") if str(path).endswith(".gz") else open(path)


def request_hash(line: str) -> str:
    request = json.loads(line[len("|request|"):])
    request.pop("rqid", None)
    text = json.dumps(request, sort_keys=True, separators=(",", ":"), ensure_ascii=False)
    return hashlib.sha256(text.encode()).hexdigest()


def first_difference(a, b):
    for i in range(min(len(a), len(b))):
        if a[i] != b[i]:
            return i, a[i], b[i]
    return (min(len(a), len(b)), None, None) if len(a) != len(b) else None


def draw_tuples(draws):
    return [(d["method"], list(d["args"]), list(d["result"])) for d in draws]


def check(row, dex, writes):
    """Returns a dict check -> None (passed) or a description of the first difference."""
    out = {}
    packed, seed = row["packed"], row["seed"]
    omniscient = row.get("omniscient") or row["omniscient_log"]

    # channels and request hashes, through the batched API
    batch = ninjask.Batch(dex, 1, names=NAMES, channels=True)
    batch.reset(0, packed["p1"], packed["p2"], seed)
    seen = {"p1": 0, "p2": 0}
    seq = {"p1": 0, "p2": 0}
    out["hashes"] = None
    out["candidates"] = None

    def drain():
        for s, viewer in enumerate(NAMES):
            new = batch.channel(0, viewer, seen[viewer])
            seen[viewer] += len(new)
            requests = [line for line in new if line.startswith("|request|")]
            for i, line in enumerate(requests):
                recorded = row["requests"][viewer][seq[viewer]]
                if out["hashes"] is None and request_hash(line) != recorded["request_hash"]:
                    out["hashes"] = f"{viewer} request {seq[viewer]}"
                if i == len(requests) - 1 and out["candidates"] is None:
                    # only the current request has candidates
                    got = batch.candidates(0, s)
                    legal = recorded.get("legal")
                    if legal is None:
                        if got:
                            out["candidates"] = f"{viewer} request {seq[viewer]}: candidates for a wait request"
                    elif [c for c, p in got if not p] != legal["choices"] or [c for c, p in got if p] != legal["pending"]:
                        out["candidates"] = f"{viewer} request {seq[viewer]}"
                seq[viewer] += 1

    drain()
    for c in row["choices"]:
        flags = batch.choose([(0, NAMES.index(c["player"]), c["choice"])])
        if bool(flags[0] & 1) != bool(c.get("refused")) or flags[0] & 4:
            out["channels"] = f"choice {c['choice']!r} of {c['player']}: flags {flags[0]}"
            return out
        drain()
    out["channels"] = None
    for viewer in NAMES:
        d = first_difference(row["channels"][viewer], batch.channel(0, viewer))
        if d:
            out["channels"] = f"{viewer} line {d[0]}: showdown {d[1]!r} engine {d[2]!r}"
            break
    if out["channels"] is None:
        d = first_difference(omniscient, batch.channel(0, "omniscient"))
        if d:
            out["channels"] = f"omniscient line {d[0]}: showdown {d[1]!r} engine {d[2]!r}"

    if writes is None:
        return out
    # the engine's own draws against Showdown's
    sd = [draw_tuples(w["draws"]) for w in writes]
    replay = ninjask.Battle.reference(dex, packed["p1"], packed["p2"], seed, names=NAMES, record_draws=True)
    mine = [replay.take_draws()]
    for c in row["choices"]:
        try:
            replay.choose(c["player"], c["choice"])
        except ValueError:
            pass
        mine.append(replay.take_draws())
    flat_sd = [x for w in sd for x in w]
    flat_mine = [x for w in mine for x in w]
    d = first_difference(flat_sd, flat_mine)
    out["draws"] = None if d is None else f"draw {d[0]}: showdown {d[1]} engine {d[2]}"

    # forced: the fast generator, every call answered from Showdown's draws
    forced = ninjask.Battle(dex, packed["p1"], packed["p2"], 12345, names=NAMES, channels=True,
                            forced_draws=sd[0] + sd[1])
    forced.take_draws()
    out["forced"] = None
    for k, c in enumerate(row["choices"]):
        forced.set_forced_draws(sd[2 + k])
        try:
            forced.choose(c["player"], c["choice"])
        except ValueError:
            pass
        made = forced.take_draws()
        if made != sd[2 + k]:
            out["forced"] = f"choice {k}: the engine made {len(made)} calls, Showdown {len(sd[2 + k])}"
            break
    if out["forced"] is None:
        for viewer in NAMES:
            if forced.channel(viewer) != row["channels"][viewer]:
                out["forced"] = f"{viewer} channel differs"
    return out


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--dex", required=True)
    ap.add_argument("--draws", help="Showdown draw traces, aligned with the battles (one file per battles file, comma separated)")
    ap.add_argument("--limit", type=int, default=0, help="battles per file (0: all)")
    ap.add_argument("battles", nargs="+")
    a = ap.parse_args()
    dex = ninjask.Dex.load(a.dex)
    draw_files = a.draws.split(",") if a.draws else [None] * len(a.battles)
    totals, failures, n = Counter(), Counter(), 0
    for path, draw_path in zip(a.battles, draw_files):
        draw_rows = opener(draw_path) if draw_path else None
        for k, line in enumerate(opener(path)):
            if a.limit and k >= a.limit:
                break
            row = json.loads(line)
            drow = json.loads(next(draw_rows)) if draw_rows else None
            if "failure" in row:
                continue
            if drow is not None and drow["seed"] != row["seed"]:
                sys.exit(f"{draw_path}: draw trace {k} is for another battle")
            n += 1
            result = check(row, dex, drow["writes"] if drow else None)
            for name, problem in result.items():
                totals[name] += 1
                if problem:
                    failures[name] += 1
                    print(f"{path}:{row.get('id', row.get('index'))}: {name}: {problem}")
    print(f"{n} battles")
    for name in totals:
        print(f"  {name:11} {totals[name] - failures[name]:6} of {totals[name]} equal")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
