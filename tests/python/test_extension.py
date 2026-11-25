"""Another module provides Ninjask's classes only when the process says so (`NINJASK_EXTENSION`).

The entry-point group `ninjask.extension` is how a project that compiles the engine into an
extension module of its own names it (README, "Embedding in another extension"). Any installed
distribution can declare the group, so declaring it must not be enough to replace the engine's
classes: that is the process's choice, made by name.
"""

from __future__ import annotations

import subprocess
import sys
from pathlib import Path

import pytest

import ninjask
import ninjask._extension as extension

FAKE_MODULE = """
import ninjask._core as core

class _Stand:
    pass

Dex, Battle, Tracker, Batch, Outcome, EnumerationMode, BattleStateKey = (
    type(name, (_Stand,), {}) for name in "Dex Battle Tracker Batch Outcome EnumerationMode BattleStateKey".split()
)
for _name in ("SHOWDOWN_COMMIT", "OBS_LEN", "IDS_LEN", "ACTION_FIELDS", "ACTION_WIDTH", "DEFAULT_MAX_PATHS"):
    globals()[_name] = getattr(core, _name)
"""


@pytest.fixture
def declared(tmp_path: Path) -> Path:
    """A directory with a module `fake_ninjask_ext` and a distribution declaring it as `fake`."""
    (tmp_path / "fake_ninjask_ext.py").write_text(FAKE_MODULE)
    info = tmp_path / "fake_ninjask_ext-1.0.dist-info"
    info.mkdir()
    (info / "METADATA").write_text("Metadata-Version: 2.1\nName: fake-ninjask-ext\nVersion: 1.0\n")
    (info / "entry_points.txt").write_text("[ninjask.extension]\nfake = fake_ninjask_ext\n")
    return tmp_path


def run(declared: Path, code: str, env: dict[str, str] | None = None, *flags: str):
    return subprocess.run(
        [sys.executable, *flags, "-c", f"import sys; sys.path.insert(0, {str(declared)!r})\n" + code],
        env={"PATH": "/usr/bin:/bin", "PYTHONPATH": str(Path(ninjask.__file__).parent.parent), **(env or {})},
        capture_output=True,
        text=True,
    )


WHICH = "import ninjask, ninjask._core; print('core' if ninjask.Battle is ninjask._core.Battle else ninjask.Battle.__module__)"


def test_a_declared_extension_is_not_used_by_default(declared):
    done = run(declared, WHICH)
    assert done.returncode == 0, done.stderr
    assert done.stdout.strip() == "core"


def test_a_declared_extension_that_is_ignored_is_said_so(declared):
    done = run(declared, WHICH, None, "-W", "default")
    assert done.stdout.strip() == "core"
    assert "fake = fake_ninjask_ext" in done.stderr and "NINJASK_EXTENSION" in done.stderr, done.stderr


def test_the_process_opts_in_by_name(declared):
    done = run(declared, WHICH, {"NINJASK_EXTENSION": "fake"})
    assert done.returncode == 0, done.stderr
    assert done.stdout.strip() == "fake_ninjask_ext"


def test_an_unknown_name_is_an_error_not_the_engine(declared):
    done = run(declared, WHICH, {"NINJASK_EXTENSION": "nope"})
    assert done.returncode != 0
    assert "nope" in done.stderr and "fake" in done.stderr, done.stderr


def test_an_empty_opt_in_is_none(declared):
    done = run(declared, WHICH, {"NINJASK_EXTENSION": ""})
    assert done.stdout.strip() == "core", done.stderr


def test_the_opt_in_with_no_declaration_is_an_error():
    with pytest.raises(ImportError, match="declares no module"):
        extension.load("fake", entry_points=[])


def test_load_takes_the_name_as_an_argument(declared, monkeypatch):
    monkeypatch.syspath_prepend(str(declared))
    assert extension.load().__name__ == "ninjask._core"
    assert extension.load("fake").__name__ == "fake_ninjask_ext"


def test_two_declarations_of_one_name_are_ambiguous():
    class Point:
        def __init__(self, name, value):
            self.name, self.value = name, value

        def load(self):  # pragma: no cover - never reached
            raise AssertionError

    with pytest.raises(ImportError, match="several"):
        extension.load("a", entry_points=[Point("a", "x"), Point("a", "y")])
