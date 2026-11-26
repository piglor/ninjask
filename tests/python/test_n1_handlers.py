"""The sorted handler lists of the engine against the handlers the format has.

Showdown collects every handler of an event and sorts them, drawing between equal ones; the engine's lists for
`ModifyMove` (src/moves/use_move.rs) and `Update` (src/events/update.rs) hold the handlers by their priorities from the
export's tables (`dex/events.rs`), plus the volatiles and field effects by hand. These tests hold the data they depend
on: the abilities and items with such a handler, and that none is an `onAny`, `onFoe`, `onAlly` or `onSource` one (the
lists read only the user's own). A new pin that adds one fails here, and the list has to be looked at
(scripts/areneon/handler_audit.py prints them for any event).
"""

from __future__ import annotations

import importlib.util
from pathlib import Path

import pytest

from n1_fixtures import FIXTURES

AUDIT = Path(__file__).resolve().parents[2] / "scripts" / "areneon" / "handler_audit.py"
spec = importlib.util.spec_from_file_location("handler_audit", AUDIT)
audit = importlib.util.module_from_spec(spec)
spec.loader.exec_module(audit)

import json  # noqa: E402

DEX = json.loads((FIXTURES / "dex-gen9championsvgc2026regmc-7244fe8e.json").read_text())


def with_handler(kind: str, event: str) -> dict[str, list[str]]:
    out = {}
    for e in DEX[kind]:
        hs = [h for h in audit.handlers(e, event) if not h.endswith(("Priority", "Order", "SubOrder"))]
        if hs:
            out[e["id"]] = hs
    return out


MODIFY_MOVE_ABILITIES = {
    "battlebond", "illuminate", "infiltrator", "keeneye", "longreach", "moldbreaker", "scrappy", "sheerforce", "skilllink",
    "stalwart", "stancechange", "stench",
}
UPDATE_ABILITIES = {
    "disguise", "iceface", "immunity", "insomnia", "limber", "magmaarmor", "oblivious", "owntempo", "thermalexchange", "trace",
    "vitalspirit", "waterbubble",
}
UPDATE_ITEMS = {
    "aspearberry", "cheriberry", "chestoberry", "leppaberry", "lumberry", "mentalherb", "oranberry", "pechaberry", "persimberry",
    "rawstberry", "sitrusberry",
}


@pytest.mark.parametrize(
    "event,abilities,items",
    [("ModifyMove", MODIFY_MOVE_ABILITIES, {"choicescarf", "kingsrock"}), ("Update", UPDATE_ABILITIES, UPDATE_ITEMS)],
)
def test_the_abilities_and_items_with_a_handler_are_the_ones_the_lists_were_checked_against(event, abilities, items):
    assert set(with_handler("abilities", event)) == abilities
    assert set(with_handler("items", event)) == items


@pytest.mark.parametrize("event", ["ModifyMove", "Update"])
def test_only_the_holders_own_handler_exists(event):
    for kind in ("abilities", "items"):
        for id_, hs in with_handler(kind, event).items():
            assert hs == [f"on{event}"], (kind, id_, hs)


# ---------------------------------------------------------------------------------------------------------------------
# Every event the engine sorts handlers for: the committed sets of the export (tests/fixtures/areneon/handler-sets.json)

HANDLER_SETS = FIXTURES / "handler-sets.json"
EVENTS_RS = Path(__file__).resolve().parents[2] / "src" / "dex" / "events.rs"


def rust_events() -> list[str]:
    """The events of `Event::ALL` in src/dex/events.rs: the ones whose ability and item handlers the engine's sorted
    lists read from the dex tables."""
    import re

    text = EVENTS_RS.read_text()
    start = text.index("pub const ALL: [Event;")
    return re.findall(r"Event::(\w+)", text[start : text.index("];", start)])


def test_the_handler_sets_of_every_sorted_event_are_the_committed_ones():
    """What Showdown collects for each event the engine sorts (`handler_audit.py --json`), by kind and role: the lists
    the engine builds have to account for exactly these. A pin that adds or removes a handler fails here, and the list
    of that event has to be looked at again (the per-event table of docs/n1-seeds-channels-candidates.md)."""
    committed = json.loads(HANDLER_SETS.read_text())["events"]
    assert set(rust_events()) <= set(committed), "an event the engine reads from the tables has no committed set"
    assert set(committed) - set(rust_events()) == {"Start"}, "Start is the one extra: it is the SwitchIn handler of an ability or item"
    assert audit.handler_sets(DEX, sorted(committed)) == committed
