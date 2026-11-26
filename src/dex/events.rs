//! Handler tables for the events the engine sorts, built once at load.
//!
//! Showdown's `runEvent` collects every handler for an event (`on{Event}` on
//! the target, `onSource{Event}` on the source, `onAlly{Event}`,
//! `onFoe{Event}` and `onAny{Event}` on the active Pokémon) and orders them
//! with `speedSort`, drawing between equal ones. Looking those callbacks up
//! by name for every event is slow, so each ability and item gets a table of
//! its handlers' priorities, indexed by [`Event`] and [`Role`].

use crate::dex::Handlers;

/// The events whose ability and item handlers the engine sorts. `ModifyMove` and
/// `Update` are here for the draws their sorts make, not for what the handlers do
/// (the engine runs those where it needs them): the sort's framing, `shuffle(len, ...)`,
/// counts every handler Showdown collects.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    AfterBoost,
    AfterEachBoost,
    AfterFaint,
    BasePower,
    ChangeBoost,
    Immunity,
    ModifyAccuracy,
    ModifyAtk,
    ModifyBoost,
    ModifyDamage,
    ModifyDef,
    ModifyMove,
    ModifySpA,
    ModifySpD,
    ModifySpe,
    SetStatus,
    TryAddVolatile,
    TryBoost,
    TryEatItem,
    TryMove,
    Update,
    BeforeMove,
    Damage,
    DisableMove,
    MaybeTrapPokemon,
    TrapPokemon,
    TryHeal,
    TryHit,
    SwitchIn,
    TryHitSide,
}

impl Event {
    pub const ALL: [Event; 30] = [
        Event::AfterBoost,
        Event::AfterEachBoost,
        Event::AfterFaint,
        Event::BasePower,
        Event::ChangeBoost,
        Event::Immunity,
        Event::ModifyAccuracy,
        Event::ModifyAtk,
        Event::ModifyBoost,
        Event::ModifyDamage,
        Event::ModifyDef,
        Event::ModifyMove,
        Event::ModifySpA,
        Event::ModifySpD,
        Event::ModifySpe,
        Event::SetStatus,
        Event::TryAddVolatile,
        Event::TryBoost,
        Event::TryEatItem,
        Event::TryMove,
        Event::Update,
        Event::BeforeMove,
        Event::Damage,
        Event::DisableMove,
        Event::MaybeTrapPokemon,
        Event::TrapPokemon,
        Event::TryHeal,
        Event::TryHit,
        Event::SwitchIn,
        Event::TryHitSide,
    ];
    pub const COUNT: usize = Event::ALL.len();

    /// The event's name in Showdown's callbacks (`on{name}`).
    pub fn name(self) -> &'static str {
        match self {
            Event::AfterBoost => "AfterBoost",
            Event::AfterEachBoost => "AfterEachBoost",
            Event::AfterFaint => "AfterFaint",
            Event::BasePower => "BasePower",
            Event::ChangeBoost => "ChangeBoost",
            Event::Immunity => "Immunity",
            Event::ModifyAccuracy => "ModifyAccuracy",
            Event::ModifyAtk => "ModifyAtk",
            Event::ModifyBoost => "ModifyBoost",
            Event::ModifyDamage => "ModifyDamage",
            Event::ModifyDef => "ModifyDef",
            Event::ModifyMove => "ModifyMove",
            Event::ModifySpA => "ModifySpA",
            Event::ModifySpD => "ModifySpD",
            Event::ModifySpe => "ModifySpe",
            Event::SetStatus => "SetStatus",
            Event::TryAddVolatile => "TryAddVolatile",
            Event::TryBoost => "TryBoost",
            Event::TryEatItem => "TryEatItem",
            Event::TryMove => "TryMove",
            Event::Update => "Update",
            Event::BeforeMove => "BeforeMove",
            Event::Damage => "Damage",
            Event::DisableMove => "DisableMove",
            Event::MaybeTrapPokemon => "MaybeTrapPokemon",
            Event::TrapPokemon => "TrapPokemon",
            Event::TryHeal => "TryHeal",
            Event::TryHit => "TryHit",
            Event::SwitchIn => "SwitchIn",
            Event::TryHitSide => "TryHitSide",
        }
    }
}

/// Where a handler's holder stands relative to the event's target: the
/// prefix of Showdown's callback name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    /// `on{Event}`: the target itself.
    Own,
    /// `onSource{Event}`: the event's source.
    Source,
    /// `onAlly{Event}`: a Pokémon on the target's side, itself included.
    Ally,
    /// `onFoe{Event}`: a Pokémon on the other side.
    Foe,
    /// `onAny{Event}`: any active Pokémon.
    Any,
}

impl Role {
    pub const ALL: [Role; 5] = [Role::Own, Role::Source, Role::Ally, Role::Foe, Role::Any];
    pub const COUNT: usize = Role::ALL.len();

    /// The callback name's prefix.
    pub fn prefix(self) -> &'static str {
        match self {
            Role::Own => "on",
            Role::Source => "onSource",
            Role::Ally => "onAlly",
            Role::Foe => "onFoe",
            Role::Any => "onAny",
        }
    }
}

/// One effect's handlers for the sorted events: the priority of each
/// handler it has (its `...Priority` field, 0 when absent).
#[derive(Clone, Debug)]
pub struct EventHandlers {
    /// Bit `e` is set when the effect has a handler, in any role, for the
    /// event numbered `e`: most effects have none, and the test is cheaper
    /// than the table.
    events: u32,
    priority: [[Option<i32>; Role::COUNT]; Event::COUNT],
}

impl EventHandlers {
    /// Reads the table from an effect's callback names and numeric fields.
    pub fn new(effect: &impl Handlers) -> EventHandlers {
        EventHandlers::build(effect, false)
    }

    /// The table of an ability or an item: its `onStart` is also its `SwitchIn` handler
    /// (`getCallback`: Showdown runs these two effects' `onStart` as part of the `SwitchIn`
    /// event, with the `onSwitchInPriority` of the effect), unless it has an `onAnySwitchIn` of
    /// its own, which keeps the `onStart` for the Pokémon's own start.
    pub fn for_ability_or_item(effect: &impl Handlers) -> EventHandlers {
        EventHandlers::build(effect, true)
    }

    fn build(effect: &impl Handlers, start_is_switch_in: bool) -> EventHandlers {
        let mut events = 0;
        let mut priority = [[None; Role::COUNT]; Event::COUNT];
        for (e, event) in Event::ALL.into_iter().enumerate() {
            for (r, role) in Role::ALL.into_iter().enumerate() {
                let callback = format!("{}{}", role.prefix(), event.name());
                if effect.has_callback(&callback) {
                    let p = effect.num(&format!("{callback}Priority")).unwrap_or(0.0);
                    priority[e][r] = Some(p as i32);
                    events |= 1 << e;
                }
            }
        }
        if start_is_switch_in
            && effect.has_callback("onStart")
            && !effect.has_callback("onSwitchIn")
            && !effect.has_callback("onAnySwitchIn")
        {
            let e = Event::SwitchIn as usize;
            priority[e][Role::Own as usize] =
                Some(effect.num("onSwitchInPriority").unwrap_or(0.0) as i32);
            events |= 1 << e;
        }
        EventHandlers { events, priority }
    }

    /// The priority of the effect's `role` handler for `event`, if it has one.
    #[inline]
    pub fn priority(&self, event: Event, role: Role) -> Option<i32> {
        if self.events & (1 << event as u32) == 0 {
            return None;
        }
        self.priority[event as usize][role as usize]
    }

    /// Whether the effect has a handler for `event` in any role.
    #[inline]
    pub fn has_event(&self, event: Event) -> bool {
        self.events & (1 << event as u32) != 0
    }

    /// Whether the effect has a `role` handler for `event`.
    #[inline]
    pub fn has(&self, event: Event, role: Role) -> bool {
        self.priority(event, role).is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn events_fit_the_mask() {
        assert!(Event::COUNT <= u32::BITS as usize);
    }

    #[test]
    fn tables_index_in_declaration_order() {
        for (i, e) in Event::ALL.into_iter().enumerate() {
            assert_eq!(e as usize, i);
        }
        for (i, r) in Role::ALL.into_iter().enumerate() {
            assert_eq!(r as usize, i);
        }
    }
}
