/**
 * The lines Pokémon Showdown writes around the start of a battle that the
 * engine's own log does not carry, read from the pinned Showdown itself
 * (never written by hand): `|gen|`, the `|rule|` lines of the format's
 * rules, and the per-player `|uhtml|` / `|uhtmlchange|` prompts of the
 * Open Team Sheets rule.
 *
 * A short battle of the format is played far enough to start (both players
 * choose a team) and its raw log is cut at protocol anchors (`|player|`,
 * `|tier|`, `|clearpoke`, `|teampreview`, `|start`, `|switch|`), so the
 * result is four lists of entries, each either a plain line or a split
 * (secret to one side, `shared` to the other, which may be empty):
 *
 *   after_player      between the last `|player|` line and `|tier|`
 *   after_tier        between `|tier|` and `|clearpoke`
 *   after_teampreview after `|teampreview|N`, up to the first request
 *   after_start       after `|start`, up to the first `|switch|`
 *
 * `ninjask::channel` puts them back where Showdown has them, so a player's
 * channel equals the stream Showdown sends that player. A format whose
 * banner depends on the teams (Force Open Team Sheets prints both sheets)
 * has none (`null`).
 */
'use strict';

const path = require('path');

const FILLER = 'Pikachu|||Static|Tackle|Hardy|||||50|';

function exportBanner(PS, format) {
	const { Battle, Teams } = require(path.join(PS, 'dist', 'sim'));
	if (format.ruleset.some(r => /forceopenteamsheets/i.test(r.replace(/\s/g, '')))) return null;
	const size = 6;
	const team = Teams.unpack(Array(size).fill(FILLER).join(']'));
	const battle = new Battle({ format, formatid: format.id, seed: 'sodium,00000000000000000000000000000000', send() {} });
	battle.setPlayer('p1', { name: 'p1', team });
	battle.setPlayer('p2', { name: 'p2', team });
	const toStart = battle.ruleTable.pickedTeamSize || size;
	const picks = 'team ' + Array.from({ length: toStart }, (_, i) => i + 1).join('');
	if (battle.requestState === 'teampreview') {
		battle.choose('p1', picks);
		battle.choose('p2', picks);
	}
	const raw = battle.log.filter(l => !l.startsWith('|t:|'));
	const at = (prefix, from = 0) => {
		for (let i = from; i < raw.length; i++) if (raw[i] === prefix || raw[i].startsWith(prefix)) return i;
		return -1;
	};
	// Raw entries to banner entries: a `|split|pN` marker takes the next two.
	const entries = (from, to) => {
		const out = [];
		for (let i = from; i < to; i++) {
			const m = /^\|split\|p([12])$/.exec(raw[i]);
			if (m) {
				out.push({ side: Number(m[1]) - 1, secret: raw[i + 1], shared: raw[i + 2] });
				i += 2;
			} else {
				out.push(raw[i]);
			}
		}
		return out;
	};
	let lastPlayer = -1;
	raw.forEach((l, i) => { if (l.startsWith('|player|')) lastPlayer = i; });
	const tier = at('|tier|');
	const clear = at('|clearpoke');
	const preview = at('|teampreview');
	const start = at('|start');
	let firstSwitch = at('|switch|', start);
	// A switch line is a split: its marker is the first entry of the next part.
	if (firstSwitch > 0 && raw[firstSwitch - 1].startsWith('|split|')) firstSwitch--;
	if ([lastPlayer, tier, clear, preview, start, firstSwitch].some(i => i < 0)) return null;
	// The prompt lines after `|teampreview` end at the blank line that opens the first turn loop.
	let previewEnd = preview + 1;
	while (previewEnd < start && raw[previewEnd] !== '|') previewEnd++;
	return {
		after_player: entries(lastPlayer + 1, tier),
		after_tier: entries(tier + 1, clear),
		after_teampreview: entries(preview + 1, previewEnd),
		after_start: entries(start + 1, firstSwitch),
	};
}

module.exports = { exportBanner };
