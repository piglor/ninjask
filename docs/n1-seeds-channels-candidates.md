# Showdown seeds, player channels, candidates and draw logs for batched battles

What a training or differential harness needs from the engine beyond the
numeric observation of `Batch.observe()`: games that are *Showdown's* games
(same seed, same draws), each player's *stream* as Showdown sends it, the
*candidates* a player may submit in the order of a fixed reference
enumerator, and the *draws* of a battle, loggable and forceable. All of it is
exact against Pokémon Showdown at the pin the format export was made from,
checked on recorded Showdown battles
(`tests/fixtures/areneon/README.md`, `scripts/check_recorded.py`).

The Python API is in `python/ninjask/_core.pyi`; the Rust API in
`ninjask::{batch, channel, candidates, identity, rng}`.

## 1. Showdown seeds in `Batch`

`Batch.reset(i, team1, team2, seed)` and `reset_many` take the seed as an
integer (the fast xoshiro256++ generator, unchanged) or as a Showdown seed,
`"sodium,<hex>"` (Showdown's own `SodiumRNG`: ChaCha20 keyed by the previous
draw, as in `Battle.reference`). A battle seeded the second way draws the numbers
a Showdown battle with that seed draws, in the same order, so given the same
choices it is the same game: `tests/python/test_n1_seeds.py` plays batched
games and `Battle.reference` twins choice for choice and requires equal logs,
winner, turns and final generator state; `scripts/check_recorded.py` plays
Showdown's own recorded games.

`Batch(dex, n, names=("p1", "p2"))` names the players (the `|player|` lines and
the request's `side.name` carry the names; the default is `Player 1`,
`Player 2` as before).

Which seed a training run uses is the caller's: a game that must be a function
of its plan entry (so a Showdown path and this engine play the same game) uses
the Showdown seed; throughput runs that need no such equality can keep the
integer seed. The cost of the Showdown generator is one ChaCha20 block per
draw; see "Cost" below.

## 2. Channels

A *channel* is what Showdown writes to a viewer: for `p1` and `p2` the stream
of that player (the battle log as they see it, with `|request|` lines where
Showdown sends them), for `omniscient` the log with both sides' secrets and no
requests. `Batch(dex, n, channels=True)` or `Battle.reference(...,
channels=True)` keeps them; `batch.channel(i, "p1", start)` returns the lines
from line `start` on (so a consumer reads incrementally), `channel_len` their
number.

The channel is **byte-equal to the Showdown stream** (the one Areneon's runner
records from `getPlayerStreams`, `|t:|` lines dropped), on all 3,200 recorded
battles of the evaluation corpus (the nine that hold No Retreat differed until the
pin sync, #1531) and on the hand-built scenarios of the hint audit below:

- the **banner**, which the engine's own log does not carry: `|gen|`, the
  `|rule|` lines, the Open Team Sheets prompt (`|uhtml|`) and its clearing
  (`|uhtmlchange|`) per player, and the blank line before `|win|` / `|tie`.
  They come from the pinned Showdown: `refsim/export_format.js` writes
  `format.banner` by playing a battle of the format to its start and cutting
  its raw log at protocol anchors (`refsim/export_banner.js`); `channels=True`
  needs an export that has it (`ValueError` otherwise) and a format whose
  banner depends on the teams (Force Open Team Sheets) has none.
- the **`|request|` lines**, where Showdown's `sendUpdates` sends them: after
  the log of the stop, to *both* sides, a `{"wait":true,...}` request for the
  side with nothing to choose. Written as Showdown writes them (key order),
  without `rqid`, which only the server path numbers (the stream of the
  in-process simulator has none and the request's identity ignores it).
- **refused choices**: `|error|[Invalid choice] <text>` for an invalid one;
  `|error|[Unavailable choice] <text>` for one the request hid the reason
  for, followed by the side's request again with `"update":true` (the next
  `request_seq`).
- the **hints** (`|-hint|...`) that Showdown prints where the engine's log had
  none: ten of the fifteen places a battle of this format can reach, listed in the audit below. They change the engine's own log
  (they are in `omniscient` too), which moves `examples/fingerprint` (see
  CHANGELOG).

A channel holds no copy of the log: its segments name ranges of it and lines
are made when asked for, but it keeps every request (as a JSON value, for the
candidates and the hash), which is most of its memory, so channels are not free
for a battle nobody reads. Measured (`scripts/areneon/memory_per_battle.py`,
1,024 battles, 73 decisions each, resident memory per battle): channels off 64
KiB; channels on and never read 218 KiB (about 154 KiB more); read and
discarded after each step (`discard_channel`, the omniscient channel too) 159 KiB,
which gives back about 38% of the extra. A harness that reads every step should
discard what it has read, and one that needs no channel for a battle should not
ask for them.
`Batch.observe()` is separate: a batch builds the trackers it needs only when
`observe` is first called, so a batch read through channels builds none.

`batch.request_hash(i, side)` is the identity of the side's current request:
SHA-256 of its canonical JSON (RFC 8785) without `rqid` (`ninjask::identity`,
no dependency). It equals Areneon's `request_hash` for every request of the
corpus.

**Choosing.** `Batch.choose([(battle, side, text), ...])` submits Showdown
syntax and returns one byte per pick: bit 0 refused as `[Unavailable
choice]` (the decision stays pending, new request in the stream), bit 1 the
battle ended, bit 2 refused as invalid. `choose_candidates` takes the index of
a candidate instead. `Batch.step` (indices into the engine's own lists) keeps
its meaning and also feeds the channels. `Batch.pending()` lists the decisions
(battle, side) without building observations.

### Hint audit

Showdown's `|-hint|` lines are in the players' channels (and the omniscient log), so a
missing one changes any digest of a channel. The audit covers **every** place the pinned
Showdown (`7244fe8e`) writes one: `battle.hint(...)` and `add('-hint', ...)` in `sim/` and
`data/` (the mods other than `champions` are not played here); there are 32. Each is classed
by whether a battle of `gen9championsvgc2026regmc` can reach it, and each reachable one has a hand-built
scenario recorded from Showdown (`tests/fixtures/areneon/scenarios.jsonl.gz`,
`tests/python/test_n1_scenarios.py`) and, where its absence is the interesting part,
a control that prints none.

| Site (pin) | Trigger | Reachable in Reg M-C | Engine |
|---|---|---|---|
| `battle-actions.ts` 680 | a Prankster move at a Dark foe | yes | prints (scenarios `prankster-dark-*`) |
| `battle-actions.ts` 318 | a `cantusetwice` move (Gigaton Hammer) used again right after itself (Instruct) | yes | **added** (`gigatonhammer-instruct`, `-once`) |
| `moves.ts` 5098, 5484 | Fake Out, First Impression after the first turn out (Copycat, Instruct) | yes | **added** (`fakeout-copycat`, `fakeout-instruct`, `firstimpression-instruct`, controls `fakeout-first-turn`, `fakeout-after-switch-in`) |
| `moves.ts` 20925 | Wish used on turn 256 (8-bit turn count) | yes | **added** (`turn-overflow-wish-futuresight`, control `wish-late`) |
| `conditions.ts` 386 | Future Sight used from turn 253 | yes | **added** (same scenario) |
| `conditions.ts` 399 | Future Sight lands on a slot whose Pokémon fainted | yes | prints (`futuresight-target-fainted`) |
| `battle.ts` 1332 | Skill Swap between allies | yes | prints (`skillswap-ally`, `-foe`) |
| `moves.ts` 15138 | Rising Voltage with Electric Terrain at a grounded foe | yes | prints (`risingvoltage-*`) |
| `moves.ts` 14118 | a priority move at an airborne target under Psychic Terrain | yes | prints (`psychicterrain-*`) |
| `moves.ts` 16238, 16241 | Shell Side Arm hits a foe / its Substitute | yes | prints (`shellsidearm-*`) |
| `moves.ts` 20989 | Body Press under Wonder Room | yes | prints (`wonderroom-bodypress`) |
| `abilities.ts` 2609 | a Mimicry holder (here a Ditto that transformed into one) changes type when the terrain ends | yes | prints (`mimicry-transformed-terrain-ends`) |
| `moves.ts` 798 | Aura Wheel used by a non-Morpeko (Copycat) | yes | prints (`aurawheel-copycat`) |
| `moves.ts` 3370 Dark Void, 9174 Hyperspace Fury, 9690 Ion Deluge, 10997 Mat Block | the moves | no: not in the format's move list | not applicable |
| `moves.ts` 3504 Destiny Bond, 15442 Roost, `battle-actions.ts` 1925 | Dynamax, Terastallization, Ogerpon's Tera | no: neither exists in the format | not applicable |
| `moves.ts` 17999 Stockpile; `battle.ts` 2060, 2240, 2243; `battle-actions.ts` 143 | Gen 7, Gen 2, Gen 1, Gen 3-4 rules | no: Gen 9 | not applicable |
| `battle.ts` 2774, 2780 | a switch cancelled by a Pursuit faint (`pursuitfaint`) | no: Pursuit is not a move of the format | not applicable |
| `abilities.ts` 2085 Illusion Level Mod; `rulesets.ts` 1396 Sleep Clause Mod | rules the format does not have | no | not applicable |
| `battle-actions.ts` 64 | `switchIn` of a Pokémon that is already in | not shown reachable: a validated choice never names an active Pokémon and a switch action precedes every move; no recorded battle or scenario reaches it | not printed (unverified) |

Of the fifteen reachable sites the engine already printed five (Aura Wheel, Mimicry, Shell
Side Arm in both places, Wonder Room); the first scan of the 3,200 recorded battles found
five more it omitted (Skill Swap, Future Sight that does not hit, Prankster, Rising Voltage,
Psychic Terrain); the audit found five that none of those battles holds (Fake Out and First
Impression after Copycat or Instruct, the `cantusetwice` hint, and the two turn-overflow
hints). The claim is therefore: these are **all** the `|-hint|` lines of the pin that a
battle of this format can produce, except the last row, which is argued, not shown. Mat
Block, Dark Void and the others would need a format that has the move.

## 3. Candidates

`Battle.candidates(side)` / `Batch.candidates(i, side)` return
`[(choice, pending)]` for the side's current request: **a port of Areneon's
reference enumerator** (`enumerateCandidates`, ADR-006), a pure function of
the request JSON (`ninjask::candidates`):

- Team Preview lists every *ordered* pick (360 of four from six), as digit
  strings in lexicographic order; the engine's `legal_choices` (90, unordered
  leads) is untouched.
- A move request: for the first active slot its options (each move in request
  order; for each target the move may name in the order foe 1, foe 2, ally;
  its Mega Evolution variants after it; then the bench switches), and for each
  the second slot's options; pairs the engine always refuses (one Pokémon
  switched in twice, Mega Evolution on both) are left out. A forced switch:
  the bench in order, then `pass`; Revival Blessing as in the reference.
- `pending` is true when the request cannot tell the player whether the
  choice is legal (`maybeTrapped`, `maybeDisabled`, `maybeLocked`): the engine
  may refuse it, and the refusal is how the player finds out. A refusal of a
  candidate that is **not** pending is a defect of the enumerator (abort); a
  refusal of a pending one is a new request.
- A request field the enumerator does not know fails closed (`ValueError`).

Checked: the golden choice corpus of Areneon (17 cases), the reference lists
at all 120,000 requests of the 3,200 recorded battles (equal, in order, pending
flags included), and 100,000 fresh requests (below).

## 4. Draw log and forced draws

Showdown's battle makes every random number through four calls: `random(n)`
or `random(from, to)`, `randomChance(numerator, denominator)`,
`sample(items)` and `shuffle(items, start, end)`. A *draw* is one such call as
the differential harness writes it: `(method, args, result)` with decimal
strings (`sample`: the number of items and the index; `shuffle`: the number of
items and the range, then the permutation; `randomChance`: `true`/`false`).

- `record_draws=True` (on `Battle`, `Batch`) logs every call with the result
  used; `take_draws()` returns and empties the log. The engine's draws equal
  Showdown's, call for call, for the same seed and choices on all 3,200 corpus
  battles (two differed in the framing of a shuffle until the sorted lists below
  were completed).
- `set_forced_draws(draws)` is the forced mode of the harness: the n-th call
  made from now on is answered from the n-th entry when its method and
  arguments equal the entry's, otherwise by the engine's own generator; the
  generator advances for every call either way. Every call is reported by
  `take_draws()` with the answer used, so the harness sees a call the trace had
  no entry for. With it a battle on the *fast* generator replays a Showdown
  trace: the game and both players' channels equal Showdown's.
  `reset(..., forced_draws=...)` / `Battle(..., forced_draws=...)` force the
  draws the start makes (genders of Pokémon with none).
- Entries of a method the battle never calls are kept in place (they match
  nothing); an entry of a known method with arguments or a result the call
  cannot have is a `ValueError`.
- Switched off, the log costs one check per draw.

To make the labels equal Showdown's, five call sites that drew `random(n)`
directly now call the method Showdown calls (`sample` for a random target,
the Roar and Dragon Tail switch-in and Trace's pick; `randomChance` for
accuracy, with a fractional numerator where the stages leave one): the numbers
drawn are unchanged.

**Sorted lists and the framing of a shuffle.** Showdown sorts the handlers of an
event and shuffles each run of equal ones: `shuffle(len, start, end)`, where `len`
is the whole list. The engine builds each such list by hand, so a handler it
leaves out because it never acts leaves the numbers drawn unchanged and changes
`len`. A forced replay (`set_forced_draws`) then does not match that call and uses
the engine's own draw, which is wrong there; the natural game (Showdown seed or
fast generator) is not affected. The 3,200-battle run found two such lists:
`ModifyMove` (a Pokémon with Keen Eye under Gravity and Wonder Room: Showdown
`shuffle(3, 1, 3)`, the engine `shuffle(2, 0, 2)`) and `Update` (Attract and Fling
next to a held Aspear Berry: Showdown `shuffle(3, 0, 2)`, the engine
`shuffle(2, 0, 2)`). Both lists now include the user's ability and item handlers
(and the freeze status for `ModifyMove`) read from the dex's handler tables, as the
other sorted lists already do for their events; they tie with nothing (another
subOrder), so the numbers drawn are unchanged: the same 3,200 battles equal
Showdown's draws call for call, the two recorded battles are regression tests
(`tests/fixtures/areneon/gaps.jsonl.gz`, which failed before) and so is a scenario
(`keeneye-gravity-wonderroom`). `tests/python/test_n1_handlers.py` holds the sets of
abilities and items with such a handler in this format: a pin that adds one fails it.

How much of this is checked is measured, not claimed (`scripts/areneon/sort_sites.py`,
which replays the corpus with the engine's own sort trace): of the 43 call sites of the
sorting helpers in `src/`, 16 drew between ties in the 3,200 battles and their framing equals
Showdown's there (7 more sorts are reached through the helpers and attributed to a wrapper's
line, and drew too). At the other 27 no shuffle was attributed in the corpus: 11 sorted
lists of two or more with no tie, 16 were not reached at all. Their framing is
**unaudited**: it is only as complete as the list each one builds. They include the
`BeforeMove` lists of `use_move.rs` (Throat Chop and Heal Block at priority 6, the foes'
Imprisons at 4, Chilly Reception), the `Accuracy`, `ModifySecondaries`, `SetStatus`,
`AfterMove` and `AfterMoveSecondary` lists, the trap lists and the berries'. The consequence
of a gap there is the one above: a forced replay that does not match one shuffle and then
uses the engine's own draw; a natural game is unaffected.

## Evaluation only

Some of this API reads what a player must never see, and is for the harness's
evaluation and tooling, not for anything that feeds a policy: the `omniscient` channel
(both sides' exact values and secrets), `Battle.chosen(side)` (a side's pending choice,
the other side's included), `take_draws` / `set_forced_draws` (the random outcomes of a
battle), `show_team_sheets` (writes the opponent's sets into the log) and, for
the same reason, a recording's omniscient log and draw trace. A policy's input is its
own player channel and the candidates built from its own request.
`tests/python/test_n1_channels.py` has a canary for this boundary: it scans the player
channels of fresh and recorded battles for the other player's request or error, the
other side's exact hit points and an unresolved split marker, and it flags the
omniscient log fed to the same check.

## Cost

Measured on one thread (`RAYON_NUM_THREADS=1`, pinned to a core, `nice 19`) on a shared
workstation (load average 4 to 8), not claimed to repeat. Pool: 978 generated
teams.

**The existing path did not slow down.** `examples/batch_bench` (random policy,
every decision observed), `origin/main` (the pin sync, `d5db5b8`) and this branch
alternating, six runs each, decisions per second, medians (measured again after the
hint additions and the completed lists; the fixture export of the pin for both):

| battles in flight | origin/main | this branch | ratio |
|---|---|---|---|
| 64 | 69,361 | 68,865 | 0.993 |
| 256 | 62,094 | 60,365 | 0.972 |
| 1024 | 53,971 | 52,723 | 0.977 |

(Run-to-run spread within one binary is about plus or minus 8%; before the pin sync
and the hints the same comparison against the evaluated commit gave 0.999, 1.017 and
0.974. The extended lists sort a list of two for the Pokémon whose ability and item both
have a `ModifyMove` handler or that have a volatile with an `Update` one, which is rare.)

**Showdown seeds** cost about 3%: `batch_bench` with 256 battles, 54,400 to
58,600 decisions/s with `sodium,<hex>` seeds against 56,400 to 59,400 with
integer seeds. The generator is one ChaCha20 block per draw.

**The channel path** (`examples/channel_bench`: candidates, a choice by index,
both players' new lines read and discarded; no observation) plays about 12,000
to 13,000 decisions/s with 256 battles in flight, with integer or Showdown seeds
alike. Where the time goes (the engine's own step is about 11 us of the
roughly 80 us a decision): building each side's request as a JSON value
(about 36 us a request, two requests a stop), writing it, and storing it.
A request written straight to text from the engine's state, without the JSON
value, is the next step for this path.

## Known gaps

- **No `|tie` at the turn limit.** `Batch.turn_limit` (150 by default) only stops
  offering decisions (`pending()` is empty, `done(i)` is true) once a battle's turn
  counter reaches it: the battle is not ended, no `|tie` or blank line is written to any
  channel and `winner` is unset. Areneon's runner cuts a game at `max_turns` by writing
  `>forcetie` after the `|turn|` line that passes the limit, so its logs end with `|tie`
  and `turns == max_turns + 1`. A harness that must reproduce that cut sets
  `turn_limit = max_turns + 1` and writes the tie itself (the engine has no forced tie):
  N2 has to decide whether the engine writes it.
- **Framing of a shuffle at the 27 unexercised sorted-list call sites** (see section 4):
  unaudited, not known wrong.
- **`switchIn` of an active Pokémon** is the one hint site not shown unreachable (above).
- **No direct request-text writer.** A request is built as a JSON value and then
  written; about 60% of the channel path's time is that value (see "Cost"). A writer
  from the engine's state to the request text is the next step for throughput.
- Wait requests, `update`, `noCancel` and every key order were checked against Showdown's
  recorded requests; `rqid` is not emitted (see above).
- The channel path's figures of "Cost" were measured before the pin sync and the hint
  additions; the default path was measured again afterwards ("Cost", first table).

## Evidence

Run on the final code, on top of the pin sync (`d5db5b8`), with the scripts of
`scripts/areneon/README.md` (the numbers are of the run reported on PR #1):

| Check | Result |
|---|---|
| 3,200 recorded battles (random-legal, pending-first, uniform-all, Illusion teams) replayed into the engine: request hashes | 3,200 of 3,200 equal |
| candidates at every request against the recorded reference lists (order and `pending` included) | 3,200 of 3,200 equal (about 120,000 requests) |
| both channels and the omniscient log, line for line | 3,200 of 3,200 equal (3,191 before the pin sync and the hints) |
| the engine's draws against Showdown's, call for call | 3,200 of 3,200 equal (3,198 before the sorted lists) |
| forced replay on the fast generator (game and channels equal Showdown's) | 3,200 of 3,200 equal (3,189 before) |
| 146,352 fresh requests (3,600 battles, random candidates incl. pending) against Areneon's `enumerateCandidates` (TypeScript, same request text) | 146,352 equal, 0 differ, 0 refused by the reference's schema; 13,243,850 candidates, 59,673 pending |
| fresh battles judged by the engine (brute force over every syntactic choice string) | 104,318 requests of 3,600 battles: no non-pending candidate refused by the engine; 11,062 pending candidates refused, all as `[Unavailable choice]` and each followed by a changed request; 2,607 requests brute-forced over every syntactic choice string of their phase, no accepted action missing |
| `examples/fingerprint` (2,000 fixture games), against `origin/main` | logs: +116 hint lines; requests: equal; the numbers drawn: equal (258,916 draws, one hash before and after); the draws entry of the fingerprint moves only by 64 `# sort` trace entries; `c826ee9bb327b2d8` to `36adc509dbe129d1` |
| 25 hand-built hint scenarios and the two gap battles, channels and draws | 27 of 27 equal (5 differed in channels and 1 in draws, and both gap battles in draws, before the fixes) |
