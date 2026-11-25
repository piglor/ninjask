#!/usr/bin/env python3
"""Copy chosen battles out of a recording (and out of its draw trace) into a fixture file.

    python3 scripts/areneon/extract_rows.py --rec REC.jsonl.gz --draws DRAWS.jsonl.gz --name ua-500-a 152 \\
        --out tests/fixtures/areneon/gaps.jsonl.gz --out-draws tests/fixtures/areneon/gaps-draws.jsonl.gz [--append]

`--rec` is a JSON Lines file of recorded battles (the row format of tests/fixtures/areneon/README.md; `omniscient_log`
is renamed `omniscient`, the unused `teams` is dropped), `--draws` the draw trace made by scenario.cjs or the harness's
recorder for the same battles in the same order. Each battle gets the id `NAME:INDEX`.
"""
import argparse
import gzip
import json


def opener(path, mode="rt"):
    return gzip.open(path, mode) if str(path).endswith(".gz") else open(path, mode)


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--rec", required=True)
    ap.add_argument("--draws", required=True)
    ap.add_argument("--name", required=True, help="prefix of the ids (the recording's name)")
    ap.add_argument("--out", required=True)
    ap.add_argument("--out-draws", required=True)
    ap.add_argument("--append", action="store_true")
    ap.add_argument("indices", nargs="+", type=int)
    a = ap.parse_args()
    want = set(a.indices)
    rows, traces = [], []
    with opener(a.rec) as rec, opener(a.draws) as draws:
        for k, (line, dline) in enumerate(zip(rec, draws)):
            if k not in want:
                continue
            row, trace = json.loads(line), json.loads(dline)
            assert row["seed"] == trace["seed"], f"battle {k}: the draw trace is for another battle"
            row["id"] = f"{a.name}:{k}"
            row.pop("teams", None)
            if "omniscient_log" in row:
                row["omniscient"] = row.pop("omniscient_log")
            trace["id"] = row["id"]
            rows.append(row)
            traces.append(trace)
    mode = "at" if a.append else "wt"
    with opener(a.out, mode) as f:
        for row in rows:
            f.write(json.dumps(row, separators=(",", ":")) + "\n")
    with opener(a.out_draws, mode) as f:
        for trace in traces:
            f.write(json.dumps(trace, separators=(",", ":")) + "\n")
    print(f"{len(rows)} battles")


if __name__ == "__main__":
    main()
