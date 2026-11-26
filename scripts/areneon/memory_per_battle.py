#!/usr/bin/env python3
"""Resident memory a battle costs with channels off, with channels on and never read, and with channels read and
discarded after each step: N battles on generated teams play STEPS decisions each (a random candidate per side), and the
process's resident set is read before and after.

    python3 scripts/areneon/memory_per_battle.py [N=1024] [STEPS=73] [MODE]

MODE is one of `off`, `unread`, `discard`; with none the three run in turn, each in a fresh process (so the figures do
not leak into each other). The result is KiB per battle: what making the batch (reset, Team Preview) cost, and what playing the steps added.
"""
import os
import random
import subprocess
import sys
from pathlib import Path

import ninjask

FIX = Path(__file__).resolve().parents[2] / "tests" / "fixtures" / "areneon"
NAMES = ("p1", "p2")


def rss_kib() -> int:
    for line in open("/proc/self/status"):
        if line.startswith("VmRSS:"):
            return int(line.split()[1])
    raise RuntimeError("no VmRSS")


def run(n: int, steps: int, mode: str) -> tuple[float, float]:
    dex = ninjask.Dex.load(str(FIX / "dex-gen9championsvgc2026regmc-7244fe8e.json"))
    pool = [t for t in (FIX / "teams.txt").read_text().split("\n") if t]
    rnd = random.Random(5)
    start = rss_kib()
    batch = ninjask.Batch(dex, n, names=NAMES, channels=mode != "off")
    batch.reset_many(
        list(range(n)),
        [pool[rnd.randrange(len(pool))] for _ in range(n)],
        [pool[rnd.randrange(len(pool))] for _ in range(n)],
        [f"sodium,{i + 1:032x}" for i in range(n)],
    )
    base = rss_kib()
    seen = [[0, 0] for _ in range(n)]
    for _ in range(steps):
        pending = batch.pending()
        picks = []
        for i, side in pending:
            picks.append((i, side, rnd.randrange(len(batch.candidates(i, side)))))
        batch.choose_candidates(picks)
        if mode == "discard":
            for i in range(n):
                for s, viewer in enumerate(NAMES):
                    length = batch.channel_len(i, viewer)
                    batch.channel(i, viewer, seen[i][s])
                    seen[i][s] = length
                    batch.discard_channel(i, viewer, length)
                length = batch.channel_len(i, "omniscient")
                batch.discard_channel(i, "omniscient", length)
    return (base - start) / n, (rss_kib() - base) / n


def main():
    n = int(sys.argv[1]) if len(sys.argv) > 1 else 1024
    steps = int(sys.argv[2]) if len(sys.argv) > 2 else 73
    if len(sys.argv) > 3:
        made, played = run(n, steps, sys.argv[3])
        print(f"{sys.argv[3]:8} {made:5.0f} KiB per battle to make the batch, {played:5.0f} KiB more to play {steps} decisions")
        return
    for mode in ("off", "unread", "discard"):
        out = subprocess.run([sys.executable, __file__, str(n), str(steps), mode], check=True, capture_output=True, text=True)
        print(out.stdout.strip())


if __name__ == "__main__":
    main()
