"""Build tests/fixtures/areneon/battles.jsonl.gz from the recorded corpus: select a varied subset (every battle with a
refused choice, a few of each hint, trap, disable, Illusion and policy), record Showdown's draws for them and write the
rows. The recording files are made by record_battles.ts.

    python3 scripts/areneon/build_fixture.py RECORDINGS_DIR OUT.jsonl.gz

RECORDINGS_DIR holds rl-2000-a, rl-2000-b, ua-500-a, ua-500-b, pf-500-a, pf-500-b, illusion-200, rl-500-0 and rl-500-1
(`.jsonl.gz`). The selection skips battles with No Retreat or Revival Blessing: they were made before the engine
followed the pin (#1531), and the full 3,200 are checked by check_recorded.py instead."""
import gzip, json, sys, collections, subprocess, os
import pathlib
REC = sys.argv[1].rstrip('/') + '/'
OUT = sys.argv[2]
HERE = pathlib.Path(__file__).resolve().parent
files = ['rl-2000-a','rl-2000-b','ua-500-a','ua-500-b','pf-500-a','pf-500-b','illusion-200','rl-500-0','rl-500-1']
HINTS = {'skillswap': 'Skill Swap does not announce','futuresight': 'Future Sight did not hit','prankster': 'Dark is immune to Prankster',
         'risingvoltage': "Rising Voltage's BP doubled",'psychicterrain': "Psychic Terrain doesn't affect"}
rows = {}
tags = collections.defaultdict(list)
for fn in files:
    for line in gzip.open(REC+fn+'.jsonl.gz','rt'):
        row = json.loads(line)
        if 'failure' in row or row.get('forfeit'): continue
        om = row['omniscient_log']
        if any('move: No Retreat' in l or 'Revival Blessing' in l for l in om): continue
        if not om[-1].startswith('|win|'): continue
        key = (fn, row['index'])
        row['_fn'] = fn
        n = len(om)
        t = []
        for k, txt in HINTS.items():
            if any(txt in l for l in om): t.append('hint-'+k)
        if any(c.get('refused') for c in row['choices']): t.append('refused')
        reqs = [l for l in row['channels']['p1'] if l.startswith('|request|')]
        if any('"maybeTrapped"' in l for l in reqs): t.append('maybeTrapped')
        if any('"maybeDisabled"' in l for l in reqs): t.append('maybeDisabled')
        if any('|trapped' in l for l in om): t.append('trapped')
        if fn.startswith('illusion'): t.append('illusion')
        if fn.startswith('rl-'): t.append('plain')
        if fn.startswith('ua-'): t.append('uniform')
        if fn.startswith('pf-'): t.append('pending-first')
        for x in t: tags[x].append((n, key))
        rows[key] = row
want = {'refused': 10, 'hint-skillswap': 3, 'hint-futuresight': 3, 'hint-prankster': 3, 'hint-risingvoltage': 3, 'hint-psychicterrain': 2,
        'maybeTrapped': 3, 'maybeDisabled': 3, 'trapped': 3, 'illusion': 2, 'plain': 8, 'uniform': 3, 'pending-first': 3}
picked = []
for t, k in want.items():
    got = 0
    for n, key in sorted(tags[t]):
        if key in picked: continue
        picked.append(key); got += 1
        if got >= k: break
print('picked', len(picked))
sel = os.path.join(os.path.dirname(OUT), 'selected-tmp.jsonl.gz')
with gzip.open(sel, 'wt') as f:
    for key in picked: f.write(json.dumps(rows[key]) + '\n')
draws_out = sel.replace('selected-tmp', 'draws-tmp')
subprocess.run(['node', str(HERE / 'record_draws.cjs'), sel, draws_out], check=True)
draws = {}
for line in gzip.open(draws_out, 'rt'):
    r = json.loads(line)
    draws[(r['seed'])] = r['writes']
with gzip.open(OUT, 'wt') as f:
    for key in picked:
        r = rows[key]
        w = draws[r['seed']]
        assert all('error' not in x for x in w), r['index']
        out = {
            'id': f"{key[0]}:{r['index']}", 'mix': r['mix'], 'seed': r['seed'], 'packed': r['packed'], 'turns': r['turns'], 'winner': r['winner'],
            'choices': r['choices'],
            'channels': r['channels'], 'omniscient': r['omniscient_log'],
            'requests': {s: [{k: q[k] for k in ('request_seq','phase','request_hash','legal') if k in q} for q in r['requests'][s]] for s in ('p1','p2')},
            # Showdown's calls on its random source, per write: the two `>player` lines, then every choice in order.
            'draws': [x['draws'] for x in w],
        }
        f.write(json.dumps(out, separators=(',',':'), ensure_ascii=False) + '\n')
os.remove(sel); os.remove(draws_out)
print(os.path.getsize(OUT))
