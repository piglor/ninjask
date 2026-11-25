//! The declared-state layer's input: a set table and queries over it.
//!
//! The shapes mirror Areneon's `MechanicsQuery` (`damage`, `speed_order`, `stats`), with each
//! PokePaste replaced by the index of a set the caller registered first (the caller has judged
//! the paste: the engine's own validator is the authority on legality, never this crate). Every
//! struct refuses a field it does not know (`deny_unknown_fields`), so a query that carries
//! something this layer does not implement (a `history` block, a new field of a later contract)
//! is refused, never silently read without it.

use serde::Deserialize;

/// One Pokémon as the caller's validator accepted it: names (any spelling `to_id` maps to an id),
/// Stat Points in Showdown's stat order (HP, Atk, Def, SpA, SpD, Spe), the format's level when
/// `level` is absent.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SetSpec {
    pub species: String,
    #[serde(default)]
    pub item: String,
    pub ability: String,
    pub moves: Vec<String>,
    pub nature: String,
    #[serde(default)]
    pub gender: String,
    pub points: [u16; 6],
    pub level: Option<u8>,
}

/// Absolute stat stages.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Stages {
    pub atk: i8,
    pub def: i8,
    pub spa: i8,
    pub spd: i8,
    pub spe: i8,
}

impl Stages {
    pub fn as_array(&self) -> [i8; 5] {
        [self.atk, self.def, self.spa, self.spd, self.spe]
    }
}

/// One Pokémon with its declared state.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Combatant {
    /// Index into the set table.
    pub set: u32,
    pub boosts: Stages,
    pub status: Option<String>,
    pub hp: Option<u32>,
    pub mega: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SideState {
    pub aurora_veil: bool,
    pub light_screen: bool,
    pub reflect: bool,
    pub tailwind: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FieldState {
    pub weather: Option<String>,
    pub terrain: Option<String>,
    pub trick_room: bool,
    pub sides: [SideState; 2],
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Action {
    pub side: usize,
    pub position: usize,
    /// A move id, or `None` for a switch.
    #[serde(rename = "move")]
    pub move_id: Option<String>,
    pub mega: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Query {
    Stats {
        set: u32,
    },
    Damage {
        attacker: Combatant,
        defender: Combatant,
        #[serde(rename = "move")]
        move_id: String,
        state: FieldState,
    },
    SpeedOrder {
        sides: [Vec<Combatant>; 2],
        actions: Vec<Action>,
        state: FieldState,
    },
}
