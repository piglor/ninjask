//! The scenario battle and the declared state, applied the way the reference probe applies it.
//!
//! Every setter here is the engine's own (`set_weather`, `set_terrain`, `add_side_condition`,
//! `run_mega_evo`, the `SetStatus` event): a declaration that the engine refuses, or that an
//! effect of the engine changes behind its back, is an abstention, never an approximation.

use crate::battle::{Battle, MonRef};
use crate::dex::team::MonSet;
use crate::dex::types::*;
use crate::kinds::EffectRef;
use crate::rng::Rng;
use crate::state::{RequestState, SideCond, Terrain, Weather};

use super::outcome::Abstain;
use super::spec::{Combatant, FieldState};
use super::Prober;

/// Showdown's scenario seed (`SCENARIO_SEED` of the reference probe): a scenario battle draws nothing the
/// probe reads, but a set with no declared gender is drawn from it.
const SCENARIO_SEED: &str = "sodium,00000000000000000000000000000000";

/// How a query's side state names one of its four side conditions.
type SideGetter = fn(&super::spec::SideState) -> bool;

/// The side conditions a query declares, with the engine's ids.
const SIDE_CONDITIONS: [(SideGetter, SideCond); 4] = [
    (|s| s.aurora_veil, SideCond::AuroraVeil),
    (|s| s.light_screen, SideCond::LightScreen),
    (|s| s.reflect, SideCond::Reflect),
    (|s| s.tailwind, SideCond::Tailwind),
];

pub(super) type Teams = [Vec<MonSet>; 2];

impl Prober {
    /// The registered set `id`, or the abstention for a query over a set the dex refused.
    pub(super) fn registered(&self, id: u32) -> Result<&super::Registered, Abstain> {
        self.sets
            .get(id as usize)
            .and_then(|s| s.as_ref())
            .ok_or_else(|| Abstain::new("state:set", "the set is not registered"))
    }

    /// The scenario battle of `teams`, started and not yet declared: a copy of the kept start.
    pub(super) fn started(&mut self, key: Vec<u32>, teams: &Teams) -> Result<Battle, Abstain> {
        if let Some(b) = self.starts.get(&key) {
            self.counts.reused += 1;
            return Ok(b.clone());
        }
        let rng = Rng::from_showdown_seed(SCENARIO_SEED)
            .map_err(|_| Abstain::new("state:scenario", "the scenario seed"))?;
        let mut b =
            Battle::from_sets_rng(self.dex.clone(), [&teams[0], &teams[1]], ["p1", "p2"], rng);
        b.log_enabled = false;
        b.start_without_preview();
        if b.request != RequestState::Move || b.all_active().len() != 4 || b.ended {
            return Err(Abstain::new(
                "state:scenario",
                "the scenario did not start with two Pokemon a side",
            ));
        }
        self.counts.started += 1;
        self.clear_starts_if_full();
        self.starts.insert(key, b.clone());
        Ok(b)
    }
}

/// The weather/terrain/pseudo-weather/side states of `state`, set on `b` absolute: every one cleared and then set as
/// declared (the reference's `declareField`). `lead` is the Pokémon the setters name as source.
///
/// `clear_first`: the damage path clears the weather and the terrain before it sets the declared ones (two events); the
/// speed path only clears what is not declared and sets what is (a weather already up is not set again: no event).
pub(super) fn declare_field(
    b: &mut Battle,
    state: &FieldState,
    lead: MonRef,
    clear_first: bool,
) -> Result<(), Abstain> {
    // The reference clears Trick Room and the four side conditions first on the speed path and
    // never sets them on a fresh scenario on the damage path; both are "absolute" and a fresh
    // scenario holds none of them, so setting is enough, but a clear is made for the second pass.
    b.field.trick_room = 0;
    for side in 0..2 {
        for (_, cond) in SIDE_CONDITIONS {
            *b.sides[side].conds.get_mut(cond) = 0;
        }
    }
    if clear_first {
        b.clear_weather();
        b.clear_terrain();
    }
    match &state.weather {
        None => b.clear_weather(),
        Some(id) => {
            let w = Weather::from_id(id).ok_or_else(|| {
                Abstain::new(
                    format!("state:weather:{id}"),
                    "weather the engine does not model",
                )
            })?;
            b.set_weather(w, lead, None);
            if b.field.weather != Some(w) {
                return Err(Abstain::new(
                    format!("state:weather:{id}"),
                    "the engine refused the weather",
                ));
            }
        }
    }
    match &state.terrain {
        None => b.clear_terrain(),
        Some(id) => {
            let t = Terrain::from_id(id).ok_or_else(|| {
                Abstain::new(
                    format!("state:terrain:{id}"),
                    "terrain the engine does not model",
                )
            })?;
            b.set_terrain(t, lead, None);
            if b.field.terrain != Some(t) {
                return Err(Abstain::new(
                    format!("state:terrain:{id}"),
                    "the engine refused the terrain",
                ));
            }
        }
    }
    if state.trick_room {
        let turns = b.cond_u8("trickroom", "duration", 5);
        b.field.trick_room = turns;
    }
    for (index, declared) in state.sides.iter().enumerate() {
        for (get, cond) in SIDE_CONDITIONS {
            if get(declared) {
                let source = (index, 0);
                b.add_side_condition(index, cond, source);
                if b.sides[index].conds.get(cond) == 0 {
                    return Err(Abstain::new(
                        format!("state:side:{}", cond.id()),
                        "the engine refused the side condition",
                    ));
                }
            }
        }
    }
    Ok(())
}

/// Evolve a Pokémon that is declared evolved (or evolves this turn): the engine's own `runMegaEvo`, refused when the
/// Pokémon did not change (no stone, a side that has Mega Evolved already).
pub(super) fn evolve(b: &mut Battle, r: MonRef) -> Result<(), Abstain> {
    let before = b.mon(r).species;
    b.run_mega_evo(r);
    if b.mon(r).species == before {
        return Err(Abstain::new("state:mega", "the Pokemon cannot Mega Evolve"));
    }
    Ok(())
}

/// A declared major status through the engine's own `SetStatus` event and `AfterSetStatus`, with type immunities
/// ignored (an observed fact: Corrosion poisons a Steel type). The result must be the declared status: an ability or
/// item (Lum Berry) that undoes it is a refusal.
pub(super) fn declare_status(b: &mut Battle, r: MonRef, id: &str) -> Result<(), Abstain> {
    let st = match Status::from_id(id) {
        Some(
            st
            @ (Status::Brn | Status::Par | Status::Psn | Status::Tox | Status::Slp | Status::Frz),
        ) => st,
        _ => {
            return Err(Abstain::new(
                format!("state:status:{id}"),
                "status the engine does not model",
            ))
        }
    };
    let m = b.mon(r);
    if m.hp == 0 || m.status != Status::None {
        return Err(Abstain::new(
            format!("state:status:{id}"),
            "the engine does not take that status",
        ));
    }
    if b.run_set_status(r, st, r, EffectRef::Other, false, false)
        .is_some()
    {
        return Err(Abstain::new(
            format!("state:status:{id}"),
            "the engine refused the status",
        ));
    }
    let time = match st {
        Status::Slp => b.rng.sample(&[2u8, 3, 3]),
        Status::Frz => 3,
        _ => 0,
    };
    let m = b.mon_mut(r);
    m.status = st;
    m.status_state = st;
    m.status_time = time;
    m.status_start_time = time;
    m.toxic_stage = 0;
    b.after_set_status(r, st, r, EffectRef::Other);
    if b.mon(r).status != st {
        return Err(Abstain::new(
            format!("state:status:{id}"),
            "the engine undid the status",
        ));
    }
    Ok(())
}

/// A declared Pokémon's HP (the current HP, 1 to the maximum).
pub(super) fn declare_hp(b: &mut Battle, r: MonRef, c: &Combatant) -> Result<(), Abstain> {
    if let Some(hp) = c.hp {
        let m = b.mon_mut(r);
        if hp as u16 > m.maxhp || hp == 0 {
            return Err(Abstain::new("state:hp", "hp is above the maximum"));
        }
        m.hp = hp as u16;
    }
    Ok(())
}

/// The declared stages and items, absolute, written after every Evolution: a Mega's ability can lower another
/// Pokémon's stage (Intimidate) and consume its item (White Herb). The item is put back silently, not through the
/// engine's item setter whose start event would fire a Seed item again.
pub(super) fn restore_stages_and_item(
    b: &mut Battle,
    r: MonRef,
    c: &Combatant,
    item: Option<ItemId>,
) {
    let m = b.mon_mut(r);
    if m.item != item {
        m.item = item;
    }
    let stages = c.boosts.as_array();
    for (i, s) in stages.iter().enumerate() {
        m.boosts[i] = *s;
    }
}
