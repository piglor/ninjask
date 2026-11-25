//! `Dex::from_json` takes only an export of the pinned Showdown commit: the engine is a port of
//! that commit (docs/repin.md), so game data from another one would be played as if it were the
//! pin's. `Dex::from_json_unpinned` is the explicit override, for a hand-made export (a test
//! fixture) or one of another commit that someone has checked. Hermetic: the exports here are the
//! committed fixture with the commit written into its format block.

use ninjask::{Dex, SHOWDOWN_COMMIT};
use serde_json::Value;

/// The Showdown commit the engine was ported from before the pin of this fork's sync.
const PREVIOUS_PIN: &str = "3a766f2cfb5e51c1e428cfe87d0dba91a8f01640";

const FIXTURE: &str = include_str!("fixtures/enumeration.json");

/// The fixture as an export that says it is from `commit` (none: as it is).
fn export_from(commit: Option<&str>) -> String {
    let mut root: Value = serde_json::from_str(FIXTURE).unwrap();
    if let Some(c) = commit {
        root["format"]["showdownCommit"] = Value::from(c);
    }
    root.to_string()
}

#[test]
fn an_export_of_the_pin_loads() {
    let dex = Dex::from_json(&export_from(Some(SHOWDOWN_COMMIT))).unwrap();
    assert_eq!(dex.format.showdown_commit.as_deref(), Some(SHOWDOWN_COMMIT));
}

#[test]
fn an_export_of_another_commit_is_refused() {
    // The 3a766f2 export (the pin before the sync to 7244fe8e) under this engine.
    let err = Dex::from_json(&export_from(Some(PREVIOUS_PIN))).unwrap_err();
    assert!(err.contains(PREVIOUS_PIN), "{err}");
    assert!(err.contains(SHOWDOWN_COMMIT), "{err}");
    assert!(err.contains("from_json_unpinned"), "{err}");
}

#[test]
fn an_export_that_names_no_commit_is_refused() {
    // Every export of refsim/export_format.js names its commit: a file that does not is hand-made.
    let err = Dex::from_json(&export_from(None)).unwrap_err();
    assert!(err.contains("no Showdown commit"), "{err}");
    assert!(err.contains("from_json_unpinned"), "{err}");
}

#[test]
fn a_malformed_commit_is_refused_like_another_one() {
    for bad in ["", "7244fe8e", "7244FE8E28FA8BC4D0809AE30D183C608B62BB2B"] {
        let err = Dex::from_json(&export_from(Some(bad))).unwrap_err();
        assert!(err.contains(SHOWDOWN_COMMIT), "{bad}: {err}");
    }
    let mut root: Value = serde_json::from_str(FIXTURE).unwrap();
    root["format"]["showdownCommit"] = Value::from(7);
    assert!(Dex::from_json(&root.to_string()).is_err());
}

#[test]
fn the_unpinned_loader_takes_any_export_and_reports_its_commit() {
    let hand_made = Dex::from_json_unpinned(&export_from(None)).unwrap();
    assert_eq!(hand_made.format.showdown_commit, None);
    let previous = Dex::from_json_unpinned(&export_from(Some(PREVIOUS_PIN))).unwrap();
    assert_eq!(previous.format.showdown_commit.as_deref(), Some(PREVIOUS_PIN));
}

#[test]
fn the_unpinned_loader_is_not_a_way_around_a_broken_file() {
    assert!(Dex::from_json_unpinned("{").is_err());
    assert!(Dex::from_json("{").is_err());
}
