#!/usr/bin/env python3
"""Every sorted list of two or more the engine makes, against the ones the pinned Showdown makes, battle by battle.

    python3 scripts/areneon/sort_sequence.py --dex EXPORT.json [--limit N] [--show K] INPUT [...]

INPUT is a scenario input (`scenario.cjs`: a JSON array or JSON Lines, or gzipped; the recorded corpus is one, and so is
what `fuzz_replay.py --export` writes). Each battle is replayed on Showdown with `SCENARIO_SORTS=1` (every `speedSort` of
two or more, in call order, with what it holds) and through the engine with its own trace (`Battle.set_rng_trace`: one
`# sort N @ site` entry per list of two or more). The two sequences must be the same length for length: a shuffle shows a
handler the engine's list leaves out only where handlers tie, but the *length* of a sorted list shows it wherever the
list is sorted, tie or not, and a list the engine sorts that Showdown does not (or the reverse) shows as well.

Prints the number of battles and sorts compared and, per site of the engine, how many sorts of which lengths were
equal; then the first difference of each differing battle (`--show`, default 10): Showdown's sorts around it with their
contents and the engine's with their call sites.
"""
import argparse
import collections
import difflib
import gzip
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
SORT = re.compile(r"^# sort (\d+) @ (\S+) of (.+)$")


def opener(path):
    return gzip.open(path, "rt") if str(path).endswith(".gz") else open(path)


def battles(path):
    text = opener(path).read()
    return json.loads(text) if text.lstrip().startswith("[") else [json.loads(l) for l in text.split("\n") if l]


def tie_capable(sort):
    """Two of the list's entries have the same order, priority and subOrder (a handler's `speed` is the only thing left
    that could separate them), so a tie, and with it a shuffle, can happen in this list."""
    seen = set()
    for k in sort.get("keys", []):
        key = (k[0], k[1], k[2])
        if key in seen:
            return True
        seen.add(key)
    return False


def engine_sorts(dex, row):
    battle = ninjask.Battle.reference(dex, row["packed"]["p1"], row["packed"]["p2"], row["seed"], names=NAMES)
    battle.set_rng_trace(True)
    for c in row["choices"]:
        try:
            battle.choose(c["player"], c["choice"])
        except ValueError:
            pass
    # the engine also traces the sorts of fewer than two, which Showdown's `speedSort` returns from at once
    return [(int(m.group(1)), m.group(2), m.group(3)) for line in battle.rng_trace() if (m := SORT.match(line)) and int(m.group(1)) >= 2]


HANDLER_TYPES = ("Handler", "::H,", "::H)", "::B,")


def engine_is_handlers(entry):
    """A list of handlers (what the engine builds by hand) as opposed to the active Pokémon or the queue's actions."""
    # a bare `i32` is the Unaware holders' stored speeds (ModifyBoost handlers)
    return entry[2] == "i32" or any(t in entry[2] for t in HANDLER_TYPES)


def showdown_is_handlers(sort):
    # a list of Pokémon sorted inside a `runEvent` is a handler's own sort (Magician sorts `move.hitTargets`)
    inside_run_event = sort.get("event", "").startswith(("runEvent", "singleEvent"))
    return inside_run_event or not all(x.startswith(("pokemon:", "action:")) for x in sort["items"])


def intervals(entries, is_handlers):
    """Split a sequence at its anchors (the lists of Pokémon and actions, which both sides build alike): the handler lists
    between two anchors, as the lengths."""
    out, cur, anchors = [], [], []
    for e in entries:
        if is_handlers(e):
            cur.append(e)
        else:
            out.append(cur)
            cur = []
            anchors.append(e)
    out.append(cur)
    return out, anchors


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--dex", required=True)
    ap.add_argument("--limit", type=int, default=0)
    ap.add_argument("--show", type=int, default=10)
    ap.add_argument("--chunk", type=int, default=200)
    ap.add_argument("inputs", nargs="+")
    a = ap.parse_args()
    dex = ninjask.Dex.load(a.dex)
    rows = []
    for path in a.inputs:
        for i, r in enumerate(battles(path)):
            r.setdefault("id", f"{Path(path).name.split('.')[0]}-{i}")
            rows.append(r)
    if a.limit:
        rows = rows[: a.limit]
    battles_done = sorts = ties = matched = anchor_bad = clean = 0
    all_handler = 0
    classes = collections.Counter()
    examples = {}
    shown = 0
    for start in range(0, len(rows), a.chunk):
        chunk = rows[start : start + a.chunk]
        with tempfile.TemporaryDirectory() as tmp:
            src, out_rows, out_draws = (os.path.join(tmp, n) for n in ("in.json", "rows.jsonl", "draws.jsonl"))
            Path(src).write_text(json.dumps([{k: r[k] for k in ("id", "seed", "packed", "choices", "turn_limit") if k in r} for r in chunk]))
            subprocess.run(["node", str(HERE / "scenario.cjs"), src, out_rows, out_draws], check=True, env={**os.environ, "SCENARIO_SORTS": "1"})
            with open(out_draws) as f:
                traces = [json.loads(l) for l in f]
        for row, trace in zip(chunk, traces):
            sd = [s for w in trace["writes"] for s in w.get("sorts", [])]
            en = engine_sorts(dex, row)
            battles_done += 1
            sorts += len(sd)
            sd_int, sd_anchors = intervals(sd, showdown_is_handlers)
            en_int, en_anchors = intervals(en, engine_is_handlers)
            bad = False
            if [x["n"] for x in sd_anchors] != [x[0] for x in en_anchors]:
                anchor_bad += 1
                bad = True
                if shown < a.show:
                    shown += 1
                    n = next((i for i, (x, y) in enumerate(zip(sd_anchors, en_anchors)) if x["n"] != y[0]), min(len(sd_anchors), len(en_anchors)))
                    print(f"{row['id']}: anchor {n} differs: showdown {sd_anchors[n]['n'] if n < len(sd_anchors) else None} "
                          f"engine {en_anchors[n][:2] if n < len(en_anchors) else None}")
                continue
            for sdi, eni in zip(sd_int, en_int):
                have = collections.Counter(e[0] for e in eni)
                for s in sdi:
                    all_handler += 1
                    if not tie_capable(s):
                        continue
                    ties += 1
                    if have[s["n"]] > 0:
                        have[s["n"]] -= 1
                        matched += 1
                    else:
                        bad = True
                        kind = s.get("event", "") + " | " + ", ".join(sorted(x.split("@")[0] for x in s["items"]))
                        classes[(kind, s["n"])] += 1
                        examples.setdefault((kind, s["n"]), row["id"])
            clean += 0 if bad else 1
        print(f"... {min(start + a.chunk, len(rows))} battles", file=sys.stderr, flush=True)
    print(f"{battles_done} battles, {sorts} sorts of two or more in Showdown; the lists of Pokémon and actions (the anchors) differ in {anchor_bad} battles")
    print(f"handler lists: {all_handler} sorted by Showdown, {ties} of them with a possible tie (two entries of one order, priority and subOrder), "
          f"{matched} of those with an engine list of the same length between the same anchors; {clean} battles with every one matched")
    print("tie-capable Showdown lists with no engine list of that length (count, event | effects, length, example):")
    for (kind, n), count in classes.most_common():
        print(f"  {count:4}  {kind}  (length {n}, e.g. {examples[(kind, n)]})")
    return 0 if not classes and not anchor_bad else 1


if __name__ == "__main__":
    sys.exit(main())
