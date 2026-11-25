//! The engine at the Showdown pin 7244fe8e. Needs the format export
//! (`data/derived/format/`, see `common`); skipped without it. The expected
//! lines are Showdown's: `tests/python/test_pin_sync.py` plays the same teams
//! against the pinned Showdown and compares them after every input.
//!
//! - Showdown d12b9c1e9 (#12124): No Retreat's condition adds the `trapped`
//!   volatile itself (`|-activate|POKEMON|trapped`, then
//!   `|-start|POKEMON|move: No Retreat`) and traps no more through its own
//!   `onTrapPokemon`. An already trapped or Ghost-type user keeps the
//!   `noretreat` volatile and prints nothing; Baton Pass hands `noretreat` on
//!   (`trapped` is `noCopy`) without its trap.
//! - Revival Blessing's forced switch: the reviving Pokémon may pass when no
//!   living Pokémon waits on the bench (`pass, pass`).

mod common;

use std::sync::Arc;

use ninjask::battle::log::Viewer;
use ninjask::{Battle, Dex};
use serde_json::Value;

fn dex() -> Option<Arc<Dex>> {
    let json = common::format_export("gen9championsvgc2026regmc")?;
    Some(Arc::new(Dex::from_json(&json).ok()?))
}

fn mon(species: &str, ability: &str, moves: &[&str], gender: &str) -> String {
    format!(
        "{species}|||{ability}|{}|Hardy||{gender}|||50|",
        moves.join(",")
    )
}

const IDLE: [&str; 4] = ["Teatime", "BodySlam", "Protect", "Detect"];

fn team(mons: &[String]) -> String {
    let mut all: Vec<String> = mons.to_vec();
    // Species differ within a team.
    all.push(mon("Slowbro", "oblivious", &IDLE, "M"));
    all.push(mon("Pikachu", "static", &IDLE, "M"));
    all.join("]")
}

fn falinks() -> String {
    mon(
        "Falinks",
        "defiant",
        &["NoRetreat", "Protect", "BatonPass", "Teatime"],
        "M",
    )
}

/// A battle after team preview (`team 1234` each).
fn start(dex: &Arc<Dex>, t1: &[String], t2: &[String], seed: u64) -> Battle {
    let (a, b) = (team(t1), team(t2));
    let mut battle = Battle::new(dex.clone(), [&a, &b], ["p1", "p2"], seed).unwrap();
    battle.choose(0, "team 1234").unwrap();
    battle.choose(1, "team 1234").unwrap();
    battle
}

fn play(b: &mut Battle, c1: &str, c2: &str) {
    b.choose(0, c1).unwrap();
    b.choose(1, c2).unwrap();
}

fn log(b: &Battle) -> Vec<String> {
    b.log_lines(Viewer::Omniscient, 0)
}

fn snorlax_clefable() -> Vec<String> {
    vec![
        mon("Snorlax", "thickfat", &IDLE, "M"),
        mon("Clefable", "magicguard", &IDLE, "F"),
    ]
}

fn request_active(b: &Battle, side: usize, slot: usize) -> Value {
    b.request_json(side).unwrap()["active"][slot].clone()
}

#[test]
fn no_retreat_adds_the_trapped_volatile_before_it_starts() {
    let Some(dex) = dex() else { return };
    for seed in 1..4 {
        let mut b = start(
            &dex,
            &[falinks(), mon("Snorlax", "thickfat", &IDLE, "M")],
            &snorlax_clefable(),
            seed,
        );
        play(&mut b, "move 1, move 1", "move 1, move 1");
        let lines = log(&b);
        let start = lines
            .iter()
            .position(|l| l == "|-start|p1a: Falinks|move: No Retreat")
            .expect("No Retreat starts");
        assert_eq!(lines[start - 1], "|-activate|p1a: Falinks|trapped");
        assert!(lines[start - 2].starts_with("|-boost|p1a: Falinks|spe|"));
        // Trapped from the next turn on, by the `trapped` volatile alone.
        assert_eq!(request_active(&b, 0, 0)["trapped"], true);
        let falinks = &b.sides[0].mons[0];
        assert!(falinks.vol.no_retreat);
    }
}

#[test]
fn no_retreat_by_a_pokemon_that_is_already_trapped_starts_silently() {
    let Some(dex) = dex() else { return };
    let umbreon = mon(
        "Umbreon",
        "synchronize",
        &["MeanLook", "Protect", "Teatime", "Wish"],
        "M",
    );
    let t2 = vec![umbreon, mon("Clefable", "magicguard", &IDLE, "F")];
    for seed in 1..4 {
        let mut b = start(
            &dex,
            &[falinks(), mon("Snorlax", "thickfat", &IDLE, "M")],
            &t2,
            seed,
        );
        // Falinks protects; Mean Look reaches it anyway.
        play(&mut b, "move 2, move 1", "move 1 1, move 1");
        play(&mut b, "move 1, move 1", "move 3, move 1");
        let lines = log(&b);
        let trapped = lines
            .iter()
            .filter(|l| *l == "|-activate|p1a: Falinks|trapped")
            .count();
        assert_eq!(trapped, 1, "only Mean Look's activation");
        assert!(!lines.iter().any(|l| l.contains("move: No Retreat")));
        let falinks = &b.sides[0].mons[0];
        assert!(falinks.vol.no_retreat, "the volatile is added all the same");
        assert!(falinks.vol.trapped.is_some(), "Mean Look's own trap");
        assert_eq!(request_active(&b, 0, 0)["trapped"], true);
    }
}

#[test]
fn no_retreat_by_a_ghost_type_starts_silently_and_does_not_trap() {
    let Some(dex) = dex() else { return };
    let gengar = mon(
        "Gengar",
        "cursedbody",
        &["NoRetreat", "Protect", "Teatime", "BatonPass"],
        "M",
    );
    for seed in 1..4 {
        let mut b = start(
            &dex,
            &[gengar.clone(), mon("Snorlax", "thickfat", &IDLE, "M")],
            &snorlax_clefable(),
            seed,
        );
        play(&mut b, "move 1, move 1", "move 1, move 1");
        let lines = log(&b);
        assert!(lines
            .iter()
            .any(|l| l.starts_with("|-boost|p1a: Gengar|atk|")));
        assert!(!lines.iter().any(|l| {
            (l.starts_with("|-activate|") || l.starts_with("|-start|"))
                && (l.contains("trapped") || l.contains("No Retreat"))
        }));
        assert!(b.sides[0].mons[0].vol.no_retreat);
        assert!(request_active(&b, 0, 0).get("trapped").is_none());
    }
}

#[test]
fn baton_pass_hands_on_no_retreat_without_its_trap() {
    let Some(dex) = dex() else { return };
    for seed in 1..4 {
        let mut b = start(
            &dex,
            &[falinks(), mon("Snorlax", "thickfat", &IDLE, "M")],
            &snorlax_clefable(),
            seed,
        );
        play(&mut b, "move 1, move 1", "move 1, move 1");
        play(&mut b, "move 3, move 1", "move 1, move 1");
        // Baton Pass: p1 chooses the replacement for slot a.
        b.choose(0, "switch 3, pass").unwrap();
        // The newcomer takes the passer's place in the team.
        let newcomer = &b.sides[0].mons[0];
        assert!(newcomer.is_active && newcomer.vol.no_retreat);
        assert!(newcomer.vol.trapped.is_none());
        let request = b.request_json(0).unwrap();
        assert!(request["active"][0].get("trapped").is_none(), "{request}");
        assert!(request["active"][0]["moves"].is_array());
    }
}

/// A recorded battle (random-legal play, Showdown 7244fe8e) that reaches p2's
/// Revival Blessing request with no living Pokémon on the bench. Showdown
/// accepted `pass, pass` there (its `forcedPassesLeft` is 1).
#[test]
fn revival_blessing_request_offers_pass_when_the_bench_has_no_living_pokemon() {
    let Some(dex) = dex() else { return };
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/pin_sync/revival_blessing.json"
    );
    let fixture: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let teams = &fixture["teams"];
    let rng = ninjask::rng::Rng::from_showdown_seed(fixture["seed"].as_str().unwrap()).unwrap();
    let mut b = Battle::new_with_rng(
        dex,
        [teams["p1"].as_str().unwrap(), teams["p2"].as_str().unwrap()],
        ["p1", "p2"],
        rng,
    )
    .unwrap();
    for input in fixture["inputs"].as_array().unwrap() {
        let side = if input[0] == "p1" { 0 } else { 1 };
        b.choose(side, input[1].as_str().unwrap()).unwrap();
    }
    let request = b.request_json(1).unwrap();
    assert_eq!(request["forceSwitch"], serde_json::json!([false, true]));
    let legal: Vec<String> = b
        .player_choices(1)
        .iter()
        .map(|c| b.choice_string(c))
        .collect();
    assert_eq!(legal, ["pass, switch 3", "pass, switch 4", "pass, pass"]);
    let all: Vec<String> = b
        .joint_choices(1)
        .iter()
        .map(|c| b.choice_string(c))
        .collect();
    assert_eq!(all, legal);
    b.choose(1, fixture["showdown_accepted"].as_str().unwrap())
        .unwrap();
    // Showdown asks again: the revival lasts to the turn's residual. Then p2 revives.
    assert_eq!(
        b.request_json(1).unwrap()["forceSwitch"],
        serde_json::json!([false, true])
    );
    for input in fixture["then"].as_array().unwrap() {
        b.choose(1, input[1].as_str().unwrap()).unwrap();
    }
}

/// The tracker reads Showdown's log: No Retreat's `-activate|..|trapped` is the trap, and the
/// `noretreat` volatile a Baton Pass hands on is not one (Showdown d12b9c1e9).
#[test]
fn the_tracker_sees_no_retreat_trap_only_as_the_trapped_volatile() {
    use ninjask::tracker::Tracker;
    let Some(dex) = dex() else { return };
    let mut t = Tracker::new(dex, 0);
    let lines = [
        "|player|p1|Player 1|",
        "|player|p2|Player 2|",
        "|gametype|doubles",
        "|switch|p1a: Persian|Persian, L50, M|100/100",
        "|switch|p1b: Kleavor|Kleavor, L50, M|100/100",
        "|switch|p2a: Falinks|Falinks, L50, M|100/100",
        "|switch|p2b: Scrafty|Scrafty, L50, M|100/100",
        "|turn|1",
        "|move|p2a: Falinks|No Retreat|p2a: Falinks",
        "|-boost|p2a: Falinks|atk|1",
        "|-activate|p2a: Falinks|trapped",
        "|-start|p2a: Falinks|move: No Retreat",
    ];
    for l in lines {
        t.feed(l);
    }
    let index =
        |t: &Tracker, name: &str| t.sides[1].mons.iter().position(|m| m.name == name).unwrap();
    assert!(
        t.trap_state(1, index(&t, "Falinks")).0,
        "trapped by the volatile"
    );
    for l in [
        "|upkeep",
        "|turn|2",
        "|move|p2a: Falinks|Baton Pass|p2a: Falinks",
        "|switch|p2a: Snorlax|Snorlax, L50, M|100/100|[from] Baton Pass",
    ] {
        t.feed(l);
    }
    let newcomer = index(&t, "Snorlax");
    assert!(
        t.sides[1].mons[newcomer]
            .volatiles
            .iter()
            .any(|v| v == "noretreat"),
        "handed on"
    );
    assert!(!t.trap_state(1, newcomer).0, "but it traps no more");
}

/// Two Revival Blessing slots at once (a revival passed with `pass, pass` lasts to the residual, and the
/// other reviver then uses its own). Showdown never records a revival in `switchIns`, so every combination
/// of the two fainted teammates and a pass is accepted, the same teammate twice included (checked on the
/// pin: all nine); the second revival heals the same Pokémon again.
#[test]
fn two_revival_slots_may_name_the_same_fainted_teammate() {
    let Some(dex) = dex() else { return };
    let fodder = [
        mon("Pikachu", "static", &["Protect", "Teatime"], "M"),
        mon("Dedenne", "cheekpouch", &["Protect", "Teatime"], "M"),
    ];
    let revivers = [
        "Pawmot|||naturalcure|RevivalBlessing,Protect,Teatime|Jolly|0,0,0,0,0,32||||50|"
            .to_string(),
        "Pawmot|||naturalcure|RevivalBlessing,Protect,Teatime|Hardy||||||50|".to_string(),
    ];
    let t1 = [fodder.to_vec(), revivers.to_vec()].concat().join("]");
    let t2 = [
        "Garchomp|||roughskin|Earthquake,Protect|Adamant|0,32,0,0,0,32||||50|".to_string(),
        mon("Corviknight", "pressure", &["Protect", "Teatime"], "F"),
        mon("Snorlax", "thickfat", &["Protect"], "M"),
        mon("Clefable", "magicguard", &["Protect"], "F"),
    ]
    .join("]");
    let reach = |last: Option<&str>| {
        let mut b = Battle::new(dex.clone(), [&t1, &t2], ["p1", "p2"], 11).unwrap();
        for (s, c) in [
            (0, "team 1234"),
            (1, "team 1234"),
            (0, "move 2, move 2"),
            (1, "move 1, move 2"),
            (0, "switch 3, switch 4"),
            (0, "move 1, move 1"),
            (1, "move 2, move 2"),
            (0, "pass, pass"),
        ] {
            b.choose(s, c).unwrap();
        }
        if let Some(c) = last {
            b.choose(0, c).unwrap();
        }
        b
    };
    let b = reach(None);
    assert_eq!(
        b.request_json(0).unwrap()["forceSwitch"],
        serde_json::json!([true, true])
    );
    let mut legal: Vec<String> = b
        .player_choices(0)
        .iter()
        .map(|c| b.choice_string(c))
        .collect();
    legal.sort();
    let mut want: Vec<String> = ["switch 3", "switch 4", "pass"]
        .iter()
        .flat_map(|a| {
            ["switch 3", "switch 4", "pass"]
                .iter()
                .map(move |c| format!("{a}, {c}"))
        })
        .collect();
    want.sort();
    assert_eq!(legal, want);
    // The same teammate twice is played: both revivals heal it.
    let b = reach(Some("switch 3, switch 3"));
    let heals = log(&b)
        .iter()
        .filter(|l| l.starts_with("|-heal|") && l.contains("Revival Blessing"))
        .count();
    assert_eq!(heals, 2, "{:?}", log(&b));
}
