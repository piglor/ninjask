//! What the layer answers and what it leaves to the reference probe.
//!
//! The reference runs the engine's whole move step (`useMoveInner`) with only the accuracy step and the damage loop
//! replaced; this layer runs the read-only calculator. They agree on every damage number (the calculator IS the
//! engine's `getDamage`), but the reference also runs steps the calculator does not: a move's own `onTry`, `TryMove`,
//! `PrepareHit` and `TryHit` handlers, the `TryMove` event of the abilities in play, and it classifies a move that never
//! reaches the damage loop as `unsupported` (`query_kind`). The callbacks a move's data carries tell which moves have
//! such steps (the export lists each handler by name); a move with one is an abstention unless this module models the
//! step ([`Pre`]: each modelled kind is held by a differential test against the reference probe).

use crate::battle::{Battle, MonRef};
use crate::dex::types::*;
use crate::kinds::{ChargeKind, EffectRef, MoveKind};

use super::outcome::Abstain;
use super::Prober;

/// Handlers of a move that the reference runs before the damage loop and the calculator does not model. A move with one
/// is an abstention unless its kind is modelled in [`Prober::pre_hit`]. (`onMoveFail` runs after the move failed: it
/// changes no number, so it is not listed.)
const PRE_HIT: [&str; 6] = [
    "onTry",
    "onTryMove",
    "onTryHit",
    "onPrepareHit",
    "onModifyTarget",
    "onUseMoveMessage",
];

/// The result of the move's own steps before the hit, as the reference classifies it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Pre {
    /// The move reaches the damage loop.
    Proceed,
    /// The move stops before the loop (a failed `onTry`, a charge turn): `unsupported` (`query_kind`).
    Stops,
}

impl Prober {
    /// The move's own pre-hit steps, run on the scenario battle `b` (a disposable copy): what the reference's
    /// `useMoveInner` does between choosing the move and the damage loop, for the kinds this layer models; an
    /// abstention for a move with such a step it does not model.
    pub(super) fn pre_hit(
        &self,
        b: &mut Battle,
        r: MonRef,
        t: MonRef,
        id: MoveId,
    ) -> Result<Pre, Abstain> {
        let mv = self.dex.mv(id);
        let kind = self.dex.move_kind[id as usize];
        let has = |c: &str| mv.callbacks.iter().any(|x| x == c);
        // Beat Up makes itself a multi-hit move in its `ModifyMove` (no pre-hit callback names it).
        if kind == MoveKind::BeatUp {
            return Ok(Pre::Stops);
        }
        if !PRE_HIT.iter().any(|c| has(c)) {
            return Ok(Pre::Proceed);
        }
        let unmodelled = |why: &'static str| Abstain::new(format!("move:{}", mv.id), why);
        let stops_if = |fails: bool| Ok(if fails { Pre::Stops } else { Pre::Proceed });
        let pre = match kind {
            // A charge turn ends the move; in its weather the move is instant. Power Herb's `ChargeMove` is not modelled.
            MoveKind::Charge(ck) => {
                if b.mon(r)
                    .item
                    .is_some_and(|i| self.dex.item(i).id == "powerherb")
                {
                    return Err(Abstain::new(
                        "item:powerherb",
                        "a charge move's instant charge is not modelled",
                    ));
                }
                // Electro Shot and Meteor Beam raise Special Attack first, charge turn or not.
                if ck.spa_boost() {
                    let mut up = [0i8; 7];
                    up[Boost::Spa as usize] = 1;
                    b.boost(r, &up, Some(r), EffectRef::Move(id), false, false);
                }
                match ck {
                    ChargeKind::ElectroShot | ChargeKind::Solar => {
                        stops_if(b.charge_move_weather(r, ck) != ck.instant_in())?
                    }
                    _ => Pre::Stops,
                }
            }
            MoveKind::DoubleShock | MoveKind::BurnUp => {
                let ty = if kind == MoveKind::DoubleShock {
                    "Electric"
                } else {
                    "Fire"
                };
                stops_if(!self.dex.type_id(ty).is_some_and(|e| b.has_type_of(r, e)))?
            }
            MoveKind::Snore => stops_if(b.mon(r).status != Status::Slp)?,
            // Turn zero: no action done yet.
            MoveKind::FakeOut | MoveKind::FirstImpression => {
                stops_if(b.mon(r).active_move_actions > 1)?
            }
            // These read a queue the scenario does not have.
            MoveKind::SuckerPunch | MoveKind::UpperHand => Pre::Stops,
            // Last Resort needs every other move used; Spit Up needs a Stockpile: a fresh scenario has neither.
            MoveKind::LastResort | MoveKind::SpitUp => Pre::Stops,
            MoveKind::Belch => stops_if(!b.mon(r).ate_berry)?,
            MoveKind::Poltergeist => stops_if(b.mon(t).item.is_none())?,
            MoveKind::SteelRoller => stops_if(b.field.terrain.is_none())?,
            // Fling needs an item it can throw.
            MoveKind::Fling => stops_if(b.fling_item(r).is_none())?,
            // Future Sight only sets a slot condition: no damage this turn.
            MoveKind::FutureSight => Pre::Stops,
            // Focus Punch's `onTry` needs a lost-focus volatile; Pollen Puff's only matters aimed at an ally (a Heal Block).
            MoveKind::FocusPunch | MoveKind::PollenPuff => Pre::Proceed,
            // Round moves another Round up in the queue: there is none.
            MoveKind::Round => Pre::Proceed,
            // The weight moves' own `onTryHit` only refuses a Dynamaxed target.
            MoveKind::WeightPower | MoveKind::WeightRatioPower => Pre::Proceed,
            // The move's own `TryHit` (screens broken, sleepers woken) runs inside the hit loop, which the reference
            // replaces: it changes nothing here.
            MoveKind::ScreenBreak | MoveKind::RagingBull | MoveKind::Uproar => Pre::Proceed,
            _ => {
                return Err(unmodelled(
                    "the move has a step before the hit the layer does not model",
                ))
            }
        };
        Ok(pre)
    }

    /// The `TryMove` event of the abilities in play (Armor Tail, Dazzling, Queenly Majesty, Damp) blocks the move before the
    /// hit: `unsupported`. The event reads the move's own priority, the dex's (the reference has no queue to modify it).
    pub(super) fn try_move_blocks(&self, b: &mut Battle, r: MonRef, t: MonRef, id: MoveId) -> bool {
        let mv = self.dex.mv(id);
        b.priority_override = Some((r, id, mv.priority));
        // Mold Breaker's `ModifyMove` sets the move's ignore-ability before `TryMove`.
        b.run_ability_modify_move(r, mv);
        let blocked = b.run_try_move(r, t, mv, 0);
        b.priority_override = None;
        b.ability_breaker = None;
        blocked
    }
}
