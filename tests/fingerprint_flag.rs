//! `examples/fingerprint --skip-sort-entries` hashes the numbers drawn without the `# sort` trace
//! entries, so "the draws are unchanged" between two builds is reproducible from committed code
//! (N1: `main` and this branch print the same draws hash that way, `1269806311788d01`).

use std::process::Command;

fn run(extra: &[&str]) -> String {
    let mut cmd = Command::new(env!("CARGO"));
    cmd.args([
        "run",
        "--release",
        "--locked",
        "--quiet",
        "--example",
        "fingerprint",
        "--",
    ])
    .arg("data/derived/format/gen9championsvgc2026regmc.json")
    .arg("tests/fixtures/regmc_teams.txt")
    .arg("2000");
    cmd.args(extra);
    let out = cmd.output().expect("run the fingerprint example");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap()
}

fn line<'a>(out: &'a str, key: &str) -> &'a str {
    out.lines()
        .find(|l| l.starts_with(key))
        .unwrap_or_else(|| panic!("no {key} line"))
}

#[test]
fn skipping_sort_entries_leaves_logs_and_requests_and_pins_the_numbers_drawn() {
    if !std::path::Path::new("data/derived/format/gen9championsvgc2026regmc.json").exists() {
        eprintln!("skipped: the Reg M-C export is not built");
        return;
    }
    let full = run(&[]);
    let skip = run(&["--skip-sort-entries"]);
    assert_eq!(line(&full, "logs "), line(&skip, "logs "));
    assert_eq!(line(&full, "requests "), line(&skip, "requests "));
    assert_ne!(
        line(&full, "draws "),
        line(&skip, "draws "),
        "the flag must change the draws entry"
    );
    assert_eq!(line(&skip, "draws "), "draws 1269806311788d01");
    assert_ne!(line(&full, "fingerprint "), line(&skip, "fingerprint "));
}
