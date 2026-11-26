//! What the declared-state layer answers: the answer part of Areneon's `MechanicsResult`
//! (`computed`, or `unsupported` with the probe's own closed reason), or an abstention.
//!
//! An abstention is not an answer: it says "this layer does not claim to equal the Showdown
//! probe here" and names the entity (a move, ability, item, a state kind) that decides it, so
//! the caller routes the query to the Showdown probe and counts it per entity. A layer that
//! guessed instead would make the admission evidence worthless: every refusal of the engine, every
//! state this layer cannot declare the way the reference does and every move whose pre-hit
//! steps it does not model is an abstention.

use serde::Serialize;

/// The closed reasons of an `unsupported` result the layer itself decides (the contract has more:
/// `scenario_rule_table` and `history_unknown` belong to the caller's envelope).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Reason {
    QueryKind,
    RandomEffect,
}

/// A damage answer's crit chance as the engine's own `randomChance` pair, or a crit it always
/// makes, or none.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CritChance {
    Never,
    Always,
    Chance { numerator: u32, denominator: u32 },
}

/// KO counts from the roll vectors, or withheld when the defender holds a blocker.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Ko {
    Counts {
        ohko_no_crit: u32,
        ohko_crit: u32,
        thko_no_crit: u32,
    },
    Unsupported {
        reason: &'static str,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct StatsOut {
    pub hp: u32,
    pub atk: u32,
    pub def: u32,
    pub spa: u32,
    pub spd: u32,
    pub spe: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct ActionOut {
    pub side: usize,
    pub position: usize,
    pub mega: bool,
    pub order: u32,
    pub priority: i32,
    pub speed: i32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Answer {
    Stats {
        stats: StatsOut,
    },
    Damage {
        no_crit: [u32; 16],
        crit: [u32; 16],
        crit_chance: CritChance,
        hp_current: u32,
        hp_max: u32,
        ko: Ko,
    },
    SpeedOrder {
        order_basis: &'static str,
        actions: Vec<ActionOut>,
        groups: Vec<Vec<usize>>,
    },
}

/// A decision not to answer, with the entity that decides it and a fixed reason.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Abstain {
    /// `<kind>:<id>` (`move:suckerpunch`, `ability:protean`, `item:lumberry`, `state:status`,
    /// `query:history`...): what the caller counts and, at admission, leaves unadmitted.
    pub entity: String,
    pub why: &'static str,
}

impl Abstain {
    pub fn new(entity: impl Into<String>, why: &'static str) -> Abstain {
        Abstain {
            entity: entity.into(),
            why,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Outcome {
    Computed {
        answer: Answer,
    },
    Unsupported {
        reason: Reason,
        position: Option<u32>,
    },
    Abstain {
        #[serde(flatten)]
        abstain: Abstain,
    },
}

impl From<Abstain> for Outcome {
    fn from(abstain: Abstain) -> Outcome {
        Outcome::Abstain { abstain }
    }
}
