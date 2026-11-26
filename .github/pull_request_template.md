## What this changes

## Checks

- [ ] `cargo fmt --check`, `cargo clippy` (with and without `--features python-module`), `cargo test --release` and `pytest -q` pass
- [ ] `check_reference.py --synthetic --games 200` matches every game, and `--require ID` for each entity changed
- [ ] `python scripts/repin.py provenance check` passes (after a deliberate pin or export change: `provenance write`, reviewed)
- [ ] Fingerprint: unchanged (`91fb64fa6e58900a`), or changed because behaviour changed, and that change matches Showdown (explained above)
- [ ] `CHANGELOG.md` has a line under "Unreleased", if a user would notice
