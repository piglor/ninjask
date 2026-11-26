"""The reference the candidate enumerator was ported from: its commit and hashes are written down
(tests/fixtures/areneon/SOURCE.json, src/candidates.rs), the committed copy of the golden choice corpus is the one
they name, and, when a checkout of Areneon is at hand (AREONEON_ROOT), the reference there is still the one ported.

A failure of the last test means Areneon's enumerator, its request schema or its golden corpus changed: run the
comparisons of docs/n1-seeds-channels-candidates.md again (scripts/areneon/README.md) and then rewrite the manifest
(scripts/areneon/write_source_manifest.py) and the header of src/candidates.rs.
"""

from __future__ import annotations

import hashlib
import json
import os
from pathlib import Path

import pytest

from n1_fixtures import FIXTURES

ROOT = Path(__file__).resolve().parents[2]
MANIFEST = json.loads((FIXTURES / "SOURCE.json").read_text())


def sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def test_the_committed_golden_corpus_is_the_one_the_manifest_names():
    named = MANIFEST["golden_choices"]["files"]
    committed = {p.name: sha(p.read_bytes()) for p in sorted((FIXTURES / "choices").glob("*.json"))}
    assert committed == named


def test_the_header_of_the_enumerator_names_what_the_manifest_does():
    header = (ROOT / "src" / "candidates.rs").read_text().split("use serde_json", 1)[0]
    for needed in (
        MANIFEST["areneon_commit"],
        MANIFEST["choices_ts"]["last_changed_in"],
        MANIFEST["choices_ts"]["sha256"],
        MANIFEST["engine_request_schema"]["sha256"],
    ):
        assert needed in header, needed
    assert f"({len(MANIFEST['golden_choices']['files'])} cases)" in header


def drift(root: Path, manifest: dict) -> list[str]:
    """What differs between an Areneon checkout and the manifest (empty when the reference is the one ported)."""
    out = []

    def digest(path: str) -> str | None:
        file = root / path
        return sha(file.read_bytes()) if file.exists() else None

    if digest(manifest["choices_ts"]["path"]) != manifest["choices_ts"]["sha256"]:
        out.append("choices.ts changed")
    if digest(manifest["engine_request_schema"]["path"]) != manifest["engine_request_schema"]["sha256"]:
        out.append("the engine-request schema changed")
    corpus = root / manifest["golden_choices"]["dir"]
    now = {p.name: sha(p.read_bytes()) for p in sorted(corpus.glob("*.json"))} if corpus.is_dir() else {}
    if now != manifest["golden_choices"]["files"]:
        out.append("the golden choice corpus changed")
    return out


def areneon_root() -> Path | None:
    root = os.environ.get("AREONEON_ROOT")
    return Path(root) if root else None


@pytest.mark.skipif(areneon_root() is None, reason="AREONEON_ROOT names no Areneon checkout")
def test_the_reference_in_the_areneon_checkout_is_the_one_that_was_ported():
    assert drift(areneon_root(), MANIFEST) == []


def test_the_drift_check_sees_a_changed_reference(tmp_path):
    """Negative control: a checkout whose enumerator, schema or corpus differs is reported, one that matches is not."""
    root = tmp_path
    (root / MANIFEST["choices_ts"]["path"]).parent.mkdir(parents=True)
    (root / MANIFEST["engine_request_schema"]["path"]).parent.mkdir(parents=True)
    corpus = root / MANIFEST["golden_choices"]["dir"]
    corpus.mkdir(parents=True)
    ts, schema = b"export function enumerateCandidates() {}", b"{}"
    for name in MANIFEST["golden_choices"]["files"]:
        (corpus / name).write_bytes((FIXTURES / "choices" / name).read_bytes())
    (root / MANIFEST["choices_ts"]["path"]).write_bytes(ts)
    (root / MANIFEST["engine_request_schema"]["path"]).write_bytes(schema)
    manifest = json.loads(json.dumps(MANIFEST))
    manifest["choices_ts"]["sha256"] = sha(ts)
    manifest["engine_request_schema"]["sha256"] = sha(schema)
    assert drift(root, manifest) == []
    (root / MANIFEST["choices_ts"]["path"]).write_bytes(ts + b"\n")
    assert drift(root, manifest) == ["choices.ts changed"]
    (root / MANIFEST["choices_ts"]["path"]).write_bytes(ts)
    first = next(iter(MANIFEST["golden_choices"]["files"]))
    (corpus / first).write_bytes(b"{}")
    assert drift(root, manifest) == ["the golden choice corpus changed"]
    (corpus / first).unlink()
    assert drift(root, manifest) == ["the golden choice corpus changed"]
