#!/usr/bin/env python3
"""List the effects of the format that Showdown's `runEvent` collects for an event: every ability, item, move condition
and condition of the export with a handler `on<Event>`, `onAlly<Event>`, `onFoe<Event>`, `onAny<Event>` or
`onSource<Event>` (with or without a `Priority` / `Order` / `SubOrder` suffix), by kind.

    python3 scripts/areneon/handler_audit.py EXPORT.json ModifyMove [Update ...]

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


def main():
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
