// Records seeded Showdown battles with Areneon's runner (`runBattle` of packages/showdown-bridge on the pinned engine)
// with everything needed to replay the same choices into Ninjask and compare: seed, teams (sets and Showdown packed
// form), the submitted choices in order (refusals included), each player's requests with the reference enumerator's
// legal sets, each player's own channel, the omniscient log.
//
//   AREONEON_ROOT=/path/to/areneon node scripts/areneon/record_battles.ts --mix random-legal --start 0 --battles 500 \
//       --run-key ninjask-eval-1 --dataset DIR --dataset-hash HEX64 --out FILE.jsonl.gz
//
// AREONEON_ROOT is a checkout of the Areneon monorepo at the commit named in tests/fixtures/areneon/SOURCE.json (or
// later, with the evidence of docs/n1-seeds-channels-candidates.md re-run), with its Showdown submodule built. Needs
// Node 24 (type stripping). Read-only on that checkout; the teams are the synthetic dev and training splits of a
// dataset made with a public key (`--dataset`), never anything private. `--mix` is the actor of both seats:
// `random-legal`, `pending-first` or `uniform-all` (every candidate, pending ones included).
import { createWriteStream } from "node:fs";
import { createGzip } from "node:zlib";
import { once } from "node:events";
import { parseArgs } from "node:util";

const root = process.env.AREONEON_ROOT;
if (!root) throw new Error("set AREONEON_ROOT to a checkout of the Areneon monorepo");
const R = `${root}/packages`;

const bridge = await import(`${R}/showdown-bridge/src/index.ts`);
const sweep = await import(`${R}/team-dataset/src/reducer-sweep.ts`);
const poolMod = await import(`${R}/team-dataset/src/loop-pool.ts`);

const { values } = parseArgs({
  options: {
    mix: { type: "string" }, start: { type: "string" }, battles: { type: "string" }, "run-key": { type: "string" },
    out: { type: "string" }, dataset: { type: "string" }, "dataset-hash": { type: "string" }, "max-turns": { type: "string" },
    "exclude": { type: "string" }, "only-ability": { type: "string" },
  },
  strict: true,
});
const mix = values.mix ?? "random-legal";
const start = Number(values.start ?? "0");
const battles = Number(values.battles ?? "10");
const runKey = values["run-key"] ?? "ninjask-eval-1";
const maxTurns = Number(values["max-turns"] ?? "100");
const dataset = values.dataset;
if (!dataset || !values["dataset-hash"]) throw new Error("--dataset DIR and --dataset-hash HEX64 are required");
const datasetHash = values["dataset-hash"] as string;
const excluded = values.exclude === undefined ? ["Illusion"] : values.exclude === "" ? [] : values.exclude.split(",");
const formatId = "gen9championsvgc2026regmc";

const engine = bridge.loadEngine(process.env.SHOWDOWN_DIR || undefined);
const detail = poolMod.datasetPoolDetail(engine, {
  format_id: formatId,
  team_dataset: { path: dataset, dataset_hash: datasetHash },
  generation: { excluded_abilities: excluded },
});
let pool = detail.pool;
if (values["only-ability"] !== undefined) pool = sweep.restrictPool(pool, values["only-ability"]);
console.error(`pool: ${pool.teams.length} teams (records before exclusion: ${detail.records_before})`);

const actorSeedOf = (s: { baseline_seed: number }) => ({ p1: s.baseline_seed, p2: (s.baseline_seed ^ 0x9e3779b9) >>> 0 });
const makeActors = (m: string, seeds: { p1: number; p2: number }) => {
  switch (m) {
    case "random-legal": return { p1: bridge.randomLegalActor(seeds.p1), p2: bridge.randomLegalActor(seeds.p2) };
    case "pending-first": return { p1: bridge.pendingFirstActor(seeds.p1), p2: bridge.pendingFirstActor(seeds.p2) };
    case "uniform-all": return { p1: sweep.uniformAllActor(seeds.p1), p2: sweep.uniformAllActor(seeds.p2) };
    default: throw new Error(`unknown mix ${m}`);
  }
};

const gz = createGzip({ level: 6 });
const sink = createWriteStream(values.out as string);
gz.pipe(sink);
let wrote = 0;
for (let index = start; index < start + battles; index++) {
  const streams = bridge.leagueGameStreams(`${runKey}/${mix}`, 1, index);
  const teams = {
    p1: pool.teams[streams.team_draws.p1 % pool.teams.length],
    p2: pool.teams[streams.team_draws.p2 % pool.teams.length],
  };
  const packed = { p1: engine.sim.Teams.pack(teams.p1), p2: engine.sim.Teams.pack(teams.p2) };
  const t0 = performance.now();
  let line: Record<string, unknown>;
  try {
    const rec = await bridge.runBattle({ engine, formatId, seed: streams.seed, teams, actors: makeActors(mix, actorSeedOf(streams)), maxTurns });
    const requests = (p: "p1" | "p2") => rec.requests[p].map((r: any) => ({
      request_seq: r.request_seq, phase: r.phase, request_hash: r.request_hash, request: r.request,
      legal: r.legal === undefined ? null : { choices: r.legal.choices, pending: r.legal.pending },
    }));
    line = {
      index, mix, seed: streams.seed, teams, packed, turns: rec.turns, winner: rec.winner, forfeit: rec.forfeit ?? null,
      choices: rec.choices, requests: { p1: requests("p1"), p2: requests("p2") }, channels: rec.channels,
      omniscient_log: rec.omniscient_log, record_ms: Math.round(performance.now() - t0),
    };
  } catch (error) {
    line = { index, mix, seed: streams.seed, packed, failure: { kind: (error as Error).name, message: (error as Error).message.slice(0, 300) } };
  }
  if (!gz.write(`${JSON.stringify(line)}\n`)) await once(gz, "drain");
  wrote++;
}
gz.end();
await once(sink, "finish");
console.error(`recorded ${wrote} battles to ${values.out}`);
