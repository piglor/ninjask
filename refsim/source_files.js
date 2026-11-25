#!/usr/bin/env node
/**
 * The Pokémon Showdown source files the engine's data and handlers come from, with a
 * SHA-256 of each, for the formats named on the command line:
 *
 *   node refsim/source_files.js <formatid>... > sources.json
 *
 * Prints `{ commit, formats: { <id>: { mod_chain } }, files: { <path>: <sha256> } }`. Nothing here
 * is a list of names: the mod chain of each format is the one Showdown's own Dex resolves
 * (`parentMod`, from each mod's `inherit`), and `files` is the union of
 *
 *   - every `sim/*.ts` (the battle code the engine ports and the data classes around it);
 *   - every source file the Dex and the validator loaded while the formats were exported
 *     (`require.cache`, mapped from `dist/*.js` to its `.ts`: the base data files, `config/formats.ts`,
 *     `lib/`, and the mod files of every chain);
 *   - every `.ts` file of a mod directory in a chain (a file the Dex does not load yet is one a
 *     later pin may start to).
 *
 * `scripts/repin.py` records it in refsim/PROVENANCE.json and checks it, and classifies the paths
 * of a Showdown diff by the same chains (docs/repin.md). The checkout must be the pin with no
 * tracked file changed (NINJASK_ALLOW_UNPINNED=1: any checkout, for a candidate pin).
 */
'use strict';

const crypto = require('crypto');
const fs = require('fs');
const path = require('path');
const { showdownPath, sourceCommit, resolveFormat } = require('./showdown_path');

const PS = showdownPath();
const formatIds = process.argv.slice(2);
if (formatIds.length === 0) {
	console.error('usage: source_files.js <formatid>...');
	process.exit(2);
}
const commit = sourceCommit(PS);
const { Dex } = require(path.join(PS, 'dist', 'sim'));

const sha256 = file => crypto.createHash('sha256').update(fs.readFileSync(path.join(PS, file))).digest('hex');
const isTs = f => f.endsWith('.ts') && !f.endsWith('.d.ts');
const list = dir => fs.readdirSync(path.join(PS, dir)).filter(isTs).sort().map(f => `${dir}/${f}`);

// The chain of mods a format's data comes from: its own mod, then the one it inherits, ... then base.
const formats = {};
const chainMods = new Set();
for (const id of formatIds) {
	const { format } = resolveFormat(PS, id);
	const dex = Dex.forFormat(format);
	dex.loadData();
	const chain = [];
	for (let d = dex; d; d = d.parentMod ? Dex.mod(d.parentMod) : null) {
		chain.push(d.currentMod);
		if (d.isBase) break;
	}
	formats[id] = { mod_chain: chain };
	for (const mod of chain) if (mod !== 'base') chainMods.add(mod);
	// What the export reads of the format: its rule table, validator and the data every entry resolves.
	Dex.formats.getRuleTable(format);
	dex.moves.all(); dex.abilities.all(); dex.items.all(); dex.species.all();
}

const files = new Set(list('sim'));
for (const mod of chainMods) for (const f of list(`data/mods/${mod}`)) files.add(f);

const distRoot = path.join(PS, 'dist') + path.sep;
for (const key of Object.keys(require.cache)) {
	if (!key.startsWith(distRoot)) continue; // node_modules and the like are not Showdown's sources
	const source = path.relative(PS, key).replace(/^dist[\\/]/, '').replace(/\.js$/, '.ts').split(path.sep).join('/');
	if (!fs.existsSync(path.join(PS, source))) throw new Error(`${path.relative(PS, key)} is loaded, but ${source} does not exist`);
	files.add(source);
}

const hashes = {};
for (const f of [...files].sort()) hashes[f] = sha256(f);
process.stdout.write(JSON.stringify({ commit, formats, files: hashes }) + '\n');
