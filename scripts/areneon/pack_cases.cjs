// The teams of Areneon's golden choice cases (fixtures/showdown/<format>/choices/*.json) in Showdown's packed form,
// made with Showdown's own `Teams.pack`: the input of the tests that replay those cases into the engine
// (tests/fixtures/areneon/choices-packed.json).
//
//   node scripts/areneon/pack_cases.cjs CHOICES_DIR > choices-packed.json
'use strict';
const fs = require('fs');
const path = require('path');
const PS = require('../../refsim/showdown_path').showdownPath();
const { Teams } = require(PS + '/dist/sim');
const dir = process.argv[2];
const out = {};
for (const f of fs.readdirSync(dir).filter(x => x.endsWith('.json')).sort()) {
  const c = JSON.parse(fs.readFileSync(path.join(dir, f), 'utf8'));
  out[c.id] = { p1: Teams.pack(c.teams.p1), p2: Teams.pack(c.teams.p2) };
}
process.stdout.write(JSON.stringify(out, null, 1) + '\n');
