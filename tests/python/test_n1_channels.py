"""Per-viewer channels: the stream Showdown writes to each player, line for line, from a batched or a single battle.

The oracle is the pinned Showdown's own streams (tests/fixtures/areneon/battles.jsonl.gz): its two player channels
(`|request|` lines, wait requests and `|error|` lines in the places Showdown sends them, no `rqid`: the in-process
simulator has none) and its omniscient log. A channel is `ninjask.Batch.channel(i, viewer, start)`
or `ninjask.Battle.channel(viewer, start)`; the identity of a request is `request_hash`."""

from __future__ import annotations

import copy
import hashlib
import json

import pytest

import ninjask

from n1_fixtures import NAMES, battles, dex

VIEWERS = ("p1", "p2", "omniscient")


def jcs(value) -> str:
    return json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=False)


def hash_of(request_line: str) -> str:
    request = json.loads(request_line[len("|request|"):])
    request.pop("rqid", None)
    return hashlib.sha256(jcs(request).encode()).hexdigest()


def play_batch(row, channels=True):
    batch = ninjask.Batch(dex(), 1, names=NAMES, channels=channels)
    batch.reset(0, row["packed"]["p1"], row["packed"]["p2"], row["seed"])
    return batch


def replay_batch(row, check=None):
    """Submit the recorded choices in order; `check(batch, flags, choice)` after each."""
    batch = play_batch(row)
    for c in row["choices"]:
        side = NAMES.index(c["player"])
        flags = batch.choose([(0, side, c["choice"])])
        refused = bool(flags[0] & 1)
        assert refused == bool(c.get("refused")), (row["id"], c)
        assert not flags[0] & 4, (row["id"], c)  # an invalid choice is not a refusal
        if check is not None:
            check(batch, flags, c)
    return batch


def test_the_channels_are_showdowns_streams_line_for_line():
    for row in battles():
        batch = replay_batch(row)
        for viewer in ("p1", "p2"):
            assert batch.channel(0, viewer) == row["channels"][viewer], (row["id"], viewer)
        assert batch.channel(0, "omniscient") == row["omniscient"], row["id"]
        assert batch.winner(0) == row["winner"]


def test_the_same_channels_from_a_single_battle():
    for row in battles()[:10]:
        battle = ninjask.Battle.reference(dex(), row["packed"]["p1"], row["packed"]["p2"], row["seed"], names=NAMES, channels=True)
        for c in row["choices"]:
            try:
                battle.choose(c["player"], c["choice"])
            except ValueError as error:
                assert c.get("refused"), (row["id"], error)
        for viewer in VIEWERS:
            expected = row["omniscient"] if viewer == "omniscient" else row["channels"][viewer]
            assert battle.channel(viewer) == expected, (row["id"], viewer)
            assert battle.channel_len(viewer) == len(expected)


def test_a_channel_is_read_from_any_line_on():
    row = next(r for r in battles() if any(c.get("refused") for c in r["choices"]))
    batch = replay_batch(row)
    whole = batch.channel(0, "p1")
    for start in (0, 1, 7, len(whole) // 2, len(whole) - 1, len(whole), len(whole) + 5):
        assert batch.channel(0, "p1", start) == whole[start:]
    assert batch.channel_len(0, "p1") == len(whole)


def test_requests_arrive_in_the_stream_with_the_hash_the_harness_gives_them():
    for row in battles():
        seen = {"p1": 0, "p2": 0}
        seq = {"p1": 0, "p2": 0}

        def drain(batch, row=row, seen=seen, seq=seq):
            for s, viewer in enumerate(NAMES):
                new = batch.channel(0, viewer, seen[viewer])
                seen[viewer] += len(new)
                requests = [line for line in new if line.startswith("|request|")]
                for line in requests:
                    expected = row["requests"][viewer][seq[viewer]]
                    assert hash_of(line) == expected["request_hash"], (row["id"], viewer, seq[viewer])
                    seq[viewer] += 1
                if requests:
                    assert batch.request_hash(0, s) == hash_of(requests[-1])

        batch = play_batch(row)
        drain(batch)
        for c in row["choices"]:
            batch.choose([(0, NAMES.index(c["player"]), c["choice"])])
            drain(batch)
        assert seq == {s: len(row["requests"][s]) for s in NAMES}, row["id"]
        # The game is over: no request is current.
        assert batch.request_hash(0, 0) is None and batch.request_hash(0, 1) is None


def test_a_player_with_nothing_to_choose_is_sent_a_wait_request():
    waits = 0
    for row in battles():
        batch = replay_batch(row)
        for viewer in ("p1", "p2"):
            for line in batch.channel(0, viewer):
                if line.startswith('|request|{"wait":true'):
                    waits += 1
                    assert json.loads(line[len("|request|"):]).keys() == {"wait", "side"}
    assert waits > 40


def test_a_refused_choice_is_an_error_line_and_the_request_again_with_update():
    refusals = 0
    for row in battles():
        if not any(c.get("refused") for c in row["choices"]):
            continue
        batch = replay_batch(row)
        for viewer in ("p1", "p2"):
            channel = batch.channel(0, viewer)
            for i, line in enumerate(channel):
                if line.startswith("|error|"):
                    refusals += 1
                    assert line.startswith("|error|[Unavailable choice] ")
                    assert channel[i + 1].startswith("|request|")
                    assert json.loads(channel[i + 1][len("|request|"):]).get("update") is True
    assert refusals >= 10


def test_an_invalid_choice_is_an_error_line_only_and_the_game_goes_on():
    row = battles()[0]
    batch = play_batch(row)
    before = batch.channel_len(0, "p1")
    flags = batch.choose([(0, 0, "foo")])
    assert flags[0] & 4 and not flags[0] & 1
    assert batch.channel(0, "p1", before) == ["|error|[Invalid choice] Unrecognized choice: foo"]
    assert batch.channel_len(0, "p2") == len(row["channels"]["p2"][: batch.channel_len(0, "p2")])
    # The player may choose again.
    flags = batch.choose([(0, 0, row["choices"][0]["choice"])])
    assert not flags[0] & 5


def test_the_start_has_showdowns_banner_and_the_end_its_blank_line():
    row = battles()[0]
    batch = play_batch(row)
    lines = batch.channel(0, "p1")
    kinds = [line.split("|")[1] for line in lines]
    assert kinds[:4] == ["gametype", "player", "player", "gen"]
    assert "rule" in kinds and kinds.count("uhtml") == 1
    assert lines[-1].startswith("|request|")
    batch = replay_batch(row)
    assert batch.channel(0, "p2")[-2:][0] == "|" and batch.channel(0, "p2")[-1].startswith("|win|")
    assert [l for l in batch.channel(0, "omniscient") if l.startswith("|uhtml|")].__len__() == 2


def test_the_request_is_written_in_showdowns_key_order_and_without_rqid():
    row = battles()[0]
    batch = play_batch(row)
    line = batch.channel(0, "p1")[-1]
    assert line == row["channels"]["p1"][len(batch.channel(0, "p1")) - 1]
    assert line.startswith('|request|{"teamPreview":true,"maxChosenTeamSize":4,"side":{')
    assert '"rqid"' not in line


def test_a_channel_needs_the_banner_of_the_export():
    export = json.loads((ninjask_fixtures() / "dex-gen9championsvgc2026regmc-7244fe8e.json").read_text())
    del export["format"]["banner"]
    import tempfile

    with tempfile.NamedTemporaryFile("w", suffix=".json") as f:
        json.dump(export, f)
        f.flush()
        bare = ninjask.Dex.load(f.name)
    row = battles()[0]
    with pytest.raises(ValueError, match="banner"):
        ninjask.Batch(bare, 1, names=NAMES, channels=True)
    with pytest.raises(ValueError, match="banner"):
        ninjask.Battle.reference(bare, row["packed"]["p1"], row["packed"]["p2"], row["seed"], names=NAMES, channels=True)


def ninjask_fixtures():
    from n1_fixtures import FIXTURES

    return FIXTURES


def test_a_battle_copy_keeps_its_own_channels():
    row = battles()[1]
    battle = ninjask.Battle.reference(dex(), row["packed"]["p1"], row["packed"]["p2"], row["seed"], names=NAMES, channels=True)
    first = row["choices"][:2]
    for c in first:
        battle.choose(c["player"], c["choice"])
    twin = battle.copy()
    for c in row["choices"][2:6]:
        try:
            battle.choose(c["player"], c["choice"])
        except ValueError:
            pass
    assert twin.channel_len("p1") < battle.channel_len("p1")
    for c in row["choices"][2:6]:
        try:
            twin.choose(c["player"], c["choice"])
        except ValueError:
            pass
    assert twin.channel("p1") == battle.channel("p1")


def test_a_batch_without_channels_has_none():
    row = battles()[0]
    batch = play_batch(row, channels=False)
    with pytest.raises(ValueError, match="channels"):
        batch.channel(0, "p1")


def test_channels_do_not_change_the_game():
    row = battles()[2]
    with_channels = replay_batch(row)
    plain = ninjask.Batch(dex(), 1, names=NAMES)
    plain.reset(0, row["packed"]["p1"], row["packed"]["p2"], row["seed"])
    for c in row["choices"]:
        plain.choose([(0, NAMES.index(c["player"]), c["choice"])])
    assert plain.battle(0).log("omniscient", 0) == with_channels.battle(0).log("omniscient", 0)
    assert plain.battle(0).rng_seed == with_channels.battle(0).rng_seed


def test_a_channel_forgets_what_the_reader_has_taken():
    row = battles()[3]
    batch = replay_batch(row)
    whole = batch.channel(0, "p1")
    middle = len(whole) // 2
    batch.discard_channel(0, "p1", middle)
    assert batch.channel_len(0, "p1") == len(whole)  # line numbers do not change
    assert batch.channel(0, "p1", len(whole) - 5) == whole[-5:]
    assert batch.channel(0, "p1", middle + 40)[:3] == whole[middle + 40: middle + 43]
    with pytest.raises(ValueError, match="discarded"):
        batch.channel(0, "p1", 0)
    # the other channels are untouched, and a discard past the end empties nothing that is to come
    assert batch.channel(0, "p2") == row["channels"]["p2"]
    batch.discard_channel(0, "p1", len(whole))
    assert batch.channel(0, "p1", len(whole)) == []
    battle = ninjask.Battle.reference(dex(), row["packed"]["p1"], row["packed"]["p2"], row["seed"], names=NAMES, channels=True)
    battle.discard_channel("omniscient", 10)
    with pytest.raises(ValueError):
        battle.channel("omniscient", 0)
    plain = ninjask.Batch(dex(), 1, names=NAMES)
    plain.reset(0, row["packed"]["p1"], row["packed"]["p2"], row["seed"])
    with pytest.raises(ValueError, match="channels"):
        plain.discard_channel(0, "p1", 1)
