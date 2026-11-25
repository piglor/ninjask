"""Rules the CI workflows keep (docs/repin.md, "CI"): a push to main is never cancelled, no checkout
leaves credentials behind, and no cache holds code that the jobs run. Plain text scans: the workflows
use no feature that needs a YAML parser, and the tests need no extra dependency."""

from __future__ import annotations

import re

import pytest

from ninjask.paths import ENGINE_ROOT

pytestmark = pytest.mark.skipif(not (ENGINE_ROOT / ".github").is_dir(), reason="not a source checkout")

WORKFLOWS = sorted((ENGINE_ROOT / ".github" / "workflows").glob("*.yml"))


def test_there_are_workflows():
    assert {p.name for p in WORKFLOWS} >= {"ci.yml", "parity-at-scale.yml"}


@pytest.mark.parametrize("path", WORKFLOWS, ids=lambda p: p.name)
def test_every_checkout_drops_its_credentials(path):
    text = path.read_text()
    checkouts = [m.start() for m in re.finditer(r"uses:\s*actions/checkout@", text)]
    assert checkouts, "no checkout"
    for at in checkouts:
        step = text[at : text.find("\n      - ", at) if text.find("\n      - ", at) != -1 else len(text)]
        assert re.search(r"persist-credentials:\s*false", step), f"{path.name}: a checkout keeps its credentials:\n{step}"


@pytest.mark.parametrize("path", WORKFLOWS, ids=lambda p: p.name)
def test_every_action_is_pinned_to_a_commit(path):
    for use in re.findall(r"uses:\s*(\S+)", path.read_text()):
        assert re.fullmatch(r"[\w./-]+@[0-9a-f]{40}", use), f"{path.name}: {use} is not pinned to a commit"


@pytest.mark.parametrize("path", WORKFLOWS, ids=lambda p: p.name)
def test_no_cache_holds_the_showdown_checkout(path):
    """A cached checkout would run an earlier job's code (`node build`, the exporter) with this job's
    privileges and no check that it is the pin: Showdown is cloned and built on every run."""
    text = path.read_text()
    assert "actions/cache@" not in text, f"{path.name}: a cache step"
    assert "third_party" not in text.replace("third_party/pokemon-showdown ready", ""), f"{path.name} names third_party"


def test_a_push_to_main_is_not_cancelled():
    text = (ENGINE_ROOT / ".github" / "workflows" / "ci.yml").read_text()
    cancel = re.search(r"cancel-in-progress:\s*(.+)", text)
    assert cancel
    assert cancel.group(1).strip() != "true", "a newer push would cancel the run of an earlier commit on main"
    assert "pull_request" in cancel.group(1)


def test_the_showdown_setup_refuses_a_checkout_that_is_not_the_pin():
    script = (ENGINE_ROOT / "scripts" / "setup_showdown.sh").read_text()
    assert 'git rev-parse HEAD)" != "$COMMIT"' in script
    assert "git status --porcelain --untracked-files=no" in script
    assert script.index("git status --porcelain") < script.index("\nnode build\n")
