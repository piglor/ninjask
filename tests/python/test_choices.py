from ninjask.choices import joint_choices, team_preview_choices


def mon(name, active=False, hp="100/100", **extra):
    return {"ident": f"p1: {name}", "details": f"{name}, L50", "condition": hp, "active": active, **extra}


def move(name, target="normal", disabled=False):
    return {"move": name, "id": name.lower(), "pp": 8, "maxpp": 8, "target": target, "disabled": disabled}


def side(*mons):
    return {"side": {"name": "A", "id": "p1", "pokemon": list(mons)}}


def test_team_preview_has_90_plans_for_bring_four():
    req = {"teamPreview": True, "maxChosenTeamSize": 4, **side(*[mon(str(i)) for i in range(6)])}
    plans = team_preview_choices(req)
    assert len(plans) == 90
    assert len(set(plans)) == 90
    assert "team 1234" in plans and "team 3412" in plans


def test_targets_mega_and_switches():
    req = {
        "active": [
            {"moves": [move("Tackle"), move("Protect", "self")], "canMegaEvo": True},
            {"moves": [move("Helping Hand", "adjacentAlly")]},
        ],
        **side(mon("A", True), mon("B", True), mon("C"), mon("D", hp="0 fnt")),
    }
    choices = joint_choices(req)
    # Slot 1: Tackle at foe 1 or 2, Protect, each with or without Mega, or switch to C.
    # Slot 2: Helping Hand at our slot 1 (-1), or switch to C.
    assert "move 1 1 mega, move 1 -1" in choices
    assert "move 2, move 1 -1" in choices
    assert "switch 3, move 1 -1" in choices
    assert "switch 3, switch 3" not in choices  # same Pokémon twice
    assert not any("switch 4" in c for c in choices)  # fainted
    # Slot 1: Tackle (2 targets) and Protect, each with or without Mega, or
    # switch: 7 options. Slot 2: Helping Hand at slot 1, or switch: 2.
    # 7 * 2 minus the double switch.
    assert len(choices) == 13


def test_only_one_mega_per_turn():
    req = {
        "active": [
            {"moves": [move("Protect", "self")], "canMegaEvo": True},
            {"moves": [move("Protect", "self")], "canMegaEvo": True},
        ],
        **side(mon("A", True), mon("B", True)),
    }
    choices = joint_choices(req)
    assert "move 1 mega, move 1 mega" not in choices
    assert set(choices) == {"move 1, move 1", "move 1 mega, move 1", "move 1, move 1 mega"}


def test_locked_move_takes_no_target():
    req = {
        "active": [
            {"moves": [{"move": "Electro Shot", "id": "electroshot"}], "trapped": True},
            {"moves": [move("Protect", "self")]},
        ],
        **side(mon("A", True), mon("B", True), mon("C")),
    }
    choices = joint_choices(req)
    assert "move 1, move 1" in choices
    assert not any(c.startswith("move 1 1") for c in choices)
    assert not any(c.startswith("switch") for c in choices)  # trapped


def test_one_active_request_reads_as_singles_without_a_game_type():
    # A side down to its last Pokémon: the request names only the survivor.
    # Nothing in it says the battle is doubles, so the default (and an
    # explicit singles) reading drops move targets, as singles requires.
    req = {
        "active": [{"moves": [move("Fake Out"), move("Quick Attack"), move("Protect", "self")]}],
        **side(mon("Pikachu", True, "97/110"), mon("B", hp="0 fnt"), mon("C", hp="0 fnt")),
    }
    assert joint_choices(req) == ["move 1", "move 2", "move 3"]
    assert joint_choices(req, game_type="singles") == ["move 1", "move 2", "move 3"]


def test_one_active_doubles_endgame_enumerates_targets():
    # The same request in a doubles battle: single-target moves still need
    # a target (the engine rejects them bare), so naming the game type
    # enumerates both foe targets and no bare form.
    req = {
        "active": [{"moves": [move("Fake Out"), move("Quick Attack"), move("Protect", "self")]}],
        **side(mon("Pikachu", True, "97/110"), mon("B", hp="0 fnt"), mon("C", hp="0 fnt")),
    }
    choices = joint_choices(req, game_type="doubles")
    assert sorted(choices) == ["move 1 1", "move 1 2", "move 2 1", "move 2 2", "move 3"]
    assert "move 1" not in choices


def test_game_type_changes_nothing_with_a_full_field():
    req = {
        "active": [
            {"moves": [move("Tackle"), move("Protect", "self")]},
            {"moves": [move("Tackle")]},
        ],
        **side(mon("A", True), mon("B", True), mon("C")),
    }
    assert joint_choices(req, game_type="doubles") == joint_choices(req)


def test_forced_switch_with_one_replacement_for_two_slots():
    req = {"forceSwitch": [True, True], **side(mon("A", True, "0 fnt"), mon("B", True, "0 fnt"), mon("C"))}
    assert sorted(joint_choices(req)) == ["pass, switch 3", "switch 3, pass"]


def test_revival_blessing_picks_a_fainted_teammate():
    req = {
        "forceSwitch": [True, False],
        **side(mon("Pawmot", True, reviving=True), mon("B", True), mon("C", hp="0 fnt"), mon("D")),
    }
    assert joint_choices(req) == ["switch 3, pass"]


def test_revival_blessing_may_pass_when_no_living_pokemon_waits_on_the_bench():
    # Showdown: a forced pass is allowed for each Pokémon that must switch and has
    # nobody to replace it, the reviving one included (`forcedPassesLeft`).
    req = {
        "forceSwitch": [False, True],
        **side(mon("A", True), mon("Pawmot", True, reviving=True), mon("C", hp="0 fnt"), mon("D", hp="0 fnt")),
    }
    assert joint_choices(req) == ["pass, switch 3", "pass, switch 4", "pass, pass"]


def test_revival_blessing_must_revive_while_a_living_pokemon_waits_on_the_bench():
    req = {
        "forceSwitch": [False, True],
        **side(mon("A", True), mon("Pawmot", True, reviving=True), mon("C", hp="0 fnt"), mon("D")),
    }
    assert joint_choices(req) == ["pass, switch 3"]


def test_revival_blessing_and_a_faint_in_one_request():
    # Both slots must switch (the fainted one and the reviver); one living Pokémon waits, so one
    # forced switch is needed and one pass is allowed (Showdown's forcedSwitchesLeft and
    # forcedPassesLeft). A revival counts as that switch, as it does in Showdown, and may pick the
    # Pokémon that has just fainted in the other slot.
    req = {
        "forceSwitch": [True, True],
        **side(mon("A", True, "0 fnt"), mon("Pawmot", True, reviving=True), mon("C", hp="0 fnt"), mon("D")),
    }
    assert joint_choices(req) == [
        "switch 4, switch 1",
        "switch 4, switch 3",
        "switch 4, pass",
        "pass, switch 1",
        "pass, switch 3",
    ]


def test_two_reviving_slots_may_name_the_same_fainted_teammate():
    # Showdown records no revival in `switchIns`: all nine combinations of two revivals and a pass are
    # accepted (a forced pass is allowed for each, nobody lives on the bench), the same teammate twice
    # included. Checked on the pin with two Pawmot.
    req = {
        "forceSwitch": [True, True],
        **side(mon("R1", True, reviving=True), mon("R2", True, reviving=True), mon("C", hp="0 fnt"), mon("D", hp="0 fnt")),
    }
    assert sorted(joint_choices(req)) == sorted(f"{a}, {b}" for a in ("switch 3", "switch 4", "pass") for b in ("switch 3", "switch 4", "pass"))
    # A normal switch still goes to one Pokémon at most.
    plain = {"forceSwitch": [True, True], **side(mon("A", True, "0 fnt"), mon("B", True, "0 fnt"), mon("C"), mon("D"))}
    assert sorted(joint_choices(plain)) == ["switch 3, switch 4", "switch 4, switch 3"]
