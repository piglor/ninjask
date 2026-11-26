//! The declared-state layer (`ninjask::declared`) against the Pokémon Showdown mechanics probe it stands in for.
//!
//! `tests/fixtures/declared/cases.jsonl` holds queries in the layer's own form, each with the answer the pinned probe gave it
//! (made by Areneon's `native-probe-cases.ts`, which refuses to write a case the layer answers differently): the hand-worked damage
//! fixtures, targeted cases for every class of difference found (Protean and Libero, a weather's forme change, a priority move under
//! Psychic Terrain, Armor Tail, Damp, a charge move in its weather, Beat Up, a draw), strides over the interaction, move and speed
//! sweeps and over the queries a feature builder issues. Needs the format export (`common`); skipped without it.

mod common;

use std::sync::Arc;

use ninjask::declared::{wire, Outcome, Prober, Query, SetSpec};
use ninjask::Dex;
use serde_json::{json, Value};

fn prober() -> Option<Prober> {
    let json = common::format_export("gen9championsvgc2026regmc")?;
    let mut p = Prober::new(Arc::new(
        Dex::from_json(&json).expect("the format export loads"),
    ));
    // The mechanics config's `ko_blockers`, which the cases' KO counts depend on.
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/declared/blockers.json"
    );
    let b: Value =
        serde_json::from_str(&std::fs::read_to_string(path).expect("the blockers")).unwrap();
    let list = |k: &str| -> std::collections::HashSet<String> {
        b[k].as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_string())
            .collect()
    };
    p.set_ko_blockers(ninjask::declared::KoBlockers {
        items: list("items"),
        abilities: list("abilities"),
    });
    Some(p)
}

struct Case {
    id: String,
    sets: Vec<SetSpec>,
    query: Query,
    expected: Value,
}

fn cases() -> Vec<Case> {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/declared/cases.jsonl"
    );
    std::fs::read_to_string(path)
        .expect("the case file")
        .lines()
        .map(|line| {
            let v: Value = serde_json::from_str(line).expect("a case is JSON");
            Case {
                id: v["id"].as_str().unwrap().to_string(),
                sets: serde_json::from_value(v["sets"].clone()).expect("the sets"),
                query: serde_json::from_value(v["query"].clone()).expect("the query"),
                expected: v["expected"].clone(),
            }
        })
        .collect()
}

/// Register a case's sets at 0.. (the case numbers them so) and answer it.
fn answer(p: &mut Prober, case: &Case) -> Value {
    for (i, set) in case.sets.iter().enumerate() {
        p.register_set(i as u32, set)
            .unwrap_or_else(|e| panic!("{}: {e}", case.id));
    }
    serde_json::to_value(p.answer(&case.query)).unwrap()
}

#[test]
fn every_case_equals_the_probe() {
    let Some(mut p) = prober() else { return };
    let cases = cases();
    assert!(cases.len() > 500, "{} cases", cases.len());
    let (mut computed, mut unsupported, mut abstain) = (0, 0, 0);
    for case in &cases {
        let got = answer(&mut p, case);
        match case.expected["kind"].as_str().unwrap() {
            // The probe refuses (it throws): the layer must not answer, whatever it names.
            "abstain" => {
                assert_eq!(got["kind"], "abstain", "{}: {got}", case.id);
                abstain += 1;
            }
            kind => {
                assert_eq!(got, case.expected, "{}", case.id);
                if kind == "computed" {
                    computed += 1;
                } else {
                    unsupported += 1;
                }
            }
        }
    }
    // The file holds each class: answers, the probe's own `unsupported`, and the probe's refusals.
    assert!(
        computed > 300 && unsupported > 50 && abstain > 5,
        "{computed} {unsupported} {abstain}"
    );
}

#[test]
fn an_answer_does_not_depend_on_the_kept_starts_or_on_the_order_of_a_batch() {
    let Some(mut p) = prober() else { return };
    let cases = cases();
    // Cold: a fresh prober answers each case as the warm one did, in reverse order too.
    let warm: Vec<Value> = cases.iter().map(|c| answer(&mut p, c)).collect();
    let mut cold = prober().unwrap();
    for (i, case) in cases.iter().enumerate().rev() {
        assert_eq!(answer(&mut cold, case), warm[i], "{}", case.id);
    }
}

#[test]
fn a_query_with_a_field_the_layer_does_not_implement_is_refused_as_a_frame_not_read_without_it() {
    let Some(mut p) = prober() else { return };
    // `history` is not a field of the layer: the whole frame is refused, so the caller routes every query of it.
    let frame = json!({"op": "batch", "queries": [{"kind": "stats", "set": 0, "history": {}}]});
    let out: Value =
        serde_json::from_slice(&wire::serve(&mut p, frame.to_string().as_bytes())).unwrap();
    assert!(
        out["error"].as_str().unwrap().contains("not a request"),
        "{out}"
    );
    // Not JSON at all, an unknown op, a query of an unknown kind: errors, never a panic.
    for bad in [
        "",
        "not json",
        "{\"op\":\"nope\"}",
        "{\"op\":\"batch\",\"queries\":[{\"kind\":\"move_outcome\"}]}",
    ] {
        let out: Value = serde_json::from_slice(&wire::serve(&mut p, bad.as_bytes())).unwrap();
        assert!(out.get("error").is_some(), "{bad}: {out}");
    }
}

#[test]
fn a_set_the_dex_does_not_know_is_refused_and_a_query_over_it_abstains() {
    let Some(mut p) = prober() else { return };
    let set = json!({"species": "Nopemon", "ability": "Blaze", "moves": ["Tackle"], "nature": "Hardy", "points": [0, 0, 0, 0, 0, 0]});
    let out: Value = serde_json::from_slice(&wire::serve(
        &mut p,
        json!({"op": "sets", "sets": [[0, set]]})
            .to_string()
            .as_bytes(),
    ))
    .unwrap();
    assert_eq!(out["failed"][0][0], 0);
    assert!(
        out["failed"][0][1]
            .as_str()
            .unwrap()
            .contains("unknown species"),
        "{out}"
    );
    let query: Query = serde_json::from_value(json!({"kind": "stats", "set": 0})).unwrap();
    assert!(matches!(p.answer(&query), Outcome::Abstain { .. }));
}

#[test]
fn the_counts_name_each_abstention_by_entity() {
    let Some(mut p) = prober() else { return };
    let cases = cases();
    for case in cases
        .iter()
        .filter(|c| c.expected["kind"] == "abstain")
        .take(8)
    {
        answer(&mut p, case);
    }
    let out: Value = serde_json::from_slice(&wire::serve(&mut p, br#"{"op":"counts"}"#)).unwrap();
    assert_eq!(out["counts"]["answered"], 0);
    assert!(out["counts"]["abstained"].as_u64().unwrap() >= 8);
    assert!(!out["counts"]["by_entity"].as_object().unwrap().is_empty());
}
