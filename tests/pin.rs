//! The pin: `refsim/SHOWDOWN_COMMIT` is the one source of truth for the Showdown
//! commit the engine is a port of; every other place that names it is held equal
//! here, so a pin bump that forgets one fails the build (docs/repin.md).
//! The checks on the format exports need the export (`data/derived/format/`, see
//! `common`; skipped without it); the others read files of this checkout.

mod common;

use std::path::Path;

fn read(path: &str) -> String {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(root.join(path)).unwrap_or_else(|e| panic!("{path}: {e}"))
}

fn is_commit(s: &str) -> bool {
    s.len() == 40
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

#[test]
fn the_compiled_pin_is_the_pin_file() {
    assert!(
        is_commit(ninjask::SHOWDOWN_COMMIT),
        "{}",
        ninjask::SHOWDOWN_COMMIT
    );
    assert_eq!(
        read("refsim/SHOWDOWN_COMMIT"),
        format!("{}\n", ninjask::SHOWDOWN_COMMIT)
    );
}

#[test]
fn the_manifests_name_the_pin() {
    let cargo = read("Cargo.toml");
    assert!(
        cargo.contains(&format!(
            "showdown_commit = \"{}\"",
            ninjask::SHOWDOWN_COMMIT
        )),
        "Cargo.toml [package.metadata.areneon] showdown_commit"
    );
    let provenance: serde_json::Value =
        serde_json::from_str(&read("refsim/PROVENANCE.json")).unwrap();
    assert_eq!(provenance["showdown"]["commit"], ninjask::SHOWDOWN_COMMIT);
    assert!(
        read("NOTICE").contains(&format!("commit {}", ninjask::SHOWDOWN_COMMIT)),
        "NOTICE"
    );
}

#[test]
fn this_is_the_areneon_fork_of_the_release_it_is_based_on() {
    let version = env!("CARGO_PKG_VERSION");
    assert!(version.contains("+areneon."), "{version}");
    assert!(read("pyproject.toml").contains(&format!("version = \"{version}\"")));
}

#[test]
fn the_format_exports_are_from_the_pin() {
    // `format_export` itself fails on an export of another commit.
    let Some(text) = common::format_export("gen9championsvgc2026regmc") else {
        return;
    };
    assert_eq!(common::export_commit(&text), Some(ninjask::SHOWDOWN_COMMIT));
}
