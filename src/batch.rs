//! Many battles stepped together, for training: each battle keeps both
//! players' [`Tracker`]s, and one call reports every pending decision as
//! contiguous numeric buffers ([`crate::encode`]), one call applies a choice
//! to each, and both run the battles in parallel. Python's `ninjask.Batch`
//! wraps it.
//!
//! The choices offered are the player's view ([`Battle::player_choices`]),
//! so, as in Showdown, a choice can be refused for something hidden; the
//! refusal reveals it and the decision stays pending with the updated
//! options. Battles are independent and seeded, so results don't depend on
//! the thread count or the order of calls.
//!
//! A battle is seeded with a 64-bit integer (fast generator) or a Showdown
//! seed (`sodium,<hex>`, Showdown's own generator: [`Seed`]), so a batched
//! game can be Showdown's game for that seed and those choices. Beyond the
//! observation of [`Batch::observe`], a batch built with
//! [`BatchOptions::channels`] keeps each battle's [`Channel`]: the stream
//! Showdown sends each player, requests included ([`Batch::channel`]), and
//! the candidates of each player's request ([`Batch::candidates`]), chosen
//! by [`Batch::choose`] or [`Batch::choose_candidates`]; and with
//! [`BatchOptions::record_draws`] the draws of each battle, which a caller
//! may force ([`Batch::set_forced_draws`]).

use std::sync::Arc;

use rayon::prelude::*;

use std::collections::HashMap;

use crate::battle::{Battle, Team};
use crate::candidates::{Candidate, CandidateError};
use crate::channel::Channel;
use crate::dex::team::parse_packed;
use crate::dex::Dex;
use crate::encode;
use crate::rng::{Draw, Rng, Seed};
use crate::state::RequestState;
use crate::tracker::Tracker;

/// One battle and what each player has seen of it.
struct Env {
    battle: Battle,
    /// Each player's view for [`Batch::observe`], made when it is first
    /// asked for: a batch that is read through channels builds none.
    trackers: Option<[Tracker; 2]>,
    /// Protocol lines each tracker has been fed.
    seen: [usize; 2],
    channel: Option<Channel>,
}

impl Env {
    /// Bring the side's tracker up to date with the log and its request,
    /// reading the battle's log lines in place (as `log_for` renders them).
    fn sync(&mut self, s: usize) {
        let viewer = crate::battle::log::Viewer::Side(s);
        let log = &self.battle.log;
        let dex = &self.battle.dex;
        let trackers = self
            .trackers
            .get_or_insert_with(|| [Tracker::new(dex.clone(), 0), Tracker::new(dex.clone(), 1)]);
        let t = &mut trackers[s];
        for e in &log[self.seen[s].min(log.len())..] {
            t.feed(e.render(viewer));
        }
        self.seen[s] = log.len();
        t.feed_engine(&self.battle);
    }

    /// Submit `text` for side `s` and keep the channels up to date.
    fn choose(&mut self, s: usize, text: &str) -> Result<(), String> {
        let result = self.battle.choose(s, text);
        if let Some(channel) = &mut self.channel {
            if let Err(message) = &result {
                channel.refusal(&self.battle, s, message);
            }
            channel.sync(&self.battle);
        }
        result
    }
}

/// How a [`Batch`] is built.
#[derive(Clone, Debug)]
pub struct BatchOptions {
    /// The players' names, as they appear in `|player|` lines and requests.
    pub names: [String; 2],
    /// Keep each battle's [`Channel`] (needs a format export with the
    /// banner, `Dex::format.banner`).
    pub channels: bool,
    /// Log each battle's draws from its start ([`Batch::take_draws`]).
    pub record_draws: bool,
}

impl Default for BatchOptions {
    fn default() -> BatchOptions {
        BatchOptions {
            names: ["Player 1".into(), "Player 2".into()],
            channels: false,
            record_draws: false,
        }
    }
}

/// Width of an action row in [`Batch::observe`]: the larger of a doubles
/// move row and a team preview of six.
pub const ACTION_WIDTH: usize = 2 * encode::ACTION_FIELDS;

/// The pending decisions of a batch, as [`Batch::observe`] fills them.
/// Row `k` is the decision of side `who[2k + 1]` in battle `who[2k]`; its
/// legal choices are rows `offsets[k]..offsets[k + 1]` of `actions`.
#[derive(Default, Debug, Clone)]
pub struct Observation {
    /// (battle, side) pairs.
    pub who: Vec<u32>,
    /// 1 for a team preview decision, else 0.
    pub preview: Vec<u8>,
    /// `OBS_LEN` floats per decision.
    pub obs: Vec<f32>,
    /// `IDS_LEN` values per decision.
    pub ids: Vec<i16>,
    /// `ACTION_WIDTH` values per legal choice: [`encode::action_row`] for
    /// move and switch requests; at team preview the team positions sent
    /// (1-based, leads first), padded with 0.
    pub actions: Vec<i8>,
    /// Row offsets into `actions`, one more than the decisions.
    pub offsets: Vec<u32>,
}

/// The outcome of applying one choice in [`Batch::step`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StepResult {
    /// Showdown refused the choice (a hidden disable or trap); the
    /// decision is still pending.
    pub refused: bool,
    /// The battle ended with this choice.
    pub ended: bool,
}

/// The outcome of one choice submitted as text ([`Batch::choose`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Chosen {
    /// Accepted: kept until the other side chooses, or the turn ran.
    Accepted,
    /// Showdown refused it for something the request hid
    /// (`[Unavailable choice]`): the decision is still pending, with a new
    /// request. For a candidate that is not pending, a defect.
    Refused,
    /// Showdown refused it as invalid (`[Invalid choice]`): the choice is
    /// not one the engine takes. For a candidate, a defect.
    Invalid,
}

/// Prepared teams kept by [`Batch::reset`]: a training run reuses a pool.
const TEAM_CACHE: usize = 4096;

/// Many battles, stepped together.
pub struct Batch {
    dex: Arc<Dex>,
    envs: Vec<Option<Env>>,
    /// Packed team -> prepared team, so a reset parses each team once.
    teams: HashMap<String, Arc<Team>>,
    options: BatchOptions,
    /// The turn after which a battle is stopped as a draw.
    pub turn_limit: u32,
}

impl Batch {
    /// `n` empty slots; [`Batch::reset`] puts a battle in one.
    pub fn new(dex: Arc<Dex>, n: usize) -> Batch {
        Batch {
            dex,
            envs: (0..n).map(|_| None).collect(),
            teams: HashMap::new(),
            options: BatchOptions::default(),
            turn_limit: 150,
        }
    }

    /// `new` with options. Channels need the banner of the format export.
    pub fn with_options(dex: Arc<Dex>, n: usize, options: BatchOptions) -> Result<Batch, String> {
        if options.channels && dex.format.banner.is_none() {
            return Err(
                "channels need the banner of the format export: export the format again with refsim/export_format.js"
                    .into(),
            );
        }
        let mut batch = Batch::new(dex, n);
        batch.options = options;
        Ok(batch)
    }

    pub fn len(&self) -> usize {
        self.envs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.envs.is_empty()
    }

    /// A packed team, prepared (and kept for the next reset).
    fn team(&mut self, packed: &str) -> Result<Arc<Team>, String> {
        if let Some(t) = self.teams.get(packed) {
            return Ok(t.clone());
        }
        let team = Arc::new(Team::new(&self.dex, &parse_packed(&self.dex, packed)?));
        if self.teams.len() >= TEAM_CACHE {
            self.teams.clear();
        }
        self.teams.insert(packed.to_string(), team.clone());
        Ok(team)
    }

    /// Start a battle in slot `i` from two packed teams, seeded: the battle
    /// `Battle::new` builds from them (each team is parsed once per batch).
    pub fn reset(&mut self, i: usize, teams: [&str; 2], seed: u64) -> Result<(), String> {
        self.reset_seeded(i, teams, &Seed::Fast(seed), None)
    }

    /// [`Batch::reset`] with either kind of seed, and optionally the draws
    /// to force while the battle is built (its start draws the genders of
    /// Pokémon that have none; see [`Batch::set_forced_draws`]).
    pub fn reset_seeded(
        &mut self,
        i: usize,
        teams: [&str; 2],
        seed: &Seed,
        forced: Option<&[Draw]>,
    ) -> Result<(), String> {
        if i >= self.envs.len() {
            return Err(format!("battle {i} out of range"));
        }
        let (a, b) = (self.team(teams[0])?, self.team(teams[1])?);
        let rng = self.rng_for(seed, forced)?;
        self.envs[i] = Some(start_env(&self.dex, &self.options, [&a, &b], rng)?);
        Ok(())
    }

    /// The generator of a new battle: the seed's, logging if the batch
    /// does, forced if a list is given.
    fn rng_for(&self, seed: &Seed, forced: Option<&[Draw]>) -> Result<Rng, String> {
        let mut rng = seed.rng()?;
        if self.options.record_draws {
            rng.record_draws();
        }
        if let Some(list) = forced {
            rng.force_draws(list)?;
        }
        Ok(rng)
    }

    /// Start several battles at once: `starts[k] = (slot, [team1, team2],
    /// seed)`, as many `reset` calls, with the battles built in parallel.
    /// Nothing is started when a slot or a team is bad.
    pub fn reset_many(&mut self, starts: &[(usize, [&str; 2], u64)]) -> Result<(), String> {
        let starts: Vec<(usize, [&str; 2], Seed)> = starts
            .iter()
            .map(|&(i, teams, seed)| (i, teams, Seed::Fast(seed)))
            .collect();
        self.reset_many_seeded(&starts)
    }

    /// [`Batch::reset_many`] with either kind of seed.
    pub fn reset_many_seeded(&mut self, starts: &[(usize, [&str; 2], Seed)]) -> Result<(), String> {
        let mut prepared = Vec::with_capacity(starts.len());
        for (i, teams, seed) in starts {
            if *i >= self.envs.len() {
                return Err(format!("battle {i} out of range"));
            }
            let rng = self.rng_for(seed, None)?;
            prepared.push((*i, [self.team(teams[0])?, self.team(teams[1])?], rng));
        }
        let (dex, options) = (self.dex.clone(), self.options.clone());
        let built: Vec<Result<(usize, Env), String>> = prepared
            .into_par_iter()
            .map(|(i, [a, b], rng)| Ok((i, start_env(&dex, &options, [&a, &b], rng)?)))
            .collect();
        let built = built.into_iter().collect::<Result<Vec<_>, _>>()?;
        for (i, env) in built {
            self.envs[i] = Some(env);
        }
        Ok(())
    }

    /// For each slot, whether its battle is over ([`Batch::done`]).
    pub fn dones(&self) -> Vec<bool> {
        (0..self.envs.len()).map(|i| self.done(i)).collect()
    }

    /// The battle in slot `i`, if one was started.
    pub fn battle(&self, i: usize) -> Option<&Battle> {
        self.envs.get(i)?.as_ref().map(|e| &e.battle)
    }

    /// Whether slot `i`'s battle is over: ended, or past the turn limit.
    pub fn done(&self, i: usize) -> bool {
        self.battle(i)
            .is_none_or(|b| b.ended || b.turn >= self.turn_limit)
    }

    /// Every pending decision: each side of each running battle that has to
    /// choose, with its observation and legal choices.
    pub fn observe(&mut self) -> Observation {
        let limit = self.turn_limit;
        // Each battle's decisions, in parallel; then concatenated in order.
        let parts: Vec<Observation> = self
            .envs
            .par_iter_mut()
            .enumerate()
            .map(|(i, env)| {
                let mut o = Observation::default();
                let Some(env) = env else { return o };
                if env.battle.ended || env.battle.turn >= limit {
                    return o;
                }
                for s in 0..2 {
                    if !env.battle.side_needs_choice(s) {
                        continue;
                    }
                    env.sync(s);
                    o.who.extend([i as u32, s as u32]);
                    let trackers = env.trackers.as_ref().expect("synced just now");
                    encode::observation(&trackers[s], &mut o.obs);
                    encode::ids(&trackers[s], &mut o.ids);
                    o.offsets.push(o.actions.len() as u32 / ACTION_WIDTH as u32);
                    if env.battle.request == RequestState::TeamPreview {
                        o.preview.push(1);
                        let actions = &mut o.actions;
                        env.battle.for_each_team_preview_order(s, &mut |order| {
                            let mut row = [0i8; ACTION_WIDTH];
                            for (k, &p) in order.iter().take(ACTION_WIDTH).enumerate() {
                                row[k] = p as i8 + 1;
                            }
                            actions.extend(row);
                        });
                    } else {
                        o.preview.push(0);
                        let actions = &mut o.actions;
                        env.battle.for_each_player_choice(s, &mut |c| {
                            let start = actions.len();
                            encode::action_row(c, actions);
                            actions.resize(start + ACTION_WIDTH, 0);
                        });
                    }
                }
                o
            })
            .collect();
        let mut out = Observation::default();
        for p in parts {
            let base = out.actions.len() as u32 / ACTION_WIDTH as u32;
            out.who.extend(p.who);
            out.preview.extend(p.preview);
            out.obs.extend(p.obs);
            out.ids.extend(p.ids);
            out.offsets.extend(p.offsets.iter().map(|o| o + base));
            out.actions.extend(p.actions);
        }
        out.offsets
            .push(out.actions.len() as u32 / ACTION_WIDTH as u32);
        out
    }

    /// Apply a choice to each pending decision: `picks[k] = (battle, side,
    /// index)` with `index` into that decision's legal choices as
    /// [`Batch::observe`] listed them. Every pick is checked before any is
    /// applied, so an error leaves the batch as it was. The battles advance
    /// in parallel.
    pub fn step(&mut self, picks: &[(usize, usize, usize)]) -> Result<Vec<StepResult>, String> {
        // Group the picks by battle, keeping each one's position in `picks`.
        let mut by_env: Vec<Vec<(usize, usize, usize)>> = vec![Vec::new(); self.envs.len()];
        for (k, &(i, s, index)) in picks.iter().enumerate() {
            if i >= self.envs.len() || s > 1 {
                return Err(format!("no battle {i}, side {s}"));
            }
            by_env[i].push((k, s, index));
        }
        // First every choice, as Showdown syntax: an index out of range
        // fails the call before anything moves.
        let resolved: Vec<Result<Resolved, String>> = self
            .envs
            .par_iter()
            .zip(by_env.par_iter())
            .enumerate()
            .map(|(i, (env, mine))| {
                if mine.is_empty() {
                    return Ok(Vec::new());
                }
                let b = &env
                    .as_ref()
                    .ok_or_else(|| format!("battle {i} was never reset"))?
                    .battle;
                mine.iter()
                    .map(|&(k, s, index)| Ok((k, s, choice_at(b, i, s, index)?)))
                    .collect()
            })
            .collect();
        let mut chosen = Vec::with_capacity(resolved.len());
        for r in resolved {
            chosen.push(r?);
        }
        // Then apply them, battle by battle in parallel.
        let results: Vec<Vec<(usize, StepResult)>> = self
            .envs
            .par_iter_mut()
            .zip(chosen.into_par_iter())
            .map(|(env, mine)| {
                let mut out = Vec::with_capacity(mine.len());
                let Some(env) = env else { return out };
                for (k, s, choice) in mine {
                    let refused = env.choose(s, &choice).is_err();
                    out.push((
                        k,
                        StepResult {
                            refused,
                            ended: env.battle.ended,
                        },
                    ));
                }
                out
            })
            .collect();
        let mut out = vec![
            StepResult {
                refused: false,
                ended: false,
            };
            picks.len()
        ];
        for r in results {
            for (k, res) in r {
                out[k] = res;
            }
        }
        Ok(out)
    }

    /// Every decision that is pending: each side of each running battle
    /// that has to choose, as (battle, side), in order. What
    /// [`Batch::observe`] reports, without the observations.
    pub fn pending(&self) -> Vec<(usize, usize)> {
        let mut out = Vec::new();
        for (i, env) in self.envs.iter().enumerate() {
            let Some(env) = env else { continue };
            if env.battle.ended || env.battle.turn >= self.turn_limit {
                continue;
            }
            for s in 0..2 {
                if env.battle.side_needs_choice(s) {
                    out.push((i, s));
                }
            }
        }
        out
    }

    fn env(&self, i: usize) -> Result<&Env, String> {
        self.envs
            .get(i)
            .and_then(Option::as_ref)
            .ok_or_else(|| format!("no battle in slot {i}"))
    }

    fn env_mut(&mut self, i: usize) -> Result<&mut Env, String> {
        self.envs
            .get_mut(i)
            .and_then(Option::as_mut)
            .ok_or_else(|| format!("no battle in slot {i}"))
    }

    fn channel_of(&self, i: usize) -> Result<(&Env, &Channel), String> {
        let env = self.env(i)?;
        let channel = env
            .channel
            .as_ref()
            .ok_or("this batch has no channels (BatchOptions::channels)")?;
        Ok((env, channel))
    }

    /// Lines `start..` of a channel of battle `i`: `viewer` 0 is `p1`'s
    /// stream, 1 `p2`'s, 2 the omniscient log ([`Channel`]).
    pub fn channel(&self, i: usize, viewer: usize, start: usize) -> Result<Vec<String>, String> {
        let (env, channel) = self.channel_of(i)?;
        channel.lines(&env.battle, viewer, start)
    }

    /// Forget the lines of a channel of battle `i` before line `upto`, which
    /// the reader has taken ([`Channel::discard`]).
    pub fn discard_channel(&mut self, i: usize, viewer: usize, upto: usize) -> Result<(), String> {
        match self.env_mut(i)?.channel.as_mut() {
            Some(channel) => {
                channel.discard(viewer, upto);
                Ok(())
            }
            None => Err("this batch has no channels (BatchOptions::channels)".into()),
        }
    }

    /// The number of lines of a channel of battle `i`.
    pub fn channel_len(&self, i: usize, viewer: usize) -> Result<usize, String> {
        Ok(self.channel_of(i)?.1.len(viewer))
    }

    /// The identity of `side`'s current request in battle `i` (see
    /// [`crate::identity`]); `None` when the side has no request.
    pub fn request_hash(&self, i: usize, side: usize) -> Result<Option<String>, String> {
        match &self.env(i)?.channel {
            Some(channel) => channel.request_hash(side),
            None => match self.env(i)?.battle.request_json(side) {
                Some(request) => crate::identity::request_hash(&request).map(Some),
                None => Ok(None),
            },
        }
    }

    /// The candidates of `side`'s current request in battle `i`
    /// ([`crate::candidates`]).
    pub fn candidates(&mut self, i: usize, side: usize) -> Result<Vec<Candidate>, CandidateError> {
        let env = self.env_mut(i).map_err(CandidateError)?;
        match &mut env.channel {
            Some(channel) => Ok(channel.candidates(side)?.to_vec()),
            None => env.battle.candidates(side),
        }
    }

    /// Submit choices as text: `picks[k] = (battle, side, choice)`, in
    /// Showdown's syntax. The battles advance in parallel; a battle's picks
    /// are applied in order. A refused choice leaves the decision pending
    /// (and, with channels, its error line and new request in the stream).
    pub fn choose(
        &mut self,
        picks: &[(usize, usize, &str)],
    ) -> Result<Vec<(Chosen, bool)>, String> {
        let mut by_env: Vec<Vec<(usize, usize, String)>> = vec![Vec::new(); self.envs.len()];
        for (k, &(i, s, text)) in picks.iter().enumerate() {
            if i >= self.envs.len() || s > 1 {
                return Err(format!("no battle {i}, side {s}"));
            }
            if self.envs[i].is_none() {
                return Err(format!("battle {i} was never reset"));
            }
            by_env[i].push((k, s, text.to_string()));
        }
        Ok(self.apply(by_env, picks.len()))
    }

    /// [`Batch::choose`] with each pick naming one of the side's
    /// candidates by its index in [`Batch::candidates`]. Every pick is
    /// resolved before any is applied, so an error leaves the batch as it was.
    pub fn choose_candidates(
        &mut self,
        picks: &[(usize, usize, usize)],
    ) -> Result<Vec<(Chosen, bool)>, String> {
        let mut by_env: Vec<Vec<(usize, usize, usize)>> = vec![Vec::new(); self.envs.len()];
        for (k, &(i, s, index)) in picks.iter().enumerate() {
            if i >= self.envs.len() || s > 1 {
                return Err(format!("no battle {i}, side {s}"));
            }
            by_env[i].push((k, s, index));
        }
        let resolved: Vec<Result<Resolved, String>> = self
            .envs
            .par_iter_mut()
            .zip(by_env.par_iter())
            .enumerate()
            .map(|(i, (env, mine))| {
                if mine.is_empty() {
                    return Ok(Vec::new());
                }
                let env = env
                    .as_mut()
                    .ok_or_else(|| format!("battle {i} was never reset"))?;
                mine.iter()
                    .map(|&(k, s, index)| {
                        // Only the picked choice is copied, never the list: with
                        // channels the list is the channel's own (kept for the
                        // request), without them it is built here and dropped.
                        let pick = |list: &[Candidate]| {
                            list.get(index).map(|c| c.choice.clone()).ok_or_else(|| {
                                format!("battle {i} side {s}: candidate {index} of {}", list.len())
                            })
                        };
                        let text = match &mut env.channel {
                            Some(channel) => pick(
                                channel
                                    .candidates(s)
                                    .map_err(|e| format!("battle {i} side {s}: {e}"))?,
                            ),
                            None => pick(
                                &env.battle
                                    .candidates(s)
                                    .map_err(|e| format!("battle {i} side {s}: {e}"))?,
                            ),
                        }?;
                        Ok((k, s, text))
                    })
                    .collect()
            })
            .collect();
        let mut chosen: Vec<Resolved> = Vec::with_capacity(resolved.len());
        for r in resolved {
            chosen.push(r?);
        }
        Ok(self.apply(chosen, picks.len()))
    }

    /// Apply resolved picks, battle by battle in parallel; one result per
    /// pick, in the picks' order: how it went and whether the battle ended.
    fn apply(&mut self, by_env: Vec<Resolved>, n: usize) -> Vec<(Chosen, bool)> {
        let results: Vec<Vec<(usize, (Chosen, bool))>> = self
            .envs
            .par_iter_mut()
            .zip(by_env.into_par_iter())
            .map(|(env, mine)| {
                let mut out = Vec::with_capacity(mine.len());
                let Some(env) = env else { return out };
                for (k, s, text) in mine {
                    let how = match env.choose(s, &text) {
                        Ok(()) => Chosen::Accepted,
                        Err(m) if m.starts_with(crate::battle::UNAVAILABLE) => Chosen::Refused,
                        Err(_) => Chosen::Invalid,
                    };
                    out.push((k, (how, env.battle.ended)));
                }
                out
            })
            .collect();
        let mut out = vec![(Chosen::Accepted, false); n];
        for r in results {
            for (k, res) in r {
                out[k] = res;
            }
        }
        out
    }

    /// The draws battle `i` made since the last take, with the result used
    /// for each (needs [`BatchOptions::record_draws`]), emptying the log.
    pub fn take_draws(&mut self, i: usize) -> Result<Vec<Draw>, String> {
        Ok(self.env_mut(i)?.battle.rng.take_draws())
    }

    /// Force the next calls of battle `i`'s generator from `draws` (see
    /// [`crate::rng`]); the log is emptied and logging starts.
    pub fn set_forced_draws(&mut self, i: usize, draws: &[Draw]) -> Result<(), String> {
        self.env_mut(i)?.battle.rng.force_draws(draws)
    }
}

/// A battle at team preview from prepared teams, with what the options ask for.
fn start_env(
    dex: &Arc<Dex>,
    options: &BatchOptions,
    teams: [&Team; 2],
    rng: Rng,
) -> Result<Env, String> {
    let names = [options.names[0].as_str(), options.names[1].as_str()];
    let mut battle = Battle::from_teams_rng(dex.clone(), teams, names, rng);
    battle.log_enabled = true;
    let channel = options
        .channels
        .then(|| Channel::new(&battle))
        .transpose()?;
    Ok(Env {
        battle,
        trackers: None,
        seen: [0, 0],
        channel,
    })
}

/// One battle's picks resolved to choices: (position in `picks`, side,
/// Showdown syntax).
type Resolved = Vec<(usize, usize, String)>;

/// Choice `index` of side `s` in battle `i` (named in errors), as Showdown
/// syntax: a team-preview order, or a joint choice of the player's view.
fn choice_at(b: &Battle, i: usize, s: usize, index: usize) -> Result<String, String> {
    let mut seen = 0usize;
    let mut picked = None;
    if b.request == RequestState::TeamPreview {
        b.for_each_team_preview_order(s, &mut |o| {
            if seen == index {
                let p: String = o.iter().map(|p| (p + 1).to_string()).collect();
                picked = Some(format!("team {p}"));
            }
            seen += 1;
        });
    } else {
        b.for_each_player_choice(s, &mut |c| {
            if seen == index {
                picked = Some(b.choice_string(c));
            }
            seen += 1;
        });
    }
    picked.ok_or_else(|| format!("battle {i} side {s}: choice {index} of {seen}"))
}
