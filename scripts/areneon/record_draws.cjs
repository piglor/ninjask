// Replays recorded battles (seed, packed teams, choices) on the pinned Showdown and records every call on its random source
// (the recorder of packages/showdown-bridge/src/battle-trace.ts of Areneon, reproduced): method, arguments, result, per
// write (the two `>player` lines, then each choice in order). The output is the draw trace scripts/check_recorded.py
// reads with --draws.
//
//   node scripts/areneon/record_draws.cjs RECORDING.jsonl.gz DRAWS.jsonl.gz [LIMIT [FORMAT]]
//
// The Showdown checkout is $NINJASK_SHOWDOWN (default third_party/pokemon-showdown), built.
'use strict';
const fs = require('fs');
const zlib = require('zlib');
const readline = require('readline');
const PS = require('../../refsim/showdown_path').showdownPath();
const { BattleStream, Teams } = require(PS + '/dist/sim');

const [,, recFile, outFile, limitArg, formatId = 'gen9championsvgc2026regmc'] = process.argv;
const limit = Number(limitArg || '0');
const PASS = new Set(['getSeed', 'clone']);
const text = (v) => (typeof v === 'string' ? v : String(v));
const indices = (n) => Array.from({ length: n }, (_, i) => i);

function recordingSource(target, log) {
  return new Proxy(target, {
    get(t, name) {
      const value = Reflect.get(t, name);
      if (typeof name !== 'string' || typeof value !== 'function' || PASS.has(name)) return typeof value === 'function' ? value.bind(t) : value;
      switch (name) {
        case 'random':
          return (...args) => {
            const given = args.filter((a) => a !== undefined);
            const r = t.random(...args);
            log().push({ method: name, args: given.map(text), result: [text(r)] });
            return r;
          };
        case 'randomChance':
          return (n, d) => {
            const r = t.randomChance(n, d);
            log().push({ method: name, args: [text(n), text(d)], result: [text(r)] });
            return r;
          };
        case 'sample':
          return (items) => {
            const idx = t.sample(indices(items.length));
            log().push({ method: name, args: [text(items.length)], result: [text(idx)] });
            return items[idx];
          };
        case 'shuffle':
          return (items, ...range) => {
            const order = indices(items.length);
            t.shuffle(order, ...range);
            const given = range.filter((a) => a !== undefined);
            log().push({ method: name, args: [text(items.length), ...given.map(text)], result: order.map(text) });
            const before = [...items];
            order.forEach((from, i) => { items[i] = before[from]; });
          };
        default:
          return (...args) => {
            const r = value.apply(t, args);
            log().push({ method: name, args: args.map(text), result: [text(r)] });
            return r;
          };
      }
    },
  });
}

(async () => {
  const rl = readline.createInterface({ input: fs.createReadStream(recFile).pipe(zlib.createGunzip()) });
  const out = zlib.createGzip(); const sink = fs.createWriteStream(outFile); out.pipe(sink);
  let n = 0;
  for await (const line of rl) {
    if (limit && n >= limit) break;
    const row = JSON.parse(line);
    if (row.failure) continue;
    n++;
    let cur = [];
    const stream = new BattleStream();
    // Intercept messages synchronously to know where the write ended.
    stream._write.bind(stream);
    stream._writeLines(`>start ${JSON.stringify({ formatid: formatId, seed: row.seed })}`);
    stream.battle.prng = recordingSource(stream.battle.prng, () => cur);
    const writes = [];
    const w = (kind, payload) => { cur = []; try { stream._writeLines(payload); } catch (e) { writes.push({ kind, error: String(e) }); return; } stream.battle.sendUpdates(); writes.push({ kind, draws: cur }); };
    w('player', `>player p1 ${JSON.stringify({ name: 'p1', team: row.packed.p1 })}`);
    w('player', `>player p2 ${JSON.stringify({ name: 'p2', team: row.packed.p2 })}`);
    for (const c of row.choices) w(c.player, `>${c.player} ${c.choice}`);
    out.write(JSON.stringify({ index: row.index, seed: row.seed, writes }) + '\n');
  }
  out.end();
  await new Promise((r) => sink.on('finish', r));
  console.error('recorded', n);
})().catch((e) => { console.error(e); process.exit(1); });
