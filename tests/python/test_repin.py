"""scripts/repin.py: what ties the engine to a Showdown commit, and how a diff of two commits is read.

The classification of a Showdown path is derived, not guessed from a path prefix: the mods a format
inherits (recorded in refsim/PROVENANCE.json by `refsim/source_files.js`, which asks Showdown's own Dex)
give the data directories that matter, a reviewed table classifies the sim files and the kinds of data
file, and anything the table does not know is "unreviewed", never quietly "other" (docs/repin.md).
"""

from __future__ import annotations

import importlib.util
import json
import os
import subprocess
import sys
from pathlib import Path

import pytest

ENGINE_ROOT = Path(__file__).resolve().parents[2]
SCRIPT = ENGINE_ROOT / "scripts" / "repin.py"
PROVENANCE = json.loads((ENGINE_ROOT / "refsim" / "PROVENANCE.json").read_text())


def _repin():
    spec = importlib.util.spec_from_file_location("repin_under_test", SCRIPT)
    assert spec and spec.loader
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


repin = _repin()
MODS = {"champions", "championsregmb"}
SD = repin.SD  # Showdown's own data directory (this checkout's tests may not quote a path of that name)


def test_repin_does_not_need_the_compiled_package():
    """The pin tools read files and run git and node: they work on a checkout whose engine is not built
    (a repin starts there), and the engine's package is not imported to read them."""
    env = {"PATH": os.environ["PATH"], "HOME": os.environ.get("HOME", "/")}
    done = subprocess.run([sys.executable, str(SCRIPT), "--help"], env=env, capture_output=True, text=True)
    assert done.returncode == 0, done.stderr
    assert "diff-showdown" in done.stdout


# classify


@pytest.mark.parametrize(
    ("path", "want"),
    [
        # Mod files: the mods of a chain, every file by what it is.
        (f"{SD}/mods/champions/moves.ts", "mechanics"),
        (f"{SD}/mods/champions/scripts.ts", "mechanics"),
        (f"{SD}/mods/championsregmb/scripts.ts", "mechanics"),  # a path prefix of the other mod missed this one
        (f"{SD}/mods/championsregmb/moves.ts", "mechanics"),
        (f"{SD}/mods/championsregmb/learnsets.ts", "data"),
        (f"{SD}/mods/champions/formats-data.ts", "data"),
        (f"{SD}/mods/champions/rulesets.ts", "mechanics"),
        (f"{SD}/mods/champions/conditions.ts", "mechanics"),
        (f"{SD}/mods/champions/typechart.ts", "data"),
        (f"{SD}/mods/champions/somethingnew.ts", "unreviewed"),
        (f"{SD}/mods/gen7letsgo/scripts.ts", "other"),  # a mod no bundled format inherits
        (f"{SD}/mods/championsregmb/README.md", "other"),
        # The base data directory.
        (f"{SD}/moves.ts", "mechanics"),
        (f"{SD}/abilities.ts", "mechanics"),
        (f"{SD}/scripts.ts", "mechanics"),
        (f"{SD}/learnsets.ts", "data"),
        (f"{SD}/pokedex.ts", "data"),
        (f"{SD}/typechart.ts", "data"),
        (f"{SD}/brandnew.ts", "unreviewed"),
        (f"{SD}/text/moves.ts", "other"),
        (f"{SD}/random-battles/gen9/sets.json", "other"),
        ("config/formats.ts", "data"),
        # The sim files, one by one: type-chart code (dex.ts, dex-data.ts) is mechanics.
        ("sim/battle.ts", "mechanics"),
        ("sim/battle-actions.ts", "mechanics"),
        ("sim/battle-queue.ts", "mechanics"),
        ("sim/prng.ts", "mechanics"),
        ("sim/dex.ts", "mechanics"),
        ("sim/dex-data.ts", "mechanics"),
        ("sim/dex-moves.ts", "mechanics"),
        ("sim/battle-stream.ts", "other"),  # a prefix "sim/battle" called it mechanics
        ("sim/team-validator.ts", "data"),
        ("sim/dex-species.ts", "data"),
        ("sim/dex-text.ts", "other"),
        ("sim/brand-new-file.ts", "unreviewed"),  # a new sim file is a prompt to read it, not "other"
        ("sim/README.md", "other"),
        ("sim/tools/anything.ts", "other"),
        ("lib/utils.ts", "mechanics"),
        ("lib/net.ts", "other"),
        ("server/chat-plugins/teams.ts", "other"),
        ("test/sim/champions.js", "other"),
    ],
)
def test_classify_sorts_showdown_paths(path, want):
    assert repin.classify(path, MODS) == want


def test_a_mod_outside_the_chains_is_other_until_it_is_one():
    assert repin.classify(f"{SD}/mods/championsregmd/moves.ts", MODS) == "other"
    assert repin.classify(f"{SD}/mods/championsregmd/moves.ts", MODS | {"championsregmd"}) == "mechanics"


def test_every_recorded_source_is_classified_by_the_reviewed_tables():
    mods = repin.chain_mods(PROVENANCE)
    assert mods >= {"champions", "championsregmb"}
    classes = {path: repin.classify(path, mods) for path in PROVENANCE["showdown"]["sources"]}
    assert not [p for p, c in classes.items() if c == "unreviewed"]
    assert {"mechanics", "data", "other"} <= set(classes.values())


# the record


def test_the_record_holds_a_hash_of_every_sim_file_and_every_mod_file_the_formats_load():
    sources = PROVENANCE["showdown"]["sources"]
    assert all(len(h) == 64 for h in sources.values())
    assert {"sim/battle.ts", "sim/dex.ts", "sim/dex-data.ts", f"{SD}/mods/champions/scripts.ts"} <= set(sources)
    assert f"{SD}/mods/championsregmb/scripts.ts" in sources
    for fid, record in PROVENANCE["export"]["formats"].items():
        chain = record["mod_chain"]
        assert chain[-1] == "base", fid
        for mod in chain[:-1]:
            assert any(p.startswith(f"{SD}/mods/{mod}/") for p in sources), (fid, mod)
    assert PROVENANCE["export"]["formats"]["gen9championsvgc2026regmb"]["mod_chain"] == ["championsregmb", "champions", "base"]


def test_the_tool_files_include_the_source_lister():
    assert "refsim/source_files.js" in PROVENANCE["export"]["tool_files"]
    assert sorted(PROVENANCE["export"]["tool_files"]) == sorted(repin.TOOL_FILES)


def test_a_changed_source_file_is_a_difference():
    doctored = json.loads(json.dumps(PROVENANCE))
    doctored["showdown"]["sources"]["sim/battle.ts"] = "0" * 64
    assert repin.differences(PROVENANCE, doctored) == [
        f"/showdown/sources/sim/battle.ts: recorded {PROVENANCE['showdown']['sources']['sim/battle.ts']!r}, found {'0' * 64!r}"
    ]
    removed = json.loads(json.dumps(PROVENANCE))
    del removed["showdown"]["sources"]["sim/side.ts"]
    assert any("sim/side.ts" in line for line in repin.differences(PROVENANCE, removed))


# diff-showdown on a small repository of its own


def git(repo: Path, *args: str) -> str:
    env = {
        "PATH": os.environ["PATH"],
        "HOME": str(repo),
        "GIT_AUTHOR_NAME": "t",
        "GIT_AUTHOR_EMAIL": "t@example.com",
        "GIT_COMMITTER_NAME": "t",
        "GIT_COMMITTER_EMAIL": "t@example.com",
    }
    return subprocess.run(["git", "-C", str(repo), *args], check=True, capture_output=True, text=True, env=env).stdout.strip()


def commit(repo: Path, message: str, files: dict[str, str | None]) -> str:
    for path, text in files.items():
        target = repo / path
        if text is None:
            target.unlink()
            continue
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(text)
    git(repo, "add", "-A")
    git(repo, "commit", "-q", "-m", message)
    return git(repo, "rev-parse", "HEAD")


@pytest.fixture
def showdown(tmp_path):
    repo = tmp_path / "showdown"
    repo.mkdir()
    git(repo, "init", "-q")
    old = commit(
        repo,
        "old",
        {
            "sim/battle.ts": "a",
            "sim/battle-stream.ts": "a",
            "sim/team-validator.ts": "a",
            f"{SD}/mods/champions/moves.ts": "a",
            f"{SD}/mods/championsregmb/scripts.ts": "a",
            f"{SD}/mods/championsregmb/learnsets.ts": "a",
            f"{SD}/mods/gen8/moves.ts": "a",
            f"{SD}/learnsets.ts": "a",
            f"{SD}/gone.ts": "a",
            "README.md": "a",
        },
    )
    return repo, old


def run_diff(repo: Path, old: str, new: str, capsys, *extra: str) -> tuple[int, str]:
    code = repin.main(["diff-showdown", old, new, "--showdown", str(repo), *extra])
    return code, capsys.readouterr().out


def test_diff_showdown_names_each_changed_file_by_class(showdown, capsys):
    repo, old = showdown
    new = commit(
        repo,
        "new",
        {
            "sim/battle.ts": "b",
            "sim/battle-stream.ts": "b",
            "sim/team-validator.ts": "b",
            "sim/added-file.ts": "b",
            f"{SD}/mods/championsregmb/scripts.ts": "b",
            f"{SD}/mods/championsregmb/learnsets.ts": "b",
            f"{SD}/mods/gen8/moves.ts": "b",
            f"{SD}/gone.ts": None,
            "README.md": "b",
        },
    )
    code, out = run_diff(repo, old, new, capsys)
    assert code == 0, out
    lines = {line.split()[-1]: line.split() for line in out.splitlines() if line.startswith("  ") and not line.startswith("    ")}
    assert lines["sim/battle.ts"][:2] == ["mechanics", "changed"]
    assert lines[f"{SD}/mods/championsregmb/scripts.ts"][:2] == ["mechanics", "changed"]
    assert lines["sim/added-file.ts"][:2] == ["unreviewed", "added"]
    assert lines[f"{SD}/mods/championsregmb/learnsets.ts"][:2] == ["data", "changed"]
    assert lines["sim/team-validator.ts"][:2] == ["data", "changed"]
    assert lines[f"{SD}/gone.ts"][:2] == ["unreviewed", "removed"]
    # Other files (a mod no format inherits, the stream, the readme) are counted, not listed.
    assert f"{SD}/mods/gen8/moves.ts" not in lines and "README.md" not in lines and "sim/battle-stream.ts" not in lines
    assert "files that differ: 2 mechanics, 2 data, 2 unreviewed, 3 other" in out


def test_diff_showdown_strict_fails_on_mechanics_and_on_unreviewed_files(showdown, capsys):
    repo, old = showdown
    only_data = commit(repo, "data", {f"{SD}/learnsets.ts": "b", "README.md": "b"})
    assert run_diff(repo, old, only_data, capsys, "--strict")[0] == 0
    mechanics = commit(repo, "mechanics", {f"{SD}/mods/champions/moves.ts": "b"})
    code, out = run_diff(repo, only_data, mechanics, capsys, "--strict")
    assert code == 1 and "mechanics" in out
    assert run_diff(repo, only_data, mechanics, capsys)[0] == 0  # without --strict it is a report
    unreviewed = commit(repo, "new file", {"sim/surprise.ts": "x"})
    assert run_diff(repo, mechanics, unreviewed, capsys, "--strict")[0] == 1


def test_diff_showdown_reads_a_mod_it_is_told_about(showdown, capsys):
    repo, old = showdown
    new = commit(repo, "new", {f"{SD}/mods/championsregmd/moves.ts": "b"})
    assert "championsregmd" not in run_diff(repo, old, new, capsys)[1].split("files that differ")[0]
    code, out = run_diff(repo, old, new, capsys, "--mod", "championsregmd", "--strict")
    assert code == 1 and f"{SD}/mods/championsregmd/moves.ts" in out


def test_diff_showdown_checks_the_recorded_hashes_against_git_at_the_pin(showdown, capsys, monkeypatch):
    """When OLD is the recorded pin, the hashes the record holds must be the ones git has: a record
    edited by hand, or made from a modified checkout, is caught here."""
    repo, old = showdown
    record = json.loads(json.dumps(PROVENANCE))
    record["showdown"]["commit"] = old
    record["showdown"]["sources"] = {"sim/battle.ts": repin.sha256(b"a"), "sim/team-validator.ts": repin.sha256(b"WRONG")}
    monkeypatch.setattr(repin, "load_provenance", lambda: record)
    new = commit(repo, "new", {"README.md": "b"})
    code, out = run_diff(repo, old, new, capsys)
    assert code == 1
    said = [line for line in out.splitlines() if line.startswith("provenance:")]
    assert any("sim/team-validator.ts" in line for line in said), out
    assert not any("sim/battle.ts" in line for line in said), out


def test_diff_showdown_lists_commits_with_their_files(showdown, capsys):
    repo, old = showdown
    new = commit(repo, "touch the battle", {"sim/battle.ts": "b", "README.md": "b"})
    out = run_diff(repo, old, new, capsys)[1]
    assert "1 commit(s)" in out and "touch the battle" in out
    assert "mechanics sim/battle.ts" in " ".join(out.split())
