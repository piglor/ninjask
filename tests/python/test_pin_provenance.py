"""The Showdown pin and the provenance of the data exported from it (docs/repin.md).

`refsim/SHOWDOWN_COMMIT` is the pin. `refsim/PROVENANCE.json` records the commit, the export
tool and the hash of every format the engine bundles, and `scripts/repin.py provenance check`
exports them again and compares. A pin bump that forgets any of it fails here.
"""

from __future__ import annotations

import importlib.util
import json
import re
from pathlib import Path

import pytest

ninjask = pytest.importorskip("ninjask")

from ninjask import format as formats  # noqa: E402
from ninjask.paths import ENGINE_ROOT, REFSIM_DIR, SHOWDOWN_DIR  # noqa: E402

needs_showdown = pytest.mark.skipif(
    not (SHOWDOWN_DIR / "dist").exists(),
    reason="Pokémon Showdown not built (scripts/setup_showdown.sh)",
)


def _repin():
    spec = importlib.util.spec_from_file_location("repin", ENGINE_ROOT / "scripts" / "repin.py")
    assert spec and spec.loader
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


repin = _repin()
SD = repin.SD  # Showdown's own data directory (this checkout's tests may not quote a path of that name)
PIN = (REFSIM_DIR / "SHOWDOWN_COMMIT").read_text().strip()
PROVENANCE = json.loads((REFSIM_DIR / "PROVENANCE.json").read_text())


def test_the_pin_is_a_full_commit_hash():
    assert re.fullmatch(r"[0-9a-f]{40}", PIN)
    assert (REFSIM_DIR / "SHOWDOWN_COMMIT").read_text() == PIN + "\n"


def test_the_pin_is_in_the_compiled_module_and_the_provenance_record():
    assert ninjask.SHOWDOWN_COMMIT == PIN
    assert PROVENANCE["schema"] == repin.SCHEMA
    assert PROVENANCE["showdown"]["commit"] == PIN
    assert PROVENANCE["showdown"]["repository"] == "https://github.com/smogon/pokemon-showdown"
    assert PIN not in [p["commit"] for p in PROVENANCE["showdown"]["previous_pins"]]
    # The pin this one replaced (the 0.4.1 engine's) is on record.
    assert "3a766f2cfb5e51c1e428cfe87d0dba91a8f01640" in [p["commit"] for p in PROVENANCE["showdown"]["previous_pins"]]


def test_the_notice_and_the_readme_name_the_pin():
    assert f"commit {PIN}" in (ENGINE_ROOT / "NOTICE").read_text()
    assert f"`{PIN[:7]}`" in (ENGINE_ROOT / "README.md").read_text()


def test_the_export_tool_is_the_one_recorded():
    recorded = PROVENANCE["export"]["tool_files"]
    assert sorted(recorded) == sorted(repin.TOOL_FILES)
    for path, digest in recorded.items():
        assert repin.sha256((ENGINE_ROOT / path).read_bytes()) == digest, (
            f"{path} changed: re-run `python scripts/repin.py provenance write` and read docs/repin.md"
        )


def test_the_recorded_formats_are_the_bundled_ones():
    script = (ENGINE_ROOT / "scripts" / "bundle_data.sh").read_text()
    bundled = re.search(r'^FORMATS="([^"]+)"', script, re.M)
    assert bundled
    assert sorted(bundled.group(1).split()) == sorted(repin.FORMATS) == sorted(PROVENANCE["export"]["formats"])
    for record in PROVENANCE["export"]["formats"].values():
        assert set(record["sections"]) == set(repin.SECTIONS)
        assert re.fullmatch(r"[0-9a-f]{64}", record["sha256"])


def test_handler_hashes_do_not_change_what_the_engine_supports(tmp_path):
    """`callbackHashes` is metadata for diffing exports: the engine reads the export as if it
    were not there (an unknown field would make a generic move "unsupported")."""
    export = formats.load("gen9championsvgc2026regmc").raw
    assert any(m.get("callbackHashes") for m in export["moves"])
    stripped = json.loads(json.dumps(export))
    for section in ("moves", "items", "abilities"):
        for entry in stripped[section]:
            entry.pop("callbackHashes", None)
    for entry in stripped["conditions"].values():
        entry.pop("callbackHashes", None)
    path = tmp_path / "stripped.json"
    path.write_text(json.dumps(stripped))
    with_hashes, without = ninjask.dex("gen9championsvgc2026regmc"), ninjask.Dex.load(str(path))
    for m in export["moves"]:
        assert with_hashes.move_support(m["id"]) == without.move_support(m["id"]), m["id"]
    assert with_hashes.supported_items() == without.supported_items()
    assert with_hashes.supported_abilities() == without.supported_abilities()
    assert len(with_hashes.generic_moves()) > 200


@needs_showdown
def test_the_exports_are_what_the_provenance_record_says(capsys):
    args = type("Args", (), {"action": "check"})()
    assert repin.cmd_provenance(args) == 0, capsys.readouterr().out


@needs_showdown
def test_an_export_of_another_commit_is_made_again(tmp_path, monkeypatch):
    other = "0" * 40
    stale = tmp_path / "gen9championsvgc2026regmc.json"
    stale.write_text(json.dumps({"format": {"showdownCommit": other}}, separators=(",", ":")))
    assert formats.export_commit(stale) == other
    monkeypatch.setattr(formats, "CACHE", tmp_path)
    path = formats.export_path("gen9championsvgc2026regmc")
    assert path == stale
    assert formats.export_commit(path) == PIN


@needs_showdown
def test_the_export_refuses_a_checkout_that_is_not_the_pin(tmp_path):
    """The export is only the pinned Showdown's data if it comes from the pin: the script
    reads the commit of the checkout itself, and refuses one that is not the pin file's."""
    import os
    import shutil
    import subprocess

    refsim = tmp_path / "refsim"
    refsim.mkdir()
    for name in ("export_format.js", "showdown_path.js"):
        shutil.copy(REFSIM_DIR / name, refsim / name)
    env = {"PATH": os.environ["PATH"], "NINJASK_SHOWDOWN": str(SHOWDOWN_DIR)}
    run = lambda: subprocess.run(  # noqa: E731
        ["node", str(refsim / "export_format.js"), "gen9championsvgc2026regmc"], env=env, capture_output=True, text=True
    )
    (refsim / "SHOWDOWN_COMMIT").write_text(PIN + "\n")
    ok = run()
    assert ok.returncode == 0, ok.stderr
    assert json.loads(ok.stdout)["format"]["showdownCommit"] == PIN
    (refsim / "SHOWDOWN_COMMIT").write_text("0" * 40 + "\n")
    bad = run()
    assert bad.returncode != 0
    assert f"the pin is {'0' * 40}" in bad.stderr, bad.stderr
    # Exporting a checkout that is not the pin is possible, and says which commit it is.
    env["NINJASK_ALLOW_UNPINNED"] = "1"
    unpinned = run()
    assert unpinned.returncode == 0, unpinned.stderr
    assert json.loads(unpinned.stdout)["format"]["showdownCommit"] == PIN  # the checkout's own commit


@needs_showdown
def test_the_source_lister_refuses_a_checkout_that_is_not_the_pin(tmp_path):
    """`refsim/source_files.js` makes the record of the Showdown sources: like the export, it is only
    the pin's if the checkout is the pin."""
    import os
    import shutil
    import subprocess

    refsim = tmp_path / "refsim"
    refsim.mkdir()
    for name in repin.TOOL_FILES:
        shutil.copy(ENGINE_ROOT / name, refsim / Path(name).name)
    env = {"PATH": os.environ["PATH"], "NINJASK_SHOWDOWN": str(SHOWDOWN_DIR)}
    run = lambda *ids: subprocess.run(  # noqa: E731
        ["node", str(refsim / "source_files.js"), *ids], env=env, capture_output=True, text=True
    )
    (refsim / "SHOWDOWN_COMMIT").write_text(PIN + "\n")
    ok = run("gen9championsvgc2026regmb")
    assert ok.returncode == 0, ok.stderr
    listed = json.loads(ok.stdout)
    assert listed["commit"] == PIN
    assert listed["formats"] == {"gen9championsvgc2026regmb": {"mod_chain": ["championsregmb", "champions", "base"]}}
    assert run().returncode == 2  # no format named
    (refsim / "SHOWDOWN_COMMIT").write_text("0" * 40 + "\n")
    bad = run("gen9championsvgc2026regmb")
    assert bad.returncode != 0 and f"the pin is {'0' * 40}" in bad.stderr, bad.stderr


@needs_showdown
def test_the_record_covers_every_sim_file_and_every_mod_file_of_a_chain():
    """The sources are found, not listed: every `sim/*.ts` of the checkout and every `.ts` file of a mod in a
    format's chain is in the record, with the hash of the file as it is."""
    sources = PROVENANCE["showdown"]["sources"]
    sim = sorted(f"sim/{p.name}" for p in (SHOWDOWN_DIR / "sim").glob("*.ts"))
    assert sim and set(sim) <= set(sources)
    for mod in repin.chain_mods(PROVENANCE):
        files = sorted(f"{SD}/mods/{mod}/{p.name}" for p in (SHOWDOWN_DIR / "data" / "mods" / mod).glob("*.ts"))
        assert files and set(files) <= set(sources), mod
    for path, digest in sources.items():
        assert repin.sha256((SHOWDOWN_DIR / path).read_bytes()) == digest, path
    # Every path in the record is a place the classification knows (no "unreviewed" file is recorded).
    mods = repin.chain_mods(PROVENANCE)
    assert [p for p in sources if repin.classify(p, mods) == "unreviewed"] == []


@needs_showdown
def test_the_node_tools_are_commonjs_inside_a_project_whose_package_is_a_module(tmp_path):
    """The engine is vendored into other repositories (a git submodule). Node reads a `.js` file by the nearest
    `package.json`, and a project that sets `"type": "module"` would make these scripts ES modules and break
    `require`: refsim/package.json says commonjs for the directory."""
    import os
    import shutil
    import subprocess

    project = tmp_path / "project"
    (project / "vendor" / "ninjask").mkdir(parents=True)
    (project / "package.json").write_text('{"name": "enclosing", "private": true, "type": "module"}')
    refsim = project / "vendor" / "ninjask" / "refsim"
    shutil.copytree(REFSIM_DIR, refsim, ignore=shutil.ignore_patterns("PROVENANCE.json", "node_modules"))
    env = {"PATH": os.environ["PATH"], "NINJASK_SHOWDOWN": str(SHOWDOWN_DIR)}
    run = lambda tool: subprocess.run(  # noqa: E731
        ["node", str(refsim / tool), "gen9championsvgc2026regmc"], env=env, capture_output=True, text=True, cwd=project
    )
    for tool in ("export_format.js", "source_files.js"):
        done = run(tool)
        assert done.returncode == 0, (tool, done.stderr[:300])
    assert json.loads((refsim / "package.json").read_text())["type"] == "commonjs"
    # Without it the enclosing project's type decides, and the tools fail: the file is what makes them work.
    (refsim / "package.json").unlink()
    assert "require is not defined" in run("export_format.js").stderr


def test_the_source_lister_names_no_mod_and_no_format():
    """Mod chains come from Showdown's Dex, not from this file."""
    text = (ENGINE_ROOT / "refsim" / "source_files.js").read_text()
    assert "champions" not in text and "gen9" not in text


# The helpers of scripts/repin.py, on small inputs.


def test_describe_hashes_the_export_and_each_section():
    export = {name: [] for name in repin.SECTIONS}
    export["moves"] = [{"id": "a"}, {"id": "b"}]
    data = json.dumps(export).encode()
    record = repin.describe(data)
    assert record["bytes"] == len(data)
    assert record["sha256"] == repin.sha256(data)
    assert record["sections"]["moves"] == {"count": 2, "sha256": repin.sha256(repin.canonical(export["moves"]))}
    assert record["sections"]["species"]["count"] == 0


def test_diff_exports_names_entities_and_changed_handlers():
    old = {
        "format": {"showdownCommit": "a"},
        "moves": [
            {"id": "x", "callbacks": ["onTry", "condition.onTrapPokemon"], "callbackHashes": {"onTry": "1"}, "pp": 5},
            {"id": "gone"},
        ],
        "items": [{"id": "i", "callbacks": [], "callbackHashes": {}}],
    }
    new = {
        "format": {"showdownCommit": "b"},
        "moves": [
            {"id": "x", "callbacks": ["onTry"], "callbackHashes": {"onTry": "2"}, "pp": 5},
            {"id": "fresh"},
        ],
        "items": [{"id": "i", "callbacks": [], "callbackHashes": {}}],
    }
    assert repin.diff_exports(old, new) == [
        "format: changed showdownCommit",
        "moves: removed gone",
        "moves: added fresh",
        "moves: changed x: callbackHashes; callbacks -['condition.onTrapPokemon'] +[]",
    ]
    assert repin.diff_exports(new, new) == []


def test_differences_lists_what_moved():
    want = {"a": {"b": 1, "c": 2}, "d": 3}
    assert repin.differences(want, want) == []
    assert repin.differences(want, {"a": {"b": 1, "c": 9}, "e": 4}) == [
        "/a/c: recorded 2, found 9",
        "/d: recorded 3, found '<none>'",
        "/e: recorded '<none>', found 4",
    ]
