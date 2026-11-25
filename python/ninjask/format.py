"""A format's rules and legal data, loaded from refsim/export_format.js output."""

from __future__ import annotations

import json
import math
import re
import subprocess
from dataclasses import dataclass, field
from functools import cached_property, lru_cache
from pathlib import Path

from ninjask._extension import module as _compiled
from ninjask.ids import to_id
from ninjask.paths import FORMAT_DIR, IN_CHECKOUT, PACKAGE_DATA, REFSIM_DIR, node_env, require_checkout

CACHE = FORMAT_DIR
BUNDLED = PACKAGE_DATA / "formats"
STATS = ("hp", "atk", "def", "spa", "spd", "spe")

# Showdown's damageTaken codes: 0 neutral, 1 weak (2x), 2 resists (0.5x), 3 immune.
_TAKEN = {0: 1.0, 1: 2.0, 2: 0.5, 3: 0.0}


@dataclass
class FormatSpec:
    raw: dict
    species: dict[str, dict] = field(init=False)
    moves: dict[str, dict] = field(init=False)
    items: dict[str, dict] = field(init=False)
    abilities: dict[str, dict] = field(init=False)

    def __post_init__(self) -> None:
        self.species = {s["id"]: s for s in self.raw["species"]}
        self.moves = {m["id"]: m for m in self.raw["moves"]}
        self.items = {i["id"]: i for i in self.raw["items"]}
        self.abilities = {a["id"]: a for a in self.raw["abilities"]}

    @property
    def id(self) -> str:
        return self.raw["format"]["id"]

    @property
    def game_type(self) -> str:
        return self.raw["format"]["gameType"]

    @property
    def open_team_sheets(self) -> bool:
        """Battles in this format are played with both team sheets shown:
        its rules force them open (`forceopenteamsheets`). The base Reg M-C
        format only offers open sheets (`openteamsheets`), which a battle
        accepts or declines case by case."""
        return any(r["rule"] == "forceopenteamsheets" for r in self.raw["format"]["rules"])

    @property
    def sim_format_id(self) -> str:
        """The format id the reference simulator creates battles and
        validations with. A derived format (see refsim/export_format.js)
        has no battle format of its own in Showdown, so its battles use
        the format it derives from, with the difference (open sheets)
        applied by the battle itself."""
        return self.raw["format"].get("derivedFrom") or self.id

    @property
    def picked_team_size(self) -> int | None:
        return self.raw["format"]["pickedTeamSize"]

    @property
    def stat_point_limit(self) -> int | None:
        return self.raw["format"]["evLimit"]

    @property
    def uses_stat_points(self) -> bool:
        return self.raw["format"]["mod"].startswith("champions")

    @cached_property
    def team_legal_species(self) -> set[str]:
        return {s["id"] for s in self.raw["species"] if not s["battleOnly"]}

    def learnset(self, species: str) -> set[str]:
        return set(self.raw["learnsets"].get(to_id(species), []))

    def effectiveness(self, attack_type: str, defender_types: list[str]) -> float:
        mult = 1.0
        for t in defender_types:
            mult *= _TAKEN[self.raw["typechart"][t].get(attack_type, 0)]
        return mult

    def stat(self, species: str, stat: str, points: int, nature: str) -> int:
        """A stat at level 50 under the Champions formula (Showdown's statModify).

        HP = base + points + 75; others = (base + points + 20) x nature.
        """
        if not self.uses_stat_points:
            raise NotImplementedError("only the Champions stat formula is implemented")
        base = self.species[to_id(species)]["baseStats"][stat]
        if stat == "hp":
            return base + points + 75
        value = base + points + 20
        n = self.raw["natures"].get(to_id(nature), {})
        if n.get("plus") == stat:
            value = math.floor(value * 110 / 100)
        elif n.get("minus") == stat:
            value = math.floor(value * 90 / 100)
        return value


class ExportCommitError(ValueError):
    """An export is not from the Showdown commit the engine is a port of (or names none)."""


def export_commit(path: Path) -> str | None:
    """The Showdown commit an export says it is from (`format.showdownCommit`, written
    near the start of the file), or None."""
    with path.open("rb") as f:
        head = f.read(16384).decode("utf-8", "replace")
    found = re.search(r'"showdownCommit":"([0-9a-f]{40})"', head)
    return found.group(1) if found else None


def _checked(path: Path, allow_unpinned: bool) -> Path:
    """`path` if its export is from the engine's pin (or `allow_unpinned`), else an `ExportCommitError`.
    The engine's own loader (`Dex.load`) holds the same rule: the Python side asks first so that a
    `FormatSpec` read from an export is held to it too."""
    if allow_unpinned:
        return path
    found, pin = export_commit(path), _compiled.SHOWDOWN_COMMIT
    if found == pin:
        return path
    what = f"is from Showdown commit {found}" if found else "names no Showdown commit (it is not an export of refsim/export_format.js)"
    raise ExportCommitError(
        f"the export {path} {what}, but this engine is a port of {pin}: export the format again from the pinned "
        "checkout (docs/repin.md), or pass allow_unpinned=True to take a hand-made export or one of another "
        "commit as it is"
    )


def export_path(
    format_id: str,
    refresh: bool = False,
    *,
    data_dir: str | Path | None = None,
    allow_unpinned: bool = False,
) -> Path:
    """The format's export, which must be from the engine's pin (`ninjask.SHOWDOWN_COMMIT`; an
    `ExportCommitError` otherwise, unless `allow_unpinned` takes the file as it is).

    By default: the one in the cache (`CACHE`, `$NINJASK_DATA/format/`; one from another commit is
    exported again from the checkout's Showdown), else the copy bundled with an installed wheel, else
    exported from Showdown into the cache now (or always, with `refresh`). With `data_dir`, a
    directory laid out like `$NINJASK_DATA` (`format/<id>.json` in it), only that directory is read:
    the way to load a format in an installed wheel, which has no Showdown to export from. A file in
    it from another commit is an error, never replaced (`refresh` exports it again, in a checkout)."""
    explicit = data_dir is not None
    cache = Path(data_dir) / "format" if explicit else CACHE
    path = cache / f"{format_id}.json"
    if not refresh and path.exists():
        found = export_commit(path)
        # In a checkout a cached export of another commit is stale: it is exported again below. One
        # that names no commit is not ours to replace: it is taken only with `allow_unpinned`.
        if explicit or not IN_CHECKOUT or found is None or found == _compiled.SHOWDOWN_COMMIT or allow_unpinned:
            return _checked(path, allow_unpinned)
    if not refresh and not explicit and (bundled := BUNDLED / f"{format_id}.json").exists():
        return _checked(bundled, allow_unpinned)
    if not IN_CHECKOUT:
        raise FileNotFoundError(
            f"no export of {format_id} at {path}, and exporting one needs Ninjask's source checkout "
            "(refsim/ and Showdown): export it there with `node refsim/export_format.js "
            f"{format_id}` into <dir>/format/{format_id}.json, then point NINJASK_DATA or data_dir= at <dir>"
        )
    require_checkout(f"exporting {format_id} from Showdown")
    data = subprocess.run(
        ["node", str(REFSIM_DIR / "export_format.js"), format_id],
        check=True,
        capture_output=True,
        text=True,
        env=node_env(),
    ).stdout
    # Checked before it is kept: a stale file left in the cache would be taken as the cache's.
    scratch = path.with_suffix(".json.part")
    path.parent.mkdir(parents=True, exist_ok=True)
    scratch.write_text(data)
    try:
        _checked(scratch, allow_unpinned).replace(path)
    except ExportCommitError as e:
        scratch.unlink(missing_ok=True)
        raise ExportCommitError(str(e).replace(str(scratch), str(path))) from None
    return path


def load(
    format_id: str,
    refresh: bool = False,
    *,
    data_dir: str | Path | None = None,
    allow_unpinned: bool = False,
) -> FormatSpec:
    """The format's rules and data (see `export_path` for where it is read from and which
    commit it must be from). Loaded once per export file and process:
    agents and game loops call this per game, and an export is ~700 KB of
    JSON. The spec is shared, so treat it as read-only; `refresh` re-exports
    and reloads."""
    if refresh:
        _parse.cache_clear()
    path = export_path(format_id, refresh, data_dir=data_dir, allow_unpinned=allow_unpinned)
    return _parse(str(path), path.stat().st_mtime_ns)


@lru_cache(maxsize=8)
def _parse(path: str, mtime_ns: int) -> FormatSpec:
    return FormatSpec(json.loads(Path(path).read_text()))
