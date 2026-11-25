//! The choices a player may submit, enumerated from the player's own
//! request, in a fixed order, each with whether its legality depends on
//! something the request hides.
//!
//! This is a port of the enumerator Areneon's training harness uses as its
//! reference (ADR-006, `enumerateCandidates` in `packages/showdown-bridge`),
//! and it must give the same list in the same order for the same request.
//! It is a pure function of the request JSON: it proposes every
//! engine-distinct joint choice; only the engine decides which are legal.
//!
//! Differences from [`Battle::legal_choices`](crate::Battle), which stays as
//! it is: Team Preview lists every *ordered* pick (360 for four of six, not
//! the 90 of unordered leads), the order of the list is the one below, and
//! each choice carries a `pending` flag. A choice is *pending* when the
//! request cannot tell the player whether it is legal (`maybeTrapped`,
//! `maybeDisabled`, `maybeLocked`): the engine may refuse it, and the
//! refusal is how the player finds out. A refusal of a choice that is not
//! pending means this enumerator and the engine disagree.
//!
//! The order. Team Preview: the ordered picks as digit strings, lexicographic.
//! A move request: for the first active slot its options (each move in
//! request order, for each target the move may name in the order foe 1, foe
//! 2, ally, with its Mega Evolution variants after it, then each switch in
//! bench order), and for each of those the second slot's options. A forced
//! switch: the bench Pokémon in order, then `pass`. Request fields the
//! enumerator does not know make it fail (`CandidateError`) instead of
//! guessing: the caller must not play the battle on this engine then.

use serde_json::{Map, Value};

/// One joint choice in Showdown's syntax, e.g. `move 1 2 mega, switch 3`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Candidate {
    pub choice: String,
    /// Its legality depends on information the request hides from the player.
    pub pending: bool,
}

/// The request holds something the enumerator does not understand.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CandidateError(pub String);

impl std::fmt::Display for CandidateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for CandidateError {}

type Result<T> = std::result::Result<T, CandidateError>;

fn fail<T>(message: impl Into<String>) -> Result<T> {
    Err(CandidateError(message.into()))
}

/// Doubles: request slots 1 and 2 are the active Pokémon, the rest the bench.
const ACTIVE_SLOTS: usize = 2;

// The fields of a request the enumerator understands: those the pinned
// engine's request types have (`sim/side.ts`) and a supported format uses.
const REQUEST_KEYS: &[&str] = &[
    "wait",
    "teamPreview",
    "forceSwitch",
    "maxChosenTeamSize",
    "active",
    "side",
    "noCancel",
    "update",
    "rqid",
];
const SIDE_KEYS: &[&str] = &["name", "id", "pokemon", "noCancel"];
const POKEMON_KEYS: &[&str] = &[
    "ident",
    "details",
    "condition",
    "active",
    "stats",
    "moves",
    "baseAbility",
    "item",
    "pokeball",
    "ability",
    "commanding",
    "reviving",
    "teraType",
    "terastallized",
];
const ACTIVE_KEYS: &[&str] = &[
    "moves",
    "maybeDisabled",
    "maybeLocked",
    "trapped",
    "maybeTrapped",
    "canMegaEvo",
    "canMegaEvoX",
    "canMegaEvoY",
    "canTerastallize",
];
const MOVE_KEYS: &[&str] = &[
    "move",
    "id",
    "pp",
    "maxpp",
    "target",
    "disabled",
    "disabledSource",
];

/// Move targets that take no target location in the choice string.
const UNTARGETED: &[&str] = &[
    "all",
    "allAdjacent",
    "allAdjacentFoes",
    "allies",
    "allySide",
    "allyTeam",
    "foeSide",
    "randomNormal",
    "scripted",
    "self",
];

/// Mega Evolution and Terastallization, in the order their options follow a move.
const EVENTS: &[(&str, &str, Event)] = &[
    ("canMegaEvo", "mega", Event::Mega),
    ("canMegaEvoX", "megax", Event::Mega),
    ("canMegaEvoY", "megay", Event::Mega),
    ("canTerastallize", "terastallize", Event::Tera),
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Event {
    Mega,
    Tera,
}

fn object<'a>(
    value: Option<&'a Value>,
    what: &str,
    keys: &[&str],
) -> Result<&'a Map<String, Value>> {
    let Some(Value::Object(map)) = value else {
        return fail(format!("{what} is not an object"));
    };
    let unknown: Vec<&str> = map
        .keys()
        .map(String::as_str)
        .filter(|k| !keys.contains(k))
        .collect();
    if !unknown.is_empty() {
        return fail(format!(
            "{what} has unsupported field(s): {}",
            unknown.join(", ")
        ));
    }
    Ok(map)
}

fn array<'a>(value: Option<&'a Value>, what: &str) -> Result<&'a Vec<Value>> {
    match value {
        Some(Value::Array(items)) => Ok(items),
        _ => fail(format!("{what} is not an array")),
    }
}

/// Target locations a move may name from active slot `slot` (0 or 1).
fn targets_for(target: Option<&Value>, slot: usize) -> Result<Vec<i32>> {
    let ally = -((ACTIVE_SLOTS - slot) as i32);
    let this = -(slot as i32 + 1);
    match target {
        // A locked move (Outrage, Recharge) is sent without a target type.
        None => Ok(Vec::new()),
        Some(Value::String(t)) => match t.as_str() {
            "normal" | "any" => Ok(vec![1, 2, ally]),
            "adjacentFoe" => Ok(vec![1, 2]),
            "adjacentAlly" => Ok(vec![ally]),
            "adjacentAllyOrSelf" => Ok(vec![ally, this]),
            t if UNTARGETED.contains(&t) => Ok(Vec::new()),
            t => fail(format!("unsupported move target \"{t}\"")),
        },
        Some(other) => fail(format!("unsupported move target {other}")),
    }
}

/// One slot's part of a joint choice.
#[derive(Clone)]
struct SlotOption {
    text: String,
    pending: bool,
    switch_to: Option<usize>,
    /// Mega Evolution or Terastallization: at most one per side per choice.
    event: Option<Event>,
}

impl SlotOption {
    fn plain(text: impl Into<String>, pending: bool) -> SlotOption {
        SlotOption {
            text: text.into(),
            pending,
            switch_to: None,
            event: None,
        }
    }
}

/// Pair the two slots' options. Pairs the engine always refuses are left
/// out: one bench Pokémon switched in twice, and Mega Evolution (or
/// Terastallization) on both slots in one choice; `keep` may drop more.
fn joint(
    first: &[SlotOption],
    second: &[SlotOption],
    keep: impl Fn(&SlotOption, &SlotOption) -> bool,
) -> Vec<Candidate> {
    let mut out = Vec::with_capacity(first.len() * second.len());
    for a in first {
        for b in second {
            if a.switch_to.is_some() && a.switch_to == b.switch_to {
                continue;
            }
            if a.event.is_some() && a.event == b.event {
                continue;
            }
            if !keep(a, b) {
                continue;
            }
            let mut choice = String::with_capacity(a.text.len() + 2 + b.text.len());
            choice.push_str(&a.text);
            choice.push_str(", ");
            choice.push_str(&b.text);
            out.push(Candidate {
                choice,
                pending: a.pending || b.pending,
            });
        }
    }
    out
}

struct Mon {
    fainted: bool,
    commanding: bool,
    /// The engine asks this Pokémon (Revival Blessing) to pick a fainted ally.
    reviving: bool,
}

fn read_side(request: &Map<String, Value>) -> Result<Vec<Mon>> {
    let side = object(request.get("side"), "request.side", SIDE_KEYS)?;
    array(side.get("pokemon"), "side.pokemon")?
        .iter()
        .enumerate()
        .map(|(i, raw)| {
            let mon = object(Some(raw), &format!("side.pokemon[{i}]"), POKEMON_KEYS)?;
            Ok(Mon {
                reviving: mon.get("reviving") == Some(&Value::Bool(true)),
                fainted: mon
                    .get("condition")
                    .and_then(Value::as_str)
                    .is_some_and(|c| c.ends_with(" fnt")),
                commanding: mon.get("commanding") == Some(&Value::Bool(true)),
            })
        })
        .collect()
}

/// Bench slots (1-based, ascending) that can be switched in.
fn bench_slots(team: &[Mon]) -> Vec<usize> {
    team.iter()
        .enumerate()
        .filter(|(i, m)| *i >= ACTIVE_SLOTS && !m.fainted)
        .map(|(i, _)| i + 1)
        .collect()
}

/// Ordered `k`-permutations of 1..=n as digit strings, lexicographic.
fn permutations(n: usize, k: usize) -> Vec<String> {
    fn walk(n: usize, k: usize, used: u16, prefix: &mut String, out: &mut Vec<String>) {
        if prefix.len() == k {
            out.push(format!("team {prefix}"));
            return;
        }
        for i in 1..=n {
            if used & (1 << i) == 0 {
                prefix.push(char::from(b'0' + i as u8));
                walk(n, k, used | (1 << i), prefix, out);
                prefix.pop();
            }
        }
    }
    let mut out = Vec::new();
    walk(n, k, 0, &mut String::with_capacity(k), &mut out);
    out
}

fn move_slot(
    raw: &Value,
    mon: Option<&Mon>,
    slot: usize,
    bench: &[usize],
) -> Result<Vec<SlotOption>> {
    let active = object(Some(raw), &format!("active[{slot}]"), ACTIVE_KEYS)?;
    let Some(mon) = mon else {
        return fail(format!("no side.pokemon for active slot {}", slot + 1));
    };
    if mon.fainted || mon.commanding {
        return Ok(vec![SlotOption::plain("pass", false)]);
    }
    let flag = |key: &str| active.get(key) == Some(&Value::Bool(true));
    // Where the engine hides the truth, legality is pending.
    let moves_pending = flag("maybeDisabled") || flag("maybeLocked");
    let events: Vec<&(&str, &str, Event)> = EVENTS
        .iter()
        .filter(|(key, _, _)| {
            !matches!(
                active.get(*key),
                None | Some(Value::Bool(false)) | Some(Value::Null)
            )
        })
        .collect();
    let mut options = Vec::new();
    for (j, raw_move) in array(active.get("moves"), &format!("active[{slot}].moves"))?
        .iter()
        .enumerate()
    {
        let mv = object(
            Some(raw_move),
            &format!("active[{slot}].moves[{j}]"),
            MOVE_KEYS,
        )?;
        if !matches!(mv.get("disabled"), None | Some(Value::Bool(false))) {
            continue;
        }
        let targets = targets_for(mv.get("target"), slot)?;
        let targets: Vec<Option<i32>> = if targets.is_empty() {
            vec![None]
        } else {
            targets.into_iter().map(Some).collect()
        };
        for target in targets {
            let base = match target {
                None => format!("move {}", j + 1),
                Some(t) => format!("move {} {t}", j + 1),
            };
            options.push(SlotOption::plain(base.clone(), moves_pending));
            for (_, word, event) in &events {
                options.push(SlotOption {
                    text: format!("{base} {word}"),
                    pending: moves_pending,
                    switch_to: None,
                    event: Some(*event),
                });
            }
        }
    }
    if !flag("trapped") {
        let switch_pending = flag("maybeTrapped");
        for &b in bench {
            options.push(SlotOption {
                text: format!("switch {b}"),
                pending: switch_pending,
                switch_to: Some(b),
                event: None,
            });
        }
    }
    Ok(options)
}

fn preview_candidates(request: &Map<String, Value>, team: &[Mon]) -> Result<Vec<Candidate>> {
    let k = request.get("maxChosenTeamSize").and_then(Value::as_u64);
    let k = match k {
        Some(k) if k >= 1 && k as usize <= team.len() => k as usize,
        _ => {
            return fail(format!(
                "unsupported maxChosenTeamSize {}",
                request
                    .get("maxChosenTeamSize")
                    .map_or("undefined".to_string(), Value::to_string)
            ))
        }
    };
    // From 10 Pokémon the engine reads `team` input as comma-separated.
    if team.len() > 9 {
        return fail(format!("unsupported team size {}", team.len()));
    }
    Ok(permutations(team.len(), k)
        .into_iter()
        .map(|choice| Candidate {
            choice,
            pending: false,
        })
        .collect())
}

/// A Revival Blessing request (`side.pokemon[i].reviving`): the one flagged
/// slot names a fainted Pokémon of the side by position, active slots
/// included, and may pass only when no living bench Pokémon could switch in.
/// Anything but exactly one flagged, reviving slot fails closed.
fn revival_candidates(flags: &[bool], team: &[Mon]) -> Result<Vec<Candidate>> {
    let flagged = flags.iter().filter(|f| **f).count();
    let slot = flags.iter().position(|f| *f);
    if flagged != 1
        || team.iter().filter(|m| m.reviving).count() != 1
        || !slot.is_some_and(|s| team.get(s).is_some_and(|m| m.reviving))
    {
        return fail("Revival Blessing outside one flagged reviving slot is not supported");
    }
    let mut options: Vec<SlotOption> = team
        .iter()
        .enumerate()
        .filter(|(_, m)| m.fainted)
        .map(|(i, _)| SlotOption::plain(format!("switch {}", i + 1), false))
        .collect();
    if options.is_empty() {
        return fail("Revival Blessing request with no fainted Pokémon to revive");
    }
    if bench_slots(team).is_empty() {
        options.push(SlotOption::plain("pass", false));
    }
    let pass = [SlotOption::plain("pass", false)];
    let per_slot: Vec<&[SlotOption]> = flags
        .iter()
        .map(|f| if *f { &options[..] } else { &pass[..] })
        .collect();
    Ok(joint(per_slot[0], per_slot[1], |_, _| true))
}

fn force_switch_candidates(request: &Map<String, Value>, team: &[Mon]) -> Result<Vec<Candidate>> {
    let raw = array(request.get("forceSwitch"), "forceSwitch")?;
    let flags: Option<Vec<bool>> = raw.iter().map(Value::as_bool).collect();
    let flags = match flags {
        Some(f) if f.len() == ACTIVE_SLOTS => f,
        _ => return fail("forceSwitch must hold one boolean per doubles slot"),
    };
    if team.iter().any(|m| m.reviving) {
        return revival_candidates(&flags, team);
    }
    let bench = bench_slots(team);
    let forced = flags.iter().filter(|f| **f).count().min(bench.len());
    let pass = [SlotOption::plain("pass", false)];
    let mut switching = Vec::with_capacity(bench.len() + 1);
    for &b in &bench {
        switching.push(SlotOption {
            text: format!("switch {b}"),
            pending: false,
            switch_to: Some(b),
            event: None,
        });
    }
    switching.push(SlotOption::plain("pass", false));
    let per_slot: Vec<&[SlotOption]> = flags
        .iter()
        .map(|f| if *f { &switching[..] } else { &pass[..] })
        .collect();
    let switches = |o: &SlotOption| usize::from(o.switch_to.is_some());
    Ok(joint(per_slot[0], per_slot[1], |a, b| {
        switches(a) + switches(b) == forced
    }))
}

/// Every engine-distinct joint choice for the player's own request, in the
/// order described in the module documentation. `request` is the request as
/// the engine sends it (`Battle::request_json`); a `wait` request has no
/// candidates.
pub fn candidates_from_request(request: &Value) -> Result<Vec<Candidate>> {
    let request = object(Some(request), "request", REQUEST_KEYS)?;
    let team = read_side(request)?;
    let is = |key: &str| request.get(key) == Some(&Value::Bool(true));
    let phases = [
        is("teamPreview"),
        is("wait"),
        request.get("forceSwitch").is_some_and(Value::is_array),
        request.get("active").is_some_and(Value::is_array),
    ];
    if phases.iter().filter(|p| **p).count() != 1 {
        return fail("request has no single recognisable phase");
    }
    let [preview, wait, force_switch, _] = phases;
    if wait {
        return Ok(Vec::new());
    }
    if preview {
        return preview_candidates(request, &team);
    }
    if team.iter().any(|m| m.reviving) && !force_switch {
        return fail("a reviving Pokémon outside a forced switch request");
    }
    if force_switch {
        return force_switch_candidates(request, &team);
    }
    let active = array(request.get("active"), "active")?;
    if active.len() != ACTIVE_SLOTS {
        return fail(format!(
            "expected {ACTIVE_SLOTS} active slots, got {}",
            active.len()
        ));
    }
    let bench = bench_slots(&team);
    let first = move_slot(&active[0], team.first(), 0, &bench)?;
    let second = move_slot(&active[1], team.get(1), 1, &bench)?;
    Ok(joint(&first, &second, |_, _| true))
}

impl crate::Battle {
    /// The candidates of `side`'s current request (see the module
    /// documentation); empty when the side has none (the game is over) or is
    /// only waiting.
    pub fn candidates(&self, side: usize) -> Result<Vec<Candidate>> {
        match self.request_json(side) {
            Some(request) => candidates_from_request(&request),
            None => Ok(Vec::new()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn mon(fainted: bool) -> Value {
        json!({
            "ident": "p1: X", "details": "X, L50", "condition": if fainted { "0 fnt" } else { "100/100" },
            "active": false, "stats": {"atk": 1, "def": 1, "spa": 1, "spd": 1, "spe": 1},
            "moves": ["tackle"], "baseAbility": "a", "item": "", "pokeball": "pokeball", "ability": "a",
            "commanding": false, "reviving": false
        })
    }

    fn side(fainted: &[bool]) -> Value {
        json!({"name": "p1", "id": "p1", "pokemon": fainted.iter().map(|f| mon(*f)).collect::<Vec<_>>()})
    }

    fn mv(id: &str, target: &str) -> Value {
        json!({"move": id, "id": id, "pp": 8, "maxpp": 8, "target": target, "disabled": false})
    }

    fn choices(request: &Value) -> Vec<String> {
        candidates_from_request(request)
            .unwrap()
            .into_iter()
            .map(|c| c.choice)
            .collect()
    }

    #[test]
    fn team_preview_lists_every_ordered_pick_lexicographically() {
        let request =
            json!({"teamPreview": true, "maxChosenTeamSize": 4, "side": side(&[false; 6])});
        let all = candidates_from_request(&request).unwrap();
        assert_eq!(all.len(), 360);
        assert_eq!(all[0].choice, "team 1234");
        assert_eq!(all[1].choice, "team 1235");
        assert_eq!(all[359].choice, "team 6543");
        assert!(all.iter().all(|c| !c.pending));
        let mut sorted: Vec<&str> = all.iter().map(|c| c.choice.as_str()).collect();
        sorted.sort_unstable();
        assert_eq!(
            sorted,
            all.iter().map(|c| c.choice.as_str()).collect::<Vec<_>>()
        );
    }

    #[test]
    fn a_move_request_pairs_the_slots_in_order() {
        let request = json!({
            "active": [
                {"moves": [mv("tackle", "normal")]},
                {"moves": [mv("growl", "allAdjacentFoes"), mv("protect", "self")]},
            ],
            "side": side(&[false; 4]),
        });
        // Slot one: tackle at foe 1, foe 2, ally; then the two bench switches.
        // Slot two: growl, protect, then the same switches.
        let all = choices(&request);
        assert_eq!(
            all.len(),
            5 * 4 - 2 /* the same bench Pokémon twice */
        );
        assert_eq!(all[0], "move 1 1, move 1");
        assert_eq!(all[1], "move 1 1, move 2");
        assert_eq!(all[2], "move 1 1, switch 3");
        assert_eq!(all[3], "move 1 1, switch 4");
        assert_eq!(all[4], "move 1 2, move 1");
        assert!(all.contains(&"move 1 -2, move 2".to_string()));
        assert!(all.contains(&"switch 3, switch 4".to_string()));
        assert!(all.contains(&"switch 4, switch 3".to_string()));
        assert!(!all.contains(&"switch 3, switch 3".to_string()));
    }

    #[test]
    fn pending_follows_the_flags_of_each_part() {
        let request = json!({
            "active": [
                {"moves": [mv("tackle", "normal")], "maybeTrapped": true},
                {"moves": [mv("growl", "allAdjacentFoes")], "maybeDisabled": true, "maybeLocked": true},
            ],
            "side": side(&[false; 3]),
        });
        let all = candidates_from_request(&request).unwrap();
        let find = |s: &str| all.iter().find(|c| c.choice == s).unwrap().pending;
        // The second slot's moves are pending; the first slot's moves are not, its switch is.
        assert!(find("move 1 1, move 1"));
        assert!(find("switch 3, move 1"));
        assert!(!find("move 1 1, switch 3"));
        let only_plain = json!({
            "active": [{"moves": [mv("tackle", "normal")]}, {"moves": [mv("growl", "allAdjacentFoes")]}],
            "side": side(&[false; 3]),
        });
        assert!(candidates_from_request(&only_plain)
            .unwrap()
            .iter()
            .all(|c| !c.pending));
    }

    #[test]
    fn a_trapped_pokemon_cannot_switch_and_a_disabled_move_is_left_out() {
        let mut disabled = mv("tackle", "normal");
        disabled["disabled"] = json!(true);
        let request = json!({
            "active": [
                {"moves": [disabled, mv("growl", "allAdjacentFoes")], "trapped": true},
                {"moves": [mv("protect", "self")]},
            ],
            "side": side(&[false; 3]),
        });
        assert_eq!(
            choices(&request),
            ["move 2, move 1", "move 2, switch 3"].map(String::from)
        );
    }

    #[test]
    fn mega_evolution_follows_its_move_and_only_one_slot_may_use_it() {
        let request = json!({
            "active": [
                {"moves": [mv("growl", "allAdjacentFoes")], "canMegaEvo": true},
                {"moves": [mv("protect", "self")], "canMegaEvo": true},
            ],
            "side": side(&[false; 2]),
        });
        assert_eq!(
            choices(&request),
            [
                "move 1, move 1",
                "move 1, move 1 mega",
                "move 1 mega, move 1"
            ]
            .map(String::from)
        );
    }

    #[test]
    fn a_forced_switch_lists_the_bench_then_pass() {
        let request =
            json!({"forceSwitch": [true, false], "side": side(&[true, false, false, false])});
        assert_eq!(
            choices(&request),
            ["switch 3, pass", "switch 4, pass"].map(String::from)
        );
        // Nothing to switch to: the pair that passes is the only one.
        let alone = json!({"forceSwitch": [true, false], "side": side(&[true, false])});
        assert_eq!(choices(&alone), ["pass, pass"].map(String::from));
        let both = json!({"forceSwitch": [true, true], "side": side(&[true, true, false, false])});
        assert_eq!(
            choices(&both),
            ["switch 3, switch 4", "switch 4, switch 3"].map(String::from)
        );
    }

    #[test]
    fn revival_blessing_names_a_fainted_pokemon_and_passes_only_without_a_bench() {
        let mut reviver = side(&[false, true, true, false]);
        reviver["pokemon"][0]["reviving"] = json!(true);
        let request = json!({"forceSwitch": [true, false], "side": reviver});
        assert_eq!(
            choices(&request),
            ["switch 2, pass", "switch 3, pass"].map(String::from)
        );
        // A fainted partner and no living bench: `pass` is offered too.
        let mut no_bench = side(&[false, true, true, true]);
        no_bench["pokemon"][0]["reviving"] = json!(true);
        let request = json!({"forceSwitch": [true, false], "side": no_bench});
        assert_eq!(
            choices(&request),
            [
                "switch 2, pass",
                "switch 3, pass",
                "switch 4, pass",
                "pass, pass"
            ]
            .map(String::from)
        );
    }

    #[test]
    fn a_wait_request_has_no_candidates_and_the_unknown_fails_closed() {
        let wait = json!({"wait": true, "side": side(&[false; 4])});
        assert!(candidates_from_request(&wait).unwrap().is_empty());
        let mut unknown = wait.clone();
        unknown["surprise"] = json!(1);
        assert!(candidates_from_request(&unknown).is_err());
        let bad_target = json!({
            "active": [{"moves": [mv("x", "weird")]}, {"moves": [mv("y", "self")]}],
            "side": side(&[false; 4]),
        });
        assert!(candidates_from_request(&bad_target)
            .unwrap_err()
            .0
            .contains("unsupported move target"));
        let two_phases = json!({"wait": true, "teamPreview": true, "side": side(&[false; 4])});
        assert!(candidates_from_request(&two_phases).is_err());
        let one_slot = json!({"active": [{"moves": []}], "side": side(&[false; 4])});
        assert!(candidates_from_request(&one_slot).is_err());
        let bad_preview =
            json!({"teamPreview": true, "maxChosenTeamSize": 7, "side": side(&[false; 6])});
        assert!(candidates_from_request(&bad_preview).is_err());
    }

    #[test]
    fn a_fainted_or_commanding_active_passes() {
        let mut team = side(&[true, false, false, false]);
        team["pokemon"][1]["commanding"] = json!(true);
        let request = json!({
            "active": [{"moves": [mv("x", "normal")]}, {"moves": [mv("y", "normal")]}],
            "side": team,
        });
        assert_eq!(choices(&request), ["pass, pass"].map(String::from));
    }
}
