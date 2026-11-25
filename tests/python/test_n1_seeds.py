"""Showdown seeds for batches: `Batch.reset` takes `"sodium,<hex>"` as well as an integer, and a batched game is the
game `Battle.reference` plays from the same seed, draw for draw (Showdown's own generator, so the same game as
Showdown's from that seed and those choices: tests/python/test_n1_channels.py replays the recorded Showdown games)."""

from __future__ import annotations

import array

import pytest

import ninjask

from n1_fixtures import NAMES, battles, dex


def decisions(observation):
    who = array.array("I")
    who.frombytes(observation["who"])
    offsets = array.array("I")
    offsets.frombytes(observation["offsets"])
    return [(who[2 * k], who[2 * k + 1], offsets[k + 1] - offsets[k]) for k in range(observation["n"])]


def play_both(rows, make_seed):
    """The rows' games in one Batch and, mirrored choice for choice, in `Battle.reference` twins."""
    d = dex()
    batch = ninjask.Batch(d, len(rows), names=NAMES)
    twins = []
    for i, row in enumerate(rows):
        batch.reset(i, row["packed"]["p1"], row["packed"]["p2"], make_seed(row))
        twins.append(ninjask.Battle.reference(d, row["packed"]["p1"], row["packed"]["p2"], make_seed(row), names=NAMES))
        twins[i].set_logging(True)
    steps = 0
    while True:
        pending = decisions(batch.observe())
        if not pending:
            break
        picks = [(i, s, (7 * steps + 3 * i + s) % count) for i, s, count in pending]
        flags = batch.step([index for _, _, index in picks])
        for (i, s, index), flag in zip(picks, flags):
            side = NAMES[s]
            try:
                twins[i].choose_index(side, index)
                refused = False
            except ValueError:
                refused = True
            assert refused == bool(flag & 1), (i, steps)
        steps += 1
    return batch, twins


def test_a_batch_game_from_a_showdown_seed_equals_battle_reference():
    rows = battles()[:12]
    batch, twins = play_both(rows, lambda row: row["seed"])
    for i, twin in enumerate(twins):
        played = batch.battle(i)
        assert played.log("omniscient", 0) == twin.log("omniscient", 0), i
        assert played.log("p1", 0) == twin.log("p1", 0), i
        assert played.log("p2", 0) == twin.log("p2", 0), i
        assert played.rng_seed == twin.rng_seed, i
        assert batch.winner(i) == twin.winner_side, i
        assert batch.turn(i) == twin.turn, i
        assert twin.ended or batch.done(i)


def test_the_generator_is_showdowns_so_the_final_seed_has_the_stream_state_of_a_reference_battle():
    row = battles()[0]
    batch = ninjask.Batch(dex(), 1, names=NAMES)
    batch.reset(0, row["packed"]["p1"], row["packed"]["p2"], row["seed"])
    # Reference mode prints its state the way Showdown's `battle.prng.getSeed()` does.
    assert batch.battle(0).rng_seed is not None
    assert batch.battle(0).rng_seed.startswith("sodium,")


def test_integer_seeds_still_play_the_fast_generator():
    row = battles()[0]
    d = dex()
    batch = ninjask.Batch(d, 1, names=NAMES)
    batch.reset(0, row["packed"]["p1"], row["packed"]["p2"], 7)
    assert batch.battle(0).rng_seed is None
    twin = ninjask.Battle(d, row["packed"]["p1"], row["packed"]["p2"], 7, names=NAMES)
    assert batch.battle(0).log("omniscient", 0) == twin.log("omniscient", 0)
    # The same teams and another kind of seed are another game.
    other = ninjask.Batch(d, 1, names=NAMES)
    other.reset(0, row["packed"]["p1"], row["packed"]["p2"], row["seed"])
    assert other.battle(0).rng_seed != batch.battle(0).rng_seed


def test_reset_many_takes_both_kinds_of_seed_at_once():
    rows = battles()[:3]
    d = dex()
    batch = ninjask.Batch(d, 3, names=NAMES)
    seeds = [rows[0]["seed"], 5, rows[2]["seed"]]
    batch.reset_many([0, 1, 2], [r["packed"]["p1"] for r in rows], [r["packed"]["p2"] for r in rows], seeds)
    for i, seed in enumerate(seeds):
        one = ninjask.Batch(d, 1, names=NAMES)
        one.reset(0, rows[i]["packed"]["p1"], rows[i]["packed"]["p2"], seed)
        assert batch.battle(i).log("omniscient", 0) == one.battle(0).log("omniscient", 0)
        assert batch.battle(i).rng_seed == one.battle(0).rng_seed


@pytest.mark.parametrize("seed", ["sodium,xyz", "gen5,1234", "0123", "sodium," + "0" * 65, -1, 2**64, 1.5, None])
def test_a_bad_seed_is_refused_and_starts_nothing(seed):
    row = battles()[0]
    batch = ninjask.Batch(dex(), 1, names=NAMES)
    with pytest.raises((ValueError, TypeError, OverflowError)):
        batch.reset(0, row["packed"]["p1"], row["packed"]["p2"], seed)
    assert batch.done(0)  # never started


def test_names_are_the_players_names():
    row = battles()[0]
    batch = ninjask.Batch(dex(), 1, names=("alice", "bob"))
    batch.reset(0, row["packed"]["p1"], row["packed"]["p2"], row["seed"])
    log = batch.battle(0).log("omniscient", 0)
    assert "|player|p1|alice||" in log and "|player|p2|bob||" in log
    default = ninjask.Batch(dex(), 1)
    default.reset(0, row["packed"]["p1"], row["packed"]["p2"], 1)
    assert "|player|p1|Player 1||" in default.battle(0).log("omniscient", 0)
