"""Pin tools: what ties the engine to a Showdown commit, and what to read when the pin moves.

The engine is a port of Pokémon Showdown at the commit in `refsim/SHOWDOWN_COMMIT`, and
loads its game data from exports of that checkout. `refsim/PROVENANCE.json` records the
commit, the export tool and the hash of every exported format; this script writes and
checks it. docs/repin.md is the procedure for a pin bump.

    python scripts/repin.py provenance check            # the exports are what the file says
    python scripts/repin.py provenance write            # after a deliberate pin bump or tool change
    python scripts/repin.py diff-showdown OLD NEW       # commits and files between two pins, classified
    python scripts/repin.py diff-exports OLD.json NEW.json   # what changed in the exported data

`provenance` exports each bundled format from the Showdown checkout (`NINJASK_SHOWDOWN`,
else `third_party/pokemon-showdown`), which `refsim/export_format.js` accepts only at the
pin with no tracked file changed.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import subprocess
import sys
from pathlib import Path

from ninjask.paths import ENGINE_ROOT, REFSIM_DIR, SHOWDOWN_DIR, node_env, require_checkout

PROVENANCE = REFSIM_DIR / "PROVENANCE.json"
PIN_FILE = REFSIM_DIR / "SHOWDOWN_COMMIT"
# The formats scripts/bundle_data.sh exports into a wheel.
FORMATS = (
    "gen9championsvgc2026regmc",
    "gen9championsvgc2026regmcots",
    "gen9championsvgc2026regmb",
    "gen9championsdoublescustomgame",
)
TOOL_FILES = ("refsim/export_format.js", "refsim/showdown_path.js")
SECTIONS = ("format", "species", "learnsets", "moves", "items", "abilities", "natures", "typechart", "conditions")
SCHEMA = 1


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def canonical(value: object) -> bytes:
    return json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=False).encode()


def pin() -> str:
    return PIN_FILE.read_text().strip()


def export_bytes(format_id: str) -> bytes:
    require_checkout("exporting a format from Showdown")
    return subprocess.run(
        ["node", str(REFSIM_DIR / "export_format.js"), format_id],
        check=True,
        capture_output=True,
        env=node_env(),
    ).stdout


def describe(data: bytes) -> dict:
    """An export's size, hash and, per section, the entry count and hash (a repin's diff reads
    the sections that moved)."""
    export = json.loads(data)
    sections = {}
    for name in SECTIONS:
        value = export[name]
        sections[name] = {"count": len(value) if isinstance(value, (list, dict)) else 1, "sha256": sha256(canonical(value))}
    return {"bytes": len(data), "sha256": sha256(data), "sections": sections}


def build_provenance(exports: dict[str, bytes], previous: dict | None = None) -> dict:
    """The provenance record for `exports` (format id -> export bytes)."""
    old = (previous or {}).get("showdown", {}).get("previous_pins", [])
    old_pin = (previous or {}).get("showdown", {}).get("commit")
    pins = list(old)
    if old_pin and old_pin != pin() and old_pin not in [p["commit"] for p in pins]:
        pins.append({"commit": old_pin})
    return {
        "schema": SCHEMA,
        "showdown": {
            "repository": "https://github.com/smogon/pokemon-showdown",
            "commit": pin(),
            "previous_pins": pins,
        },
        "export": {
            "tool_files": {f: sha256((ENGINE_ROOT / f).read_bytes()) for f in TOOL_FILES},
            "formats": {f: describe(exports[f]) for f in sorted(exports)},
        },
    }


def render(record: dict) -> str:
    return json.dumps(record, indent=2, sort_keys=True) + "\n"


def differences(want: dict, got: dict) -> list[str]:
    """Where two provenance records differ, one line each."""
    out: list[str] = []

    def walk(a: object, b: object, path: str) -> None:
        if isinstance(a, dict) and isinstance(b, dict):
            for k in sorted(set(a) | set(b)):
                if k not in a or k not in b:
                    out.append(f"{path}/{k}: recorded {a.get(k, '<none>')!r}, found {b.get(k, '<none>')!r}")
                else:
                    walk(a[k], b[k], f"{path}/{k}")
        elif a != b:
            out.append(f"{path}: recorded {a!r}, found {b!r}")

    walk(want, got, "")
    return out


def cmd_provenance(args: argparse.Namespace) -> int:
    exports = {f: export_bytes(f) for f in FORMATS}
    previous = json.loads(PROVENANCE.read_text()) if PROVENANCE.exists() else None
    record = build_provenance(exports, previous)
    if args.action == "write":
        PROVENANCE.write_text(render(record))
        print(f"wrote {PROVENANCE.relative_to(ENGINE_ROOT)}: pin {record['showdown']['commit'][:8]}, {len(exports)} formats")
        return 0
    if previous is None:
        print(f"{PROVENANCE} is missing: run `provenance write`", file=sys.stderr)
        return 1
    problems = differences(previous, record)
    for line in problems:
        print(line)
    print("provenance: ok" if not problems else f"provenance: {len(problems)} difference(s); after a deliberate pin or tool change run `provenance write` and read docs/repin.md")
    return 1 if problems else 0


# What the files of a Showdown commit range say about the engine. A guide for the reviewer, not a
# verdict: the classes name where to look, and a handler change is never "data".
# Showdown's own `data/` directory (not this checkout's: the tests refuse a quoted path of that name).
SD = "data"
MECHANICS = (
    f"{SD}/mods/champions/",
    f"{SD}/moves.ts",
    f"{SD}/abilities.ts",
    f"{SD}/items.ts",
    f"{SD}/conditions.ts",
    f"{SD}/scripts.ts",
    f"{SD}/rulesets.ts",
    f"{SD}/typechart.ts",
    f"{SD}/natures.ts",
    "sim/battle",
    "sim/pokemon.ts",
    "sim/side.ts",
    "sim/field.ts",
    "sim/prng.ts",
    "sim/state.ts",
    "sim/dex-moves.ts",
    "sim/dex-abilities.ts",
    "sim/dex-conditions.ts",
    "sim/dex-items.ts",
)
DATA = (
    "config/formats.ts",
    f"{SD}/learnsets.ts",
    f"{SD}/pokedex.ts",
    f"{SD}/formats-data.ts",
    f"{SD}/tags.ts",
    "sim/team-validator.ts",
    "sim/dex-species.ts",
    "sim/dex-formats.ts",
    "sim/dex.ts",
    "sim/teams.ts",
)


def classify(path: str) -> str:
    """'mechanics' (handler code: port or confirm), 'data' (an export carries it) or 'other'."""
    if path.startswith(MECHANICS):
        return "mechanics"
    if path.startswith(DATA):
        return "data"
    return "other"


def cmd_diff_showdown(args: argparse.Namespace) -> int:
    checkout = Path(args.showdown) if args.showdown else SHOWDOWN_DIR

    def git(*a: str) -> str:
        return subprocess.run(["git", "-C", str(checkout), *a], check=True, capture_output=True, text=True).stdout

    commits = git("log", "--reverse", "--format=%H %ad %s", "--date=short", f"{args.old}..{args.new}").splitlines()
    print(f"{len(commits)} commit(s) {args.old[:8]}..{args.new[:8]}")
    totals: dict[str, int] = {"mechanics": 0, "data": 0, "other": 0}
    for line in commits:
        sha, _, rest = line.partition(" ")
        files = git("show", "--format=", "--name-only", sha).split()
        classes = {c: [f for f in files if classify(f) == c] for c in totals}
        for c, fs in classes.items():
            totals[c] += len(fs)
        print(f"\n{sha[:9]} {rest}")
        for c in ("mechanics", "data"):
            for f in classes[c]:
                print(f"    {c:9s} {f}")
        if classes["other"]:
            print(f"    other     {len(classes['other'])} file(s) (server, tests, text...)")
    print(f"\nfiles touched: {totals['mechanics']} mechanics, {totals['data']} data, {totals['other']} other")
    print("read every mechanics file's diff (`git diff OLD NEW -- PATH`): a handler change is ported, never exported;")
    print("then `diff-exports` shows what the data exports carry of the rest.")
    return 0


def diff_exports(old: dict, new: dict) -> list[str]:
    """The entities that differ between two exports: added, removed, and changed fields."""
    out: list[str] = []
    for name in SECTIONS:
        a, b = old.get(name), new.get(name)
        if a == b:
            continue
        if name in ("format", "typechart", "natures") or not isinstance(a, (list, dict)):
            keys = sorted(k for k in set(a or {}) | set(b or {}) if (a or {}).get(k) != (b or {}).get(k))
            out.append(f"{name}: changed {', '.join(keys) if keys else '(value)'}")
            continue
        ia = {e["id"]: e for e in a} if isinstance(a, list) else dict(a)
        ib = {e["id"]: e for e in b} if isinstance(b, list) else dict(b)
        for k in sorted(set(ia) - set(ib)):
            out.append(f"{name}: removed {k}")
        for k in sorted(set(ib) - set(ia)):
            out.append(f"{name}: added {k}")
        for k in sorted(set(ia) & set(ib)):
            if ia[k] != ib[k]:
                x, y = ia[k], ib[k]
                if isinstance(x, dict) and isinstance(y, dict):
                    fields = sorted(f for f in set(x) | set(y) if x.get(f) != y.get(f))
                    detail = []
                    for f in fields:
                        if isinstance(x.get(f), list) and isinstance(y.get(f), list) and f in ("callbacks", "move_callbacks"):
                            detail.append(f"{f} -{sorted(set(x[f]) - set(y[f]))} +{sorted(set(y[f]) - set(x[f]))}")
                        else:
                            detail.append(f)
                    out.append(f"{name}: changed {k}: {'; '.join(detail)}")
                else:
                    out.append(f"{name}: changed {k}")
    return out


def cmd_diff_exports(args: argparse.Namespace) -> int:
    lines = diff_exports(json.loads(Path(args.old).read_text()), json.loads(Path(args.new).read_text()))
    print("\n".join(lines) if lines else "the exports are equal")
    return 0


def main() -> int:
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = p.add_subparsers(dest="cmd", required=True)
    pr = sub.add_parser("provenance", help="write or check refsim/PROVENANCE.json")
    pr.add_argument("action", choices=("write", "check"))
    pr.set_defaults(run=cmd_provenance)
    ds = sub.add_parser("diff-showdown", help="the commits and files between two Showdown commits, classified")
    ds.add_argument("old")
    ds.add_argument("new")
    ds.add_argument("--showdown", help="the Showdown checkout (default: the engine's)")
    ds.set_defaults(run=cmd_diff_showdown)
    de = sub.add_parser("diff-exports", help="what differs between two format exports")
    de.add_argument("old")
    de.add_argument("new")
    de.set_defaults(run=cmd_diff_exports)
    args = p.parse_args()
    return args.run(args)


if __name__ == "__main__":
    sys.exit(main())
