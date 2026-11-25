# Fixtures recorded from the pinned Pokémon Showdown (Areneon's pin)

Test data for the batch seeds, the per-viewer channels, the candidate lists and
the draw log (`tests/python/test_n1_*.py`, `tests/n1_*.rs`). All of it is
synthetic and CC0: the teams are generated, the battles were played by random
policies, and nothing in it comes from a real player.

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

The battles were chosen from a corpus of 3,200 recorded with Areneon's runner
(random-legal, uniform-over-all-candidates and pending-first policies on
generated teams): all of the 10 with a refused choice, a few of each situation
named above, and the shortest of the plain ones. Battles with No Retreat and
Revival Blessing are left out: they hold the two known differences between the
engine's pin and this one (see `docs/n1-seeds-channels-candidates.md`).
