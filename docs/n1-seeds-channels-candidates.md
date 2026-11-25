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
battles of the evaluation corpus but the nine that hold No Retreat (a
behaviour Showdown changed after the engine's pin; ticket #1531):

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
  none: Skill Swap between allies, Future Sight that does not hit, Dark's
  immunity to Prankster, Rising Voltage on a grounded target, Psychic Terrain
  not protecting an airborne target. They change the engine's own log (they
  are in `omniscient` too), which moves `examples/fingerprint` (see
  CHANGELOG).

A channel holds no copy of the log: its segments name ranges of it and lines
are made when asked for, so a battle nobody reads costs only its requests.
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
  Showdown's, call for call, for the same seed and choices on 3,198 of the
  3,200 corpus battles (the two others: see "Known gaps").
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

To make the labels equal Showdown's, six call sites that drew `random(n)`
directly now call the method Showdown calls (`sample` for a random target,
the Roar and Dragon Tail switch-in and Trace's pick; `randomChance` for
accuracy, with a fractional numerator where the stages leave one): the numbers
drawn are unchanged.

## Cost

Measured on one thread (`RAYON_NUM_THREADS=1`, pinned to a core, `nice 19`) on a shared
workstation (load average 4 to 8), not claimed to repeat. Pool: 978 generated
teams.

**The existing path did not slow down.** `examples/batch_bench` (random policy,
every decision observed), the evaluated commit and this branch alternating, six
runs each, decisions per second, medians:

| battles in flight | evaluated commit | this branch | ratio |
|---|---|---|---|
| 64 | 57,999 | 57,969 | 0.999 |
| 256 | 51,400 | 52,263 | 1.017 |
| 1024 | 45,604 | 44,409 | 0.974 |

(Run-to-run spread within one binary is about plus or minus 8%.)

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

## Known gaps (found by the corpus runs, not fixed here)

- No Retreat (9 of 3,200 battles): Showdown's `|-activate|...|trapped` against
  the engine's `|-start|...|move: No Retreat`; the pin sync (#1531).
- The engine's handler lists for a sorted event omit a handler that never acts
  (2 of 3,200 battles: Keen Eye next to Gravity and Wonder Room; Aspear Berry
  next to Attract and Fling). The numbers drawn are equal; the call is
  `shuffle(2,0,2)` where Showdown's is `shuffle(3,1,3)`, so a forced trace
  does not match there and the natural draw is used. The differential harness
  reports it as a difference at that step.
- Wait requests, `update`, `noCancel` and every key order were checked
  against Showdown's recorded requests; `rqid` is not emitted (see above).
