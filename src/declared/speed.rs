//! The `speed_order` query: the declared actions' order, as the reference orders them.
//!
//! The reference resolves each declared action with the engine's own queue (`resolveAction`: speed, the priority events,
//! the fractional priority) and orders them with `comparePriority`; two actions that compare equal are one tie group,
//! never sampled. Mega Evolutions are their own actions (order 104), run in speed order before any move is
//! read, and every move's priority and Speed are read after them. Any draw ends the reference probe with `unsupported`
//! (`random_effect`); the layer finds the same draws by running the same engine steps with the engine's own random source
//! recording (a Quick Claw roll, a declared sleep, a tie-break in a handler sort).

use crate::battle::{Battle, MonRef};
use crate::dex::types::*;

use super::outcome::{Abstain, ActionOut, Answer, Outcome, Reason};
use super::scenario::{declare_field, declare_status, evolve, restore_stages_and_item, Teams};
use super::spec::{Action, Combatant, FieldState};
use super::Prober;

const SWITCH_ORDER: u32 = 103;
const MEGA_ORDER: u32 = 104;
const MOVE_ORDER: u32 = 200;

/// How many draws (not sorts) the battle's random source has recorded.
fn draws(b: &Battle) -> usize {
    b.rng
        .trace
        .as_ref()
        .map_or(0, |t| t.iter().filter(|l| !l.starts_with('#')).count())
}

/// A run's early end: the answer to give, or an abstention.
enum Stop {
    Draw(u32),
    Abstain(Abstain),
}

impl From<Abstain> for Stop {
    fn from(a: Abstain) -> Stop {
        Stop::Abstain(a)
    }
}

struct Resolved {
    index: usize,
    r: MonRef,
    /// A move id, or `None` for a switch.
    move_id: Option<MoveId>,
    /// The fractional priority the queue computed when the action was resolved (before any Evolution), in tenths.
    frac: i32,
    /// The speed of the Evolution action (before the Evolution), when the action Mega Evolves.
    mega_speed: Option<i32>,
    /// The own action's priority and speed, read after the Evolutions.
    priority: i32,
    speed: i32,
}

impl Prober {
    pub(super) fn speed_order(
        &mut self,
        sides: &[Vec<Combatant>; 2],
        actions: &[Action],
        state: &FieldState,
    ) -> Outcome {
        match self.speed_inner(sides, actions, state) {
            Ok(outcome) => outcome,
            Err(Stop::Draw(position)) => Outcome::Unsupported {
                reason: Reason::RandomEffect,
                position: Some(position),
            },
            Err(Stop::Abstain(abstain)) => abstain.into(),
        }
    }

    fn speed_inner(
        &mut self,
        sides: &[Vec<Combatant>; 2],
        actions: &[Action],
        state: &FieldState,
    ) -> Result<Outcome, Stop> {
        // The two teams as the reference declares them: a side with one Pokémon starts a copy of it, fainted at once.
        let mut teams: Teams = [Vec::new(), Vec::new()];
        let mut key = vec![1u32];
        let mut items = [[None, None], [None, None]];
        for (s, combatants) in sides.iter().enumerate() {
            if combatants.is_empty() || combatants.len() > 2 {
                return Err(Abstain::new("query:sides", "a side holds one or two Pokemon").into());
            }
            key.push(combatants.len() as u32);
            for (i, c) in combatants.iter().enumerate() {
                let reg = self.registered(c.set)?;
                teams[s].push(reg.mon.clone());
                items[s][i] = reg.item;
                key.push(c.set);
            }
            if combatants.len() == 1 {
                let reg = self.registered(combatants[0].set)?;
                teams[s].push(reg.mon.clone());
                items[s][1] = reg.item;
            }
        }
        let mut b = self.started(key, &teams)?;
        // Retire the copy of a side with one declared Pokémon.
        for (s, combatants) in sides.iter().enumerate() {
            if combatants.len() == 1 {
                let r = (s, 1);
                b.mon_mut(r).hp = 0;
                b.queue_faint(r);
            }
        }
        b.faint_messages();
        b.rng.trace = Some(Vec::new());
        let mut mark = draws(&b);
        let mut at: u32;
        macro_rules! check_draw {
            () => {
                if draws(&b) != mark {
                    return Err(Stop::Draw(at));
                }
            };
        }

        let lead: MonRef = (0, 0);
        let places: Vec<(usize, usize)> = sides
            .iter()
            .enumerate()
            .flat_map(|(s, cs)| (0..cs.len()).map(move |i| (s, i)))
            .collect();
        let combatant = |s: usize, i: usize| &sides[s][i];

        // A status the player saw before the field changed is an observed fact: a Pokemon that does not Mega Evolve
        // first tries to take it before the declared field. Anything but success changes nothing. A draw is kept and
        // raised at the old place (so the position it reports is the combatant's).
        let mut taken = vec![false; places.len()];
        let mut drawn: Option<usize> = None;
        for (place, &(s, i)) in places.iter().enumerate() {
            let c = combatant(s, i);
            if c.status.is_none() || c.mega || drawn.is_some() {
                continue;
            }
            let before = draws(&b);
            if declare_status(&mut b, (s, i), c.status.as_deref().unwrap_or("")).is_ok() {
                taken[place] = true;
                if draws(&b) != before {
                    drawn = Some(place);
                }
            }
            mark = draws(&b);
        }
        at = 0;
        declare_field(&mut b, state, lead, false)?;
        check_draw!();
        for (place, &(s, i)) in places.iter().enumerate() {
            at = place as u32;
            let r = (s, i);
            let c = combatant(s, i);
            if c.mega {
                evolve(&mut b, r)?;
                check_draw!();
            }
            if let Some(status) = &c.status {
                if drawn == Some(place) {
                    return Err(Stop::Draw(at));
                }
                if !taken[place] {
                    declare_status(&mut b, r, status)?;
                    check_draw!();
                }
            }
            if let Some(hp) = c.hp {
                let m = b.mon_mut(r);
                if hp == 0 || hp as u16 > m.maxhp {
                    return Err(
                        Abstain::new("state:hp", "the declared HP exceeds the maximum").into(),
                    );
                }
                m.hp = hp as u16;
            }
        }
        if sides.iter().any(|cs| cs.iter().any(|c| c.mega)) {
            at = 0;
            declare_field(&mut b, state, lead, false)?;
            check_draw!();
        }
        for (s, cs) in sides.iter().enumerate() {
            for (i, c) in cs.iter().enumerate() {
                restore_stages_and_item(&mut b, (s, i), c, items[s][i]);
            }
        }

        // The engine's queue entries for the declared actions.
        let mut resolved: Vec<Resolved> = Vec::with_capacity(actions.len());
        for (index, action) in actions.iter().enumerate() {
            at = index as u32;
            let r = (action.side, action.position);
            if action.side > 1 || action.position >= sides[action.side].len() {
                return Err(Abstain::new(
                    "query:action",
                    "the action names a Pokemon the sides do not declare",
                )
                .into());
            }
            let move_id = match &action.move_id {
                None => None,
                Some(m) => Some(
                    self.dex
                        .move_id(m)
                        .filter(|&id| self.dex.mv(id).id == *m)
                        .ok_or_else(|| {
                            Abstain::new(format!("move:{m}"), "the move is not an engine move")
                        })?,
                ),
            };
            // One Mega Evolution a side: once a side has evolved (declared or earlier in this loop), no other Pokémon of it can.
            if action.mega
                && (move_id.is_none()
                    || b.sides[action.side].mega_used
                    || b.can_mega_evo(r).is_none())
            {
                return Err(Abstain::new("state:mega", "the Pokemon cannot Mega Evolve").into());
            }
            let mega_speed = if action.mega {
                Some(b.action_speed_draw(r))
            } else {
                None
            };
            check_draw!();
            let mut frac = 0;
            if let Some(id) = move_id {
                let mv = self.dex.mv(id);
                // The queue entry's fractional priority is drawn before the entry is checked for extra actions.
                frac = b.run_fractional_priority(r, mv.category == Category::Status);
                check_draw!();
                if mv
                    .callbacks
                    .iter()
                    .any(|c| c == "beforeTurnCallback" || c == "priorityChargeCallback")
                {
                    return Err(Abstain::new(
                        format!("move:{}", mv.id),
                        "the move queues more than a move and a Mega Evolution",
                    )
                    .into());
                }
            }
            // The action's own speed (and priority), as `getActionSpeed` computes them when the action is queued.
            b.action_speed_draw(r);
            check_draw!();
            resolved.push(Resolved {
                index,
                r,
                move_id,
                frac,
                mega_speed,
                priority: 0,
                speed: 0,
            });
        }

        // The Evolutions, in speed order: a group of equal speed in index order.
        let mut mega_items: Vec<(usize, i32, i32)> = resolved
            .iter()
            .filter_map(|x| x.mega_speed.map(|s| (x.index, 0, s)))
            .collect();
        let mega_groups = groups_of(&mut mega_items);
        for group in &mega_groups {
            for &index in group {
                at = index as u32;
                evolve(&mut b, resolved[index].r)?;
                check_draw!();
            }
        }

        // Every own action's speed and priority, read after the Evolutions.
        for item in resolved.iter_mut() {
            at = item.index as u32;
            item.speed = b.action_speed_draw(item.r);
            check_draw!();
            item.priority = match item.move_id {
                Some(id) => {
                    let mv = self.dex.mv(id);
                    b.move_priority(item.r, mv) as i32 * 10 + item.frac
                }
                None => 0,
            };
            if item.priority % 10 != 0 {
                return Err(Abstain::new(
                    "state:fractional_priority",
                    "the action's priority is not a whole number",
                )
                .into());
            }
            item.priority /= 10;
        }

        let order_of = |x: &Resolved| {
            if x.move_id.is_some() {
                MOVE_ORDER
            } else {
                SWITCH_ORDER
            }
        };
        let mut switches: Vec<(usize, i32, i32)> = resolved
            .iter()
            .filter(|x| x.move_id.is_none())
            .map(|x| (x.index, x.priority, x.speed))
            .collect();
        let mut moves: Vec<(usize, i32, i32)> = resolved
            .iter()
            .filter(|x| x.move_id.is_some())
            .map(|x| (x.index, x.priority, x.speed))
            .collect();
        let switch_groups = groups_of(&mut switches);
        let move_groups = groups_of(&mut moves);
        let _ = MEGA_ORDER;

        let out: Vec<ActionOut> = resolved
            .iter()
            .zip(actions)
            .map(|(x, a)| ActionOut {
                side: a.side,
                position: a.position,
                mega: a.mega,
                order: order_of(x),
                priority: x.priority,
                speed: x.speed,
            })
            .collect();
        let mut groups = switch_groups;
        groups.extend(mega_groups);
        groups.extend(move_groups);
        Ok(Outcome::Computed {
            answer: Answer::SpeedOrder {
                order_basis: "turn_start",
                actions: out,
                groups,
            },
        })
    }
}

/// The engine's order of a list of actions of one kind (`comparePriority`: a stable sort by priority then Speed, higher
/// first) cut at every pair it calls equal; each group lists the actions' indices in the list's order. An item is
/// `(index, priority, speed)`.
fn groups_of(items: &mut [(usize, i32, i32)]) -> Vec<Vec<usize>> {
    items.sort_by(|a, b| b.1.cmp(&a.1).then(b.2.cmp(&a.2)));
    let mut groups: Vec<Vec<usize>> = Vec::new();
    let mut last: Option<(i32, i32)> = None;
    for &(index, priority, speed) in items.iter() {
        if last == Some((priority, speed)) {
            groups.last_mut().expect("a group is open").push(index);
        } else {
            groups.push(vec![index]);
            last = Some((priority, speed));
        }
    }
    groups
}

#[cfg(test)]
mod tests {
    use super::groups_of;

    #[test]
    fn actions_are_ordered_by_priority_then_speed_and_a_tie_is_one_group() {
        // (index, priority, speed): 0 and 3 tie at priority 0 and speed 100; 1 has priority 1; 2 is slower.
        let mut items = [(0, 0, 100), (1, 1, 50), (2, 0, 80), (3, 0, 100)];
        assert_eq!(groups_of(&mut items), vec![vec![1], vec![0, 3], vec![2]]);
        // Trick Room negates the speeds: the slower acts first, and the order of equal items stays the list's.
        let mut items = [(0, 0, -100), (1, 0, -50), (2, 0, -100)];
        assert_eq!(groups_of(&mut items), vec![vec![1], vec![0, 2]]);
        assert!(groups_of(&mut []).is_empty());
    }
}
