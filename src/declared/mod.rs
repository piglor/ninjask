//! The declared-state layer: the engine behind Areneon's mechanics probe interface.
//!
//! Areneon asks "what does this attacker's move do to this defender in this declared state"
//! (`damage`), "in what order do these declared actions go" (`speed_order`) and "what are this
//! set's stats" (`stats`), for declared sets and an absolute declared state, never a live
//! battle. Pokémon Showdown answers by building a scenario battle and reading its own values
//! (the *probe*, about 1 to 2 ms a query); this module answers the same questions from this
//! engine's calculators ([`crate::calc`], about 10 us a query), through a scenario battle of
//! its own that it builds and declares the same way:
//!
//! 1. two Pokémon a side (a damage query's partner has the species and nothing else: no
//!    ability, no item), started at once with no Team Preview, on Showdown's scenario seed;
//! 2. each combatant's declared Mega Evolution (the engine's own `runMegaEvo`), HP, major status
//!    (the engine's own `SetStatus` events, type immunities ignored: an observed fact) and
//!    stat stages;
//! 3. the declared field, absolute: weather, terrain, Trick Room and the four side conditions
//!    cleared and set through the engine's own setters, so a forme change a weather starts
//!    (Forecast, Mimicry, Flower Gift) happens as it does in play;
//! 4. the stages and items put back after every Evolution (a Mega's ability can lower a stage
//!    or consume an item, and the declaration is absolute).
//!
//! **This layer never guesses.** What it cannot declare the way the reference does, a move whose
//! pre-hit steps it does not model, a state the engine refuses, a history block, is an
//! [`Outcome::Abstain`] naming its entity, and the caller routes that query to the Showdown probe
//! (Showdown stays the oracle and the only authority; the admission ledger is per entity). What
//! it answers it answers in the probe's own closed vocabulary (`computed`, or `unsupported` with
//! the probe's reason), so a result differs from the probe's in no byte.
//!
//! The layer holds no table of game content: every id comes from the [`Dex`] export of the pinned
//! Showdown, every number from the engine. The wire form is [`wire`].

mod classify;
mod damage;
mod outcome;
#[cfg(feature = "python")]
pub(crate) mod python;
mod scenario;
mod spec;
mod speed;
mod stats;
pub mod wire;

pub use outcome::{Abstain, ActionOut, Answer, CritChance, Ko, Outcome, Reason, StatsOut};
pub use spec::{Action, Combatant, FieldState, Query, SetSpec, SideState, Stages};

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use crate::battle::Battle;
use crate::dex::team::MonSet;
use crate::dex::types::ItemId;
use crate::dex::Dex;

/// A registered set: the engine's `MonSet`, the partner of a damage query's scenario (the same
/// species and moves, no ability, no item), and the item the set declares.
pub(crate) struct Registered {
    pub(crate) mon: MonSet,
    pub(crate) partner: MonSet,
    pub(crate) item: Option<ItemId>,
}

/// The mechanics config's `ko_blockers`: items and abilities (engine ids) after which the KO
/// counts of a damage answer are withheld.
#[derive(Clone, Debug, Default)]
pub struct KoBlockers {
    pub items: HashSet<String>,
    pub abilities: HashSet<String>,
}

/// How many started scenarios are kept before the table is emptied (a started scenario is a pure
/// function of its key, so forgetting only costs time).
const START_LIMIT: usize = 4096;

/// Counters for diagnostics; no answer depends on them.
#[derive(Clone, Debug, Default)]
pub struct Counts {
    pub answered: u64,
    pub unsupported: u64,
    pub abstained: u64,
    pub started: u64,
    pub reused: u64,
    /// Abstentions by entity.
    pub by_entity: HashMap<String, u64>,
}

pub struct Prober {
    pub(crate) dex: Arc<Dex>,
    pub(crate) sets: Vec<Option<Registered>>,
    pub(crate) blockers: KoBlockers,
    pub(crate) starts: HashMap<Vec<u32>, Battle>,
    pub counts: Counts,
}

impl Prober {
    pub fn new(dex: Arc<Dex>) -> Prober {
        Prober {
            dex,
            sets: Vec::new(),
            blockers: KoBlockers::default(),
            starts: HashMap::new(),
            counts: Counts::default(),
        }
    }

    pub fn set_ko_blockers(&mut self, blockers: KoBlockers) {
        self.blockers = blockers;
    }

    pub(crate) fn clear_starts_if_full(&mut self) {
        if self.starts.len() >= START_LIMIT {
            self.starts.clear();
        }
    }

    /// Register set number `id`. A set the dex does not know (an unknown species, item, ability
    /// or move) is an error naming the field; queries over it are abstained.
    pub fn register_set(&mut self, id: u32, spec: &SetSpec) -> Result<(), String> {
        let dex = &self.dex;
        let species = dex
            .species_id(&spec.species)
            .ok_or_else(|| format!("unknown species {}", spec.species))?;
        let item = if spec.item.is_empty() {
            None
        } else {
            Some(
                dex.item_id(&spec.item)
                    .ok_or_else(|| format!("unknown item {}", spec.item))?,
            )
        };
        let ability = dex
            .ability_id(&spec.ability)
            .ok_or_else(|| format!("unknown ability {}", spec.ability))?;
        let no_ability = dex
            .ability_id("noability")
            .ok_or_else(|| "the dex has no No Ability".to_string())?;
        let mut moves = Vec::with_capacity(spec.moves.len());
        for m in &spec.moves {
            moves.push(dex.move_id(m).ok_or_else(|| format!("unknown move {m}"))?);
        }
        let level = dex.format.level.or(spec.level).unwrap_or(100);
        let sp = dex.species(species);
        // Showdown's Pokemon constructor: a nickname equal to the species name is none.
        let mon = MonSet {
            nickname: String::new(),
            species,
            item,
            ability,
            moves: moves.clone(),
            nature: spec.nature.clone(),
            points: spec.points,
            gender: spec.gender.clone(),
            level,
            shiny: false,
        };
        // The partner of a damage query (`partnerOf`): the species and nothing else of its own.
        let partner = MonSet {
            nickname: sp.name.clone(),
            item: None,
            ability: no_ability,
            ..mon.clone()
        };
        let slot = id as usize;
        if self.sets.len() <= slot {
            self.sets.resize_with(slot + 1, || None);
        }
        self.sets[slot] = Some(Registered { mon, partner, item });
        // A started scenario that held the replaced set is stale.
        self.starts.clear();
        Ok(())
    }

    /// Answer one query. Never panics on a refusal: an abstention is an outcome.
    pub fn answer(&mut self, query: &Query) -> Outcome {
        let out = match query {
            Query::Stats { set } => self.stats(*set),
            Query::Damage {
                attacker,
                defender,
                move_id,
                state,
            } => self.damage(attacker, defender, move_id, state),
            Query::SpeedOrder {
                sides,
                actions,
                state,
            } => self.speed_order(sides, actions, state),
        };
        match &out {
            Outcome::Computed { .. } => self.counts.answered += 1,
            Outcome::Unsupported { .. } => self.counts.unsupported += 1,
            Outcome::Abstain { abstain } => {
                self.counts.abstained += 1;
                *self
                    .counts
                    .by_entity
                    .entry(abstain.entity.clone())
                    .or_insert(0) += 1;
            }
        }
        out
    }
}
