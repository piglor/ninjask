#!/usr/bin/env python3
"""Write tests/fixtures/areneon/SOURCE.json: which Areneon sources the candidate enumerator was ported from and the
fixtures were taken from, by commit and SHA-256.

    python3 scripts/areneon/write_source_manifest.py /path/to/areneon [REF]

REF (default origin/main) is read with `git show`, so the working tree of the checkout does not matter. The manifest
names the Areneon commit, the reference enumerator (`packages/showdown-bridge/src/choices.ts`), the engine-request
schema its input follows (`packages/schema/contracts/v1/engine-request.schema.json`) and every file of the golden
choice corpus (`fixtures/showdown/<format>/choices/`). tests/python/test_n1_source.py holds the committed fixtures
equal to it, and, with AREONEON_ROOT set, holds a checkout equal to it: when the reference changes, the test fails and
the evidence of docs/n1-seeds-channels-candidates.md has to be run again before the manifest is rewritten.
"""
import hashlib
import json
import subprocess
import sys
from pathlib import Path

FORMAT = "gen9championsvgc2026regmc"
CHOICES_TS = "packages/showdown-bridge/src/choices.ts"
SCHEMA = "packages/schema/contracts/v1/engine-request.schema.json"
CORPUS = f"fixtures/showdown/{FORMAT}/choices"
OUT = Path(__file__).resolve().parents[2] / "tests" / "fixtures" / "areneon" / "SOURCE.json"


def git(root, *args):
    return subprocess.run(["git", "-C", root, *args], check=True, capture_output=True).stdout


def sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def main():
    root = sys.argv[1]
    ref = sys.argv[2] if len(sys.argv) > 2 else "origin/main"
    commit = git(root, "rev-parse", ref).decode().strip()

    def last_changed(path):
        return git(root, "log", "-1", "--format=%H", ref, "--", path).decode().strip()

    corpus = {}
    for name in git(root, "ls-tree", "--name-only", ref, CORPUS + "/").decode().split():
        corpus[Path(name).name] = sha(git(root, "show", f"{ref}:{name}"))
    manifest = {
        "areneon_commit": commit,
        "format": FORMAT,
        "choices_ts": {"path": CHOICES_TS, "last_changed_in": last_changed(CHOICES_TS), "sha256": sha(git(root, "show", f"{ref}:{CHOICES_TS}"))},
        "engine_request_schema": {"path": SCHEMA, "last_changed_in": last_changed(SCHEMA), "sha256": sha(git(root, "show", f"{ref}:{SCHEMA}"))},
        "golden_choices": {"dir": CORPUS, "last_changed_in": last_changed(CORPUS), "files": dict(sorted(corpus.items()))},
    }
    OUT.write_text(json.dumps(manifest, indent=1, sort_keys=False) + "\n")
    print(f"{OUT}: {len(corpus)} corpus files at {commit}")


if __name__ == "__main__":
    main()
