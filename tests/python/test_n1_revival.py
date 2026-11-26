"""Two Revival Blessing slots at once: the candidates the engine lists are what the pinned Showdown accepts.

`tests/fixtures/areneon/two-revivers.json` holds the input of `scripts/areneon/revival_probe.cjs` and its output: the
request of a side with two reviving slots and every combination `Side.choose` accepted. The first state is reached by
play (N0's `REVIVERS_1`/`REVIVERS_2`: nobody lives on the bench, nine combinations, `pass, pass` among them); the second
is the same state with one fainted bench Pokémon made alive by hand (a state no battle reaches: the first reviver may
pass only while no Pokémon lives on the bench, and nothing changes the bench before the second reviver's request),
where Showdown's two counters leave one forced pass: three combinations, never `pass, pass`.

The Rust unit test `two_reviving_slots_equal_what_showdown_accepts` holds the enumerator to the same fixture; this file
holds the fixture to the live Showdown (so a pin bump that changes the rule fails here first) and the engine's battle to
the fixture.
"""

from __future__ import annotations

import json
import subprocess

import pytest

ninjask = pytest.importorskip("ninjask")

from ninjask import dex  # noqa: E402
from ninjask.paths import ENGINE_ROOT, SHOWDOWN_DIR  # noqa: E402

FIXTURE = ENGINE_ROOT / "tests" / "fixtures" / "areneon" / "two-revivers.json"
PROBE = ENGINE_ROOT / "scripts" / "areneon" / "revival_probe.cjs"
needs_showdown = pytest.mark.skipif(
    not (SHOWDOWN_DIR / "dist").exists(),
    reason="Pokémon Showdown not built (scripts/setup_showdown.sh)",
)


def _fixture():
    return json.loads(FIXTURE.read_text())


@needs_showdown
def test_the_fixture_is_what_the_pinned_showdown_accepts(tmp_path):
    fixture = _fixture()
    spec = tmp_path / "in.json"
    out = tmp_path / "out.json"
    spec.write_text(json.dumps(fixture["input"]))
    done = subprocess.run(["node", str(PROBE), str(spec), str(out)], capture_output=True, text=True, check=False)
    assert done.returncode == 0, done.stderr
    assert json.loads(out.read_text()) == fixture["output"]


def test_the_engine_lists_the_combinations_showdown_accepted_in_the_state_play_reaches():
    fixture = _fixture()
    spec = fixture["input"]
    battle = ninjask.Battle.reference(dex("gen9championsvgc2026regmc"), spec["packed"]["p1"], spec["packed"]["p2"], spec["seed"])
    for step in spec["choices"]:
        battle.choose(step["player"], step["choice"])
    listed = [choice for choice, _ in battle.candidates("p1")]
    assert listed == fixture["output"][0]["accepted"]
    assert len(listed) == 9
    assert "pass, pass" in listed
    assert set(battle.legal_choices("p1")) == set(listed)


def test_the_hand_made_state_cannot_be_reached_by_play():
    """Showdown's `Side.choose` accepts three combinations when one Pokémon lives on the bench; the fixture's note
    on why the engine's battle never offers that request is the argument above, so only the numbers are pinned."""
    state = _fixture()["output"][1]
    assert state["bench_alive"] == [3]
    assert state["accepted"] == ["switch 3, switch 3", "switch 3, pass", "pass, switch 3"]
