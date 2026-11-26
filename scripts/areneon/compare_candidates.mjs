// Reads JSON lines {request, candidates:[[choice,pending],...]} on stdin (scripts/areneon/dump_requests.py makes them)
// and compares each with Areneon's reference enumerator, `enumerateCandidates` of packages/showdown-bridge, run on the
// same request text: the list, its order and every `pending` flag must be equal. Exit status 1 on any difference or any
// request the reference's schema refuses.
//
// One class is held out of the equality: a request with two reviving slots, for which the reference throws
// `EnumerationError` ("Revival Blessing outside one flagged reviving slot is not supported", `choices.ts`), counted as
// `referenceThrows`. The engine lists such a request's candidates as Showdown's `Side.choose` takes them
// (tests/python/test_n1_revival.py, against a live Showdown); any other throw of the reference is a difference.
//
//   python3 scripts/areneon/dump_requests.py POOL.txt 600 0 | AREONEON_ROOT=/path/to/areneon node scripts/areneon/compare_candidates.mjs
import { createInterface } from "node:readline";
const root = process.env.AREONEON_ROOT;
if (!root) throw new Error("set AREONEON_ROOT to a checkout of the Areneon monorepo");
const R = `${root}/packages/showdown-bridge/src`;
const { decodeRequest } = await import(`${R}/request.ts`);
const { enumerateCandidates } = await import(`${R}/choices.ts`);
const rl = createInterface({ input: process.stdin, crlfDelay: Infinity });
let n = 0, equal = 0, schemaRefused = 0, differ = 0, byPhase = {}, pending = 0, total = 0, referenceThrows = 0;
for await (const line of rl) {
  if (!line) continue;
  const row = JSON.parse(line);
  n++;
  let decoded;
  try { decoded = decodeRequest(row.request); } catch (e) { schemaRefused++; if (schemaRefused <= 3) console.log("SCHEMA", String(e).slice(0, 300), row.request.slice(0, 200)); continue; }
  let want;
  try {
    want = enumerateCandidates(decoded).candidates.map((c) => [c.choice, c.pending]);
  } catch (e) {
    if (e?.name === "EnumerationError" && /Revival Blessing outside one flagged reviving slot/.test(e.message) && row.candidates.length > 0) { referenceThrows++; continue; }
    differ++; if (differ <= 5) console.log("THROW", String(e).slice(0, 300), row.request.slice(0, 200)); continue;
  }
  const got = row.candidates;
  byPhase[decoded.phase] = (byPhase[decoded.phase] ?? 0) + 1;
  total += got.length;
  pending += got.filter((c) => c[1]).length;
  if (want.length === got.length && want.every((w, i) => w[0] === got[i][0] && w[1] === got[i][1])) equal++;
  else { differ++; if (differ <= 5) console.log("DIFF", decoded.phase, want.length, got.length, JSON.stringify(want.slice(0, 3)), JSON.stringify(got.slice(0, 3))); }
}
console.log(JSON.stringify({ requests: n, equal, differ, schemaRefused, referenceThrows, byPhase, candidates: total, pending }));
process.exit(differ || schemaRefused ? 1 : 0);
