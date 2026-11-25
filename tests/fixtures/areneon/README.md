# Fixtures recorded from the pinned Pokémon Showdown (Areneon's pin)

Test data for the batch seeds, the per-viewer channels, the candidate lists and
the draw log (`tests/python/test_n1_*.py`, `tests/n1_*.rs`). All of it is
synthetic: the teams are generated, the battles were played by random policies,
and nothing in it comes from a real player. The recordings, teams and choice
cases are the work of the Areneon project (the choice cases are Areneon's
golden corpus, which is CC0). The format export,
`dex-gen9championsvgc2026regmc-7244fe8e.json`, is not CC0: it is data exported
from Pokémon Showdown (MIT, Copyright (c) 2011-2026 Guangcong Luo and other
contributors), under the notice in the repository's `NOTICE`.

The oracle is Pokémon Showdown at commit `7244fe8e28fa8bc4d0809ae30d183c608b62bb2b`
(format `gen9championsvgc2026regmc`), driven through its `BattleStream`, which is
what Areneon's training harness records.

| File | What it is |
|---|---|
| `dex-gen9championsvgc2026regmc-7244fe8e.json` | The format export of that pin, made with `refsim/export_format.js` (the data of the engine, plus `format.banner`, the lines Showdown writes around the start of a battle). |
| `battles.jsonl.gz` | 49 recorded battles, one JSON object per line: `seed`, the two packed teams, every submitted choice in order (`refused` is Showdown's error text for a refused choice), each player's channel as Showdown's stream delivered it (lines, `|request|` lines included, wall-clock `|t:|` lines dropped), the omniscient log, each player's requests (`request_seq`, `phase`, `request_hash`, and `legal`: the candidates the harness's enumerator lists, `choices` and `pending`, in its order), and `draws`: every call Showdown's battle made on its random source in each write, as `{method, args, result}` strings. The set covers refused choices, Mega Evolution, wait requests, forced switches, hidden traps and disables, Illusion, and each `|-hint|` Showdown prints that the engine's log needs to carry. |
| `choices/*.json` | The golden choice corpus of Areneon (one situation each: Team Preview, targets, Mega, forced switches, Revival Blessing, trapping, a locked move): the seed, the teams, the choices that reach it, the player, the request's hash and every candidate with the engine's verdict, in the harness's canonical order. |
| `choices-packed.json` | The teams of those cases in Showdown's packed form, made with Showdown's own `Teams.pack`. |
| `teams.txt` | The packed teams of `battles.jsonl.gz`, one per line, for tests that play fresh battles. |
| `scenarios.jsonl.gz`, `scenarios-draws.jsonl.gz` | 25 hand-built battles recorded from the pinned Showdown, one for each `|-hint|` line it prints in this format that a battle can reach (and controls that print none): the rows have the format of `battles.jsonl.gz` and the draws are per write. `scripts/areneon/make_scenarios.py` makes them. |
| `gaps.jsonl.gz`, `gaps-draws.jsonl.gz` | Two recorded battles (`ua-500-a:152`, `illusion-200:149`) in which the engine's draws used to differ from Showdown's in the framing of a shuffle; regression tests now. |
| `SOURCE.json` | The Areneon commit and the SHA-256 of the reference enumerator, its request schema and the golden choice corpus the port follows (`tests/python/test_n1_source.py`). |

The battles were chosen from a corpus of 3,200 recorded with Areneon's runner
(random-legal, uniform-over-all-candidates and pending-first policies on
generated teams): all of the 10 with a refused choice, a few of each situation
named above, and the shortest of the plain ones. Battles with No Retreat and
Revival Blessing were left out when the engine's pin was behind the one these
were recorded at (the pin sync, #1531, closed that); the whole corpus of 3,200
agrees with the engine now (`docs/n1-seeds-channels-candidates.md`, "Evidence").
