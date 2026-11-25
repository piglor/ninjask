# The declared-state layer (`ninjask::declared`)

The engine behind Areneon's mechanics probe interface (the Areneon fork, #1534). Areneon asks three questions about *declared* Pokémon
and an *absolute declared state*, never about a live battle:

- `damage`: two 16-roll vectors (no crit, crit), the crit rule, the defender's HP and the KO counts of one move;
- `speed_order`: the order of up to four declared actions (moves, switches, Mega Evolutions), as tie groups;
- `stats`: the six stats of one set.

Pokémon Showdown answers by building a scenario battle and reading its own values (about 1 to 2 ms a query). This layer answers
the same questions from this engine's calculators (`calc`, `action_speed`, `move_priority`), through a scenario battle it builds
and declares the way the reference does, in about 10 µs plus the caller's own work. **Showdown stays the oracle**: the layer is
admitted entity by entity by byte-equality with it (Areneon's differential harness), and anything it cannot answer exactly is an
abstention the caller routes to Showdown.

## What it does

1. **Sets** are registered once by number (`SetSpec`: names, Stat Points in Showdown's stat order, the format's level). The layer
   never judges a paste or a team: the caller's validator did.
2. **Scenario.** Two Pokémon a side. A damage query's partners have the species of the declared Pokémon and nothing else (no
   ability, no item), so a spread move has two foes and an ally ability cannot change the number. A speed query declares one or two
   Pokémon a side; a side with one starts a copy of it, fainted at once. The battle starts at once, with no Team Preview
   (`Battle::start_without_preview`), on Showdown's scenario seed (`sodium,0…0`), so an unset gender is drawn as it is there.
   Started scenarios are kept per set pair and cloned (about 7 µs).
3. **Declared state**, absolute, through the engine's own setters: Mega Evolution (`run_mega_evo`, with its ability's start effects),
   HP, the major status (the `SetStatus` event and `AfterSetStatus`, type immunities ignored: an observed fact), the five stat
   stages, then the field (weather, terrain, Trick Room, the four side conditions: `set_weather`, `set_terrain`,
   `add_side_condition`, so a forme change a weather starts happens as in play), then the stages and items written again (a Mega's
   ability can lower a stage or consume an item; the item is put back silently).
4. **The move step the calculator does not run.** The reference runs the engine's whole move step with only the accuracy step and the
   damage loop replaced, so it also runs the pre-hit steps. The layer reproduces them for the move kinds it models
   (`declared::classify`): the abilities' `TryMove` event (Armor Tail, Dazzling, Queenly Majesty, Damp: a stop), the move's own
   `onTry`/`onTryMove`/`onPrepareHit` (charge moves, Fake Out, Sucker Punch, Fling, Belch, ...: a stop is `unsupported` with reason
   `query_kind`), a target the move's `ModifyMove` changes (Expanding Force in Psychic Terrain), Protean and Libero's prepare-hit
   type change, Psychic Terrain's block of a priority move. A move's own `onTryHit` runs inside the hit loop the reference replaces: it
   changes nothing. The event sorts the calculator skips (a tie of equal speeds draws) are run with the engine's random source
   recording, so a draw is the reference's `random_effect`.
5. **Classification.** `computed`, `unsupported` (`query_kind` for a status, multi-hit, fixed-damage or OHKO move or a move that stops
   before the hit; `random_effect` for any draw other than the roll and the crit; a position for a speed query), or an abstention.

## What it refuses

`Abstain { entity, why }`, with the entity named (`move:<id>`, `ability:<id>`, `item:<id>`, `state:<kind>`): a move with a pre-hit
step the layer does not model (Aura Wheel, Shell Side Arm), a state the engine refuses (a status an ability blocks, a second Mega a
side), an action that queues more than a move and a Mega Evolution (Focus Punch, Counter), a fractional priority (Stall), a
set the dex does not know, a weather or terrain the engine does not model. Every query struct refuses a field it does not know
(`deny_unknown_fields`), so a `history` block is refused as a frame, not read without it.

## Wire form and the process

`declared::wire::serve(prober, frame) -> response`: JSON requests `sets`, `blockers` (the mechanics config's KO blockers), `batch`
(a list of queries; the results in order) and `counts`. `ninjask-probe --dex EXPORT.json [--in PATH --out PATH]` serves `[u32
little-endian length][JSON]` frames on standard input and output, or on two named pipes the caller made. Python:
`ninjask.Prober(dex).serve(frame)`.

## Tests

`tests/declared.rs` reads `tests/fixtures/declared/cases.jsonl`: queries in the layer's form with the answer the pinned Showdown
probe gave them (written by Areneon's `packages/team-dataset/scripts/native-probe-cases.ts`, which refuses to write a case this
layer answers differently): the hand-worked damage fixtures, a case for every class of difference found, strides over the
interaction, move and speed sweeps and over the queries a feature builder issues, and queries the probe refuses (the layer must not
answer them). The same suite also checks that an answer does not depend on the kept starts or the order of a batch. Regenerate the file
only on a deliberate Showdown pin bump (`docs/repin.md`): the file names no pin itself, the cases do not change unless the engine does.
