"""Loading game data is pinned: an export is taken only from the Showdown commit the engine is a
port of (`ninjask.SHOWDOWN_COMMIT`), unless the caller says otherwise out loud (`allow_unpinned`).
The Rust side is `tests/pin_loader.rs`. Hermetic: the exports here are the committed fixture with a
commit written into its format block (an installed wheel has no Showdown)."""

from __future__ import annotations

import json
import shutil
import subprocess
import sys
from pathlib import Path

import pytest

import ninjask
import ninjask.format as fmt
from ninjask import Dex

PIN = ninjask.SHOWDOWN_COMMIT
PREVIOUS_PIN = "3a766f2cfb5e51c1e428cfe87d0dba91a8f01640"
FIXTURE = Path(__file__).parents[1] / "fixtures" / "enumeration.json"


def write_export(path: Path, commit: str | None = PIN) -> Path:
    """The fixture as an export that says it is from `commit` (none: it names no commit)."""
    spec = json.loads(FIXTURE.read_text())
    if commit is not None:
        spec["format"]["showdownCommit"] = commit
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(spec, separators=(",", ":")))
    return path


# Dex.load


def test_dex_load_takes_an_export_of_the_pin(tmp_path):
    assert Dex.load(str(write_export(tmp_path / "a.json"))).format_id == "enumerationtest"


def test_dex_load_refuses_an_export_of_another_commit(tmp_path):
    # The 3a766f2 export (the pin before the sync to 7244fe8e) under this engine.
    path = write_export(tmp_path / "old.json", PREVIOUS_PIN)
    with pytest.raises(ValueError, match=PREVIOUS_PIN) as e:
        Dex.load(str(path))
    assert PIN in str(e.value) and "allow_unpinned" in str(e.value)


def test_dex_load_refuses_an_export_that_names_no_commit(tmp_path):
    with pytest.raises(ValueError, match="no Showdown commit"):
        Dex.load(str(write_export(tmp_path / "hand-made.json", None)))


def test_dex_load_allow_unpinned_takes_any_export(tmp_path):
    for commit in (None, PREVIOUS_PIN, PIN):
        path = write_export(tmp_path / f"{commit}.json", commit)
        assert Dex.load(str(path), allow_unpinned=True).format_id == "enumerationtest"


def test_allow_unpinned_is_keyword_only(tmp_path):
    with pytest.raises(TypeError):
        Dex.load(str(write_export(tmp_path / "a.json")), True)  # type: ignore[call-arg]


# export_path and load: the files a format is read from


def test_a_cached_export_that_names_no_commit_is_refused(tmp_path, monkeypatch):
    monkeypatch.setattr(fmt, "CACHE", tmp_path)
    path = write_export(tmp_path / "x.json", None)
    with pytest.raises(fmt.ExportCommitError, match="no Showdown commit") as e:
        fmt.export_path("x")
    assert str(path) in str(e.value) and "allow_unpinned" in str(e.value)
    # The opt-in takes a hand-made export as it is.
    assert fmt.export_path("x", allow_unpinned=True) == path
    assert fmt.load("x", allow_unpinned=True).id == "enumerationtest"


def test_a_commitless_cache_is_not_overwritten_by_an_export(tmp_path, monkeypatch):
    """A hand-made file is the user's: it is refused, not replaced by an export of Showdown."""
    monkeypatch.setattr(fmt, "CACHE", tmp_path)
    monkeypatch.setattr(subprocess, "run", lambda *a, **k: pytest.fail("exported over a hand-made file"))
    path = write_export(tmp_path / "x.json", None)
    before = path.read_bytes()
    with pytest.raises(fmt.ExportCommitError):
        fmt.export_path("x")
    assert path.read_bytes() == before


def test_a_cached_export_of_another_commit_without_a_checkout_is_refused(tmp_path, monkeypatch):
    """Without Showdown to export from, a stale export cannot be made again: it is an error."""
    monkeypatch.setattr(fmt, "CACHE", tmp_path)
    monkeypatch.setattr(fmt, "IN_CHECKOUT", False)
    monkeypatch.setattr(fmt, "BUNDLED", tmp_path / "none")
    write_export(tmp_path / "x.json", PREVIOUS_PIN)
    with pytest.raises(fmt.ExportCommitError, match=PREVIOUS_PIN):
        fmt.export_path("x")


def test_a_bundled_export_of_another_commit_is_refused(tmp_path, monkeypatch):
    monkeypatch.setattr(fmt, "CACHE", tmp_path / "cache")
    monkeypatch.setattr(fmt, "BUNDLED", tmp_path / "bundled")
    write_export(tmp_path / "bundled" / "x.json", PREVIOUS_PIN)
    with pytest.raises(fmt.ExportCommitError, match=PREVIOUS_PIN):
        fmt.export_path("x")
    write_export(tmp_path / "bundled" / "x.json", PIN)
    assert fmt.export_path("x") == tmp_path / "bundled" / "x.json"


def test_an_export_made_now_is_checked_too(tmp_path, monkeypatch):
    """`export_format.js` records the checkout's own commit (with NINJASK_ALLOW_UNPINNED=1 in the
    environment, any checkout's): the file written is checked like any other."""
    monkeypatch.setattr(fmt, "CACHE", tmp_path)
    monkeypatch.setattr(fmt, "IN_CHECKOUT", True)
    monkeypatch.setattr(fmt, "BUNDLED", tmp_path / "none")
    monkeypatch.setattr(fmt, "require_checkout", lambda what: None)
    stale = write_export(tmp_path / "scratch.json", PREVIOUS_PIN).read_text()
    monkeypatch.setattr(subprocess, "run", lambda *a, **k: subprocess.CompletedProcess(a, 0, stdout=stale))
    with pytest.raises(fmt.ExportCommitError, match=PREVIOUS_PIN):
        fmt.export_path("fresh")
    assert not (tmp_path / "fresh.json").exists()  # a refused export is not left in the cache
    assert fmt.export_path("fresh", allow_unpinned=True) == tmp_path / "fresh.json"


def test_data_dir_is_where_the_format_is_read_from(tmp_path, monkeypatch):
    monkeypatch.setattr(fmt, "CACHE", tmp_path / "cache")  # not used
    data = tmp_path / "data"
    path = write_export(data / "format" / "x.json")
    assert fmt.export_path("x", data_dir=data) == path
    assert fmt.export_path("x", data_dir=str(data)) == path  # a string is a path too
    assert fmt.load("x", data_dir=data).id == "enumerationtest"
    assert ninjask.dex.__wrapped__("x", data_dir=data).format_id == "enumerationtest"


def test_data_dir_does_not_replace_a_stale_export(tmp_path, monkeypatch):
    """The caller named the directory: a file in it of another commit is an error, never silently
    exported again (that would write into a directory the caller owns)."""
    monkeypatch.setattr(subprocess, "run", lambda *a, **k: pytest.fail("exported into data_dir"))
    path = write_export(tmp_path / "format" / "x.json", PREVIOUS_PIN)
    for kwargs in ({}, {"allow_unpinned": False}):
        with pytest.raises(fmt.ExportCommitError, match=PREVIOUS_PIN):
            fmt.export_path("x", data_dir=tmp_path, **kwargs)
    assert fmt.export_path("x", data_dir=tmp_path, allow_unpinned=True) == path
    # Another commit's export under the engine: Dex.load refuses it too, whatever called it.
    with pytest.raises(ValueError, match=PREVIOUS_PIN):
        Dex.load(str(path))


def test_data_dir_without_the_export_says_where_it_looked(tmp_path, monkeypatch):
    monkeypatch.setattr(fmt, "IN_CHECKOUT", False)
    with pytest.raises(FileNotFoundError) as e:
        fmt.export_path("nope", data_dir=tmp_path)
    assert str(tmp_path / "format" / "nope.json") in str(e.value)
    assert "NINJASK_DATA" in str(e.value) and "export_format.js" in str(e.value)


# An installed wheel: the package without the checkout around it


def wheel_run(tmp_path: Path, code: str, env: dict[str, str] | None = None) -> subprocess.CompletedProcess[str]:
    """Run `code` against a copy of the package alone (no refsim/, no Showdown: `IN_CHECKOUT` is
    false), as a wheel installed beside other projects has it."""
    site = tmp_path / "site"
    package = site / "ninjask"
    if not package.exists():
        shutil.copytree(
            Path(ninjask.__file__).parent, package, ignore=shutil.ignore_patterns("__pycache__", "data", "*.pyc")
        )
    home = tmp_path / "home"
    home.mkdir(exist_ok=True)
    return subprocess.run(
        [sys.executable, "-c", f"import sys; sys.path.insert(0, {str(site)!r})\n" + code],
        cwd=home,
        env={"PATH": "/usr/bin:/bin", "HOME": str(home), **(env or {})},
        capture_output=True,
        text=True,
    )


WHEEL_LOAD = """
import ninjask, ninjask.paths as p
assert not p.IN_CHECKOUT, "this is not a wheel"
assert ninjask.__file__.startswith(sys.path[0]), ninjask.__file__
{call}
print(dex.format_id)
"""


def test_a_wheel_loads_a_format_from_data_dir(tmp_path):
    write_export(tmp_path / "data" / "format" / "enumerationtest.json")
    done = wheel_run(
        tmp_path, WHEEL_LOAD.format(call=f"dex = ninjask.dex('enumerationtest', data_dir={str(tmp_path / 'data')!r})")
    )
    assert done.returncode == 0, done.stderr
    assert done.stdout.strip() == "enumerationtest"


def test_a_wheel_loads_a_format_from_ninjask_data(tmp_path):
    write_export(tmp_path / "data" / "format" / "enumerationtest.json")
    done = wheel_run(
        tmp_path, WHEEL_LOAD.format(call="dex = ninjask.dex('enumerationtest')"), {"NINJASK_DATA": str(tmp_path / "data")}
    )
    assert done.returncode == 0, done.stderr
    assert done.stdout.strip() == "enumerationtest"


def test_a_wheel_refuses_the_export_of_another_commit(tmp_path):
    write_export(tmp_path / "data" / "format" / "enumerationtest.json", PREVIOUS_PIN)
    for call in (
        f"ninjask.dex('enumerationtest', data_dir={str(tmp_path / 'data')!r})",
        f"ninjask.load('enumerationtest', data_dir={str(tmp_path / 'data')!r})",
    ):
        done = wheel_run(tmp_path, WHEEL_LOAD.format(call=f"dex = {call}"))
        assert done.returncode != 0
        assert PREVIOUS_PIN in done.stderr and PIN in done.stderr, done.stderr


def test_a_wheel_without_data_says_how_to_give_it_some(tmp_path):
    done = wheel_run(tmp_path, WHEEL_LOAD.format(call="dex = ninjask.dex('enumerationtest')"))
    assert done.returncode != 0
    assert "NINJASK_DATA" in done.stderr and "data_dir" in done.stderr, done.stderr
