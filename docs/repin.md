# Moving the engine to a new Pokémon Showdown commit

The engine is a port of Pokémon Showdown at one commit, `refsim/SHOWDOWN_COMMIT` (the pin). Its game
data is exported from that checkout, and its mechanics are Rust ports of that checkout's handlers. A
Showdown change after the pin is a difference from Showdown until it is followed: **data changes are
free, handler changes are ported**. This is the procedure, run once per pin bump (a new regulation,
for instance Regulation M-D, is a pin bump plus a new format to export). Plan 3 to 8 days: the work is
reading the diff, one test and one port per behaviour change, and the comparison runs.

The rules that do not bend, here as in the Areneon project this fork serves:

- Showdown at the pin decides what is right. A test or a port is "correct" when it equals Showdown's
  output; the engine never decides legality, it asks the pinned Showdown (or the export of it).
- No game rule or id is written in the engine by hand: names, stats, rules and move data come from the
  export. A port reproduces a handler the export cannot carry, citing the Showdown function it follows.
- Tests first: the failing test (a scripted scenario against live Showdown, and the same hermetically
  with Showdown's lines) is committed before the port.
- Nothing from PyPI or crates.io outside `Cargo.lock`: build with `cargo ... --locked`, and `maturin`
  only in a scratch virtual environment. The `ninjask` on PyPI is upstream's, not this fork's.
- The Rust toolchain is pinned in `rust-toolchain.toml` (and named in CI): a newer clippy flags old
  code, so a toolchain bump is its own change.

## 1. Set up

```sh
# The new Showdown commit, in a checkout of its own (the engine's third_party/ one, or any directory
# NINJASK_SHOWDOWN names), and the old pin's checkout beside it to diff against.
export NINJASK_SHOWDOWN=/path/to/showdown-new
git -C "$NINJASK_SHOWDOWN" fetch origin && git -C "$NINJASK_SHOWDOWN" checkout --detach NEW_COMMIT
OLD=$(cat refsim/SHOWDOWN_COMMIT)
git clone --local /path/to/showdown-new /path/to/showdown-old && git -C /path/to/showdown-old checkout --detach $OLD
(cd /path/to/showdown-old && node build)    # `node build` runs npm ci when node_modules is missing: copy it
```

Check the machine before building (a shared machine: `nice -n 19`, `-j4`, one test process at a time).

## 2. Read the Showdown diff

```sh
python scripts/repin.py diff-showdown $OLD NEW_COMMIT --showdown "$NINJASK_SHOWDOWN" --candidate --strict
```

lists every commit and the files it touches, then every file whose content differs between the two
commits, sorted into **mechanics** (handler code and type-chart rules the engine ports), **data**
(rules, species, learnsets, validation: the exports carry them), **unreviewed** and **other** (server,
tests, text: no effect). `--strict` exits 1 if a mechanics or unreviewed file differs, so the step is
machine-checkable: it passes only when nothing the engine ports moved. A bump that moves a mechanics
file reads that file's diff (below) and ports or confirms it; the step is done when every one is
accounted for in the pull request. If `OLD` is the recorded pin, the command first checks the hash of
every recorded source file against git at `OLD` (`provenance: ...` lines, exit 1): a record made from a
tree that is not the pin's is caught before the diff is trusted.

**How a path is classified** (`classify` in `scripts/repin.py`; nothing is a path prefix):

- the set of mods that matter is derived: the inheritance chain of each bundled format, which
  `refsim/source_files.js` reads from Showdown's own Dex (`parentMod`, from each mod's `inherit`) and
  `refsim/PROVENANCE.json` records (`export.formats.<id>.mod_chain`), so `championsregmb` is found
  because the Reg M-B format inherits `champions`, not because its name starts with that of another
  mod; `--candidate` also reads the chains of the checkout at `NEW` (a new regulation's mod), `--mod
  NAME` adds one by hand; a mod that no bundled format inherits is **other**;
- every file of a chain mod and of the base data directory is classified by what it is: moves, abilities,
  items, conditions, scripts and rulesets are **mechanics** (they hold handlers); learnsets, pokedex,
  formats-data, natures, typechart, aliases, tags and pokemongo are **data**;
- every `sim/*.ts` is in a reviewed table with its own entry (`SIM_CLASS`): the battle files, `dex.ts` and
  `dex-data.ts` (type-chart rules, effect defaults) and `lib/utils.ts` (called by the battle code) are
  **mechanics**; the species, formats, team validator and teams files are **data**; streams, text and
  types are **other**;
- anything else in those places (a new sim file, a new kind of mod file) is **unreviewed**: read it, then add
  it to the table in the same pull request. It is never quietly **other**.

It is a guide to where to read, and the table is reviewed code. Then:

- read `git diff $OLD NEW_COMMIT -- PATH` for **every** mechanics path, including the ones that look like
  data (`data/items.ts`, `sim/dex-items.ts`: a Fling rule moved in one pin bump) and the shared sim files
  (`sim/battle.ts`, `sim/side.ts` for choice rules);
- a commit titled as a mod change (`Champions: ...`) is a handler change until proven otherwise: the
  export lists a handler's **name**, not its body (Showdown d12b9c1e9 changed No Retreat's `onStart` and
  dropped `onTrapPokemon`: both are in the diff of the source, and only the removal in the names);
- write the classification of every commit in the pull request (the first one, 3a766f2 to 7244fe8, is
  the CHANGELOG's "0.4.1+areneon.0": six commits, one handler change).

## 3. Export both pins and diff the data

```sh
for f in $(sed -n 's/^FORMATS="\(.*\)"/\1/p' scripts/bundle_data.sh); do
  NINJASK_ALLOW_UNPINNED=1 NINJASK_SHOWDOWN=/path/to/showdown-old node refsim/export_format.js $f > /tmp/old-$f.json
  NINJASK_ALLOW_UNPINNED=1 NINJASK_SHOWDOWN=/path/to/showdown-new node refsim/export_format.js $f > /tmp/new-$f.json
  python scripts/repin.py diff-exports /tmp/old-$f.json /tmp/new-$f.json
done
```

(`NINJASK_ALLOW_UNPINNED=1` exports a checkout that is not the pin and records the commit it really is.
Without it `export_format.js` refuses a checkout that is not at the pin, or has a tracked file changed.)

Expected classes of difference:

| Line | Meaning | Action |
| --- | --- | --- |
| `format: changed showdownCommit` | always | none |
| `species/moves/items/abilities: added/removed ID` | roster, legality or data change | none for the export; read why (a new item that no handler names is free); a new entity the engine does not simulate shows in `Dex.team_problems` and the support report |
| `...: changed ID: FIELD` | data field changed (power, accuracy, flags, stats, learnset) | none: the engine reads it; a change to a field the engine's code assumes (a `volatileStatus`, a `target`) is read like a handler change |
| `...: changed ID: callbackHashes; callbacks -[..] +[..]` | **a handler's source text moved, or a handler was added or removed** | find the Showdown commit, read the diff, **port or confirm it is cosmetic** (a comment, a rename). `callbackHashes` is a SHA-256 prefix of every handler's source: a body that changes under an unchanged name shows here |
| `format: changed rules/valueRules/...` | a rule changed or a regulation was added | the rules are data; check `pickedTeamSize`, `adjustLevel`, `evLimit` readers if one moved |

`callbackHashes` sees entity handlers only. A change to shared code (`sim/*.ts`, `data/scripts.ts`, a
mod's `scripts.ts`, `rulesets.ts`) is in the **mechanics** list of step 2 and nowhere in the exports.

## 4. Move the pin, the tests first

1. For each behaviour change, write a failing test **before** the port: a scripted scenario played on
   live Showdown and the engine (`tests/python/test_pin_sync.py`: `check_reference.play` with
   `_ScriptedChoices`, compared after every input), the same scenario hermetically in Rust
   (`tests/pin_sync.rs`, with Showdown's lines), and, for a handler an ordinary game reaches, a
   `check_reference.py --synthetic --require ID` run (the divergent game numbers go in the test).
   Commit them red.
2. Edit the pin everywhere it is named: `refsim/SHOWDOWN_COMMIT`, `Cargo.toml`
   (`[package.metadata.areneon] showdown_commit` and the version, `0.4.1+areneon.N`), `NOTICE`, `README.md`
   (the short hash), and the CHANGELOG. `tests/pin.rs` and `tests/python/test_pin_provenance.py` fail on
   any that is left behind.
3. Delete `data/derived/format/` (a cached export of another commit is exported again, but be sure) and
   build the checkout at the new pin: `./scripts/setup_showdown.sh`.
4. Port, one behaviour change per commit, each citing the Showdown function it follows.
5. `python scripts/repin.py provenance write`, review the diff of `refsim/PROVENANCE.json` and commit it
   with the pin: the section hashes that moved are the entities of step 3, the `showdown.sources` hashes
   that moved are exactly the files `diff-showdown` listed in step 2 (a hash that moved for a file it did
   not list is a problem), and `previous_pins` gains the pin this one replaces.

## 5. Check against Showdown

All of these compare the engine with the pinned Showdown after every input; none may diverge. Run them
one at a time (each starts Node processes) and `nice -n 19`.

```sh
cargo fmt --check
cargo clippy --release --all-targets --locked -- -D warnings
cargo clippy --release --all-targets --locked --features python-module -- -D warnings
cargo test --release --locked
pytest -q
cargo run --release --locked --example fingerprint -- data/derived/format/gen9championsvgc2026regmc.json tests/fixtures/regmc_teams.txt 2000
python scripts/check_reference.py --synthetic --games 2000 --workers 4
python scripts/check_reference.py --pool pools/vgcpastes-regmc.txt --games 400 --workers 4
python scripts/check_reference.py --synthetic --games 1000 --workers 4 --require ID1,ID2     # each changed entity
for mix in chaos wide mixed; do for seed in 1 2; do
  python scripts/check_chaos.py --games 1500 --seed $seed --mix $mix --workers 4
done; done
python scripts/check_pairs.py --ids ID1,ID2,... --games-per-pair 12 --workers 4              # changed entities, and the ones they interact with
python scripts/check_choices.py --games 300 --workers 4
python scripts/check_tracker.py --games 100
python scripts/repin.py provenance check     # the exports, the export tool and every Showdown source file
```

`check_pairs.py` hangs, with idle workers and no message, when `--ids` names an id the engine does not
support (its team builder exits the worker process): list only supported ids, and kill a run that is quiet
for minutes.

The **fingerprint** (`c826ee9bb327b2d8` at 0.4.1+areneon.0) must not move in a change that preserves
behaviour. A pin bump that changes behaviour a fixture game reaches moves it: say in the commit which
changed handler moved it and that the reference check says Showdown does the same, and change it in the
CHANGELOG, the README, CONTRIBUTING, `.github/workflows/ci.yml` and the pull request template together.
(0.4.1+areneon.0 did not move it: no fixture team uses No Retreat or a forced Revival Blessing pass.)

A **new enumerator rule is a new test of everything after it.** A change to which choices the engine
offers (the Revival Blessing `pass, pass` of 0.4.1+areneon.0) makes the random players pick choices they
never picked, and every later state they reach is then compared with Showdown for the first time: the
chaos checks found the slot condition that had to end in the residual. Run them with two seeds.

## 6. The comparison on a large corpus (Areneon's gate)

The Areneon project compares the engine with the pinned Showdown on thousands of battles recorded from
its own harness (`runBattle` on `vendor/pokemon-showdown`, its generator's teams, a run key, the choices
and the requests, both players' logs): every request, every choice's acceptance, the legal sets, both
players' logs, the winner and the turn count, with `Battle.reference` seeded by the recording's seed.
The first one (run key `ninjask-eval-1`, 3,200 battles, 120,695 requests) at 0.4.1+areneon.0: **3,200 of
3,200 exact** after the ten differences of upstream 0.4.1 (nine No Retreat, one Revival Blessing legal
set) were fixed. The scripts are the project's (`record_battles.ts`, `compare_replay.py`,
`summarize.py` in the evaluation artifacts; the differential package takes them over): record against
the new pin with the new run key, replay into the fork's `Battle.reference` with
`compare_replay.py --rec FILE.jsonl.gz --dex data/derived/format/gen9championsvgc2026regmc.json --out
RESULT.json`, read `summarize.py`. Zero unclassified differences; Team Preview's 90-order legal set,
`|gen|`/`|rule|` banner lines and wait requests are the known interface differences.

## 7. The fork's CI and the Areneon side

`.github/workflows/ci.yml` runs the checks of step 5 at a small size, and `fidelity` plays the
reference comparisons at about 8,100 games (about 12 minutes). `parity-at-scale.yml` runs a million.
Every job clones and builds Showdown at the pin (nothing cached from an earlier run: a cached checkout
would be code executed on trust), its checkouts keep no credentials, and a push to `main` is never
cancelled.
After the pull request is merged to the fork's `main`, the Areneon repository bumps its `vendor/ninjask`
submodule to that commit; its pin test requires `vendor/ninjask/refsim/SHOWDOWN_COMMIT` to equal its own
`vendor/pokemon-showdown` gitlink and the compiled `ninjask.SHOWDOWN_COMMIT` to equal both.

## What 0.4.1+areneon.0 found, for the next bump

- A handler can change with its name unchanged: hashes of handler sources (`callbackHashes`) are in the
  export for this reason; at 3a766f2 to 7244fe8 they differ for `noretreat` (the move and its condition) and
  nothing else, over every move, item, ability and condition of the four bundled formats.
- The export used to record `showdownCommit` from the pin file, not from the checkout: an export of the
  wrong commit said it was the right one. It now reads the checkout's commit and refuses a checkout that
  is not the pin, or has tracked files changed.
- A cached export was used whatever commit it came from. `ninjask.format.export_path` and the Rust tests
  now check it against the pin; the loader itself (`Dex::from_json`, `Dex.load`) refuses an export of
  another commit or one that names none (`allow_unpinned` / `Dex::from_json_unpinned` override it), so an
  old export cannot be played under a new engine whatever called the loader.
- Python (`ninjask.choices`) and Rust (`Battle::joint_choices`) enumerate choices by the same rule and
  must be changed together; the tests of both live in `test_choices.py` and `tests/pin_sync.rs`.
