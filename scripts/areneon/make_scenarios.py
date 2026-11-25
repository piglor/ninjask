#!/usr/bin/env python3
"""The hand-built scenarios of tests/fixtures/areneon/scenarios.jsonl.gz: small battles that provoke one `|-hint|` line of
the pinned Showdown (or show that a nearby situation prints none), recorded with scenario.cjs together with the calls
Showdown made on its random source. tests/python/test_n1_scenarios.py replays each through the engine and requires
every channel, request hash and draw to equal Showdown's.

    NINJASK_SHOWDOWN=/path/to/pokemon-showdown python3 scripts/areneon/make_scenarios.py

writes tests/fixtures/areneon/scenarios.jsonl.gz (the rows, the format of battles.jsonl.gz) and scenarios-draws.jsonl.gz
(the draw traces). Needs node and a built Showdown checkout at the pin.

A scenario is a pair of four-Pokémon teams (the first two lead) and the choices to play, in order: `rounds(...)` gives
each turn as a (p1 choice, p2 choice) pair. `must_contain` / `must_not_contain` are lines the omniscient log has to
hold or not hold: a scenario that stops provoking its hint fails when it is recorded here, so the fixture cannot
silently stop testing what its name says.
"""
import json
import subprocess
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
OUT = ROOT / "tests" / "fixtures" / "areneon"


def mon(species, moves, ability="Pickup", item="", spe=0, hp=4, nature="Hardy"):
    return {
        "name": species, "species": species, "item": item, "ability": ability, "moves": moves, "nature": nature,
        "gender": "", "evs": {"hp": hp, "atk": 0, "def": 0, "spa": 0, "spd": 0, "spe": spe},
        "ivs": {"hp": 31, "atk": 31, "def": 31, "spa": 31, "spd": 31, "spe": 31}, "level": 50,
    }


def rounds(*pairs, start=None):
    """[(p1 choice, p2 choice), ...] as choices, after Team Preview (`team 1234` each, the first two lead)."""
    out = [{"player": "p1", "choice": "team 1234"}, {"player": "p2", "choice": "team 1234"}]
    for a, b in pairs:
        # None: that player has nothing to choose this round (the other must switch a fainted Pokémon in).
        if a is not None:
            out.append({"player": "p1", "choice": a})
        if b is not None:
            out.append({"player": "p2", "choice": b})
    return out


# Moves with no target to choose, so a filler Pokémon's choices are the same whoever it faces.
FILLER = ["Protect", "Swords Dance", "Recover", "Rest"]
STALL = ("move 3, move 3")
HINT = "|-hint|"


def team(*mons):
    return list(mons)


def fillers(*species):
    return [mon(s, FILLER) for s in species]


scenarios = [
    {
        "id": "fakeout-copycat",
        "purpose": "Copycat copies Fake Out after the Copycat user's first action: Showdown says Fake Out only works on the first turn out",
        "seed": "sodium,01010101010101010101010101010101",
        "teams": {
            "p1": team(mon("Florges", ["Copycat", "Protect", "Moonblast", "Rest"]), mon("Umbreon", FILLER), *fillers("Snorlax", "Clefable")),
            "p2": team(*fillers("Kingambit", "Incineroar"), mon("Rillaboom", ["Fake Out", "Protect", "Swords Dance", "Rest"]),
                       mon("Tinkaton", ["Gigaton Hammer", "Protect", "Swords Dance", "Rest"])),
        },
        "choices": rounds(("move 2, move 1", "switch 3, switch 4"), ("move 1, move 1", "move 1 2, move 2")),
        "must_contain": [HINT + "Fake Out only works on your first turn out."],
    },
    {
        "id": "fakeout-instruct",
        "purpose": "Instruct makes the user use Fake Out a second time; its first use hints nothing",
        "seed": "sodium,02020202020202020202020202020202",
        "teams": {
            "p1": team(mon("Kangaskhan", ["Fake Out", "Protect", "Swords Dance", "Rest"]), mon("Oranguru", ["Instruct", "Protect", "Recover", "Rest"]),
                       *fillers("Snorlax", "Clefable")),
            "p2": team(*fillers("Kingambit", "Incineroar", "Umbreon", "Garchomp")),
        },
        "choices": rounds(("move 1 1, move 1 -1", STALL), ("move 2, move 2", STALL)),
        "must_contain": [HINT + "Fake Out only works on your first turn out."],
    },
    {
        "id": "fakeout-first-turn",
        "purpose": "Fake Out on its first turn out prints no hint (the control of the two scenarios above)",
        "seed": "sodium,12121212121212121212121212121212",
        "teams": {
            "p1": team(mon("Kangaskhan", ["Fake Out", "Protect", "Swords Dance", "Rest"]), mon("Oranguru", FILLER), *fillers("Snorlax", "Clefable")),
            "p2": team(*fillers("Kingambit", "Incineroar", "Umbreon", "Garchomp")),
        },
        "choices": rounds(("move 1 1, move 3", STALL), ("move 2, move 2", STALL)),
        "must_not_contain": [HINT + "Fake Out only works on your first turn out."],
    },
    {
        "id": "fakeout-after-switch-in",
        "purpose": "Fake Out after the user switched out and in again is a first turn out: no hint",
        "seed": "sodium,13131313131313131313131313131313",
        "teams": {
            "p1": team(mon("Kangaskhan", ["Fake Out", "Protect", "Swords Dance", "Rest"]), mon("Oranguru", FILLER), *fillers("Snorlax", "Clefable")),
            "p2": team(*fillers("Kingambit", "Incineroar", "Umbreon", "Garchomp")),
        },
        "choices": rounds(("move 3, move 3", STALL), ("switch 3, move 3", STALL), ("switch 3, move 3", STALL), ("move 1 1, move 3", STALL)),
        "must_not_contain": [HINT + "Fake Out only works on your first turn out."],
    },
    {
        "id": "firstimpression-instruct",
        "purpose": "Instruct makes the user use First Impression a second time",
        "seed": "sodium,03030303030303030303030303030303",
        "teams": {
            "p1": team(mon("Golisopod", ["First Impression", "Protect", "Swords Dance", "Rest"]), mon("Oranguru", ["Instruct", "Protect", "Recover", "Rest"]),
                       *fillers("Snorlax", "Clefable")),
            "p2": team(*fillers("Kingambit", "Incineroar", "Umbreon", "Garchomp")),
        },
        "choices": rounds(("move 1 1, move 1 -1", STALL), ("move 2, move 2", STALL)),
        "must_contain": [HINT + "First Impression only works on your first turn out."],
    },
    {
        "id": "gigatonhammer-instruct",
        "purpose": "Instruct makes the user use Gigaton Hammer twice in a row: Showdown hints that effects can force that",
        "seed": "sodium,04040404040404040404040404040404",
        "teams": {
            "p1": team(mon("Tinkaton", ["Gigaton Hammer", "Protect", "Swords Dance", "Rest"]), mon("Oranguru", ["Instruct", "Protect", "Recover", "Rest"]),
                       *fillers("Snorlax", "Clefable")),
            "p2": team(*fillers("Kingambit", "Incineroar", "Umbreon", "Garchomp")),
        },
        "choices": rounds(("move 1 1, move 1 -1", STALL), ("move 2, move 2", STALL)),
        "must_contain": [HINT + "Some effects can force a Pokemon to use Gigaton Hammer again in a row."],
    },
    {
        "id": "gigatonhammer-once",
        "purpose": "Gigaton Hammer used once prints no hint (the control)",
        "seed": "sodium,14141414141414141414141414141414",
        "teams": {
            "p1": team(mon("Tinkaton", ["Gigaton Hammer", "Protect", "Swords Dance", "Rest"]), mon("Oranguru", FILLER), *fillers("Snorlax", "Clefable")),
            "p2": team(*fillers("Kingambit", "Incineroar", "Umbreon", "Garchomp")),
        },
        "choices": rounds(("move 1 1, move 3", STALL), ("move 2, move 2", STALL)),
        "must_not_contain": [HINT + "Some effects can force a Pokemon to use Gigaton Hammer again in a row."],
    },
    {
        "id": "aurawheel-copycat",
        "purpose": "Copycat copies Aura Wheel, which only a Morpeko can use",
        "seed": "sodium,05050505050505050505050505050505",
        "teams": {
            "p1": team(mon("Clefable", ["Copycat", "Protect", "Moonblast", "Rest"], spe=16), mon("Umbreon", FILLER), *fillers("Snorlax", "Garchomp")),
            "p2": team(mon("Morpeko", ["Aura Wheel", "Protect", "Swords Dance", "Rest"], spe=32), *fillers("Incineroar", "Kingambit", "Dragonite")),
        },
        "choices": rounds(("move 1, move 1", "move 1 2, move 3")),
        "must_contain": [HINT + "Only a Pokemon whose form is Morpeko or Morpeko-Hangry can use this move."],
    },
    {
        "id": "wonderroom-bodypress",
        "purpose": "Body Press under Wonder Room says it uses the other defence",
        "seed": "sodium,06060606060606060606060606060606",
        "teams": {
            "p1": team(mon("Snorlax", ["Body Press", "Protect", "Recover", "Rest"]), mon("Slowbro-Galar", ["Wonder Room", "Protect", "Recover", "Rest"]),
                       *fillers("Clefable", "Garchomp")),
            "p2": team(*fillers("Kingambit", "Incineroar", "Umbreon", "Dragonite")),
        },
        "choices": rounds(("move 2, move 1", STALL), ("move 1 1, move 3", STALL)),
        "must_contain": [HINT + "Body Press uses Def boosts when Wonder Room is active."],
    },
    {
        "id": "mimicry-transformed-terrain-ends",
        "purpose": "A Ditto transformed into a Mimicry holder changes type back when the terrain ends: the hint that Transform Mimicry does",
        "seed": "sodium,07070707070707070707070707070707",
        "teams": {
            "p1": team(mon("Slowbro-Galar", ["Psychic Terrain", "Protect", "Recover", "Rest"]), mon("Stunfisk-Galar", FILLER, ability="Mimicry"),
                       *fillers("Clefable", "Garchomp")),
            "p2": team(*fillers("Kingambit", "Incineroar"), mon("Ditto", ["Transform", "Protect", "Recover", "Rest"], ability="Imposter"), mon("Garchomp", FILLER)),
        },
        "choices": rounds(("move 1, move 3", STALL), ("move 3, move 3", "switch 3, move 3"), ("move 3, move 3", STALL), ("move 3, move 3", STALL),
                          ("move 3, move 3", STALL), ("move 3, move 3", STALL)),
        "must_contain": [HINT + "Transform Mimicry changes you to your original un-transformed types."],
    },
    {
        "id": "shellsidearm-foe",
        "purpose": "Shell Side Arm hits a foe: Showdown says which category it used (against an immune foe it says nothing)",
        "seed": "sodium,08080808080808080808080808080808",
        "teams": {
            "p1": team(mon("Slowbro-Galar", ["Shell Side Arm", "Protect", "Recover", "Rest"]), mon("Clefable", FILLER), *fillers("Snorlax", "Garchomp")),
            "p2": team(*fillers("Kingambit", "Incineroar", "Umbreon", "Dragonite")),
        },
        "choices": rounds(("move 1 1, move 3", STALL), ("move 1 2, move 3", STALL)),
        "must_contain": [HINT + "Physical Shell Side Arm"],
    },
    {
        "id": "shellsidearm-substitute",
        "purpose": "Shell Side Arm that hits a Substitute: the category hint comes from onAfterSubDamage",
        "seed": "sodium,17171717171717171717171717171717",
        "teams": {
            "p1": team(mon("Slowbro-Galar", ["Shell Side Arm", "Protect", "Recover", "Rest"]), mon("Clefable", FILLER), *fillers("Snorlax", "Garchomp")),
            "p2": team(mon("Kingambit", FILLER), mon("Incineroar", ["Substitute", "Protect", "Recover", "Rest"]), *fillers("Umbreon", "Dragonite")),
        },
        "choices": rounds(("move 3, move 3", "move 3, move 1"), ("move 1 2, move 3", STALL)),
        "must_contain": [HINT + "Special Shell Side Arm"],
    },
    {
        "id": "shellsidearm-ally",
        "purpose": "Shell Side Arm at an ally prints no category hint",
        "seed": "sodium,18181818181818181818181818181818",
        "teams": {
            "p1": team(mon("Slowbro-Galar", ["Shell Side Arm", "Protect", "Recover", "Rest"]), mon("Clefable", FILLER), *fillers("Snorlax", "Garchomp")),
            "p2": team(*fillers("Kingambit", "Incineroar", "Umbreon", "Dragonite")),
        },
        "choices": rounds(("move 1 -2, move 3", STALL)),
        "must_not_contain": [HINT + "Special Shell Side Arm", HINT + "Physical Shell Side Arm"],
    },
    {
        "id": "prankster-dark-target",
        "purpose": "A Prankster status move at a Dark-type foe is immune: Showdown says Dark is immune to Prankster moves",
        "seed": "sodium,09090909090909090909090909090909",
        "teams": {
            "p1": team(mon("Sableye", ["Thunder Wave", "Protect", "Recover", "Rest"], ability="Prankster"), mon("Clefable", FILLER), *fillers("Snorlax", "Garchomp")),
            "p2": team(*fillers("Kingambit", "Incineroar", "Umbreon", "Dragonite")),
        },
        "choices": rounds(("move 1 1, move 3", STALL)),
        "must_contain": [HINT + "Since gen 7, Dark is immune to Prankster moves."],
    },
    {
        "id": "prankster-dark-electric-target",
        "purpose": "The same move at a Dark/Electric foe, which paralysis cannot touch anyway: no hint",
        "seed": "sodium,19191919191919191919191919191919",
        "teams": {
            "p1": team(mon("Sableye", ["Thunder Wave", "Protect", "Recover", "Rest"], ability="Prankster"), mon("Clefable", FILLER), *fillers("Snorlax", "Garchomp")),
            "p2": team(mon("Morpeko", FILLER), *fillers("Incineroar", "Kingambit", "Dragonite")),
        },
        "choices": rounds(("move 1 1, move 3", STALL)),
        "must_not_contain": [HINT + "Since gen 7, Dark is immune to Prankster moves."],
    },
    {
        "id": "turn-overflow-wish-futuresight",
        "purpose": "From the 253rd turn on the engine's turn count (an 8-bit counter since Gen 8) overflows: a Future Sight used on turn 253 and a Wish used on turn 256 never resolve, and Showdown says so. The first 252 turns are both sides swapping their two active Pokémon",
        "seed": "sodium,0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a",
        "turn_limit": 400,
        "teams": {
            "p1": team(mon("Slowbro-Galar", ["Future Sight", "Protect", "Recover", "Rest"]), mon("Clefable", ["Wish", "Protect", "Recover", "Rest"]),
                       *fillers("Umbreon", "Snorlax")),
            "p2": team(*fillers("Kingambit", "Incineroar", "Dragonite", "Garchomp")),
        },
        "choices": rounds(*[("switch 3, switch 4", "switch 3, switch 4")] * 252, ("move 1 1, move 3", "move 3, move 3"),
                          ("move 3, move 3", "move 3, move 3"), ("move 3, move 3", "move 3, move 3"), ("move 3, move 1", "move 3, move 3")),
        "must_contain": [HINT + "In Gen 8+, Future attacks will never resolve when used on the 255th turn or later.",
                         HINT + "In Gen 8+, Wish will never resolve when used on the 256th turn."],
    },
    {
        "id": "wish-late",
        "purpose": "Wish used on an ordinary turn prints no hint (the control)",
        "seed": "sodium,1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a",
        "teams": {
            "p1": team(mon("Clefable", ["Wish", "Protect", "Recover", "Rest"]), *fillers("Umbreon", "Snorlax", "Garchomp")),
            "p2": team(*fillers("Kingambit", "Incineroar", "Dragonite", "Garchomp")),
        },
        "choices": rounds(("move 1, move 3", STALL), ("move 2, move 3", STALL)),
        "must_not_contain": [HINT + "In Gen 8+, Wish will never resolve when used on the 256th turn."],
    },
    {
        "id": "futuresight-target-fainted",
        "purpose": "Future Sight lands on a slot whose Pokémon fainted: Showdown says the move did not hit",
        "seed": "sodium,0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c",
        "teams": {
            "p1": team(mon("Slowbro-Galar", ["Future Sight", "Protect", "Recover", "Rest"]), *fillers("Umbreon", "Snorlax", "Garchomp")),
            "p2": team(mon("Kingambit", ["Explosion", "Protect", "Recover", "Rest"]), *fillers("Incineroar", "Dragonite", "Garchomp")),
        },
        "choices": rounds(("move 1 1, move 3", STALL), ("move 3, move 3", STALL), ("move 2, move 3", "move 1, move 3"), (None, "switch 3, pass")),
        "must_contain": [HINT + "Future Sight did not hit because the target is fainted."],
    },
    {
        "id": "skillswap-ally",
        "purpose": "Skill Swap between allies is not announced: Showdown says so",
        "seed": "sodium,0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d",
        "teams": {
            "p1": team(mon("Clefable", ["Skill Swap", "Protect", "Recover", "Rest"]), mon("Umbreon", FILLER), *fillers("Snorlax", "Garchomp")),
            "p2": team(*fillers("Kingambit", "Incineroar", "Dragonite", "Garchomp")),
        },
        "choices": rounds(("move 1 -2, move 3", STALL)),
        "must_contain": [HINT + "Skill Swap does not announce the abilities of the Pokémon when used between allies."],
    },
    {
        "id": "skillswap-foe",
        "purpose": "Skill Swap with a foe is announced and hints nothing (the control)",
        "seed": "sodium,1d1d1d1d1d1d1d1d1d1d1d1d1d1d1d1d",
        "teams": {
            "p1": team(mon("Clefable", ["Skill Swap", "Protect", "Recover", "Rest"]), mon("Umbreon", FILLER), *fillers("Snorlax", "Garchomp")),
            "p2": team(*fillers("Kingambit", "Incineroar", "Dragonite", "Garchomp")),
        },
        "choices": rounds(("move 1 1, move 3", STALL)),
        "must_not_contain": [HINT + "Skill Swap does not announce the abilities of the Pokémon when used between allies."],
    },
    {
        "id": "risingvoltage-grounded",
        "purpose": "Rising Voltage with Electric Terrain up at a grounded foe says its power doubled",
        "seed": "sodium,0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e",
        "teams": {
            "p1": team(mon("Pikachu", ["Rising Voltage", "Protect", "Recover", "Rest"]), mon("Jolteon", ["Electric Terrain", "Protect", "Recover", "Rest"]),
                       *fillers("Snorlax", "Garchomp")),
            "p2": team(*fillers("Kingambit", "Incineroar", "Dragonite", "Umbreon")),
        },
        "choices": rounds(("move 3, move 1", STALL), ("move 1 1, move 3", STALL)),
        "must_contain": [HINT + "Rising Voltage's BP doubled on grounded target."],
    },
    {
        "id": "risingvoltage-ally",
        "purpose": "Rising Voltage at an ally prints no hint",
        "seed": "sodium,1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e",
        "teams": {
            "p1": team(mon("Pikachu", ["Rising Voltage", "Protect", "Recover", "Rest"]), mon("Jolteon", ["Electric Terrain", "Protect", "Recover", "Rest"]),
                       *fillers("Snorlax", "Garchomp")),
            "p2": team(*fillers("Kingambit", "Incineroar", "Dragonite", "Umbreon")),
        },
        "choices": rounds(("move 3, move 1", STALL), ("move 1 -2, move 3", STALL)),
        "must_not_contain": [HINT + "Rising Voltage's BP doubled on grounded target."],
    },
    {
        "id": "keeneye-gravity-wonderroom",
        "purpose": "Not a hint: Gravity and Wonder Room tie in the ModifyMove list of a move, and Keen Eye is in it too, so Showdown's shuffle is shuffle(3, 1, 3) where a list without Keen Eye gives shuffle(2, 0, 2). The scenario's draws pin the framing (the first of two gaps the 3,200-battle run found)",
        "seed": "sodium,20202020202020202020202020202020",
        "teams": {
            "p1": team(mon("Sableye", FILLER, ability="Keen Eye"), mon("Clefable", ["Gravity", "Protect", "Recover", "Rest"]), *fillers("Snorlax", "Garchomp")),
            "p2": team(mon("Kingambit", FILLER), mon("Hatterene", ["Wonder Room", "Protect", "Recover", "Rest"]), *fillers("Incineroar", "Dragonite")),
        },
        "choices": rounds(("move 3, move 1", "move 3, move 1"), ("move 3, move 3", STALL), ("move 3, move 3", STALL)),
        "must_contain": ["|-fieldstart|move: Gravity", "|-fieldstart|move: Wonder Room|[of] p2b: Hatterene"],
    },
    {
        "id": "psychicterrain-airborne",
        "purpose": "A priority move at an airborne target under Psychic Terrain gets through: Showdown says the terrain does not protect it",
        "seed": "sodium,0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f",
        "teams": {
            "p1": team(mon("Corviknight", FILLER), mon("Slowbro-Galar", ["Psychic Terrain", "Protect", "Recover", "Rest"]), *fillers("Snorlax", "Garchomp")),
            "p2": team(mon("Persian", ["Quick Attack", "Protect", "Recover", "Rest"]), *fillers("Kingambit", "Incineroar", "Dragonite")),
        },
        "choices": rounds(("move 3, move 1", STALL), ("move 3, move 3", "move 1 1, move 3")),
        "must_contain": [HINT + "Psychic Terrain doesn't affect airborne Pokémon."],
    },
    {
        "id": "psychicterrain-grounded",
        "purpose": "The same priority move at a grounded target is blocked by the terrain, with no hint",
        "seed": "sodium,1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f",
        "teams": {
            "p1": team(mon("Snorlax", FILLER), mon("Slowbro-Galar", ["Psychic Terrain", "Protect", "Recover", "Rest"]), *fillers("Corviknight", "Garchomp")),
            "p2": team(mon("Persian", ["Quick Attack", "Protect", "Recover", "Rest"]), *fillers("Kingambit", "Incineroar", "Dragonite")),
        },
        "choices": rounds(("move 3, move 1", STALL), ("move 3, move 3", "move 1 1, move 3")),
        "must_not_contain": [HINT + "Psychic Terrain doesn't affect airborne Pokémon."],
    },
]


def main():
    with tempfile.NamedTemporaryFile("w", suffix=".json") as f:
        json.dump(scenarios, f)
        f.flush()
        rows, draws = OUT / "scenarios.jsonl.gz", OUT / "scenarios-draws.jsonl.gz"
        out = subprocess.run(["node", str(HERE / "scenario.cjs"), f.name, str(rows), str(draws)], capture_output=True, text=True)
    sys.stderr.write(out.stderr)
    return out.returncode


if __name__ == "__main__":
    sys.exit(main())
