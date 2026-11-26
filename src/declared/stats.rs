//! The `stats` query: the six stats of one set, as the engine builds them (Champions Stat Points).

use super::outcome::{Answer, Outcome, StatsOut};
use super::Prober;

impl Prober {
    pub(super) fn stats(&mut self, set: u32) -> Outcome {
        let reg = match self.registered(set) {
            Ok(r) => r,
            Err(a) => return a.into(),
        };
        let mon = crate::battle::build_mon(&self.dex, &reg.mon, 0);
        let s = mon.stats;
        Outcome::Computed {
            answer: Answer::Stats {
                stats: StatsOut {
                    hp: mon.maxhp as u32,
                    atk: s[1] as u32,
                    def: s[2] as u32,
                    spa: s[3] as u32,
                    spd: s[4] as u32,
                    spe: s[5] as u32,
                },
            },
        }
    }
}
