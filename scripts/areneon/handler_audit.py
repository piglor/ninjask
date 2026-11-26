#!/usr/bin/env python3
"""List the effects of the format that Showdown's `runEvent` collects for an event: every ability, item, move condition
and condition of the export with a handler `on<Event>`, `onAlly<Event>`, `onFoe<Event>`, `onAny<Event>` or
`onSource<Event>` (with or without a `Priority` / `Order` / `SubOrder` suffix), by kind.

    python3 scripts/areneon/handler_audit.py EXPORT.json ModifyMove [Update ...]
    python3 scripts/areneon/handler_audit.py --json EXPORT.json EVENT [...]    (tests/fixtures/areneon/handler-sets.json)

The engine builds the sorted handler list of an event by hand (`speed_sort_by` in src/), so a handler it leaves out
because it never acts still changes the framing of the draws Showdown makes for ties (the two known gaps in
docs/n1-seeds-channels-candidates.md). This lists what the list has to account for.
"""
import json
import re
import sys


def handlers(entity, event):
    pat = re.compile(rf"^(?:condition\.)?on(?:Ally|Foe|Any|Source)?{event}(?:Priority|Order|SubOrder)?$")
    return sorted({c for c in entity.get("callbacks", []) if pat.match(c)})


ROLES = {"": "Own", "Source": "Source", "Ally": "Ally", "Foe": "Foe", "Any": "Any"}
KINDS = ("abilities", "items", "moves", "conditions")


def entities(dex, kind):
    rows = dex.get(kind, [])
    if isinstance(rows, dict):  # the export keys `conditions` by id
        rows = [{"id": k, **v} for k, v in rows.items()]
    return rows


def by_role(entity, event):
    """The roles an entity has a handler for `event` in, with the callback names: `{role: [callback...]}`."""
    out = {}
    for c in entity.get("callbacks", []):
        m = re.match(rf"^(?:condition\.)?on(Ally|Foe|Any|Source)?{event}$", c)
        if m:
            out.setdefault(ROLES[m.group(1) or ""], []).append(c)
    return out


def handler_sets(dex, events):
    """Every ability, item, move and condition with a handler for each event, by role: the data the engine's tables are
    built from (`src/dex/events.rs`), as a JSON-able dict with sorted ids."""
    out = {}
    for event in events:
        out[event] = {}
        for kind in KINDS:
            roles = {}
            for e in entities(dex, kind):
                for role in by_role(e, event):
                    roles.setdefault(role, []).append(e["id"])
            out[event][kind] = {role: sorted(ids) for role, ids in sorted(roles.items())}
    return out


def main():
    if sys.argv[1] == "--json":
        # --json DEX EVENT...: the sets above, one JSON document on stdout
        print(json.dumps({"events": handler_sets(json.load(open(sys.argv[2])), sys.argv[3:])}, indent=1, sort_keys=True))
        return
    dex = json.load(open(sys.argv[1]))
    for event in sys.argv[2:]:
        print(f"== {event}")
        for kind in ("abilities", "items", "moves", "conditions"):
            rows = []
            entities = dex.get(kind, [])
            if isinstance(entities, dict):  # the export keys `conditions` by id
                entities = [{"id": k, **v} for k, v in entities.items()]
            for e in entities:
                hs = handlers(e, event)
                # a priority/order key alone is no handler
                hs = [h for h in hs if not re.search(r"(Priority|Order|SubOrder)$", h)]
                if hs:
                    rows.append(f"{e['id']}({','.join(hs)})")
            print(f"  {kind:10} {len(rows):3}: {' '.join(rows)}")


if __name__ == "__main__":
    main()
