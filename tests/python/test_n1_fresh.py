"""Candidates on fresh requests, judged by the engine itself.

Battles are played from generated teams by policies that pick any candidate, pending ones included. At every
request of every player:

- a candidate that is not pending is accepted by the engine (a refusal would be a defect of the enumerator);
- a pending candidate is accepted or refused as `[Unavailable choice]` (never as invalid), and the refusal sends the
  request again, changed;
- (a sample of the requests) no choice the engine accepts is missing: every syntactic choice string of the
  universe of its phase is submitted to a copy of the battle, and each accepted one, in the engine's own spelling,
  is among the candidates (the brute-force oracle of Areneon's choice corpus, ADR-006).

The size is `NINJASK_N1_BATTLES` battles (default a few; the evidence run is thousands) and
`NINJASK_N1_BRUTE_EVERY` (every n-th request is brute-forced).
"""

from __future__ import annotations

import json
import os
import random

import ninjask

from n1_fixtures import NAMES, dex, teams

BATTLES = int(os.environ.get("NINJASK_N1_BATTLES", "12"))
BRUTE_EVERY = int(os.environ.get("NINJASK_N1_BRUTE_EVERY", "40"))
MAX_MOVES = 4


def universe(preview: bool, team_size: int) -> list[str]:
    """Every syntactically plausible choice string (the oracle of Areneon's choice corpus): no knowledge of the
    request but its phase and the size of the team."""
    if preview:
        out: list[str] = []

        def walk(prefix: list[int]) -> None:
            if prefix:
                out.append("team " + "".join(map(str, prefix)))
            if len(prefix) == 4:
                return
            for i in range(1, team_size + 1):
                if i not in prefix:
                    walk(prefix + [i])

        walk([])
        return out
    slot = ["pass"]
    for m in range(1, MAX_MOVES + 1):
        for target in ("", " 1", " 2", " -1", " -2"):
            for event in ("", " mega", " megax", " megay", " terastallize"):
                slot.append(f"move {m}{target}{event}")
    for s in range(1, team_size + 1):
        slot.append(f"switch {s}")
    return [f"{a}, {b}" for a in slot for b in slot]


def after(battle, side: str, text: str):
    """What choosing `text` does, as the engine's whole state afterwards, or None when it is refused. A choice that
    runs the turn at once (a forced switch) leaves no pending choice to read, so the state is the normal form: two
    spellings of one action give one state."""
    probe = battle.copy()
    try:
        probe.choose(side, text)
    except ValueError:
        return None
    return probe.state_key()


def play(seed_base: int, pool: list[str]):
    rng = random.Random(seed_base)
    p1, p2 = rng.choice(pool), rng.choice(pool)
    seed = "sodium," + f"{rng.getrandbits(128):032x}"
    battle = ninjask.Battle.reference(dex(), p1, p2, seed, names=NAMES, channels=True)
    return battle, rng


def test_candidates_of_fresh_requests_agree_with_the_engine():
    pool = teams()
    checked = refused = brute = 0
    for g in range(BATTLES):
        battle, rng = play(g, pool)
        steps = 0
        while not battle.ended and battle.turn < 60 and steps < 400:
            steps += 1
            for side in NAMES:
                if not battle.needs_choice(side):
                    continue
                candidates = battle.candidates(side)
                assert candidates, (g, side)
                checked += 1
                done = set()
                for choice, pending in candidates:
                    probe = battle.copy()
                    try:
                        probe.choose(side, choice)
                    except ValueError as error:
                        assert pending, f"enumerator defect: {choice!r} refused: {error}"
                        assert str(error).startswith("[Unavailable choice]") or "Hack" in str(error), (choice, str(error))
                        refused += 1
                    else:
                        done.add(probe.state_key())
                if checked % BRUTE_EVERY == 0:
                    request = json.loads(battle.request(side))
                    size = len(request["side"]["pokemon"])
                    for text in universe(request.get("teamPreview", False), size):
                        key = after(battle, side, text)
                        assert key is None or key in done, (g, side, "missed", text)
                    brute += 1
            for side in NAMES:
                if not battle.needs_choice(side):
                    continue
                candidates = battle.candidates(side)
                choice = rng.choice(candidates)[0]
                try:
                    battle.choose(side, choice)
                except ValueError:
                    # a refused pending candidate: choose again from the new request
                    break
    print(f"requests {checked}, pending refused {refused}, brute-forced {brute}")
    assert checked > 50 and brute >= 1
