"""Pin tools: what ties the engine to a Showdown commit, and what to read when the pin moves.

The engine is a port of Pokémon Showdown at the commit in `refsim/SHOWDOWN_COMMIT`, and
loads its game data from exports of that checkout. `refsim/PROVENANCE.json` records the
commit, the export tool, the hash of every exported format, and a hash of every Showdown
source file the engine's data and handlers come from (every `sim/*.ts`, and every file the
bundled formats load, the mod files of each format's inheritance chain included, found by
`refsim/source_files.js`); this script writes and checks it. docs/repin.md is the procedure
for a pin bump.

    python scripts/repin.py provenance check            # the exports and the sources are what the file says
    python scripts/repin.py provenance write            # after a deliberate pin bump or tool change
    python scripts/repin.py provenance check-data DIR   # exports in DIR/format/ are the recorded ones (no Showdown needed)
    python scripts/repin.py diff-showdown OLD NEW       # commits and files between two pins, classified
    python scripts/repin.py diff-showdown OLD NEW --strict   # exit 1 if a mechanics or unreviewed file differs
    python scripts/repin.py diff-exports OLD.json NEW.json   # what changed in the exported data

`provenance` exports each bundled format from the Showdown checkout (`NINJASK_SHOWDOWN`,
else `third_party/pokemon-showdown`), which `refsim/export_format.js` accepts only at the
pin with no tracked file changed. This script reads files and runs git and node: it does not
import the engine's package, so it works on a checkout whose engine is not built.
"""

from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
import subprocess
import sys
from collections.abc import Sequence
from pathlib import Path
from types import ModuleType


def _paths() -> ModuleType:
    """`python/ninjask/paths.py` by file: it imports nothing of the package, which would need the
    compiled module."""
    here = Path(__file__).resolve().parents[1] / "python" / "ninjask" / "paths.py"
    spec = importlib.util.spec_from_file_location("ninjask_paths_for_repin", here)
    assert spec and spec.loader
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


_p = _paths()
ENGINE_ROOT: Path = _p.ENGINE_ROOT
REFSIM_DIR: Path = _p.REFSIM_DIR
SHOWDOWN_DIR: Path = _p.SHOWDOWN_DIR
node_env = _p.node_env
require_checkout = _p.require_checkout

PROVENANCE = REFSIM_DIR / "PROVENANCE.json"
PIN_FILE = REFSIM_DIR / "SHOWDOWN_COMMIT"
# The formats scripts/bundle_data.sh exports into a wheel.
FORMATS = (
    "gen9championsvgc2026regmc",
    "gen9championsvgc2026regmcots",
    "gen9championsvgc2026regmb",
    "gen9championsdoublescustomgame",
)
TOOL_FILES = ("refsim/export_format.js", "refsim/showdown_path.js", "refsim/source_files.js")
SECTIONS = ("format", "species", "learnsets", "moves", "items", "abilities", "natures", "typechart", "conditions")
SCHEMA = 2


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def canonical(value: object) -> bytes:
    return json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=False).encode()


def pin() -> str:
    return PIN_FILE.read_text().strip()


def load_provenance() -> dict | None:
    return json.loads(PROVENANCE.read_text()) if PROVENANCE.exists() else None


def export_bytes(format_id: str) -> bytes:
    require_checkout("exporting a format from Showdown")
    return subprocess.run(
        ["node", str(REFSIM_DIR / "export_format.js"), format_id],
        check=True,
        capture_output=True,
        env=node_env(),
    ).stdout


def source_files(formats: Sequence[str] = FORMATS, env: dict[str, str] | None = None) -> dict:
    """What `refsim/source_files.js` finds in the Showdown checkout: its commit, each format's mod
    chain and a hash of every source file (`{commit, formats, files}`)."""
    require_checkout("listing the Showdown sources")
    out = subprocess.run(
        ["node", str(REFSIM_DIR / "source_files.js"), *formats],
        check=True,
        capture_output=True,
        env=env or node_env(),
    ).stdout
    return json.loads(out)


def describe(data: bytes) -> dict:
    """An export's size, hash and, per section, the entry count and hash (a repin's diff reads
    the sections that moved)."""
    export = json.loads(data)
    sections = {}
    for name in SECTIONS:
        value = export[name]
        sections[name] = {"count": len(value) if isinstance(value, (list, dict)) else 1, "sha256": sha256(canonical(value))}
    return {"bytes": len(data), "sha256": sha256(data), "sections": sections}


def build_provenance(exports: dict[str, bytes], sources: dict, previous: dict | None = None) -> dict:
    """The provenance record for `exports` (format id -> export bytes) and `sources` (what
    `source_files` returns for the same formats)."""
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
            "sources": sources["files"],
        },
        "export": {
            "tool_files": {f: sha256((ENGINE_ROOT / f).read_bytes()) for f in TOOL_FILES},
            "formats": {
                f: {**describe(exports[f]), "mod_chain": sources["formats"][f]["mod_chain"]} for f in sorted(exports)
            },
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


def check_data(directory: Path, record: dict, only: Sequence[str] = ()) -> list[str]:
    """Where the exports in `directory/format/<id>.json` are not the ones `record` lists (a missing
    file, another size or hash), one line each. A directory of exports made elsewhere (the build of a
    project that vendors the engine) is held to the record this way, without a Showdown to export from;
    files the record does not list are not its concern. `only` limits the check to those format ids."""
    formats = record["export"]["formats"]
    problems = [f"{name}: the record lists no such format" for name in only if name not in formats]
    for name in sorted(formats if not only else [n for n in only if n in formats]):
        path = directory / "format" / f"{name}.json"
        if not path.is_file():
            problems.append(f"{path}: missing")
            continue
        data = path.read_bytes()
        want = formats[name]
        if sha256(data) != want["sha256"] or len(data) != want["bytes"]:
            problems.append(f"{path}: differs from the record (sha256 {sha256(data)[:12]}, {len(data)} bytes; recorded {want['sha256'][:12]}, {want['bytes']} bytes)")
    return problems


def cmd_check_data(args: argparse.Namespace) -> int:
    record = load_provenance()
    if record is None:
        print(f"{PROVENANCE} is missing", file=sys.stderr)
        return 1
    problems = check_data(Path(args.directory), record, args.only or ())
    for line in problems:
        print(line)
    print("provenance data: ok" if not problems else f"provenance data: {len(problems)} problem(s)")
    return 1 if problems else 0


def cmd_provenance(args: argparse.Namespace) -> int:
    if args.action == "check-data":
        return cmd_check_data(args)
    sources = source_files()
    if sources["commit"] != pin():
        print(f"the Showdown checkout is at {sources['commit']}, the pin is {pin()}: the record is of the pin only", file=sys.stderr)
        return 1
    exports = {f: export_bytes(f) for f in FORMATS}
    previous = load_provenance()
    record = build_provenance(exports, sources, previous)
    if args.action == "write":
        PROVENANCE.write_text(render(record))
        print(
            f"wrote {PROVENANCE.relative_to(ENGINE_ROOT)}: pin {record['showdown']['commit'][:8]}, "
            f"{len(exports)} formats, {len(record['showdown']['sources'])} source files"
        )
        return 0
    if previous is None:
        print(f"{PROVENANCE} is missing: run `provenance write`", file=sys.stderr)
        return 1
    problems = differences(previous, record)
    for line in problems:
        print(line)
    print(
        "provenance: ok"
        if not problems
        else f"provenance: {len(problems)} difference(s); after a deliberate pin or tool change run `provenance write` and read docs/repin.md"
    )
    return 1 if problems else 0


# What the files of a Showdown commit range say about the engine. A guide for the reviewer, not a
# verdict: the classes name where to look, and a handler change is never "data".
#
# The set of paths that matter is derived, and the classes are a reviewed table, so that nothing is
# decided by a path prefix:
#   - the mods of the bundled formats' inheritance chains (`chain_mods`, from the record, which
#     `refsim/source_files.js` makes by asking Showdown's own Dex), so a mod a format inherits is
#     found whatever its name, and one no bundled format inherits is `other`;
#   - the sim files, one entry per file below;
#   - the kinds of data file (`STEM_CLASS`), applied to the base data directory and to each chain mod.
# A path in one of those places that the tables do not know is `unreviewed`: a new sim file, a new
# kind of mod file. It is never quietly `other`, and `--strict` fails on it.
#
# `mechanics`: handler code and type-chart rules the engine ports; read the diff and port or confirm it.
# `data`: what an export carries (stats, learnsets, rules); `diff-exports` shows what moved.
# `other`: not the engine's concern (server, tests, text, sim files that run no battle).
SD = "data"  # Showdown's own data directory (not this checkout's: the tests refuse a quoted path of that name)
SIM_CLASS = {
    # The battle: turn loop, actions, queue, Pokémon, sides, field, the RNG and the state they keep.
    "battle.ts": "mechanics",
    "battle-actions.ts": "mechanics",
    "battle-queue.ts": "mechanics",
    "pokemon.ts": "mechanics",
    "side.ts": "mechanics",
    "field.ts": "mechanics",
    "prng.ts": "mechanics",
    "state.ts": "mechanics",
    # The Dex: type-chart rules (getImmunity, getEffectiveness), the effect classes that fill in a
    # move's, ability's, item's and condition's defaults, and the id normalisation.
    "dex.ts": "mechanics",
    "dex-data.ts": "mechanics",
    "dex-moves.ts": "mechanics",
    "dex-abilities.ts": "mechanics",
    "dex-items.ts": "mechanics",
    "dex-conditions.ts": "mechanics",
    # What an export carries: species, formats and the validator's legality rules, team packing.
    "dex-species.ts": "data",
    "dex-formats.ts": "data",
    "team-validator.ts": "data",
    "teams.ts": "data",
    # Streams, text and types: no battle rule.
    "battle-stream.ts": "other",
    "dex-text.ts": "other",
    "global-types.ts": "other",
    "index.ts": "other",
}
# The files of `lib/` the battle code calls (clampIntRange, deepClone, getString, ...); the rest of it
# (networking, process management, storage) is not the engine's concern.
LIB_CLASS = {"utils.ts": "mechanics"}
# A data file by its name, in the base data directory and in a mod: handlers are mechanics, tables are data.
STEM_CLASS = {
    "moves": "mechanics",
    "abilities": "mechanics",
    "items": "mechanics",
    "conditions": "mechanics",
    "scripts": "mechanics",
    "rulesets": "mechanics",
    "learnsets": "data",
    "pokedex": "data",
    "formats-data": "data",
    "natures": "data",
    "typechart": "data",
    "aliases": "data",
    "tags": "data",
    "pokemongo": "data",
}
FORMATS_FILE = "config/formats.ts"  # the format definitions: rules and mods
CLASSES = ("mechanics", "unreviewed", "data", "other")


def chain_mods(record: dict | None) -> set[str]:
    """The mods the bundled formats inherit from, `base` apart, as the provenance record names them."""
    if not record:
        return set()
    return {m for f in record["export"]["formats"].values() for m in f["mod_chain"] if m != "base"}


def classify(path: str, mods: set[str] | frozenset[str]) -> str:
    """'mechanics' (handler code: port or confirm), 'data' (an export carries it), 'other', or
    'unreviewed' (a place that matters, in a file the tables above do not know)."""
    parts = path.split("/")
    if not path.endswith(".ts"):
        return "other"
    if parts[0] == "sim" and len(parts) == 2:
        return SIM_CLASS.get(parts[1], "unreviewed")
    if parts[0] == "lib" and len(parts) == 2:
        return LIB_CLASS.get(parts[1], "other")
    if path == FORMATS_FILE:
        return "data"
    if parts[0] == SD and (len(parts) == 2 or (len(parts) == 4 and parts[1] == "mods" and parts[2] in mods)):
        return STEM_CLASS.get(parts[-1][: -len(".ts")], "unreviewed")
    return "other"


def git(checkout: Path, *args: str) -> str:
    return subprocess.run(["git", "-C", str(checkout), *args], check=True, capture_output=True, text=True).stdout


def blob_sha(checkout: Path, rev: str, path: str) -> str | None:
    """SHA-256 of `path` at `rev`, or None if it is not there."""
    done = subprocess.run(["git", "-C", str(checkout), "cat-file", "blob", f"{rev}:{path}"], capture_output=True)
    return sha256(done.stdout) if done.returncode == 0 else None


def cmd_diff_showdown(args: argparse.Namespace) -> int:
    checkout = Path(args.showdown) if args.showdown else SHOWDOWN_DIR
    record = load_provenance()
    mods = chain_mods(record) | set(args.mod or ())
    if args.candidate:
        # NEW is checked out and built in `checkout`: its own Dex says which mods the formats inherit there.
        found = source_files(env={**node_env(), "NINJASK_SHOWDOWN": str(checkout), "NINJASK_ALLOW_UNPINNED": "1"})
        mods |= {m for f in found["formats"].values() for m in f["mod_chain"] if m != "base"}

    problems = 0
    if record and record["showdown"]["commit"] == args.old:
        # The record's hashes are of the pin: git must agree, or the record was made from another tree.
        for path, recorded in sorted(record["showdown"]["sources"].items()):
            if blob_sha(checkout, args.old, path) != recorded:
                print(f"provenance: the recorded hash of {path} is not the one git has at {args.old[:8]}")
                problems += 1

    commits = git(checkout, "log", "--reverse", "--format=%H %ad %s", "--date=short", f"{args.old}..{args.new}").splitlines()
    print(f"{len(commits)} commit(s) {args.old[:8]}..{args.new[:8]}")
    for line in commits:
        sha, _, rest = line.partition(" ")
        files = git(checkout, "show", "--format=", "--name-only", sha).split()
        by_class = {c: [f for f in files if classify(f, mods) == c] for c in CLASSES}
        print(f"\n{sha[:9]} {rest}")
        for c in ("mechanics", "unreviewed", "data"):
            for f in by_class[c]:
                print(f"    {c:10s} {f}")
        if by_class["other"]:
            print(f"    other      {len(by_class['other'])} file(s) (server, tests, text...)")

    # The files whose content differs between the two commits, by class.
    status = {"A": "added", "D": "removed"}
    changed: dict[str, list[tuple[str, str]]] = {c: [] for c in CLASSES}
    for line in git(checkout, "diff", "--no-renames", "--name-status", args.old, args.new).splitlines():
        letter, _, path = line.partition("\t")
        changed[classify(path, mods)].append((path, status.get(letter[0], "changed")))
    print("\nfiles that differ between the two commits:")
    for c in ("mechanics", "unreviewed", "data"):
        for path, how in sorted(changed[c]):
            print(f"  {c:10s} {how:8s} {path}")
    counts = ", ".join(f"{len(changed[c])} {c}" for c in ("mechanics", "data", "unreviewed", "other"))
    print(f"files that differ: {counts}")
    print("read every mechanics and unreviewed file's diff (`git diff OLD NEW -- PATH`): a handler change is ported,")
    print("never exported; then `diff-exports` shows what the data exports carry of the rest.")
    if args.strict and (changed["mechanics"] or changed["unreviewed"]):
        print("strict: a mechanics or unreviewed file differs")
        problems += 1
    return 1 if problems else 0


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


def main(argv: Sequence[str] | None = None) -> int:
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = p.add_subparsers(dest="cmd", required=True)
    pr = sub.add_parser("provenance", help="write or check refsim/PROVENANCE.json, or check a directory of exports against it")
    pr.add_argument("action", choices=("write", "check", "check-data"))
    pr.add_argument("directory", nargs="?", help="check-data: the directory that holds format/<id>.json")
    pr.add_argument("--only", action="append", metavar="FORMAT", help="check-data: only this format (repeatable)")
    pr.set_defaults(run=cmd_provenance)
    ds = sub.add_parser("diff-showdown", help="the commits and files between two Showdown commits, classified")
    ds.add_argument("old")
    ds.add_argument("new")
    ds.add_argument("--showdown", help="the Showdown checkout (default: the engine's)")
    ds.add_argument("--strict", action="store_true", help="exit 1 if a mechanics or unreviewed file differs")
    ds.add_argument("--mod", action="append", metavar="NAME", help="a mod to read as part of a format's chain (repeatable)")
    ds.add_argument(
        "--candidate",
        action="store_true",
        help="NEW is checked out and built in the Showdown checkout: take the mod chains from it as well",
    )
    ds.set_defaults(run=cmd_diff_showdown)
    de = sub.add_parser("diff-exports", help="what differs between two format exports")
    de.add_argument("old")
    de.add_argument("new")
    de.set_defaults(run=cmd_diff_exports)
    args = p.parse_args(argv)
    if args.cmd == "provenance" and (args.action == "check-data") != (args.directory is not None):
        p.error("`provenance check-data` takes the directory to check, and no other action does")
    return args.run(args)


if __name__ == "__main__":
    sys.exit(main())
