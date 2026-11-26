//! What each player is sent: the stream Pokémon Showdown writes to a player
//! of a battle, line for line.
//!
//! The engine's own log ([`Battle::log`]) is the battle as the engine
//! prints it: both players' viewpoints are one list with the secret halves
//! marked, and the players' requests are not in it. A *channel* is a
//! viewer's stream: the log as that viewer sees it, with Showdown's own
//! `|request|` lines placed where it sends them, the `|error|` line of every
//! refused choice, and the few lines Showdown writes that the engine's log
//! does not carry (the banner of [`Banner`], read from the pinned Showdown;
//! the blank line before `|win|` and `|tie`). Three channels are kept, for
//! `p1`, `p2` and the omniscient viewer (no requests, no errors), and they
//! hold no copy of the log: a segment names a range of it and the lines are
//! made when they are asked for, so a battle that nobody reads costs only
//! its requests.
//!
//! Where Showdown puts a request. The simulator flushes the log it has
//! built (`sendUpdates`) each time the game stops for a choice, then sends
//! every side its request, a `{"wait": true, ...}` one for the side that has
//! nothing to choose, as `|request|` lines of the same stream. A choice the
//! engine refuses is answered at once with `|error|[Invalid choice] <text>`,
//! or, when the refusal revealed something the request had hidden,
//! `|error|[Unavailable choice] <text>` and the side's request sent again
//! with `"update": true`. [`Channel::sync`] and [`Channel::refusal`] are
//! those two moments; a caller calls them after every `choose`.
//!
//! The request is written as Showdown writes it (its key order), without
//! `rqid`, which only the Showdown server numbers: the stream of the
//! in-process simulator has none, and neither does a request's identity
//! ([`crate::identity`]).

use serde_json::Value;

use crate::battle::log::{LogEntry, Viewer};
use crate::battle::{Battle, UNAVAILABLE};
use crate::candidates::{candidates_from_request, Candidate, CandidateError};

/// The lines Showdown writes around the start of a battle that the engine's
/// log does not carry, read from the pinned Showdown by the format export
/// (`refsim/export_banner.js`): the `|gen|` line, the `|rule|` lines, and
/// the per-player prompts of the Open Team Sheets rule.
#[derive(Clone, Debug, Default)]
pub struct Banner {
    /// Between the last `|player|` line and `|tier|`.
    pub after_player: Vec<LogEntry>,
    /// Between `|tier|` and `|clearpoke`.
    pub after_tier: Vec<LogEntry>,
    /// After `|teampreview|N`, before the first request.
    pub after_teampreview: Vec<LogEntry>,
    /// After `|start`.
    pub after_start: Vec<LogEntry>,
}

impl Banner {
    /// From the export's `format.banner` object.
    pub fn from_json(v: &Value) -> Result<Banner, String> {
        let part = |key: &str| -> Result<Vec<LogEntry>, String> {
            v[key]
                .as_array()
                .ok_or_else(|| format!("banner.{key} is not an array"))?
                .iter()
                .map(|e| match e {
                    Value::String(s) => Ok(LogEntry::Plain(s.clone())),
                    Value::Object(_) => Ok(LogEntry::Split {
                        side: e["side"]
                            .as_u64()
                            .filter(|s| *s < 2)
                            .ok_or("banner entry side is not 0 or 1")?
                            as usize,
                        secret: e["secret"]
                            .as_str()
                            .ok_or("banner entry has no secret")?
                            .into(),
                        shared: e["shared"]
                            .as_str()
                            .ok_or("banner entry has no shared")?
                            .into(),
                    }),
                    _ => Err(format!("banner.{key} holds a {e}")),
                })
                .collect()
        };
        Ok(Banner {
            after_player: part("after_player")?,
            after_tier: part("after_tier")?,
            after_teampreview: part("after_teampreview")?,
            after_start: part("after_start")?,
        })
    }
}

/// Viewers of a channel, by index: `p1`, `p2`, omniscient.
pub const VIEWERS: [&str; 3] = ["p1", "p2", "omniscient"];

fn viewer_of(view: usize) -> Viewer {
    match view {
        0 => Viewer::Side(0),
        1 => Viewer::Side(1),
        _ => Viewer::Omniscient,
    }
}

/// A run of one channel: lines of the log, banner entries, or one line of text.
#[derive(Clone, Debug)]
enum Seg {
    Log { from: usize, to: usize },
    Entries(Vec<LogEntry>),
    Text(String),
}

#[derive(Clone, Debug, Default)]
struct View {
    /// (index of the segment's first line, its number of lines, the segment).
    segs: Vec<(usize, usize, Seg)>,
    len: usize,
    /// Lines before this one were discarded ([`Channel::discard`]).
    first: usize,
}

impl View {
    fn push(&mut self, viewer: Viewer, seg: Seg) {
        let n = match &seg {
            Seg::Log { from, to } => to - from,
            // A split with nothing for this viewer is no line.
            Seg::Entries(entries) => entries
                .iter()
                .filter(|e| !e.render(viewer).is_empty())
                .count(),
            Seg::Text(_) => 1,
        };
        if n > 0 {
            self.segs.push((self.len, n, seg));
            self.len += n;
        }
    }

    /// Forget the segments that end at or before line `upto`.
    fn discard(&mut self, upto: usize) {
        let k = self.segs.partition_point(|(at, n, _)| at + n <= upto);
        self.segs.drain(..k);
        self.first = self.segs.first().map_or(self.len, |(at, _, _)| *at);
    }

    /// The lines from line `start` on.
    fn lines(&self, battle: &Battle, viewer: Viewer, start: usize) -> Result<Vec<String>, String> {
        if start < self.first {
            return Err(format!(
                "lines before {} were discarded, asked for {start}",
                self.first
            ));
        }
        let first = self.segs.partition_point(|(at, n, _)| at + n <= start);
        let mut out = Vec::with_capacity(self.len.saturating_sub(start));
        for (at, _, seg) in &self.segs[first..] {
            let skip = start.saturating_sub(*at);
            match seg {
                Seg::Log { from, to } => out.extend(
                    battle.log[from + skip..*to]
                        .iter()
                        .map(|e| e.render(viewer).to_string()),
                ),
                Seg::Entries(entries) => out.extend(
                    entries
                        .iter()
                        .map(|e| e.render(viewer))
                        .filter(|l| !l.is_empty())
                        .skip(skip)
                        .map(str::to_string),
                ),
                Seg::Text(text) => out.push(text.clone()),
            }
        }
        Ok(out)
    }
}

/// How far the log's opening has been read (where the banner goes).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Stage {
    /// Waiting for `|tier|`.
    Tier,
    /// Waiting for `|teampreview`.
    Preview,
    /// Waiting for `|start`.
    Start,
    Done,
}

/// The three channels of one battle (see the module documentation).
#[derive(Clone, Debug)]
pub struct Channel {
    banner: Banner,
    views: [View; 3],
    /// Log entries already placed in the channels.
    synced: usize,
    stage: Stage,
    /// The `rqid` of the request point the channels are at.
    rqid: u32,
    started: bool,
    ended: bool,
    /// Each side's current request as the engine builds it (with `rqid`).
    requests: [Option<Value>; 2],
    /// Its candidates, once asked for.
    candidates: [Option<Vec<Candidate>>; 2],
}

/// `request` as Showdown writes it: its keys in the order its code builds
/// them in, `rqid` left out.
pub fn wire_request(request: &Value) -> String {
    const ORDER: [&str; 9] = [
        "teamPreview",
        "forceSwitch",
        "active",
        "wait",
        "maxChosenTeamSize",
        "side",
        "ally",
        "noCancel",
        "update",
    ];
    let Value::Object(map) = request else {
        return request.to_string();
    };
    let mut out = String::with_capacity(4096);
    out.push('{');
    let mut first = true;
    let mut put = |out: &mut String, key: &str, value: &Value| {
        if !first {
            out.push(',');
        }
        first = false;
        crate::identity::write_string(out, key);
        out.push(':');
        // Nested values keep the engine's key order, which is Showdown's.
        out.push_str(&value.to_string());
    };
    for key in ORDER {
        if let Some(value) = map.get(key) {
            put(&mut out, key, value);
        }
    }
    for (key, value) in map {
        if key != "rqid" && !ORDER.contains(&key.as_str()) {
            put(&mut out, key, value);
        }
    }
    out.push('}');
    out
}

fn is_end_line(line: &str) -> bool {
    line.starts_with("|win|") || line == "|tie"
}

impl Channel {
    /// The channels of a battle at its start. The battle's log must be on
    /// and untouched since it was created (the channel names ranges of it).
    pub fn new(battle: &Battle) -> Result<Channel, String> {
        if !battle.log_enabled {
            return Err("a channel needs the battle's log".into());
        }
        let banner = battle.dex.format.banner.clone().unwrap_or_default();
        let mut channel = Channel {
            banner,
            views: Default::default(),
            synced: 0,
            stage: Stage::Tier,
            rqid: 0,
            started: false,
            ended: false,
            requests: [None, None],
            candidates: [None, None],
        };
        channel.sync(battle);
        Ok(channel)
    }

    /// Whether the format export a dex was made from has the banner
    /// ([`Banner`]); without it a channel lacks Showdown's `|gen|`, `|rule|`
    /// and open-team-sheet lines.
    pub fn has_banner_in(dex: &crate::dex::Dex) -> bool {
        dex.format.banner.is_some()
    }

    /// Showdown's `sendUpdates`: call it after every `choose`. When the
    /// game has stopped at a new request (or ended), the log since the last
    /// stop goes into every channel, then each side's request.
    pub fn sync(&mut self, battle: &Battle) {
        if self.started && battle.rqid == self.rqid && (!battle.ended || self.ended) {
            return;
        }
        self.started = true;
        self.rqid = battle.rqid;
        self.place_log(battle);
        if battle.ended {
            self.ended = true;
            self.requests = [None, None];
            self.candidates = [None, None];
        } else {
            for side in 0..2 {
                self.send_request(battle, side);
            }
        }
    }

    /// Showdown's `emitChoiceError`: call it with the error of a choice the
    /// engine refused. The error text is the engine's (the part after
    /// `[Invalid choice] `, or `[Unavailable choice] ` and the text); a
    /// refusal that revealed something sends the side's request again.
    pub fn refusal(&mut self, battle: &Battle, side: usize, message: &str) {
        // Showdown's `testfight` refusal: no error line, only the new request.
        if message.starts_with("Hack to avoid") {
            self.send_request(battle, side);
            return;
        }
        let revealed = message.starts_with(UNAVAILABLE);
        let line = if revealed {
            format!("|error|{message}")
        } else {
            format!("|error|[Invalid choice] {message}")
        };
        self.views[side].push(viewer_of(side), Seg::Text(line));
        if revealed {
            self.send_request(battle, side);
        }
    }

    /// Place the log entries since the last stop, with the banner and the
    /// blank line before the end.
    fn place_log(&mut self, battle: &Battle) {
        let log = &battle.log;
        let to = log.len();
        let mut start = self.synced;
        let mut i = start;
        while self.stage != Stage::Done && i < to {
            let text = log[i].render(Viewer::Omniscient);
            match self.stage {
                // `|gen|` goes before the tier line, the rules after it.
                Stage::Tier if text.starts_with("|tier|") => {
                    self.place(Seg::Log { from: start, to: i });
                    self.place(Seg::Entries(self.banner.after_player.clone()));
                    self.place(Seg::Log { from: i, to: i + 1 });
                    self.place(Seg::Entries(self.banner.after_tier.clone()));
                    start = i + 1;
                    self.stage = Stage::Preview;
                }
                Stage::Preview if text.starts_with("|teampreview") => {
                    self.place(Seg::Log {
                        from: start,
                        to: i + 1,
                    });
                    self.place(Seg::Entries(self.banner.after_teampreview.clone()));
                    start = i + 1;
                    self.stage = Stage::Start;
                }
                Stage::Start if text == "|start" => {
                    self.place(Seg::Log {
                        from: start,
                        to: i + 1,
                    });
                    self.place(Seg::Entries(self.banner.after_start.clone()));
                    start = i + 1;
                    self.stage = Stage::Done;
                }
                _ => {}
            }
            i += 1;
        }
        // The rest, with the blank line Showdown writes before the result (`win()` adds it, wherever
        // the result comes: a win in the middle of a turn is followed by the end-of-battle effects
        // that still run, a White Herb restoring a drop for one).
        let end_at = if battle.ended {
            (start..to)
                .rev()
                .find(|&j| is_end_line(log[j].render(Viewer::Omniscient)))
        } else {
            None
        };
        let blank_at = end_at.filter(|&j| {
            // The engine's own log may carry it already.
            !(j >= 1 && log[j - 1].render(Viewer::Omniscient) == "|")
        });
        if let Some(j) = blank_at {
            self.place(Seg::Log { from: start, to: j });
            self.place(Seg::Entries(vec![LogEntry::Plain("|".into())]));
            self.place(Seg::Log { from: j, to });
        } else {
            self.place(Seg::Log { from: start, to });
        }
        self.synced = to;
    }

    fn place(&mut self, seg: Seg) {
        for view in 0..3 {
            self.views[view].push(viewer_of(view), seg.clone());
        }
    }

    fn send_request(&mut self, battle: &Battle, side: usize) {
        self.candidates[side] = None;
        match battle.request_json(side) {
            Some(request) => {
                let line = format!("|request|{}", wire_request(&request));
                self.views[side].push(viewer_of(side), Seg::Text(line));
                self.requests[side] = Some(request);
            }
            None => self.requests[side] = None,
        }
    }

    /// The number of lines `viewer` (0 `p1`, 1 `p2`, 2 omniscient) has.
    pub fn len(&self, viewer: usize) -> usize {
        self.views[viewer].len
    }

    pub fn is_empty(&self, viewer: usize) -> bool {
        self.views[viewer].len == 0
    }

    /// The lines of `viewer` from line `start` on; an error when they were
    /// discarded.
    pub fn lines(
        &self,
        battle: &Battle,
        viewer: usize,
        start: usize,
    ) -> Result<Vec<String>, String> {
        self.views[viewer].lines(battle, viewer_of(viewer), start)
    }

    /// Forget lines of `viewer` before line `upto` (those a reader has
    /// taken): the stored requests are most of a channel's memory. Whole
    /// segments are forgotten, so lines from `upto` on stay available;
    /// reading lines that were forgotten is an error. Line numbers do not
    /// change.
    pub fn discard(&mut self, viewer: usize, upto: usize) {
        self.views[viewer].discard(upto);
    }

    /// `side`'s current request, as the engine builds it (so with `rqid`),
    /// or `None` when it has none (the game is over).
    pub fn request(&self, side: usize) -> Option<&Value> {
        self.requests[side].as_ref()
    }

    /// `side`'s current request as it is in the channel (no `rqid`).
    pub fn request_line(&self, side: usize) -> Option<String> {
        self.requests[side].as_ref().map(wire_request)
    }

    /// The identity of `side`'s current request (see [`crate::identity`]).
    pub fn request_hash(&self, side: usize) -> Result<Option<String>, String> {
        self.requests[side]
            .as_ref()
            .map(crate::identity::request_hash)
            .transpose()
    }

    /// The candidates of `side`'s current request ([`crate::candidates`]).
    pub fn candidates(&mut self, side: usize) -> Result<&[Candidate], CandidateError> {
        if self.candidates[side].is_none() {
            let list = match &self.requests[side] {
                Some(request) => candidates_from_request(request)?,
                None => Vec::new(),
            };
            self.candidates[side] = Some(list);
        }
        Ok(self.candidates[side].as_deref().unwrap_or_default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_request_is_written_in_showdowns_key_order_without_rqid() {
        let request = json!({
            "side": {"name": "p1", "id": "p1", "pokemon": []},
            "update": true,
            "rqid": 7,
            "noCancel": true,
            "active": [{"moves": []}],
        });
        assert_eq!(
            wire_request(&request),
            r#"{"active":[{"moves":[]}],"side":{"name":"p1","id":"p1","pokemon":[]},"noCancel":true,"update":true}"#
        );
        let preview = json!({"maxChosenTeamSize": 4, "side": {}, "rqid": 1, "teamPreview": true});
        assert_eq!(
            wire_request(&preview),
            r#"{"teamPreview":true,"maxChosenTeamSize":4,"side":{}}"#
        );
        let wait = json!({"side": {}, "wait": true, "surprise": 1});
        assert_eq!(
            wire_request(&wait),
            r#"{"wait":true,"side":{},"surprise":1}"#
        );
        assert_eq!(wire_request(&json!(null)), "null");
    }

    #[test]
    fn a_banner_reads_plain_lines_and_splits_and_refuses_the_rest() {
        let ok = json!({
            "after_player": ["|gen|9"],
            "after_tier": [],
            "after_teampreview": [{"side": 0, "secret": "|uhtml|x", "shared": ""}],
            "after_start": [],
        });
        let banner = Banner::from_json(&ok).unwrap();
        assert_eq!(banner.after_player.len(), 1);
        assert_eq!(
            banner.after_teampreview[0].render(Viewer::Side(0)),
            "|uhtml|x"
        );
        assert_eq!(banner.after_teampreview[0].render(Viewer::Side(1)), "");
        assert_eq!(
            banner.after_teampreview[0].render(Viewer::Omniscient),
            "|uhtml|x"
        );
        for (key, value) in [
            ("after_player", json!(1)),
            ("after_tier", json!([1])),
            (
                "after_start",
                json!([{"side": 2, "secret": "a", "shared": "b"}]),
            ),
            ("after_start", json!([{"side": 0, "shared": "b"}])),
            ("after_start", json!([{"side": 0, "secret": "a"}])),
        ] {
            let mut bad = ok.clone();
            bad[key] = value;
            assert!(Banner::from_json(&bad).is_err());
        }
        assert!(Banner::from_json(&json!({})).is_err());
    }

    #[test]
    fn the_view_makes_lines_from_any_line_on() {
        let mut view = View::default();
        view.push(Viewer::Side(0), Seg::Text("|a".into()));
        view.push(
            Viewer::Side(0),
            Seg::Entries(vec![
                LogEntry::Plain("|b".into()),
                LogEntry::Split {
                    side: 1,
                    secret: "|c".into(),
                    shared: String::new(),
                },
                LogEntry::Plain("|d".into()),
            ]),
        );
        view.push(Viewer::Side(0), Seg::Entries(vec![]));
        assert_eq!(view.len, 3);
    }
}
