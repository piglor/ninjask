"""Which compiled module provides Ninjask's classes (`Dex`, `Battle`, `Tracker`).

Normally it is `ninjask._core`, built from this repository's crate. A project
can instead compile the engine into an extension module of its own, next to
classes of its own that take Ninjask's objects (README, "Embedding in another
extension"). It declares that module under the entry-point group
`ninjask.extension`, and a process that wants it names the entry point in the
environment variable `NINJASK_EXTENSION`: this package then uses that module's
classes, so that the process has one `Battle` class, not two incompatible ones.

The declaration alone is not enough, on purpose. Any installed distribution can
declare the group, and the module it names would replace the engine's classes in
every program that imports `ninjask`: running the engine is the process's choice,
made by name, and by default nothing but `ninjask._core` is imported.
"""

from __future__ import annotations

import os
import warnings
from collections.abc import Iterable
from importlib import import_module
from importlib.metadata import EntryPoint, entry_points
from types import ModuleType

GROUP = "ninjask.extension"
ENV = "NINJASK_EXTENSION"
CLASSES = ("Dex", "Battle", "Tracker", "Batch", "Outcome", "EnumerationMode", "BattleStateKey")


def load(extension: str | None = None, *, entry_points: Iterable[EntryPoint] | None = None) -> ModuleType:
    """The module that provides the classes: `ninjask._core`, or, when `extension` names an
    entry point of the group `ninjask.extension`, the module that entry point declares.
    (`entry_points` replaces the installed declarations, for tests.)"""
    declared = sorted(_declared() if entry_points is None else entry_points, key=lambda e: (e.name, e.value))
    if extension:
        points = [e for e in declared if e.name == extension]
        if len({e.value for e in points}) > 1:
            names = ", ".join(f"{e.name} = {e.value}" for e in points)
            raise ImportError(f"several modules are declared as {extension!r} ({GROUP}): {names}")
        if not points:
            known = ", ".join(e.name for e in declared) or "none"
            raise ImportError(
                f"{ENV}={extension!r}, but no installed distribution declares such a module under {GROUP} "
                f"(declared: {known}): it declares no module of that name"
            )
        try:
            module = points[0].load()
        except ImportError as e:
            raise ImportError(
                f"{points[0].value}, declared to provide Ninjask's classes ({GROUP}), "
                "does not import; rebuild the project that declares it"
            ) from e
    else:
        if declared:
            ignored = ", ".join(f"{e.name} = {e.value}" for e in declared)
            warnings.warn(
                f"{GROUP} declares {ignored}: ignored, Ninjask uses its own compiled module. "
                f"Set {ENV}=<name> to use a declared module (README, 'Embedding in another extension').",
                stacklevel=3,
            )
        try:
            module = import_module("ninjask._core")
        except ImportError as e:
            raise ImportError(
                "Ninjask's compiled module (ninjask._core) is not built: "
                "run `pip install -e .` (or `maturin develop --release`) in the Ninjask checkout"
            ) from e
    missing = [c for c in CLASSES if not hasattr(module, c)]
    if missing:
        raise ImportError(f"{module.__name__} lacks Ninjask's {', '.join(missing)}")
    return module


def _declared() -> list[EntryPoint]:
    return list(entry_points(group=GROUP))


module = load(os.environ.get(ENV) or None)
