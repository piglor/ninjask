"""Candidates: the choices a player may submit, enumerated from the player's own request in the order, with the
`pending` flags, of Areneon's reference enumerator (ADR-006), as `Battle.candidates(side)` and
`Batch.candidates(i, side)`: 360 ordered Team Preview picks (not the 90 of `legal_choices`), pending where the
request cannot tell the player whether a choice is legal.

The oracles are Areneon's golden choice corpus (one situation each, with the request's hash and every candidate in the
reference's order), and the candidates the reference lists at every request of the recorded Showdown battles."""

from __future__ import annotations

import ninjask

from n1_fixtures import NAMES, battles, dex, golden_cases


def reach(case, packed):
    battle = ninjask.Battle.reference(dex(), packed["p1"], packed["p2"], case["seed"], names=NAMES)
    for c in case["log"]:
        battle.choose(c["player"], c["choice"])
    return battle


def test_every_golden_case_has_the_reference_candidates_in_the_reference_order():
    cases = golden_cases()
    assert len(cases) >= 17
    for case, packed in cases:
        battle = reach(case, packed)
        side = NAMES.index(case["player"])
        assert battle.request_hash(side) == case["request_hash"], case["id"]
        got = battle.candidates(case["player"])
        want = [(c["choice"], c["pending"]) for c in case["candidates"]]
        assert got == want, case["id"]


def test_team_preview_lists_every_ordered_pick():
    case, packed = next(c for c in golden_cases() if c[0]["id"] == "preview")
    fresh = ninjask.Battle.reference(dex(), packed["p1"], packed["p2"], case["seed"], names=NAMES)
    got = fresh.candidates("p1")
    assert len(got) == 360 and got[0] == ("team 1234", False) and got[-1] == ("team 6543", False)
    assert len(fresh.legal_choices("p1")) == 90  # the engine's own list is left as it was


def test_the_candidates_of_every_recorded_request_equal_the_reference_lists():
    requests = 0
    for row in battles():
        battle = ninjask.Battle.reference(dex(), row["packed"]["p1"], row["packed"]["p2"], row["seed"], names=NAMES)
        seq = {"p1": 0, "p2": 0}

        def compare(side, row=row, battle=battle, seq=seq):
            nonlocal requests
            recorded = row["requests"][side][seq[side]]
            got = battle.candidates(side)
            if recorded.get("legal") is None:
                assert got == [] and recorded["phase"] == "wait", (row["id"], side, seq[side])
            else:
                assert [c for c, pending in got if not pending] == recorded["legal"]["choices"], (row["id"], side, seq[side])
                assert [c for c, pending in got if pending] == recorded["legal"]["pending"], (row["id"], side, seq[side])
            seq[side] += 1
            requests += 1

        for side in NAMES:
            compare(side)
        for c in row["choices"]:
            before = {s: battle.request(s) for s in NAMES}
            try:
                battle.choose(c["player"], c["choice"])
            except ValueError as error:
                assert str(error).startswith("[Unavailable choice]")
                compare(c["player"])  # a refusal that reveals sends the player's request again
                continue
            if battle.ended:
                continue
            for side in NAMES:
                # `rqid` and `update` make every new request differ from the one before
                if battle.request(side) != before[side]:
                    compare(side)
        assert seq == {s: len(row["requests"][s]) for s in NAMES}, row["id"]
    assert requests > 2000


def test_a_batch_has_the_same_candidates_and_chooses_by_their_index():
    row = battles()[0]
    batch = ninjask.Batch(dex(), 1, names=NAMES, channels=True)
    batch.reset(0, row["packed"]["p1"], row["packed"]["p2"], row["seed"])
    candidates = batch.candidates(0, 0)
    assert len(candidates) == 360
    picked = candidates.index(("team 4632", False))
    other = candidates.index(("team 1254", False))
    flags = batch.choose_candidates([(0, 0, picked), (0, 1, other)])
    assert not any(f & 5 for f in flags)
    assert batch.channel(0, "p1")[-1].startswith("|request|")
    after = batch.candidates(0, 0)
    assert after and all(c.startswith("move") or c.startswith("switch") for c, _ in after)
    assert batch.pending() == [(0, 0), (0, 1)]
    assert candidates != after
