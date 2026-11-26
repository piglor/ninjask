//! The wire form of the declared-state layer: one JSON request in, one JSON response out.
//!
//! A caller registers its sets once (`sets`), names the mechanics config's `ko_blockers` (`blockers`) and then sends
//! batches of queries (`batch`); a batch's results come back in query order, each the answer part of a
//! `MechanicsResult`, an `unsupported` outcome, or an abstention. A frame that does not parse is an error response, never
//! a panic: the process that serves frames (`ninjask-probe`) must survive any input a caller can send.

use serde::{Deserialize, Serialize};

use super::outcome::Outcome;
use super::spec::{Query, SetSpec};
use super::{KoBlockers, Prober};

#[derive(Debug, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum Request {
    Sets {
        sets: Vec<(u32, SetSpec)>,
    },
    Blockers {
        items: Vec<String>,
        abilities: Vec<String>,
    },
    Batch {
        queries: Vec<Query>,
    },
    Counts,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum Response {
    Sets { failed: Vec<(u32, String)> },
    Done { ok: bool },
    Results { results: Vec<Outcome> },
    Counts { counts: serde_json::Value },
    Error { error: String },
}

/// Serve one request frame.
pub fn serve(prober: &mut Prober, frame: &[u8]) -> Vec<u8> {
    let response = match serde_json::from_slice::<Request>(frame) {
        Err(e) => Response::Error {
            error: format!("the frame is not a request: {e}"),
        },
        Ok(Request::Sets { sets }) => {
            let mut failed = Vec::new();
            for (id, spec) in &sets {
                if let Err(message) = prober.register_set(*id, spec) {
                    failed.push((*id, message));
                }
            }
            Response::Sets { failed }
        }
        Ok(Request::Blockers { items, abilities }) => {
            prober.set_ko_blockers(KoBlockers {
                items: items.into_iter().collect(),
                abilities: abilities.into_iter().collect(),
            });
            Response::Done { ok: true }
        }
        Ok(Request::Batch { queries }) => Response::Results {
            results: queries.iter().map(|q| prober.answer(q)).collect(),
        },
        Ok(Request::Counts) => {
            let c = &prober.counts;
            let mut by: Vec<(&String, &u64)> = c.by_entity.iter().collect();
            by.sort();
            Response::Counts {
                counts: serde_json::json!({
                    "answered": c.answered, "unsupported": c.unsupported, "abstained": c.abstained,
                    "started": c.started, "reused": c.reused,
                    "by_entity": by.into_iter().collect::<std::collections::BTreeMap<_, _>>(),
                }),
            }
        }
    };
    serde_json::to_vec(&response)
        .unwrap_or_else(|_| b"{\"error\":\"unserialisable response\"}".to_vec())
}
