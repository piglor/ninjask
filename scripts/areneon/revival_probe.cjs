#!/usr/bin/env node
// What the pinned Showdown accepts from a side that has Revival Blessing slots to answer, for the request states a
// battle can be put in: `Side.choose` called on every combination of `switch N`/`pass` for the flagged slots.
//
//   node revival_probe.cjs INPUT.json OUTPUT.json
//
// INPUT: {seed, packed: {p1, p2}, choices: [{player, choice}] (a script that leaves p1 with a forced-switch request in
// which every slot in `flagged` is reviving), flagged: [slot...], bench_alive: [[index...]...]}. For each entry of
// `bench_alive` the battle is played again from the start; after the script, the side's bench Pokémon at those indices
// of `side.pokemon` are made alive (full HP, no status), the request is made again (`makeRequest('switch')`, which
// resets the side's counters as the engine does at a new request) and every combination is tried on its own: a
// combination counts as accepted when `side.choose` returns true. A mutated state is not one a battle reaches (the
// first reviver can pass only while no Pokémon lives on the bench, and nothing changes the bench before the second
// reviver's request), which is why the states are made by hand: the enumerator has to be right for what the engine
// accepts, not only for what play shows.
//
// OUTPUT: [{bench_alive, request (the request as `|request|` carries it, rqid dropped), accepted: [string]}].
'use strict';
const fs = require('fs');
const path = require('path');
const { showdownPath } = require('../../refsim/showdown_path');
const PS = showdownPath();
const { BattleStream } = require(path.join(PS, 'dist', 'sim'));

const FORMAT = process.env.NINJASK_FORMAT || 'gen9championsvgc2026regmc';

function run(spec, aliveIndices) {
	const stream = new BattleStream();
	stream._writeLines(`>start ${JSON.stringify({ formatid: FORMAT, seed: spec.seed })}`);
	stream._writeLines(`>player p1 ${JSON.stringify({ name: 'p1', team: spec.packed.p1 })}`);
	stream._writeLines(`>player p2 ${JSON.stringify({ name: 'p2', team: spec.packed.p2 })}`);
	const battle = stream.battle;
	for (const c of spec.choices) {
		stream._writeLines(`>${c.player} ${c.choice}`);
		battle.sendUpdates();
	}
	const side = battle.sides[0];
	if (battle.requestState !== 'switch') throw new Error(`script ends in a ${battle.requestState} request`);
	for (const i of aliveIndices) {
		const mon = side.pokemon[i];
		if (!mon.fainted) throw new Error(`pokemon ${i} is not fainted`);
		mon.fainted = false;
		mon.faintQueued = false;
		mon.hp = mon.maxhp;
		mon.status = '';
		side.pokemonLeft++;
	}
	if (aliveIndices.length) battle.makeRequest('switch');
	const request = JSON.parse(JSON.stringify(side.activeRequest));
	delete request.rqid;
	const slots = spec.flagged.length;
	const options = [];
	const bench = side.pokemon.length;
	for (let i = 1; i <= bench; i++) options.push(`switch ${i}`);
	options.push('pass');
	const accepted = [];
	const combos = [];
	const build = (prefix, left) => {
		if (left === 0) { combos.push(prefix.join(', ')); return; }
		for (const o of options) build([...prefix, o], left - 1);
	};
	build([], slots);
	// Choosing a revival clears the reviver's switch flag and `clearChoice` does not put it back, so each
	// combination starts from the flags the request had.
	const flags = side.active.map((mon) => mon.switchFlag);
	for (const combo of combos) {
		side.active.forEach((mon, i) => { mon.switchFlag = flags[i]; });
		side.clearChoice();
		let ok = false;
		try { ok = side.choose(combo) === true && side.isChoiceDone(); } catch (e) { ok = false; }
		if (ok) accepted.push(combo);
	}
	side.active.forEach((mon, i) => { mon.switchFlag = flags[i]; });
	side.clearChoice();
	return { bench_alive: aliveIndices, request, accepted };
}

const [input, output] = process.argv.slice(2);
const spec = JSON.parse(fs.readFileSync(input, 'utf8'));
const out = spec.bench_alive.map((a) => run(spec, a));
fs.writeFileSync(output, JSON.stringify(out));
