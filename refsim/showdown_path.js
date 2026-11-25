// Where the pinned Pokémon Showdown checkout is, and what the Node tools may take
// from it. The location follows the same rule as python/ninjask/paths.py: $NINJASK_SHOWDOWN; else
// $NINJASK_HOME/third_party/pokemon-showdown; else this checkout's own
// third_party/, or the enclosing repository's when this checkout is a
// subdirectory or a git submodule of another repository (the engine vendored
// into a project).
'use strict';

const fs = require('fs');
const path = require('path');

// A submodule's .git is a file; only a repository of its own has a .git directory.
function isDir(p) {
	try {
		return fs.statSync(p).isDirectory();
	} catch {
		return false;
	}
}

function showdownPath() {
	if (process.env.NINJASK_SHOWDOWN) return path.resolve(process.env.NINJASK_SHOWDOWN);
	const engine = path.resolve(__dirname, '..');
	let home = engine;
	if (process.env.NINJASK_HOME) {
		home = path.resolve(process.env.NINJASK_HOME);
	} else if (!isDir(path.join(engine, '.git')) && fs.existsSync(path.join(engine, '..', '.git'))) {
		home = path.resolve(engine, '..');
	}
	return path.join(home, 'third_party', 'pokemon-showdown');
}

/**
 * The commit a tool's output is from. The pin (refsim/SHOWDOWN_COMMIT) is the
 * contract: the checkout must be at it, with no tracked file changed, or the
 * data would not be the pinned Showdown's whatever it says. NINJASK_ALLOW_UNPINNED=1
 * takes any checkout (a candidate pin, an older one to diff against) and
 * records the commit it really is; such an export fails the provenance check.
 */
function sourceCommit(ps = showdownPath()) {
	const { execFileSync } = require('child_process');
	const pin = fs.readFileSync(path.resolve(__dirname, 'SHOWDOWN_COMMIT'), 'utf8').trim();
	const git = args => execFileSync('git', ['-C', ps, ...args], { encoding: 'utf8' }).trim();
	let head, dirty;
	try {
		head = git(['rev-parse', 'HEAD']);
		dirty = git(['status', '--porcelain', '--untracked-files=no']);
	} catch (e) {
		throw new Error(`cannot read the commit of the Showdown checkout at ${ps}: ${e.message}`);
	}
	if (process.env.NINJASK_ALLOW_UNPINNED === '1') return head;
	if (head !== pin) {
		throw new Error(`the Showdown checkout is at ${head}, the pin is ${pin} (scripts/setup_showdown.sh, or NINJASK_ALLOW_UNPINNED=1 to export it anyway)`);
	}
	if (dirty) throw new Error(`the Showdown checkout at ${ps} has changed tracked files:\n${dirty}`);
	return head;
}

// Format ids Showdown does not define, derived from a base format by
// swapping a ruleset entry. PKMN Arena's open-team-sheets ladder for Reg M-C
// is the base format with sheets forced open; Showdown's own spelling of
// that swap is its Bo3 format, which also adds "Best of = 3", so the arena
// format is derived here instead. Legality and game data are the base
// format's; only the sheet rule (and the recorded `derivedFrom`) differ.
const DERIVED_FORMATS = {
	gen9championsvgc2026regmcots: {
		base: 'gen9championsvgc2026regmc',
		swap: ['Open Team Sheets', 'Force Open Team Sheets'],
	},
};

/** The format `formatId` names in the Showdown checkout `ps`, and the base format it derives from (or null). */
function resolveFormat(ps, formatId) {
	const { Dex } = require(path.join(ps, 'dist', 'sim'));
	const { Format } = require(path.join(ps, 'dist', 'sim', 'dex-formats'));
	let format = Dex.formats.get(formatId, true);
	let derivedFrom = null;
	if (!format.exists && DERIVED_FORMATS[formatId]) {
		const { base: baseId, swap: [from, to] } = DERIVED_FORMATS[formatId];
		const base = Dex.formats.get(baseId, true);
		if (!base.exists) throw new Error(`unknown base format ${baseId}`);
		if (!base.ruleset.includes(from)) throw new Error(`${baseId} has no "${from}" rule to swap`);
		format = new Format({
			id: formatId,
			// The base format's name: PKMN Arena's open-sheets games print it in
			// their |tier| line, as Showdown does when it shows the sheets.
			name: base.name,
			mod: base.mod,
			effectType: 'Format',
			gameType: base.gameType,
			ruleset: base.ruleset.map(r => (r === from ? to : r)),
		});
		derivedFrom = baseId;
	}
	if (!format.exists) throw new Error(`unknown format ${formatId}`);
	return { format, derivedFrom };
}

module.exports = { showdownPath, sourceCommit, resolveFormat };
