"""The engine at the Showdown pin 7244fe8e: what changed since 3a766f2, replayed.

Showdown commits between the two pins that change battle behaviour in the
Champions mod (docs/repin.md has the full classification):

- d12b9c1e9 "Champions: Can no longer double No Retreat when already trapped
  (#12124)". The move's condition now adds the `trapped` volatile itself
  (`|-activate|POKEMON|trapped`, then `|-start|POKEMON|move: No Retreat`) and
  has no `onTrapPokemon` of its own; a Pokémon that is already trapped gets
  the `noretreat` volatile silently, and the volatile no longer traps when a
  Baton Pass hands it on (`trapped` is `noCopy`).

Two differences that are not Showdown changes, found when the engine was
compared with the pinned Showdown on 3,200 battles: Revival Blessing's
forced-switch request offers `pass` for the reviving Pokémon when no living
Pokémon waits on the bench ("pass, pass" was missing from `legal_choices`).

Every test here fails on 0.4.1 (pinned to 3a766f2) and passes on the pin sync.
"""

from __future__ import annotations

import importlib.util
import json
import random
from pathlib import Path

import pytest

ninjask = pytest.importorskip("ninjask")

from ninjask import dex  # noqa: E402
from ninjask.choices import joint_choices  # noqa: E402
from ninjask.paths import ENGINE_ROOT, SHOWDOWN_DIR  # noqa: E402
from ninjask.refsim import Battle, RefSim, seed_from_int  # noqa: E402

FIXTURES = ENGINE_ROOT / "tests" / "fixtures" / "pin_sync"
needs_showdown = pytest.mark.skipif(
    not (SHOWDOWN_DIR / "dist").exists(),
    reason="Pokémon Showdown not built (scripts/setup_showdown.sh)",
)


def _check_reference():
    spec = importlib.util.spec_from_file_location("check_reference", ENGINE_ROOT / "scripts" / "check_reference.py")
    assert spec and spec.loader
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


# ---------------------------------------------------------------------------
# No Retreat (#12124)


# Games of `check_reference.py --synthetic --require noretreat` (seed 0) that
# diverged on 0.4.1 against the pin: `|-activate|pX|trapped` missing before
# `|-start|pX|move: No Retreat`, and the trap flags of the requests that follow.
NO_RETREAT_GAMES = [0, 3, 5, 6, 7, 9, 11, 15, 19, 21, 23, 24]


@pytest.fixture(scope="module")
def reference():
    cr = _check_reference()
    cr._init_worker(False, ("noretreat",), True, None)
    return cr


@needs_showdown
@pytest.mark.parametrize("game", NO_RETREAT_GAMES)
def test_no_retreat_synthetic_game_matches_showdown(reference, game):
    _, inputs, divergence = reference.run_game((game, 0))
    assert divergence is None, divergence
    assert inputs > 0


def _set(species, ability, moves, gender="", item="", nature="Hardy"):
    return f"{species}||{item}|{ability}|{','.join(moves)}|{nature}||{gender}|||50|"


def _team(*mons):
    return "]".join(mons)


IDLE = ("Teatime", "BodySlam", "Protect", "Detect")
# Species differ within a team: the projection names a Pokémon by its side and species.
BENCH = (_set("Slowbro", "oblivious", IDLE, "M"), _set("Pikachu", "static", IDLE, "M"))


class _ScriptDone(BaseException):
    """Raised by a `stop` script when it runs out: everything before was compared with Showdown."""


class _ScriptedChoices:
    """Stands in for the `random.Random` of `check_reference.play`: the scripted
    choices first (each must be on offer), then random ones, or with `stop` the end of the play."""

    def __init__(self, script, stop=False):
        self.script = list(script)
        self.stop = stop
        self.rng = random.Random(1)

    def choice(self, options):
        if self.script:
            want = self.script.pop(0)
            assert want in options, (want, options)
            return want
        if self.stop:
            raise _ScriptDone
        return self.rng.choice(list(options))


def _play(t1, t2, script, seed, capsys, salt="pin-sync", stop=False):
    """Play `script` then random choices (with `stop`, nothing more) on Showdown and the engine, comparing
    after every input (a divergence raises); returns the omniscient log lines."""
    cr = _check_reference()
    with RefSim() as sim:
        try:
            cr.play(sim, dex(cr.FORMAT), seed_from_int(seed, salt=salt), t1, t2, _ScriptedChoices(script, stop), True)
        except _ScriptDone:
            pass
    lines = [line.strip() for line in capsys.readouterr().out.splitlines()]
    return [line for line in lines if line.startswith("|")]


FALINKS = _set("Falinks", "defiant", ("NoRetreat", "Protect", "BatonPass", "Teatime"), "M")
OPPONENTS = _team(_set("Snorlax", "thickfat", IDLE, "M"), _set("Clefable", "magicguard", IDLE, "F"), *BENCH)


@needs_showdown
@pytest.mark.parametrize("seed", range(1, 4))
def test_no_retreat_adds_the_trapped_volatile_before_it_starts(seed, capsys):
    t1 = _team(FALINKS, _set("Snorlax", "thickfat", IDLE, "M"), *BENCH)
    script = ["team 1234", "team 1234", "move 1, move 1", "move 1, move 1"]
    log = _play(t1, OPPONENTS, script, seed, capsys)
    start = log.index("|-start|p1a: Falinks|move: No Retreat")
    assert log[start - 1] == "|-activate|p1a: Falinks|trapped", log[start - 3 : start + 1]
    assert log[start - 2].startswith("|-boost|p1a: Falinks|spe|"), log[start - 3 : start + 1]


@needs_showdown
@pytest.mark.parametrize("seed", range(1, 4))
def test_no_retreat_by_a_pokemon_that_is_already_trapped_starts_silently(seed, capsys):
    """Mean Look traps Falinks; its No Retreat then boosts it and adds the volatile with no
    `-activate` and no `-start` (the `trapped` volatile is there, so the start event is
    cut short, not failed)."""
    umbreon = _set("Umbreon", "synchronize", ("MeanLook", "Protect", "Teatime", "Wish"), "M")
    t2 = _team(umbreon, _set("Clefable", "magicguard", IDLE, "F"), *BENCH)
    t1 = _team(FALINKS, _set("Snorlax", "thickfat", IDLE, "M"), *BENCH)
    # Turn 1: Falinks protects; Mean Look reaches it anyway. Turn 2: No Retreat.
    script = ["team 1234", "team 1234", "move 2, move 1", "move 1 1, move 1", "move 1, move 1", "move 3, move 1"]
    log = _play(t1, t2, script, seed, capsys)
    trapped = [i for i, line in enumerate(log) if line == "|-activate|p1a: Falinks|trapped"]
    assert len(trapped) == 1, [line for line in log if "trapped" in line]
    boosted = [i for i, line in enumerate(log) if line.startswith("|-boost|p1a: Falinks|spe|") and i > trapped[0]]
    assert boosted, "No Retreat's boosts follow the trap"
    assert "|-start|p1a: Falinks|move: No Retreat" not in log


@needs_showdown
@pytest.mark.parametrize("seed", range(1, 4))
def test_no_retreat_by_a_ghost_type_starts_silently(seed, capsys):
    """Ghost types are immune to `trapped`: the volatile cannot be added, so no message."""
    gengar = _set("Gengar", "cursedbody", ("NoRetreat", "Protect", "Teatime", "BatonPass"), "M")
    t1 = _team(gengar, _set("Snorlax", "thickfat", IDLE, "M"), *BENCH)
    script = ["team 1234", "team 1234", "move 1, move 1", "move 1, move 1"]
    log = _play(t1, OPPONENTS, script, seed, capsys)
    assert any(line.startswith("|-boost|p1a: Gengar|atk|") for line in log)
    assert not any("trapped" in line or "No Retreat" in line for line in log if line.startswith(("|-activate", "|-start")))


@needs_showdown
@pytest.mark.parametrize("seed", range(1, 4))
def test_baton_pass_hands_on_no_retreat_without_its_trap(seed, capsys):
    """`trapped` is noCopy: the newcomer has the `noretreat` volatile but can switch out."""
    t1 = _team(FALINKS, _set("Snorlax", "thickfat", IDLE, "M"), *BENCH)
    # Turn 1: No Retreat. Turn 2: Baton Pass to the first benched Pokémon.
    script = ["team 1234", "team 1234", "move 1, move 1", "move 1, move 1", "move 3, move 1", "move 1, move 1", "switch 3, pass"]
    log = _play(t1, OPPONENTS, script, seed, capsys)
    assert "|-activate|p1a: Falinks|trapped" in log
    assert any(line.startswith("|switch|p1a:") for line in log[log.index("|-activate|p1a: Falinks|trapped") :])


# ---------------------------------------------------------------------------
# Revival Blessing's forced switch with nobody left on the bench


def _revival_battle():
    fixture = json.loads((FIXTURES / "revival_blessing.json").read_text())
    battle = ninjask.Battle.reference(
        dex(fixture["format"]), fixture["teams"]["p1"], fixture["teams"]["p2"], fixture["seed"], names=("p1", "p2")
    )
    for side, choice in fixture["inputs"]:
        battle.choose(side, choice)
    return fixture, battle


def test_revival_blessing_request_can_pass_when_the_bench_has_no_living_pokemon():
    fixture, battle = _revival_battle()
    side = fixture["request_owner"]
    request = json.loads(battle.request(side))
    assert request["forceSwitch"] == [False, True]
    assert any(p.get("reviving") for p in request["side"]["pokemon"])
    legal = battle.legal_choices(side)
    assert legal == ["pass, switch 3", "pass, switch 4", "pass, pass"]
    assert joint_choices(request) == legal
    battle.choose(side, fixture["showdown_accepted"])  # accepted, as Showdown accepts it
    # Showdown asks again: the revival lasts to the turn's residual. Then p2 revives.
    assert json.loads(battle.request(side))["forceSwitch"] == [False, True]
    for who, choice in fixture["then"]:
        battle.choose(who, choice)


@needs_showdown
def test_revival_blessing_choices_are_showdowns():
    fixture, battle = _revival_battle()
    side = fixture["request_owner"]
    with RefSim() as sim:
        sd = Battle(
            sim,
            fixture["format"],
            fixture["seed"],
            ("p1", fixture["teams"]["p1"]),
            ("p2", fixture["teams"]["p2"]),
        )
        obs = sd.start()
        for who, choice in fixture["inputs"]:
            obs = sd.choose(who, choice)
            assert obs.errors[who] is None, (who, choice, obs.errors[who])
        request = obs.requests[side]
        assert request["forceSwitch"] == [False, True]
        assert set(joint_choices(request)) == set(battle.legal_choices(side))
        obs = sd.choose(side, fixture["showdown_accepted"])
        assert obs.errors[side] is None


# ---------------------------------------------------------------------------
# Two Revival Blessing slots at once (found by the review of the pin sync)


def _mon(name, species, ability, moves, nature="Hardy", evs=""):
    return f"{name}|{species}||{ability}|{','.join(moves)}|{nature}|{evs}||||50|"


# Two fodder leads that Earthquake faints, two revivers on the bench. Both revivers use Revival
# Blessing in one turn: the faster passes its revival (`pass, pass`: nobody lives on the bench), the
# slower then uses its own, and the request has both slots reviving.
REVIVERS_1 = _team(
    _mon("Fodder1", "Pikachu", "static", ("Protect", "Teatime")),
    _mon("Fodder2", "Dedenne", "cheekpouch", ("Protect", "Teatime")),
    _mon("Reviver1", "Pawmot", "naturalcure", ("RevivalBlessing", "Protect", "Teatime"), "Jolly", "0,0,0,0,0,32"),
    _mon("Reviver2", "Pawmot", "naturalcure", ("RevivalBlessing", "Protect", "Teatime")),
)
REVIVERS_2 = _team(
    _mon("Chomp", "Garchomp", "roughskin", ("Earthquake", "Protect"), "Adamant", "0,32,0,0,0,32"),
    _mon("Corv", "Corviknight", "pressure", ("Protect", "Teatime")),
    _mon("Snor", "Snorlax", "thickfat", ("Protect",)),
    _mon("Clef", "Clefable", "magicguard", ("Protect",)),
)
# The script is consumed in the order the requests come: after p1's forced switch the turn starts and p2 chooses
# before p1 does.
TWO_REVIVERS = ["team 1234", "team 1234", "move 2, move 2", "move 1, move 2", "switch 3, switch 4", "move 2, move 2", "move 1, move 1", "pass, pass"]


@needs_showdown
@pytest.mark.parametrize("target", ["switch 3, switch 3", "switch 4, switch 4", "switch 3, switch 4", "pass, switch 4"])
def test_two_revivers_choose_like_showdown(target, capsys):
    """Showdown never records a revival in `switchIns`, so two reviving slots may name the same fainted
    teammate: it accepts `switch 3, switch 3` (checked on the pin with all nine combinations), and the second
    revival heals again. The engine plays it as Showdown does, input for input."""
    log = _play(REVIVERS_1, REVIVERS_2, [*TWO_REVIVERS, target], 11, capsys, salt="two", stop=True)
    start = log.index("|move|p1a: Reviver1|Revival Blessing|p1a: Reviver1")
    heals = [line for line in log[start:] if line.startswith("|-heal|") and "Revival Blessing" in line]
    assert len(heals) == (1 if target.startswith("pass") else 2), log[start:]
    if target in ("switch 3, switch 3", "switch 4, switch 4"):
        assert heals[0] == heals[1]  # the same teammate is healed twice, as Showdown does


@needs_showdown
def test_two_revivers_request_lists_what_showdown_accepts():
    cr = _check_reference()
    with RefSim() as sim:
        battle = Battle(sim, cr.FORMAT, seed_from_int(11, "two"), ("A", REVIVERS_1), ("B", REVIVERS_2))
        obs = battle.start()
        sd_req = None
        engine = ninjask.Battle.reference(dex(cr.FORMAT), REVIVERS_1, REVIVERS_2, seed_from_int(11, "two"))
        engine.set_logging(True)
        steps = [("p1", "team 1234"), ("p2", "team 1234"), ("p1", "move 2, move 2"), ("p2", "move 1, move 2"), ("p1", "switch 3, switch 4"),
                 ("p1", "move 1, move 1"), ("p2", "move 2, move 2"), ("p1", "pass, pass")]
        for side, choice in steps:
            obs = battle.choose(side, choice)
            assert obs.errors[side] is None, (side, choice, obs.errors[side])
            engine.choose(side, choice)
        sd_req = obs.requests["p1"]
        assert sd_req["forceSwitch"] == [True, True] and [p.get("reviving") for p in sd_req["side"]["pokemon"]][:2] == [True, True]
        listed = set(joint_choices(sd_req))
        assert listed == set(engine.legal_choices("p1"))
        # Showdown accepts exactly the nine combinations of {switch 3, switch 4, pass} x the same: nothing
        # is refused, so every combination must be listed. (Each probe is a fresh battle: an undo of a
        # revival leaves its switch flag cleared.)
        assert len(listed) == 9
        assert {"switch 3, switch 3", "switch 4, switch 4"} <= listed
