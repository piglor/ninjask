//! The `damage` query: two 16-roll vectors, the crit rule, the defender's HP and the KO counts.

use crate::battle::{Battle, MonRef};
use crate::calc::CritRule;
use crate::dex::types::*;
use crate::kinds::{AbilityKind, MoveKind};
use crate::state::Terrain;

use super::classify::Pre;
use super::outcome::{Abstain, Answer, CritChance, Ko, Outcome, Reason};
use super::scenario::{
    declare_field, declare_hp, declare_status, evolve, restore_stages_and_item, Teams,
};
use super::spec::{Combatant, FieldState};
use super::Prober;

const ATTACKER: MonRef = (0, 0);
const DEFENDER: MonRef = (1, 0);

impl Prober {
    pub(super) fn damage(
        &mut self,
        a: &Combatant,
        d: &Combatant,
        mv: &str,
        state: &FieldState,
    ) -> Outcome {
        match self.damage_inner(a, d, mv, state) {
            Ok(outcome) => outcome,
            Err(abstain) => abstain.into(),
        }
    }

    fn damage_inner(
        &mut self,
        a: &Combatant,
        d: &Combatant,
        mv: &str,
        state: &FieldState,
    ) -> Result<Outcome, Abstain> {
        let (ra, rd) = (self.registered(a.set)?, self.registered(d.set)?);
        let (item_a, item_d) = (ra.item, rd.item);
        let teams: Teams = [
            vec![ra.mon.clone(), ra.partner.clone()],
            vec![rd.mon.clone(), rd.partner.clone()],
        ];
        // The move: an engine move under exactly that id (the reference refuses any other id).
        let move_id = self
            .dex
            .move_id(mv)
            .filter(|&id| self.dex.mv(id).id == mv)
            .ok_or_else(|| Abstain::new(format!("move:{mv}"), "the move is not an engine move"))?;
        let mut b = self.started(vec![0, a.set, d.set], &teams)?;
        b.rng.trace = Some(Vec::new());

        // The declared state, in the reference's order: each combatant (Evolution, HP, status, stages), then the field,
        // then the stages and items again (an Evolution's ability can change either).
        for (r, c) in [(ATTACKER, a), (DEFENDER, d)] {
            if c.mega {
                evolve(&mut b, r)?;
            }
            declare_hp(&mut b, r, c)?;
            if let Some(status) = &c.status {
                declare_status(&mut b, r, status)?;
            }
            for (i, s) in c.boosts.as_array().iter().enumerate() {
                b.mon_mut(r).boosts[i] = *s;
            }
        }
        declare_field(&mut b, state, ATTACKER, true)?;
        restore_stages_and_item(&mut b, ATTACKER, a, item_a);
        restore_stages_and_item(&mut b, DEFENDER, d, item_d);

        let mdata = self.dex.mv(move_id);
        // Outside the contract's scope: a move that deals no damage of its own, multi-hit and fixed damage.
        if mdata.category == Category::Status
            || mdata.multihit.is_some()
            || mdata.fixed_damage.is_some()
            || mdata.ohko.is_some()
            || mdata.callbacks.iter().any(|c| c == "damageCallback")
        {
            return Ok(Outcome::Unsupported {
                reason: Reason::QueryKind,
                position: None,
            });
        }
        // `setActiveMove`: the move in use and its user (Mega Sol reads them: the sun the move sees).
        b.active_move.cur_move = Some(move_id);
        b.active_move.cur_user = Some(ATTACKER);
        self.target_gate(move_id)?;
        // The steps the reference runs before the damage loop: a stop is `unsupported` (`query_kind`).
        if self.try_move_blocks(&mut b, ATTACKER, DEFENDER, move_id)
            || self.pre_hit(&mut b, ATTACKER, DEFENDER, move_id)? == Pre::Stops
        {
            return Ok(Outcome::Unsupported {
                reason: Reason::QueryKind,
                position: None,
            });
        }

        // The move step's pre-hit effect on the attacker the calculator does not apply (Protean, Libero).
        let kind = self.dex.move_kind[move_id as usize];
        // A target the move's own ModifyMove changes (Expanding Force in Psychic Terrain) is drawn again by the engine; so is
        // Fickle Beam's power. The reference records any draw other than the roll and the crit as a random effect.
        let mut random_effect = kind == MoveKind::FickleBeam;
        let target_type = b
            .modify_move_target_type(ATTACKER, mdata, Some(DEFENDER))
            .unwrap_or_else(|| b.move_target_type(ATTACKER, mdata));
        if target_type != mdata.target {
            let before = b.rng_draws();
            b.get_random_target(ATTACKER, target_type);
            random_effect |= b.rng_draws() != before;
        }
        let (move_type, _) = b.move_type(ATTACKER, mdata, kind);
        b.run_prepare_hit(ATTACKER, mdata, move_type, false);

        // The event sorts the calculator skips are run (a tie of equal speeds draws, and the engine records that as a random effect).
        b.probe_sorts = true;
        b.calc_draws = 0;
        let calc = b.calc(ATTACKER, move_id, DEFENDER);
        random_effect |= b.calc_draws > 0;
        if !calc.damaging {
            return Ok(Outcome::Unsupported {
                reason: Reason::QueryKind,
                position: None,
            });
        }
        let hp = calc.target_hp;
        let max = calc.target_maxhp;
        // A move that deals nothing of its own is out of scope whatever else it does (the reference checks it first).
        if calc.hits.len() > 1 {
            return Ok(Outcome::Unsupported {
                reason: Reason::QueryKind,
                position: None,
            });
        }
        // The engine's immunity and protection steps remove the defender: the move deals nothing. Psychic Terrain's
        // `TryHit` stops a priority move at a grounded target; the step reads the move's own priority (the reference's
        // scenario has no queue to modify it), so it is the dex priority.
        let blocked = calc.immune || self.psychic_terrain_blocks(&b, move_id);
        // Magician's after-move event sorts the move's hit targets by speed: two equal speeds (the defender and its partner, the
        // same species) draw, and the reference records any draw but the roll and the crit as a random effect.
        if !blocked
            && calc.spread
            && b.has_ability(ATTACKER, AbilityKind::Magician)
            && b.mon(ATTACKER).item.is_none()
        {
            let mut speeds: Vec<i32> = b
                .all_active()
                .into_iter()
                .filter(|&x| x != ATTACKER && (x.0 != 0 || mdata.target == Target::AllAdjacent))
                .map(|x| b.mon(x).speed)
                .collect();
            let before = b.rng_draws();
            b.speed_sort_by(&mut speeds, |a, c| (*c as i64) - (*a as i64));
            random_effect |= b.rng_draws() != before;
        }
        if random_effect {
            return Ok(Outcome::Unsupported {
                reason: Reason::RandomEffect,
                position: None,
            });
        }
        let (no_crit, crit, crit_chance) = if blocked {
            ([0; 16], [0; 16], CritChance::Never)
        } else {
            let rolls = calc.hits.first().copied().unwrap_or_default();
            match calc.crit_rule {
                // A crit the target prevents is the no-crit hit.
                CritRule::Never => (rolls.normal, rolls.normal, CritChance::Never),
                CritRule::Always => (rolls.normal, rolls.crit, CritChance::Always),
                CritRule::Chance(n, dn) => (
                    rolls.normal,
                    rolls.crit,
                    CritChance::Chance {
                        numerator: n,
                        denominator: dn,
                    },
                ),
            }
        };
        let blocked_ko = {
            let m = b.mon(DEFENDER);
            let item = m.item.map(|i| self.dex.item(i).id.as_str()).unwrap_or("");
            self.blockers.items.contains(item)
                || self
                    .blockers
                    .abilities
                    .contains(self.dex.ability(m.ability).id.as_str())
        };
        let ko = if blocked_ko {
            Ko::Unsupported {
                reason: "ko_state_change",
            }
        } else {
            Ko::Counts {
                ohko_no_crit: count_kos(&no_crit, hp),
                ohko_crit: count_kos(&crit, hp),
                thko_no_crit: count_pair_kos(&no_crit, hp),
            }
        };
        Ok(Outcome::Computed {
            answer: Answer::Damage {
                no_crit,
                crit,
                crit_chance,
                hp_current: hp,
                hp_max: max,
                ko,
            },
        })
    }

    /// The target types whose hit the layer models: the ones that reach the defender in the reference's scenario.
    fn target_gate(&self, id: MoveId) -> Result<(), Abstain> {
        use Target::*;
        let mv = self.dex.mv(id);
        match mv.target {
            Normal | AdjacentFoe | Any | AllAdjacentFoes | AllAdjacent | RandomNormal => Ok(()),
            _ => Err(Abstain::new(
                format!("move:{}", mv.id),
                "a target type the layer does not model",
            )),
        }
    }

    /// Psychic Terrain's `onTryHit`: a priority move (the dex priority above zero, the reference's `priority > 0.1`)
    /// aimed at a grounded foe is stopped.
    fn psychic_terrain_blocks(&self, b: &Battle, id: MoveId) -> bool {
        b.field.terrain == Some(Terrain::Psychic)
            && self.dex.mv(id).priority > 0
            && b.is_grounded(DEFENDER)
    }
}

/// Rolls of a 16-roll vector whose damage is at least `hp`.
fn count_kos(rolls: &[u32; 16], hp: u32) -> u32 {
    rolls.iter().filter(|&&d| d >= hp).count() as u32
}

/// Ordered pairs of two independent rolls of one vector whose summed damage is at least `hp` (256 pairs).
fn count_pair_kos(rolls: &[u32; 16], hp: u32) -> u32 {
    let mut n = 0;
    for first in rolls {
        for second in rolls {
            if first + second >= hp {
                n += 1;
            }
        }
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ko_counts_are_integers_over_the_rolls() {
        // 16 rolls 100, 99, ... 85: against 90 HP the rolls of 90 and more are the first eleven.
        let mut rolls = [0u32; 16];
        for (i, r) in rolls.iter_mut().enumerate() {
            *r = 100 - i as u32;
        }
        assert_eq!(count_kos(&rolls, 90), 11);
        assert_eq!(count_kos(&rolls, 101), 0);
        assert_eq!(count_kos(&rolls, 85), 16);
        // Two independent rolls: pairs whose sum reaches 180 are the pairs (a, b) with a + b >= 180, a, b in 85..=100.
        let expected = (85..=100u32)
            .flat_map(|a| (85..=100u32).map(move |b| (a, b)))
            .filter(|&(a, b)| a + b >= 180)
            .count() as u32;
        assert_eq!(count_pair_kos(&rolls, 180), expected);
        assert_eq!(count_pair_kos(&rolls, 170), 256);
        assert_eq!(count_pair_kos(&rolls, 201), 0);
    }
}
