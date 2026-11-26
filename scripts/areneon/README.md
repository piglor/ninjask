# Scripts that tie this engine to Areneon's harness

How the evidence of `docs/n1-seeds-channels-candidates.md` is made and re-made: recording Showdown, replaying it into the
engine, comparing the candidate lists with Areneon's reference enumerator, and the audits. Nothing here is part of the
engine or of a policy's input; it is tooling for the checks (recorded Showdown battles are evaluation data).

**What you need.** The pinned Showdown, built (`NINJASK_SHOWDOWN=/path/to/pokemon-showdown`, default
`third_party/pokemon-showdown`; `node build` inside it), a wheel or `maturin develop` build of this engine in a Python 3.12
environment, and for the Areneon side a checkout of the Areneon monorepo (`AREONEON_ROOT=/path/to/areneon`) at the commit
named in `tests/fixtures/areneon/SOURCE.json` or later, with its Showdown submodule built and Node 24 (the TypeScript is
run by type stripping, no build step). Never point any of it at private teams: the recordings use the synthetic dev and
training splits of a dataset made with a public key.

The format export every check loads is `tests/fixtures/areneon/dex-gen9championsvgc2026regmc-7244fe8e.json`, made with
`node refsim/export_format.js gen9championsvgc2026regmc` on that checkout (the exporter refuses another commit, see
`docs/repin.md`).

## The comparisons

| Script | What it does | What it proves |
|---|---|---|
| `record_battles.ts` | Plays seeded battles with Areneon's runner on the pinned engine (`--mix random-legal`, `pending-first` or `uniform-all`, both seats) and writes, per battle, the seed, the teams (sets and packed), every submitted choice (refusals included), each player's requests with the reference enumerator's candidates, each player's channel and the omniscient log. The 3,200-battle corpus is 500 + 500 + 750 + 750 `random-legal`, 250 + 250 `uniform-all`, 250 + 250 `pending-first` and 200 `uniform-all` on teams with Illusion. | The recordings are Showdown's own streams, as the harness records them. |
| `record_draws.cjs` | Replays a recording's choices on the pinned Showdown and writes every call on its random source per write (`random`, `randomChance`, `sample`, `shuffle`, with arguments and result), the way the harness's battle trace does. | The oracle for the draw log and the forced mode. |
| `../check_recorded.py` | Replays each recorded battle into the engine through `Batch` and `Battle` and compares: both channels and the omniscient log line for line, every request's `request_hash`, the candidates at every request against the recorded reference lists (order and `pending` included), the engine's draws against Showdown's call for call, and a forced replay on the fast generator. | The channels, hashes, candidates and draws on the whole corpus (`python scripts/check_recorded.py --dex EXPORT --draws D1,D2,... R1 R2 ...`). |
| `dump_requests.py` + `compare_candidates.mjs` | The engine plays fresh battles with random candidates (pending ones included) and dumps every request point of both players with its candidates; the Node side runs Areneon's `enumerateCandidates` on the same request text and compares the list, its order and every flag. | The port equals the reference on requests nobody recorded (`python dump_requests.py POOL N | node compare_candidates.mjs`). |
| `../../tests/python/test_n1_fresh.py` | Fresh battles judged by the engine: a candidate that is not pending is never refused; a pending one is accepted or refused as unavailable, never invalid; every syntactic choice string of the phase is submitted to a copy of the battle (brute force) and each one the engine accepts is among the candidates. `NINJASK_N1_BATTLES=3600` is the evidence size. | Nothing the engine accepts is missing from the list, nothing it refuses is in it unless pending. |
| `scenario.cjs`, `make_scenarios.py` | Hand-built battles recorded from Showdown (`tests/fixtures/areneon/scenarios.jsonl.gz`, one per hint of the audit, with controls): the choices, the omniscient log, both channels and the draws. `scenario.cjs` also replays any recording, which is how the recorder is checked (its channels, hashes and draws equal the harness's on the same choices). | The hints, the turn-overflow hints, and the draws' framing in situations a random battle rarely reaches. |
| `fuzz_replay.py` | Fresh battles played by the engine (`mirror`: both sides the same team, so every speed ties; `cross`: two teams of the pool) with random non-pending candidates, then replayed on the pinned Showdown with `scenario.cjs` and compared in every way `check_recorded.py` knows, forced replay included. `--games` replays given battle numbers, `--export` writes the games as scenario inputs, `--keep-failures` keeps the failing ones. | The engine's draws, channels and candidates on battles nobody recorded, including those whose ties are drawn (`python fuzz_replay.py --dex EXPORT --pool POOL --mode mirror --battles 6000 --base 300000`). |
| `explain_framing.py` | For a battle whose draws differ from Showdown's: the first differing draw, the handlers of the list Showdown sorted there (`SCENARIO_HANDLERS=1`) and the engine's call site. | Where a list is incomplete. |
| `sort_sequence.py` | Every sorted list of two or more of a battle on Showdown (`SCENARIO_SORTS=1`: its length, event, handlers with their order, priority, subOrder and speed) against the engine's own sort trace. The lists of Pokémon and actions (the same on both sides) anchor the sequences; between two anchors every handler list of Showdown with a possible tie (two entries of one order, priority and subOrder) must have an engine list of the same length. | The handler lists are complete wherever a tie is possible, tie drawn or not: a shuffle shows an omitted handler only when handlers tie, the length of the list shows it wherever the list is sorted. |
| `revival_probe.cjs` | What the pinned Showdown's `Side.choose` accepts from a side with two reviving slots, for the state play reaches and for the same state with a bench Pokémon made alive by hand. | The two-reviver candidates (`tests/fixtures/areneon/two-revivers.json`, `tests/python/test_n1_revival.py`). |
| `extract_rows.py`, `build_fixture.py`, `pack_cases.cjs` | Make the committed fixtures from a recording: the 49 battles of `battles.jsonl.gz` (with their draws), the two battles of `gaps.jsonl.gz`, the packed teams of the golden choice cases. | How each fixture was made. |

## The audits

- `handler_audit.py EXPORT.json ModifyMove Update ...`: the abilities, items, move conditions and conditions of the format
  that Showdown's `runEvent` collects for an event. The data the engine's sorted lists are held against;
  `--json EXPORT EVENT...` writes it by kind and role, which is `tests/fixtures/areneon/handler-sets.json`
  (`tests/python/test_n1_handlers.py` holds the file to the export and its events to `src/dex/events.rs`).
- `sort_sites.py --dex EXPORT --src . RECORDING...`: which of the engine's hand-built sorted lists the corpus exercises and
  which of them it saw draw between ties (the engine's own sort trace, `Battle.set_rng_trace`).
- `memory_per_battle.py`: the memory a battle costs with channels off, unread and discarded.

## Keeping the port tied to its reference

`write_source_manifest.py AREONEON_CHECKOUT [REF]` writes `tests/fixtures/areneon/SOURCE.json`: the Areneon commit, the
SHA-256 of `choices.ts` (the enumerator the port follows), of the engine-request schema and of every file of the golden
choice corpus. The header of `src/candidates.rs` names the same hashes. `tests/python/test_n1_source.py` holds all three
equal and, with `AREONEON_ROOT` set, holds the checkout equal to the manifest: when Areneon's enumerator changes the test
fails, and the comparisons above have to be run again before the manifest is rewritten.

## Re-making the evidence

```sh
# the 3,200 recorded battles (needs the recordings made by record_battles.ts and record_draws.cjs)
python scripts/check_recorded.py --dex tests/fixtures/areneon/dex-gen9championsvgc2026regmc-7244fe8e.json \
    --draws rl-500-0.draws.jsonl.gz,... rl-500-0.jsonl.gz ...
# fresh requests against the TypeScript reference
python scripts/areneon/dump_requests.py pool-packed.txt 3600 0 \
  | AREONEON_ROOT=/path/to/areneon node scripts/areneon/compare_candidates.mjs
# fresh battles, the engine as the judge
NINJASK_N1_BATTLES=3600 python -m pytest tests/python/test_n1_fresh.py -q
# the hand-built scenarios and the two recorded gap battles
python -m pytest tests/python/test_n1_scenarios.py -q
python scripts/check_recorded.py --dex ... --draws tests/fixtures/areneon/gaps-draws.jsonl.gz tests/fixtures/areneon/gaps.jsonl.gz
```
