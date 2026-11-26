"""Hand-built battles that provoke each `|-hint|` line of Showdown's that can be reached in the format, and a few
neighbours that print none (tests/fixtures/areneon/scenarios.jsonl.gz, made by scripts/areneon/make_scenarios.py from the
pinned Showdown). Each must play through the engine with the channels, request hashes and random draws Showdown's
recording has; a line the engine omits shows up as the first difference of its channel.
"""

from __future__ import annotations

import pytest

import ninjask

from n1_fixtures import dex, recorded_checker, scenarios

CASES = scenarios()


@pytest.mark.parametrize("row,writes", CASES, ids=[row["id"] for row, _ in CASES])
def test_the_engine_plays_the_scenario_as_showdown_did(row, writes):
    result = recorded_checker().check(row, dex(), writes)
    assert {name: problem for name, problem in result.items() if problem} == {}


@pytest.mark.parametrize("row,writes", CASES, ids=[row["id"] for row, _ in CASES])
def test_the_scenario_still_provokes_what_it_is_named_for(row, writes):
    """The recording says what the scenario shows; the engine's own log must agree line for line (the check above)."""
    log = set(row["omniscient"])
    if row["id"].startswith(("cross-", "mirror-")):
        return  # a whole battle found by the fuzz for the framing of a shuffle: nothing named, the draws are the test
    assert row.get("must_contain") or row.get("must_not_contain")
    for line in row.get("must_contain", []):
        assert line in log
    for line in row.get("must_not_contain", []):
        assert line not in log


def test_every_hint_that_is_reachable_in_the_format_has_a_scenario():
    """The audit of docs/n1-seeds-channels-candidates.md: the hints the scenarios hold, by their first words."""
    hints = {line for row, _ in CASES for line in row.get("must_contain", []) if line.startswith("|-hint|")}
    expected = {
        "Fake Out only works on your first turn out.",
        "First Impression only works on your first turn out.",
        "Some effects can force a Pokemon to use Gigaton Hammer again in a row.",
        "Only a Pokemon whose form is Morpeko or Morpeko-Hangry can use this move.",
        "Body Press uses Def boosts when Wonder Room is active.",
        "Transform Mimicry changes you to your original un-transformed types.",
        "Physical Shell Side Arm",
        "Special Shell Side Arm",
        "Since gen 7, Dark is immune to Prankster moves.",
        "In Gen 8+, Wish will never resolve when used on the 256th turn.",
        "In Gen 8+, Future attacks will never resolve when used on the 255th turn or later.",
        "Future Sight did not hit because the target is fainted.",
        "Skill Swap does not announce the abilities of the Pok\u00e9mon when used between allies.",
        "Rising Voltage's BP doubled on grounded target.",
        "Psychic Terrain doesn't affect airborne Pok\u00e9mon.",
        "A switch failed because the Pok\u00e9mon trying to switch in is already in.",
        "In Gen 7, Stockpile keeps track of how many times it successfully altered each stat individually.",
    }
    assert hints == {"|-hint|" + h for h in expected}


def test_a_queued_switch_follows_its_pokemon_when_another_switch_moves_it():
    """The switch queued for slot B names Clefable; the forced switch of Emergency Exit, answered in the middle of the
    switch phase, brings Clefable in at slot A and sends Golisopod to Clefable's place in the side's order. The queued
    switch must still name Clefable (it is in: a hint, and slot B stays as it was), not Golisopod, who is on the bench
    now (it used to switch him in again)."""
    row, _ = next(c for c in CASES if c[0]["id"] == "switchin-already-in-emergency-exit")
    batch = ninjask.Batch(dex(), 1, names=("p1", "p2"), channels=True)
    batch.reset(0, row["packed"]["p1"], row["packed"]["p2"], row["seed"])
    for c in row["choices"]:
        batch.choose([(0, ("p1", "p2").index(c["player"]), c["choice"])])
    log = batch.channel(0, "omniscient")
    hint = log.index("|-hint|A switch failed because the Pok\u00e9mon trying to switch in is already in.")
    assert not any(line.startswith("|switch|p1b: Golisopod") for line in log[hint:])
    assert [l for l in log[hint - 6 : hint] if l.startswith("|switch|p1a: Clefable")], "Clefable came in at slot A first"
