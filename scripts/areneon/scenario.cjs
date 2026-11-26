#!/usr/bin/env node
// Plays battles on the pinned Showdown through its BattleStream and records what it writes, in the row format of
// tests/fixtures/areneon/battles.jsonl.gz (the omniscient log, each player's channel with its `|request|` lines, every
// request's hash and phase) and, beside it, every call on its random source per write (the draw trace
// scripts/check_recorded.py reads with --draws).
//
//   node scenario.cjs INPUT.json ROWS.jsonl[.gz] DRAWS.jsonl[.gz]
//
// INPUT is a JSON array (or JSON Lines) of {id, seed, packed: {p1, p2} (or teams: {p1, p2} as arrays of sets, packed
// here), choices: [{player, choice}], turn_limit?, must_contain?: [line], must_not_contain?: [line]}. The last two list
// lines the omniscient log has to hold or not hold, so a scenario that stops provoking its hint fails when recorded;
// `turn_limit` is kept in the row for a replay that plays past the engine's default 150 turns.
// The recorded corpus's battles are valid input too (their `choices` have the same shape): replaying one must give the
// same channels, which is how the recorder itself is checked (`--check ROWS.jsonl.gz`).
//
// The Showdown checkout is $NINJASK_SHOWDOWN (default third_party/pokemon-showdown), built (`node build`).
'use strict';
const fs = require('fs');
const path = require('path');
const zlib = require('zlib');
const { showdownPath } = require('../../refsim/showdown_path');
const PS = showdownPath();
const { BattleStream, Teams } = require(path.join(PS, 'dist', 'sim'));
const { extractChannelMessages, Battle } = require(path.join(PS, 'dist', 'sim', 'battle'));

const FORMAT = process.env.NINJASK_FORMAT || 'gen9championsvgc2026regmc';
const text = (v) => (typeof v === 'string' ? v : String(v));
const indices = (n) => Array.from({ length: n }, (_, i) => i);

// The recorder of the training harness (packages/showdown-bridge/src/battle-trace.ts), reproduced: `sample` and `shuffle`
// are recorded as the index drawn and the permutation applied, `random` and `randomChance` with their arguments.
function recordingSource(target, log) {
	return new Proxy(target, {
		get(t, name) {
			const value = Reflect.get(t, name);
			if (typeof name !== 'string' || typeof value !== 'function' || name === 'getSeed' || name === 'clone') {
				return typeof value === 'function' ? value.bind(t) : value;
			}
			switch (name) {
			case 'random': return (...args) => {
				const r = t.random(...args);
				log().push({ method: name, args: args.filter(a => a !== undefined).map(text), result: [text(r)] });
				return r;
			};
			case 'randomChance': return (n, d) => {
				const r = t.randomChance(n, d);
				log().push({ method: name, args: [text(n), text(d)], result: [text(r)] });
				return r;
			};
			case 'sample': return (items) => {
				const index = t.sample(indices(items.length));
				log().push({ method: name, args: [text(items.length)], result: [text(index)] });
				return items[index];
			};
			case 'shuffle': return (items, ...range) => {
				const order = indices(items.length);
				t.shuffle(order, ...range);
				log().push({ method: name, args: [text(items.length), ...range.filter(a => a !== undefined).map(text)], result: order.map(text),
					...(currentList ? { handlers: currentList } : {}) });
				const before = [...items];
				order.forEach((from, i) => { items[i] = before[from]; });
			};
			default: return (...args) => value.apply(t, args);
			}
		},
	});
}

// What `getPlayerStreams` gives each reader of one engine message, as lines (an empty chunk gives no line, and the
// wall-clock `|t:|` lines are dropped, as the harness does).
function lines(chunk) {
	return chunk === '' ? [] : chunk.split('\n').filter(l => !l.startsWith('|t:|'));
}
function emitted(type, data) {
	if (type === 'update') {
		const m = extractChannelMessages(data, [-1, 1, 2]);
		return { omniscient: lines(m[-1].join('\n')), p1: lines(m[1].join('\n')), p2: lines(m[2].join('\n')) };
	}
	if (type === 'sideupdate') {
		const at = data.indexOf('\n');
		const side = data.slice(0, at);
		const out = lines(data.slice(at + 1));
		return { omniscient: [], p1: side === 'p1' ? out : [], p2: side === 'p2' ? out : [] };
	}
	return { omniscient: [], p1: [], p2: [] };
}

function canonical(value) {
	if (Array.isArray(value)) return `[${value.map(canonical).join(',')}]`;
	if (value && typeof value === 'object') {
		return `{${Object.keys(value).sort().map(k => `${JSON.stringify(k)}:${canonical(value[k])}`).join(',')}}`;
	}
	return JSON.stringify(value);
}
const crypto = require('crypto');
function requestRecord(line, seq) {
	const request = JSON.parse(line.slice('|request|'.length));
	const phase = request.wait ? 'wait' : request.teamPreview ? 'preview' : request.forceSwitch ? 'forceSwitch' : 'move';
	delete request.rqid;
	return { request_seq: seq, phase, request_hash: crypto.createHash('sha256').update(canonical(request)).digest('hex') };
}

// With SCENARIO_HANDLERS=1 each recorded `shuffle` made by `speedSort` also names the handlers of the list it sorted (effect,
// holder, order, priority, speed, subOrder): what the engine's hand-built list for that event has to contain
// (scripts/areneon/explain_framing.py reads it). Draws are unchanged otherwise.
let currentList = null;
if (process.env.SCENARIO_HANDLERS) {
	const sort = Battle.prototype.speedSort;
	Battle.prototype.speedSort = function (list, comparator) {
		const before = currentList;
		currentList = list.map(x => ({
			id: x.effect && x.effect.id, type: x.effect && x.effect.effectType,
			holder: x.effectHolder && (x.effectHolder.name || x.effectHolder.id || x.effectHolder.constructor.name),
			order: x.order, priority: x.priority, speed: x.speed, sub: x.subOrder,
		}));
		try { return sort.call(this, list, comparator); } finally { currentList = before; }
	};
}

// With SCENARIO_SORTS=1 every `speedSort` of a list of two or more is also written, in call order, to the `sorts` of the
// write it happened in: the list's length and what is in it (an effect's id and holder, an action's choice and Pokémon, a
// Pokémon). scripts/areneon/sort_sequence.py compares that sequence with the engine's own trace, which shows a handler the
// engine's list leaves out even where nothing is tied (a tie is the only thing a shuffle shows).
let sortLog = null;
if (process.env.SCENARIO_SORTS) {
	const sort = Battle.prototype.speedSort;
	const describe = (x) => {
		if (x && x.effect) return `${x.effect.effectType}:${x.effect.id}@${x.effectHolder && (x.effectHolder.name || x.effectHolder.id || x.effectHolder.constructor.name)}`;
		if (x && x.choice) return `action:${x.choice}@${x.pokemon && x.pokemon.name}`;
		if (x && x.name) return `pokemon:${x.name}`;
		return '?';
	};
	// The event whose handlers the list holds: the innermost `runEvent`, `eachEvent`, `fieldEvent` or `residualEvent` that is
	// sorting (the sort is the first thing each of them does, before it runs a handler that could nest another).
	const events = [];
	for (const name of ['runEvent', 'singleEvent', 'eachEvent', 'fieldEvent', 'residualEvent']) {
		const original = Battle.prototype[name];
		Battle.prototype[name] = function (...args) {
			events.push(`${name}:${typeof args[0] === 'string' ? args[0] : '?'}`);
			try { return original.apply(this, args); } finally { events.pop(); }
		};
	}
	Battle.prototype.speedSort = function (list, comparator) {
		if (sortLog && list.length >= 2) {
			sortLog.push({
				n: list.length, event: events[events.length - 1] || '', items: list.map(describe),
				keys: list.map(x => [x.order, x.priority, x.subOrder, x.speed]),
			});
		}
		return sort.call(this, list, comparator);
	};
}

function play(scenario) {
	if (scenario.teams) scenario = { ...scenario, packed: { p1: Teams.pack(scenario.teams.p1), p2: Teams.pack(scenario.teams.p2) } };
	const channels = { p1: [], p2: [], omniscient: [] };
	let cur = [];
	let ended = false;
	class Traced extends BattleStream {
		pushMessage(type, data) {
			const e = emitted(type, data);
			for (const k of Object.keys(channels)) channels[k].push(...e[k]);
			if (type === 'end') ended = true;
		}
	}
	const stream = new Traced();
	stream._writeLines(`>start ${JSON.stringify({ formatid: FORMAT, seed: scenario.seed })}`);
	stream.battle.prng = recordingSource(stream.battle.prng, () => cur);
	const writes = [];
	const write = (kind, payload) => {
		cur = [];
		sortLog = process.env.SCENARIO_SORTS ? [] : null;
		stream._writeLines(payload);
		stream.battle.sendUpdates();
		writes.push({ kind, draws: cur, ...(sortLog ? { sorts: sortLog } : {}) });
	};
	write('player', `>player p1 ${JSON.stringify({ name: 'p1', team: scenario.packed.p1 })}`);
	write('player', `>player p2 ${JSON.stringify({ name: 'p2', team: scenario.packed.p2 })}`);
	const refusals = [];
	for (const c of scenario.choices) {
		const before = { p1: channels.p1.length, p2: channels.p2.length };
		write(c.player, `>${c.player} ${c.choice}`);
		// Showdown refuses a pending candidate or an invalid choice with an `|error|` line to the player.
		const error = channels[c.player].slice(before[c.player]).find(l => l.startsWith('|error|'));
		refusals.push(error ? error.slice('|error|'.length) : undefined);
	}
	const requests = { p1: [], p2: [] };
	for (const k of ['p1', 'p2']) {
		for (const line of channels[k]) if (line.startsWith('|request|')) requests[k].push(requestRecord(line, requests[k].length + 1));
	}
	for (const line of scenario.must_contain || []) {
		if (!channels.omniscient.includes(line)) {
			const msg = `${scenario.id}: the log has no line ${JSON.stringify(line)}`;
			if (process.env.SCENARIO_DEBUG) process.stderr.write(`${msg}\n${channels.omniscient.join('\n')}\n`);
			else throw new Error(msg);
		}
	}
	for (const line of scenario.must_not_contain || []) {
		if (channels.omniscient.includes(line)) {
			const msg = `${scenario.id}: the log has the line ${JSON.stringify(line)}`;
			if (process.env.SCENARIO_DEBUG) process.stderr.write(`${msg}\n`);
			else throw new Error(msg);
		}
	}
	if (process.env.SCENARIO_DEBUG) {
		for (const k of ['p1', 'p2']) for (const l of channels[k]) if (l.startsWith('|error|')) process.stderr.write(`${scenario.id} ${k}: ${l}\n`);
	}
	const row = {
		id: scenario.id,
		...(scenario.purpose ? { purpose: scenario.purpose } : {}),
		seed: scenario.seed,
		...(scenario.turn_limit ? { turn_limit: scenario.turn_limit } : {}),
		...(scenario.must_contain ? { must_contain: scenario.must_contain } : {}),
		...(scenario.must_not_contain ? { must_not_contain: scenario.must_not_contain } : {}),
		packed: scenario.packed,
		choices: scenario.choices.map((c, i) => ({
			player: c.player, choice: c.choice,
			...(refusals[i] !== undefined && refusals[i].startsWith('[Unavailable choice]') ? { refused: refusals[i] } : {}),
		})),
		channels: { p1: channels.p1, p2: channels.p2 },
		omniscient: channels.omniscient,
		requests,
	};
	return { row, trace: { id: scenario.id, seed: scenario.seed, writes } };
}

function main() {
	const [input, rowsOut, drawsOut] = process.argv.slice(2);
	if (!input || !rowsOut || !drawsOut) {
		console.error('usage: scenario.cjs INPUT.json ROWS.jsonl DRAWS.jsonl');
		process.exit(2);
	}
	const raw = input.endsWith('.gz') ? zlib.gunzipSync(fs.readFileSync(input)).toString() : fs.readFileSync(input, 'utf8');
	const scenarios = (raw.trimStart().startsWith('[') ? JSON.parse(raw) : raw.split('\n').filter(Boolean).map(l => JSON.parse(l)))
		.filter(s => !process.env.SCENARIO_ONLY || s.id === process.env.SCENARIO_ONLY);
	const rows = [];
	const traces = [];
	for (const s of scenarios) {
		const { row, trace } = play(s);
		rows.push(JSON.stringify(row));
		traces.push(JSON.stringify(trace));
	}
	const write = (file, lines) => {
		const body = lines.join('\n') + '\n';
		fs.writeFileSync(file, file.endsWith('.gz') ? zlib.gzipSync(body, { level: 9, mtime: 0 }) : body);
	};
	// SCENARIO_ONLY is for looking at one scenario: it must never replace the full fixture with one row.
	if (!process.env.SCENARIO_ONLY) {
		write(rowsOut, rows);
		write(drawsOut, traces);
	}
}

main();
