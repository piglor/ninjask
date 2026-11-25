"""Plays fresh battles on a pool of teams with random candidates (pending ones included) and writes every request point
of both players, with the engine's candidates, as JSON lines on stdout (the input of compare_candidates.mjs).

    python3 scripts/areneon/dump_requests.py POOL.txt BATTLES [FIRST_SEED] > requests.jsonl

POOL.txt is a file of packed teams, one per line (tests/fixtures/areneon/teams.txt is one). The format export is
$NINJASK_DEX (default tests/fixtures/areneon/dex-gen9championsvgc2026regmc-7244fe8e.json)."""
import json, os, random, sys
from pathlib import Path
import ninjask
DEFAULT_DEX = Path(__file__).resolve().parents[2] / 'tests' / 'fixtures' / 'areneon' / 'dex-gen9championsvgc2026regmc-7244fe8e.json'
dex = ninjask.Dex.load(os.environ.get('NINJASK_DEX', str(DEFAULT_DEX)))
pool = [l for l in open(sys.argv[1]).read().split('\n') if l]
battles = int(sys.argv[2]); base = int(sys.argv[3]) if len(sys.argv) > 3 else 0
NAMES = ('p1', 'p2')
out = sys.stdout
n = 0
for g in range(base, base + battles):
    rng = random.Random(g)
    b = ninjask.Batch(dex, 1, names=NAMES, channels=True)
    b.reset(0, rng.choice(pool), rng.choice(pool), 'sodium,' + f'{rng.getrandbits(128):032x}')
    seen = [0, 0]
    for _ in range(600):
        # every request point of both players (waits included)
        for s in (0, 1):
            new = b.channel(0, NAMES[s], seen[s]); seen[s] += len(new)
            for line in new:
                if line.startswith('|request|'):
                    pass
        pend = b.pending()
        if not pend: break
        picks = []
        for (i, s) in pend:
            c = b.candidates(i, s)
            out.write(json.dumps({'request': b.channel(0, NAMES[s], b.channel_len(0, NAMES[s]) - 1)[0][len('|request|'):], 'candidates': c}, separators=(',', ':')) + '\n')
            n += 1
            picks.append((i, s, rng.randrange(len(c))))
        # the waiting side's request, too
        for s in (0, 1):
            if (0, s) not in pend:
                last = b.channel(0, NAMES[s], b.channel_len(0, NAMES[s]) - 1)[0]
                if last.startswith('|request|'):
                    out.write(json.dumps({'request': last[len('|request|'):], 'candidates': b.candidates(0, s)}, separators=(',', ':')) + '\n'); n += 1
        b.choose_candidates(picks)
        b.discard_channel(0, 'omniscient', b.channel_len(0, 'omniscient'))
print(n, 'requests', file=sys.stderr)
