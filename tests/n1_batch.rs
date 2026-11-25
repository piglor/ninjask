//! Showdown seeds, channels, candidates and the draw log of batched battles
//! (docs/n1-seeds-channels-candidates.md), played in Rust on generated teams.
//! They need the format export of Areneon's pin, which the repository holds
//! as a fixture (tests/fixtures/areneon), and no Showdown. The Showdown
//! oracle is in tests/python/test_n1_*.py, which replay recorded Showdown
//! battles.

use std::sync::Arc;

use ninjask::batch::{Batch, BatchOptions, Chosen};
use ninjask::candidates::candidates_from_request;
use ninjask::channel::wire_request;
use ninjask::rng::{Draw, Rng, Seed};
use ninjask::{Battle, Dex};

fn fixture(name: &str) -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/areneon")
        .join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn dex() -> Arc<Dex> {
    Arc::new(Dex::from_json(&fixture("dex-gen9championsvgc2026regmc-7244fe8e.json")).unwrap())
}

fn teams() -> Vec<String> {
    fixture("teams.txt")
        .lines()
        .filter(|l| !l.is_empty())
        .map(String::from)
        .collect()
}

fn options() -> BatchOptions {
    BatchOptions {
        names: ["p1".into(), "p2".into()],
        channels: true,
        record_draws: false,
    }
}

/// The picks written to a battle in one call and the draws it made.
type Step = (Vec<(usize, usize, String)>, Vec<Draw>);

fn showdown_seed(k: u64) -> Seed {
    Seed::Showdown(format!(
        "sodium,{:032x}",
        0x9e37_79b9_7f4a_7c15u128 * (k as u128 + 1)
    ))
}

/// The two teams of game `k`.
fn pair(teams: &[String], k: usize) -> [&str; 2] {
    [
        &teams[(7 * k) % teams.len()],
        &teams[(7 * k + 3) % teams.len()],
    ]
}

/// Play battle 0 of `batch` to its end with a random candidate for every
/// pending decision.
fn play_out(batch: &mut Batch, policy: &mut Rng, limit: usize) -> usize {
    let mut requests = 0;
    for _ in 0..limit {
        let pending = batch.pending();
        if pending.is_empty() {
            break;
        }
        let picks: Vec<(usize, usize, usize)> = pending
            .iter()
            .map(|&(i, s)| {
                let n = batch.candidates(i, s).unwrap().len() as u32;
                requests += 1;
                (i, s, policy.random(n) as usize)
            })
            .collect();
        batch.choose_candidates(&picks).unwrap();
    }
    requests
}

fn lines(batch: &Batch, viewer: usize) -> Vec<String> {
    batch.channel(0, viewer, 0).unwrap()
}

#[test]
fn a_batch_with_a_showdown_seed_plays_the_reference_battle() {
    let (dex, teams) = (dex(), teams());
    for k in 0..6 {
        let seed = showdown_seed(k);
        let [a, b] = pair(&teams, k as usize);
        let mut batch = Batch::with_options(dex.clone(), 1, options()).unwrap();
        batch.reset_seeded(0, [a, b], &seed, None).unwrap();
        let mut twin =
            Battle::new_with_rng(dex.clone(), [a, b], ["p1", "p2"], seed.rng().unwrap()).unwrap();
        let mut policy = Rng::new(k);
        for _ in 0..400 {
            let pending = batch.pending();
            if pending.is_empty() {
                break;
            }
            for &(i, s) in &pending {
                let list = batch.candidates(i, s).unwrap();
                let choice = list[policy.random(list.len() as u32) as usize]
                    .choice
                    .clone();
                let mine = batch.choose(&[(i, s, choice.as_str())]).unwrap()[0].0;
                let theirs = twin.choose(s, &choice);
                assert_eq!(theirs.is_err(), mine != Chosen::Accepted, "game {k}");
            }
        }
        let played = batch.battle(0).unwrap();
        assert_eq!(
            played.log_lines(ninjask::battle::log::Viewer::Omniscient, 0),
            twin.log_lines(ninjask::battle::log::Viewer::Omniscient, 0),
            "game {k}"
        );
        assert_eq!(
            played.rng.showdown_seed(),
            twin.rng.showdown_seed(),
            "game {k}"
        );
        assert!(played.rng.showdown_seed().is_some());
        assert_eq!(played.winner, twin.winner);
    }
}

#[test]
fn an_integer_seed_is_the_fast_generator_as_before() {
    let (dex, teams) = (dex(), teams());
    let [a, b] = pair(&teams, 0);
    let mut batch = Batch::new(dex.clone(), 2);
    batch.reset(0, [a, b], 7).unwrap();
    batch.reset_seeded(1, [a, b], &Seed::Fast(7), None).unwrap();
    assert!(batch.battle(0).unwrap().rng.showdown_seed().is_none());
    assert_eq!(
        batch
            .battle(0)
            .unwrap()
            .log_lines(ninjask::battle::log::Viewer::Omniscient, 0),
        batch
            .battle(1)
            .unwrap()
            .log_lines(ninjask::battle::log::Viewer::Omniscient, 0)
    );
    let twin = Battle::new(dex, [a, b], ["Player 1", "Player 2"], 7).unwrap();
    assert_eq!(
        batch
            .battle(0)
            .unwrap()
            .log_lines(ninjask::battle::log::Viewer::Omniscient, 0),
        twin.log_lines(ninjask::battle::log::Viewer::Omniscient, 0)
    );
}

#[test]
fn bad_seeds_slots_and_teams_start_nothing() {
    let (dex, teams) = (dex(), teams());
    let [a, b] = pair(&teams, 0);
    let mut batch = Batch::new(dex.clone(), 2);
    for bad in [
        "sodium,xyz",
        "gen5,12",
        "0123",
        &format!("sodium,{}", "0".repeat(65)),
    ] {
        assert!(
            batch
                .reset_seeded(0, [a, b], &Seed::Showdown(bad.into()), None)
                .is_err(),
            "{bad}"
        );
    }
    assert!(batch.reset_seeded(5, [a, b], &Seed::Fast(1), None).is_err());
    assert!(batch
        .reset_seeded(0, ["nonsense", b], &Seed::Fast(1), None)
        .is_err());
    assert!(batch.done(0));
    // reset_many starts none when one is bad
    let starts = [
        (0, [a, b], Seed::Fast(1)),
        (1, [a, b], Seed::Showdown("sodium,zz".into())),
    ];
    assert!(batch.reset_many_seeded(&starts).is_err());
    assert!(batch.done(0) && batch.done(1));
    let starts = [(0, [a, b], Seed::Fast(1)), (1, [a, b], showdown_seed(1))];
    batch.reset_many_seeded(&starts).unwrap();
    assert!(!batch.done(0) && !batch.done(1));
    // channels need the banner of the export
    let mut export: serde_json::Value =
        serde_json::from_str(&fixture("dex-gen9championsvgc2026regmc-7244fe8e.json")).unwrap();
    export["format"].as_object_mut().unwrap().remove("banner");
    let bare = Arc::new(Dex::from_json(&export.to_string()).unwrap());
    assert!(Batch::with_options(bare, 1, options())
        .err()
        .unwrap()
        .contains("banner"));
}

#[test]
fn the_channels_start_with_the_banner_and_end_with_the_result() {
    let (dex, teams) = (dex(), teams());
    let mut batch = Batch::with_options(dex, 1, options()).unwrap();
    let [a, b] = pair(&teams, 1);
    batch
        .reset_seeded(0, [a, b], &showdown_seed(1), None)
        .unwrap();
    let p1 = lines(&batch, 0);
    let kinds: Vec<&str> = p1
        .iter()
        .map(|l| l.split('|').nth(1).unwrap_or(""))
        .collect();
    assert_eq!(&kinds[..4], ["gametype", "player", "player", "gen"]);
    assert_eq!(kinds.iter().filter(|k| **k == "uhtml").count(), 1);
    assert!(p1.last().unwrap().starts_with("|request|"));
    assert_eq!(
        lines(&batch, 2)
            .iter()
            .filter(|l| l.starts_with("|uhtml|"))
            .count(),
        2
    );
    assert!(lines(&batch, 2).iter().all(|l| !l.starts_with("|request|")));
    play_out(&mut batch, &mut Rng::new(3), 400);
    for viewer in 0..3 {
        let all = lines(&batch, viewer);
        let n = all.len();
        assert!(all[n - 1].starts_with("|win|") || all[n - 1] == "|tie");
        assert_eq!(all[n - 2], "|");
        assert_eq!(batch.channel_len(0, viewer).unwrap(), n);
        // read from any line on
        for start in [0, 1, n / 2, n - 1, n, n + 3] {
            assert_eq!(
                batch.channel(0, viewer, start).unwrap(),
                all[start.min(n)..]
            );
        }
    }
    assert!(batch.request_hash(0, 0).unwrap().is_none());
    // a reader that has taken lines lets the channel forget them
    let all = lines(&batch, 0);
    batch.discard_channel(0, 0, all.len() / 2).unwrap();
    assert_eq!(
        batch.channel(0, 0, all.len() - 3).unwrap(),
        all[all.len() - 3..]
    );
    assert!(batch.channel(0, 0, 0).unwrap_err().contains("discarded"));
    assert_eq!(batch.channel_len(0, 0).unwrap(), all.len());
}

#[test]
fn every_request_is_in_the_stream_as_the_engine_builds_it() {
    let (dex, teams) = (dex(), teams());
    let mut waits = 0;
    for k in 0..6 {
        let mut batch = Batch::with_options(dex.clone(), 1, options()).unwrap();
        let [a, b] = pair(&teams, k);
        batch
            .reset_seeded(0, [a, b], &showdown_seed(k as u64), None)
            .unwrap();
        let mut policy = Rng::new(k as u64);
        let mut seen = [0usize; 2];
        for _ in 0..400 {
            for (s, read) in seen.iter_mut().enumerate() {
                let new = batch.channel(0, s, *read).unwrap();
                *read += new.len();
                if let Some(request) = batch.battle(0).unwrap().request_json(s) {
                    if batch.pending().iter().any(|&(_, p)| p == s) || request.get("wait").is_some()
                    {
                        let last = new.last().expect("a request point has lines");
                        assert_eq!(last, &format!("|request|{}", wire_request(&request)));
                        assert!(!last.contains("rqid"));
                        waits += usize::from(request.get("wait").is_some());
                    }
                }
            }
            let pending = batch.pending();
            if pending.is_empty() {
                break;
            }
            let picks: Vec<_> = pending
                .iter()
                .map(|&(i, s)| {
                    (
                        i,
                        s,
                        policy.random(batch.candidates(i, s).unwrap().len() as u32) as usize,
                    )
                })
                .collect();
            batch.choose_candidates(&picks).unwrap();
        }
    }
    assert!(waits > 10, "{waits}");
}

#[test]
fn a_refusal_of_a_pending_candidate_is_an_error_and_a_new_request_and_an_invalid_choice_an_error_only(
) {
    let (dex, teams) = (dex(), teams());
    let mut refusals = 0;
    for k in 0..60 {
        let mut batch = Batch::with_options(dex.clone(), 1, options()).unwrap();
        let [a, b] = pair(&teams, k);
        batch
            .reset_seeded(0, [a, b], &showdown_seed(k as u64 + 100), None)
            .unwrap();
        let mut policy = Rng::new(k as u64);
        for _ in 0..200 {
            let pending = batch.pending();
            if pending.is_empty() {
                break;
            }
            for (i, s) in pending {
                let list = batch.candidates(i, s).unwrap();
                // prefer a pending candidate to provoke refusals
                let pick = list
                    .iter()
                    .position(|c| c.pending)
                    .filter(|_| policy.random(2) == 0)
                    .unwrap_or_else(|| policy.random(list.len() as u32) as usize);
                let before = batch.channel_len(i, s).unwrap();
                let how = batch.choose_candidates(&[(i, s, pick)]).unwrap()[0].0;
                match how {
                    Chosen::Refused => {
                        refusals += 1;
                        assert!(
                            list[pick].pending,
                            "a refused candidate that is not pending is a defect"
                        );
                        let new = batch.channel(i, s, before).unwrap();
                        assert!(new[0].starts_with("|error|[Unavailable choice] "));
                        assert!(
                            new[1].starts_with("|request|") && new[1].contains("\"update\":true")
                        );
                        assert_eq!(new.len(), 2);
                    }
                    Chosen::Invalid => {
                        panic!("a candidate is never invalid: {}", list[pick].choice)
                    }
                    Chosen::Accepted => {}
                }
            }
        }
    }
    assert!(refusals > 5, "{refusals}");
    let mut batch = Batch::with_options(dex, 1, options()).unwrap();
    let [a, b] = pair(&teams, 0);
    batch
        .reset_seeded(0, [a, b], &showdown_seed(0), None)
        .unwrap();
    let before = batch.channel_len(0, 0).unwrap();
    assert_eq!(
        batch.choose(&[(0, 0, "foo")]).unwrap()[0].0,
        Chosen::Invalid
    );
    assert_eq!(
        batch.channel(0, 0, before).unwrap(),
        ["|error|[Invalid choice] Unrecognized choice: foo"]
    );
    assert!(batch.choose(&[(3, 0, "team 1234")]).is_err());
    assert!(batch.choose_candidates(&[(0, 0, 100_000)]).is_err());
    assert!(batch.choose_candidates(&[(0, 5, 0)]).is_err());
    assert!(batch.candidates(7, 0).is_err());
}

#[test]
fn candidates_are_never_refused_unless_pending_and_miss_nothing_the_engine_accepts() {
    let (dex, teams) = (dex(), teams());
    let mut checked = 0;
    for k in 0..4 {
        let mut batch = Batch::with_options(dex.clone(), 1, options()).unwrap();
        let [a, b] = pair(&teams, k);
        batch
            .reset_seeded(0, [a, b], &showdown_seed(k as u64 + 7), None)
            .unwrap();
        let mut policy = Rng::new(k as u64);
        for _ in 0..60 {
            let pending = batch.pending();
            if pending.is_empty() {
                break;
            }
            let battle = batch.battle(0).unwrap().clone();
            for &(_, s) in &pending {
                let list = batch.candidates(0, s).unwrap();
                // What choosing a candidate does, as the engine's whole state afterwards (a choice that
                // runs the turn at once, a forced switch, leaves no pending choice to read): two
                // spellings of one action give one state.
                let mut after = std::collections::HashSet::new();
                for c in &list {
                    let mut probe = battle.clone();
                    match probe.choose(s, &c.choice) {
                        Ok(()) => {
                            after.insert(probe.state_key());
                        }
                        Err(e) => assert!(
                            c.pending && e.starts_with("[Unavailable choice]"),
                            "{}: {e}",
                            c.choice
                        ),
                    }
                }
                // Every choice string of the phase's universe the engine takes does what a candidate does.
                if checked % 8 == 0 {
                    for text in universe(&battle, s) {
                        let mut probe = battle.clone();
                        if probe.choose(s, &text).is_ok() {
                            assert!(after.contains(&probe.state_key()), "missed {text}");
                        }
                    }
                }
                checked += 1;
            }
            let picks: Vec<_> = pending
                .iter()
                .map(|&(i, s)| {
                    (
                        i,
                        s,
                        policy.random(batch.candidates(i, s).unwrap().len() as u32) as usize,
                    )
                })
                .collect();
            batch.choose_candidates(&picks).unwrap();
        }
    }
    assert!(checked > 30);
}

/// Every syntactically plausible choice for side `s`, knowing only the phase
/// and the team size (the oracle of Areneon's choice corpus).
fn universe(battle: &Battle, s: usize) -> Vec<String> {
    let size = battle.sides[s].mons.len();
    if battle.request == ninjask::state::RequestState::TeamPreview {
        let mut out = Vec::new();
        fn walk(n: usize, prefix: &mut String, out: &mut Vec<String>) {
            if !prefix.is_empty() {
                out.push(format!("team {prefix}"));
            }
            if prefix.len() == 4 {
                return;
            }
            for i in 1..=n {
                let c = char::from(b'0' + i as u8);
                if !prefix.contains(c) {
                    prefix.push(c);
                    walk(n, prefix, out);
                    prefix.pop();
                }
            }
        }
        walk(size, &mut String::new(), &mut out);
        return out;
    }
    let mut slot = vec!["pass".to_string()];
    for m in 1..=4 {
        for target in ["", " 1", " 2", " -1", " -2"] {
            for event in ["", " mega", " megax", " megay", " terastallize"] {
                slot.push(format!("move {m}{target}{event}"));
            }
        }
    }
    for i in 1..=size {
        slot.push(format!("switch {i}"));
    }
    slot.iter()
        .flat_map(|a| slot.iter().map(move |b| format!("{a}, {b}")))
        .collect()
}

#[test]
fn the_candidates_are_a_function_of_the_request_alone() {
    let (dex, teams) = (dex(), teams());
    let mut batch = Batch::with_options(dex, 1, options()).unwrap();
    let [a, b] = pair(&teams, 2);
    batch
        .reset_seeded(0, [a, b], &showdown_seed(2), None)
        .unwrap();
    let mut policy = Rng::new(9);
    for _ in 0..30 {
        let pending = batch.pending();
        if pending.is_empty() {
            break;
        }
        for &(_, s) in &pending {
            let battle = batch.battle(0).unwrap().clone();
            let request = battle.request_json(s).unwrap();
            assert_eq!(
                batch.candidates(0, s).unwrap(),
                candidates_from_request(&request).unwrap()
            );
            assert_eq!(
                batch.candidates(0, s).unwrap(),
                battle.candidates(s).unwrap()
            );
        }
        let picks: Vec<_> = pending
            .iter()
            .map(|&(i, s)| {
                (
                    i,
                    s,
                    policy.random(batch.candidates(i, s).unwrap().len() as u32) as usize,
                )
            })
            .collect();
        batch.choose_candidates(&picks).unwrap();
    }
}

#[test]
fn recording_the_draws_does_not_change_the_game_and_forcing_them_replays_it() {
    let (dex, teams) = (dex(), teams());
    for k in 0..4 {
        let [a, b] = pair(&teams, k);
        let seed = showdown_seed(k as u64 + 40);
        let recording = BatchOptions {
            record_draws: true,
            ..options()
        };
        let mut recorded = Batch::with_options(dex.clone(), 1, recording).unwrap();
        recorded.reset_seeded(0, [a, b], &seed, None).unwrap();
        let mut plain = Batch::with_options(dex.clone(), 1, options()).unwrap();
        plain.reset_seeded(0, [a, b], &seed, None).unwrap();
        let mut policy = Rng::new(k as u64);
        let mut steps: Vec<Step> = vec![(vec![], recorded.take_draws(0).unwrap())];
        for _ in 0..300 {
            let pending = recorded.pending();
            if pending.is_empty() {
                break;
            }
            let picks: Vec<(usize, usize, String)> = pending
                .iter()
                .map(|&(i, s)| {
                    let list = recorded.candidates(i, s).unwrap();
                    (
                        i,
                        s,
                        list[policy.random(list.len() as u32) as usize]
                            .choice
                            .clone(),
                    )
                })
                .collect();
            let refs: Vec<(usize, usize, &str)> =
                picks.iter().map(|(i, s, c)| (*i, *s, c.as_str())).collect();
            recorded.choose(&refs).unwrap();
            plain.choose(&refs).unwrap();
            steps.push((picks, recorded.take_draws(0).unwrap()));
        }
        assert_eq!(lines(&recorded, 2), lines(&plain, 2), "game {k}");
        assert!(plain.take_draws(0).unwrap().is_empty());
        // The draws are the four calls of Showdown's generator, nothing else.
        let all: Vec<&Draw> = steps.iter().flat_map(|(_, d)| d).collect();
        assert!(all.len() > 50);
        assert!(all
            .iter()
            .all(|d| ["random", "randomChance", "sample", "shuffle"].contains(&d.method.as_str())));
        // A battle on the fast generator, forced with them, plays the same game.
        let mut forced = Batch::with_options(
            dex.clone(),
            1,
            BatchOptions {
                record_draws: true,
                ..options()
            },
        )
        .unwrap();
        forced
            .reset_seeded(0, [a, b], &Seed::Fast(99), Some(steps[0].1.as_slice()))
            .unwrap();
        assert_eq!(forced.take_draws(0).unwrap(), steps[0].1);
        for (picks, draws) in &steps[1..] {
            forced.set_forced_draws(0, draws).unwrap();
            let refs: Vec<(usize, usize, &str)> =
                picks.iter().map(|(i, s, c)| (*i, *s, c.as_str())).collect();
            forced.choose(&refs).unwrap();
            assert_eq!(&forced.take_draws(0).unwrap(), draws, "game {k}");
        }
        assert_eq!(lines(&forced, 2), lines(&recorded, 2), "game {k}");
        assert_eq!(lines(&forced, 0), lines(&recorded, 0), "game {k}");
    }
}

#[test]
fn observe_and_step_still_work_beside_channels() {
    let (dex, teams) = (dex(), teams());
    let mut batch = Batch::with_options(dex, 3, options()).unwrap();
    for i in 0..3 {
        let [a, b] = pair(&teams, i);
        batch
            .reset_seeded(i, [a, b], &showdown_seed(i as u64 + 5), None)
            .unwrap();
    }
    let mut policy = Rng::new(1);
    for _ in 0..500 {
        let o = batch.observe();
        let m = o.offsets.len() - 1;
        if m == 0 {
            break;
        }
        assert_eq!(batch.pending().len(), m);
        let picks: Vec<(usize, usize, usize)> = (0..m)
            .map(|k| {
                let n = o.offsets[k + 1] - o.offsets[k];
                (
                    o.who[2 * k] as usize,
                    o.who[2 * k + 1] as usize,
                    policy.random(n) as usize,
                )
            })
            .collect();
        batch.step(&picks).unwrap();
    }
    for i in 0..3 {
        let all = batch.channel(i, 2, 0).unwrap();
        assert!(
            all.last().unwrap().starts_with("|win|")
                || batch.battle(i).unwrap().turn >= 150
                || all.last().unwrap() == "|tie"
        );
    }
}
