"""Hand-built battles that provoke each `|-hint|` line of Showdown's that can be reached in the format, and a few
neighbours that print none (tests/fixtures/areneon/scenarios.jsonl.gz, made by scripts/areneon/make_scenarios.py from the
pinned Showdown). Each must play through the engine with the channels, request hashes and random draws Showdown's
recording has; a line the engine omits shows up as the first difference of its channel.
"""

from __future__ import annotations

import pytest

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
    }
    assert hints == {"|-hint|" + h for h in expected}
