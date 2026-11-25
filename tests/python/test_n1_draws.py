"""The draw log and forced draws, for the differential harness that compares the engine with the pinned Showdown.

Showdown's battle makes every random number through four calls on its generator: `random(n)` or `random(from, to)`,
`randomChance(numerator, denominator)`, `sample(items)` and `shuffle(items, start, end)`. A *draw* is one such call:
its method, its arguments and its result, each as a decimal string (`sample`: the length, then the index drawn;
`shuffle`: the length and the range as passed, then the permutation of the indices; `randomChance`: `true` or
`false`). The fixtures hold the draws of the recorded Showdown battles per write (the two `>player` lines, then each
choice). Here:

- `record_draws=True` logs the engine's own draws, equal to Showdown's for the same seed and choices;
- `set_forced_draws(draws)` answers the next calls from a list, as the harness does for Showdown: a call is
  answered from the list when its method and arguments equal the entry at the same position, else by the engine's own
  generator (which advances either way); `take_draws()` returns every call made since the last take with the answer used.
"""

from __future__ import annotations

import pytest

import ninjask

from n1_fixtures import NAMES, battles, dex


def as_tuples(draws):
    return [(d["method"], list(d["args"]), list(d["result"])) for d in draws]


def start_draws(row):
    """What the two `>player` writes drew in Showdown (the genders of the teams)."""
    return as_tuples(row["draws"][0] + row["draws"][1])


def test_the_engines_draws_are_showdowns_for_the_same_seed_and_choices():
    for row in battles():
        battle = ninjask.Battle.reference(
            dex(), row["packed"]["p1"], row["packed"]["p2"], row["seed"], names=NAMES, record_draws=True
        )
        assert battle.take_draws() == start_draws(row), row["id"]
        for k, c in enumerate(row["choices"]):
            try:
                battle.choose(c["player"], c["choice"])
            except ValueError:
                assert c.get("refused")
            assert battle.take_draws() == as_tuples(row["draws"][2 + k]), (row["id"], k, c)
        assert battle.take_draws() == []


def test_a_battle_on_the_fast_generator_forced_with_showdowns_draws_plays_showdowns_game():
    """The point of the forced mode: whatever generator the engine has, the game of a trace is the trace's."""
    for row in battles():
        battle = ninjask.Battle(
            dex(), row["packed"]["p1"], row["packed"]["p2"], 12345, names=NAMES, channels=True, forced_draws=start_draws(row)
        )
        assert battle.take_draws() == start_draws(row), row["id"]
        for k, c in enumerate(row["choices"]):
            battle.set_forced_draws(as_tuples(row["draws"][2 + k]))
            try:
                battle.choose(c["player"], c["choice"])
            except ValueError:
                assert c.get("refused")
            # every call was in the trace and answered from it
            assert battle.take_draws() == as_tuples(row["draws"][2 + k]), (row["id"], k, c)
        assert battle.channel("omniscient") == row["omniscient"], row["id"]
        assert battle.channel("p1") == row["channels"]["p1"], row["id"]
        assert battle.channel("p2") == row["channels"]["p2"], row["id"]


def first_step_with(method):
    for row in battles():
        for k, writes in enumerate(row["draws"][2:]):
            if writes and writes[0]["method"] == method:
                return row, k, writes
    raise AssertionError(f"no step starts with {method}")


def played_to(row, upto):
    """A battle (reference generator) played up to choice `upto`, recording."""
    battle = ninjask.Battle.reference(
        dex(), row["packed"]["p1"], row["packed"]["p2"], row["seed"], names=NAMES, record_draws=True
    )
    for c in row["choices"][:upto]:
        try:
            battle.choose(c["player"], c["choice"])
        except ValueError:
            pass
    battle.take_draws()
    return battle


def test_a_forced_result_is_used_for_a_matching_call():
    row, k, writes = first_step_with("randomChance")
    battle = played_to(row, k)
    flipped = [dict(writes[0], result=["false" if writes[0]["result"] == ["true"] else "true"])] + writes[1:]
    battle.set_forced_draws(as_tuples(flipped))
    c = row["choices"][k]
    battle.choose(c["player"], c["choice"])
    assert battle.take_draws()[0] == as_tuples(flipped)[0]


def test_a_call_that_is_not_in_the_trace_gets_the_engines_own_answer_and_is_reported():
    row, k, writes = first_step_with("random")
    battle = played_to(row, k)
    # A trace of another game: no call matches, so every answer is the engine's own and every call is reported.
    battle.set_forced_draws([("random", ["999"], ["0"])] * len(writes))
    c = row["choices"][k]
    battle.choose(c["player"], c["choice"])
    assert battle.take_draws() == as_tuples(writes)


def test_recording_is_off_by_default_and_costs_nothing_then():
    row = battles()[0]
    battle = ninjask.Battle.reference(dex(), row["packed"]["p1"], row["packed"]["p2"], row["seed"], names=NAMES)
    battle.choose("p1", row["choices"][0]["choice"])
    assert battle.take_draws() == []


def test_a_draw_in_a_forced_list_the_engine_cannot_make_is_refused():
    row = battles()[0]
    with pytest.raises(ValueError, match="draw"):
        ninjask.Battle.reference(
            dex(), row["packed"]["p1"], row["packed"]["p2"], row["seed"], names=NAMES, forced_draws=[("random", ["x"], ["0"])]
        )


def test_a_batch_records_and_forces_per_battle():
    row = battles()[0]
    batch = ninjask.Batch(dex(), 1, names=NAMES, record_draws=True)
    batch.reset(0, row["packed"]["p1"], row["packed"]["p2"], row["seed"])
    assert batch.take_draws(0) == start_draws(row)
    for k, c in enumerate(row["choices"][:8]):
        batch.choose([(0, NAMES.index(c["player"]), c["choice"])])
        assert batch.take_draws(0) == as_tuples(row["draws"][2 + k])
    forced = ninjask.Batch(dex(), 1, names=NAMES, record_draws=True)
    forced.reset(0, row["packed"]["p1"], row["packed"]["p2"], 99, forced_draws=start_draws(row))
    assert forced.take_draws(0) == start_draws(row)
    for k, c in enumerate(row["choices"][:8]):
        forced.set_forced_draws(0, as_tuples(row["draws"][2 + k]))
        forced.choose([(0, NAMES.index(c["player"]), c["choice"])])
        assert forced.take_draws(0) == as_tuples(row["draws"][2 + k])


def test_reseed_replaces_the_generator_and_keeps_the_draw_log_and_the_forced_draws():
    """`reseed` is for drawing other outcomes from a copy; it used to drop recording and forcing with the generator."""
    row = battles()[0]
    # the first choice whose write draws anything in Showdown: the turn that first needs the generator
    k = next(i for i in range(len(row["choices"])) if row["draws"][2 + i])
    queue = as_tuples(row["draws"][2 + k])

    def play_to_k(battle):
        for c in row["choices"][:k]:
            battle.choose(c["player"], c["choice"])
        battle.take_draws()

    mine = ninjask.Battle.reference(
        dex(), row["packed"]["p1"], row["packed"]["p2"], row["seed"], names=NAMES, record_draws=True
    )
    play_to_k(mine)
    mine.reseed(7)
    mine.choose(row["choices"][k]["player"], row["choices"][k]["choice"])
    assert mine.take_draws() != [], "the log stays on after a reseed"

    forced = ninjask.Battle(
        dex(), row["packed"]["p1"], row["packed"]["p2"], 12345, names=NAMES, forced_draws=start_draws(row)
    )
    play_to_k(forced)
    forced.set_forced_draws(queue)
    forced.reseed(99)
    forced.choose(row["choices"][k]["player"], row["choices"][k]["choice"])
    assert forced.take_draws() == queue, "the forced draws stay in force after a reseed"
